//! Concurrent orchestration of RIPEstat `bgp-state`, `rpki-validation`, and `as-overview` lookups
//! for the host's public IPv4 and IPv6 addresses.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use bench_api::Context;
use bench_api::HttpError;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;
use bench_api::Step;
use bench_api::Stop;
use bench_host::network::Address;

use super::asns;
use super::parse;
use super::result::Failure;
use super::result::FamilyBgp;
use super::result::Output;
use super::result::RpkiStatus;
use super::result::Settings;

/// Default RIPEstat Data API root URL.
pub const DEFAULT_BASE_URL: &str = "https://stat.ripe.net/data";

/// Maximum `bgp-state` payload read (2 MiB, covering dense global anycast prefixes).
const BGP_STATE_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Timeout of a `bgp-state` query.
const BGP_STATE_TIMEOUT: Duration = Duration::from_secs(15);

/// Timeout of an `rpki-validation` or `as-overview` enrichment query.
const ENRICH_TIMEOUT: Duration = Duration::from_secs(6);

/// Maximum unknown ASNs looked up dynamically via `as-overview` per family.
const MAX_ASN_LOOKUPS: usize = 16;

/// Concurrency cap for `as-overview` lookups.
const ASN_LOOKUP_CONCURRENCY: usize = 8;

/// Runs `network.bgp` across the host's reachable public IPv4 and IPv6 addresses.
pub async fn measure(
    ctx: &Context<Output>,
    settings: Settings,
    base_url: Option<&str>,
) -> Result<Output, Stop<Failure>> {
    let network = &ctx.host().network;
    let ipv4_reachable = network.ipv4.reachable();
    let ipv6_reachable = network.ipv6.reachable();
    if !ipv4_reachable && !ipv6_reachable {
        return Err(Stop::Skipped(Failure::NoFamily));
    }

    let v4_ip = if ipv4_reachable {
        found_ip(&network.ipv4.address)
    } else {
        None
    };
    let v6_ip = if ipv6_reachable {
        found_ip(&network.ipv6.address)
    } else {
        None
    };
    if v4_ip.is_none() && v6_ip.is_none() {
        return Err(Stop::Skipped(Failure::NoPublicAddress));
    }

    let base = base_url.unwrap_or(DEFAULT_BASE_URL);
    let mut output = Output {
        settings,
        ipv4_reachable,
        ipv6_reachable,
        ipv4: None,
        ipv6: None,
        notes: Vec::new(),
    };
    ctx.publish(&output);

    let targets = [
        (IpFamily::Ipv4, v4_ip, ipv6_reachable, false),
        (IpFamily::Ipv6, v6_ip, ipv4_reachable, true),
    ];
    let mut first_error: Option<String> = None;

    for (family, maybe_ip, other_reachable, ipv6) in targets {
        let Some(ip) = maybe_ip else {
            continue;
        };
        if ctx.cancel_token().is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }
        ctx.progress(Step::Bgp { ipv6 });
        let fallback =
            other_reachable.then_some(if ipv6 { IpFamily::Ipv4 } else { IpFamily::Ipv6 });

        match query_family(ctx, base, ip, family, fallback).await {
            Ok(report) => {
                if ipv6 {
                    output.ipv6 = Some(Box::new(report));
                } else {
                    output.ipv4 = Some(Box::new(report));
                }
                ctx.publish(&output);
            }
            Err(FamilyError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            Err(FamilyError::Failed(reason)) => {
                if first_error.is_none() {
                    first_error = Some(reason.clone());
                }
                output.notes.push((ipv6, reason));
            }
        }
    }

    if output.ipv4.is_none() && output.ipv6.is_none() {
        let reason = first_error.unwrap_or_else(|| "no BGP data returned".to_owned());
        return Err(Stop::Failed(Failure::QueryFailed(reason)));
    }

    Ok(output)
}

enum FamilyError {
    Cancelled,
    Failed(String),
}

async fn query_family(
    ctx: &Context<Output>,
    base: &str,
    ip: IpAddr,
    family: IpFamily,
    fallback: Option<IpFamily>,
) -> Result<FamilyBgp, FamilyError> {
    let state_url = format!("{base}/bgp-state/data.json?resource={ip}");
    let state_resp = fetch_with_fallback(
        ctx,
        &state_url,
        family,
        fallback,
        BGP_STATE_TIMEOUT,
        BGP_STATE_MAX_BYTES,
    )
    .await?;
    if state_resp.status != 200 {
        return Err(FamilyError::Failed(format!(
            "bgp-state returned HTTP {}",
            state_resp.status
        )));
    }
    let parsed = parse::parse_bgp_state(&state_resp.body).map_err(FamilyError::Failed)?;

    if !parsed.announced {
        return Ok(FamilyBgp {
            address: ip,
            announced: false,
            prefix: parsed.prefix,
            prefix_base: parsed.prefix_base,
            prefix_len: parsed.prefix_len,
            origin_asn: None,
            origins: Vec::new(),
            rpki: RpkiStatus::Unknown,
            total_peers: 0,
            distinct_paths: 0,
            upstreams: Vec::new(),
            paths: Vec::new(),
            asn_names: BTreeMap::new(),
        });
    }

    // Collect candidate ASNs appearing in origins, direct upstreams, and truncated graph paths.
    let mut needed_asns = BTreeSet::new();
    for origin in &parsed.origins {
        needed_asns.insert(origin.asn);
    }
    for up in parsed.upstreams.iter().take(12) {
        needed_asns.insert(up.asn);
    }
    for path in &parsed.paths {
        for asn in asns::trunc_to_tier1(&path.asns) {
            needed_asns.insert(asn);
        }
    }

    let mut asn_names = BTreeMap::new();
    let mut unknown_asns = Vec::new();
    for asn in needed_asns {
        if let Some(known) = asns::known_name(asn) {
            asn_names.insert(asn, known.to_owned());
        } else if unknown_asns.len() < MAX_ASN_LOOKUPS {
            unknown_asns.push(asn);
        }
    }

    // Run RPKI validation and unknown ASN holder lookups concurrently.
    let rpki_fut = async {
        let (Some(origin), Some(prefix)) = (parsed.origin_asn, parsed.prefix.as_deref()) else {
            return RpkiStatus::Unknown;
        };
        let url = format!("{base}/rpki-validation/data.json?resource={origin}&prefix={prefix}");
        match fetch_with_fallback(
            ctx,
            &url,
            family,
            fallback,
            ENRICH_TIMEOUT,
            bench_api::DEFAULT_HTTP_LIMIT,
        )
        .await
        {
            Ok(resp) if resp.status == 200 => {
                parse::parse_rpki(&resp.body).unwrap_or(RpkiStatus::Unknown)
            }
            _ => RpkiStatus::Unknown,
        }
    };

    let asn_tasks = unknown_asns.into_iter().map(|asn| async move {
        let url = format!("{base}/as-overview/data.json?resource=AS{asn}");
        let res = fetch_with_fallback(
            ctx,
            &url,
            family,
            fallback,
            ENRICH_TIMEOUT,
            bench_api::DEFAULT_HTTP_LIMIT,
        )
        .await;
        let name = match res {
            Ok(resp) if resp.status == 200 => parse::parse_as_overview(&resp.body),
            _ => None,
        };
        (asn, name)
    });

    let (rpki, resolved_names) =
        tokio::join!(rpki_fut, run_bounded(ASN_LOOKUP_CONCURRENCY, asn_tasks));
    if ctx.cancel_token().is_cancelled() {
        return Err(FamilyError::Cancelled);
    }
    for (asn, maybe_name) in resolved_names {
        if let Some(name) = maybe_name {
            asn_names.insert(asn, name);
        }
    }

    Ok(FamilyBgp {
        address: ip,
        announced: true,
        prefix: parsed.prefix,
        prefix_base: parsed.prefix_base,
        prefix_len: parsed.prefix_len,
        origin_asn: parsed.origin_asn,
        origins: parsed.origins,
        rpki,
        total_peers: parsed.total_peers,
        distinct_paths: parsed.distinct_paths,
        upstreams: parsed.upstreams,
        paths: parsed.paths,
        asn_names,
    })
}

async fn fetch_with_fallback(
    ctx: &Context<Output>,
    url: &str,
    family: IpFamily,
    fallback: Option<IpFamily>,
    timeout: Duration,
    max_bytes: usize,
) -> Result<HttpResponse, FamilyError> {
    let build = |fam: IpFamily| {
        HttpRequest::get(fam, url)
            .with_redirect(RedirectPolicy::Follow { max: 2 })
            .with_timeout(timeout)
            .with_max_bytes(max_bytes)
    };
    match ctx.probe(&build(family)).await {
        Ok(resp) => Ok(resp),
        Err(HttpError::Cancelled) => Err(FamilyError::Cancelled),
        Err(primary_err) => {
            if let Some(other) = fallback {
                match ctx.probe(&build(other)).await {
                    Ok(resp) => return Ok(resp),
                    Err(HttpError::Cancelled) => return Err(FamilyError::Cancelled),
                    Err(_) => {}
                }
            }
            Err(FamilyError::Failed(http_error_text(&primary_err)))
        }
    }
}

fn http_error_text(err: &HttpError) -> String {
    match err {
        HttpError::NoAddress => "API host has no address for this IP family".to_owned(),
        HttpError::TimedOut => "request timed out".to_owned(),
        HttpError::TooManyRedirects => "too many HTTP redirects".to_owned(),
        HttpError::InvalidRedirect(why) => format!("invalid redirect: {why}"),
        HttpError::NotHttps(why) => format!("non-HTTPS endpoint: {why}"),
        HttpError::Transport(why) => why.clone(),
        HttpError::Cancelled => "cancelled".to_owned(),
    }
}

fn found_ip(address: &Address) -> Option<IpAddr> {
    match address {
        Address::Found { address, .. } => Some(*address),
        _ => None,
    }
}

type BoxedTask<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

async fn run_bounded<'a, T, I, Fut>(limit: usize, tasks: I) -> Vec<T>
where
    I: IntoIterator<Item = Fut>,
    Fut: Future<Output = T> + Send + 'a,
{
    let cap = limit.max(1);
    let mut pending = tasks.into_iter().enumerate();
    let mut active: Vec<(usize, BoxedTask<'a, T>)> = Vec::new();
    let mut done: Vec<(usize, T)> = Vec::new();
    loop {
        while active.len() < cap {
            let Some((idx, fut)) = pending.next() else {
                break;
            };
            active.push((idx, Box::pin(fut)));
        }
        if active.is_empty() {
            break;
        }
        let (pos, value) = std::future::poll_fn(|cx| {
            for (index, (_, fut)) in active.iter_mut().enumerate() {
                if let Poll::Ready(value) = fut.as_mut().poll(cx) {
                    return Poll::Ready((index, value));
                }
            }
            Poll::Pending
        })
        .await;
        let (orig_idx, _) = active.swap_remove(pos);
        done.push((orig_idx, value));
    }
    done.sort_by_key(|(idx, _)| *idx);
    done.into_iter().map(|(_, value)| value).collect()
}
