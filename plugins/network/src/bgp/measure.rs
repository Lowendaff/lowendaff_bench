//! Concurrent orchestration of RIPEstat `bgp-state`, `rpki-validation`, and `as-overview` lookups
//! for the host's public IPv4 and IPv6 addresses.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench_api::Context;
use bench_api::DEFAULT_HTTP_LIMIT;
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
use super::result::QueryError;
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
    if ctx.cancel_token().is_cancelled() {
        return Err(Stop::Failed(Failure::Cancelled));
    }
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

    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let mut output = Output {
        settings,
        ipv4_reachable,
        ipv6_reachable,
        ipv4: None,
        ipv6: None,
        notes: Vec::new(),
        upload_notes: Vec::new(),
    };

    let targets = [
        (IpFamily::Ipv4, v4_ip, ipv6_reachable, false),
        (IpFamily::Ipv6, v6_ip, ipv4_reachable, true),
    ];
    let mut first_error: Option<QueryError> = None;

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
                if first_error
                    .as_ref()
                    .is_none_or(|prev| matches!(prev, QueryError::NoAddress))
                {
                    first_error = Some(reason.clone());
                }
                output.notes.push((ipv6, reason));
                if output.ipv4.is_some() || output.ipv6.is_some() {
                    ctx.publish(&output);
                }
            }
        }
    }

    if output.ipv4.is_none() && output.ipv6.is_none() {
        let reason = first_error.unwrap_or(QueryError::NoData);
        return Err(Stop::Failed(Failure::QueryFailed(reason)));
    }

    Ok(output)
}

enum FamilyError {
    Cancelled,
    Failed(QueryError),
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
        ip,
        family,
        fallback,
        BGP_STATE_TIMEOUT,
        BGP_STATE_MAX_BYTES,
    )
    .await?;
    if state_resp.status != 200 {
        return Err(FamilyError::Failed(QueryError::HttpStatus(
            state_resp.status,
        )));
    }
    let parsed = parse::parse_bgp_state(&state_resp.body)
        .map_err(|err| FamilyError::Failed(scrub_query_error(err, ip)))?;

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
            diagram_url: None,
        });
    }

    // Collect candidate ASNs in priority order: origins first, then top direct upstreams, then
    // truncated graph hops ordered by peer share so high-numbered origin ASNs are never starved.
    let mut seen_asns = BTreeSet::new();
    let mut ordered_asns = Vec::new();
    let mut push_asn = |asn: u32| {
        if seen_asns.insert(asn) {
            ordered_asns.push(asn);
        }
    };
    for origin in &parsed.origins {
        push_asn(origin.asn);
    }
    let top_upstreams = parsed.upstreams.iter().take(12);
    for up in top_upstreams {
        push_asn(up.asn);
    }
    for path in &parsed.paths {
        for asn in asns::trunc_to_tier1(&path.asns) {
            push_asn(asn);
        }
    }

    let mut asn_names = BTreeMap::new();
    let mut unknown_asns = Vec::new();
    for asn in ordered_asns {
        if let Some(known) = asns::known_name(asn) {
            asn_names.insert(asn, known.to_owned());
        } else if unknown_asns.len() < MAX_ASN_LOOKUPS {
            unknown_asns.push(asn);
        }
    }

    // Run RPKI validation and unknown ASN holder lookups concurrently.
    let rpki_prefix = match (parsed.prefix_base, parsed.prefix_len) {
        (Some(base_ip), Some(len)) => Some(format!("{base_ip}/{len}")),
        _ => parsed.prefix.as_deref().map(|s| s.trim().to_owned()),
    };
    let rpki_fut = async {
        let (Some(origin), Some(prefix)) = (parsed.origin_asn, rpki_prefix.as_deref()) else {
            return Ok(RpkiStatus::Unknown);
        };
        let url = format!("{base}/rpki-validation/data.json?resource={origin}&prefix={prefix}");
        match fetch_with_fallback(
            ctx,
            &url,
            ip,
            family,
            fallback,
            ENRICH_TIMEOUT,
            DEFAULT_HTTP_LIMIT,
        )
        .await
        {
            Ok(resp) if resp.status == 200 => {
                Ok(parse::parse_rpki(&resp.body).unwrap_or(RpkiStatus::Unknown))
            }
            Err(FamilyError::Cancelled) => Err(FamilyError::Cancelled),
            _ => Ok(RpkiStatus::Unknown),
        }
    };

    let saw_cancel = AtomicBool::new(false);
    let saw_cancel_ref = &saw_cancel;
    let asn_tasks = unknown_asns.into_iter().map(|asn| async move {
        let url = format!("{base}/as-overview/data.json?resource=AS{asn}");
        let res = fetch_with_fallback(
            ctx,
            &url,
            ip,
            family,
            fallback,
            ENRICH_TIMEOUT,
            DEFAULT_HTTP_LIMIT,
        )
        .await;
        let name = match res {
            Ok(resp) if resp.status == 200 => parse::parse_as_overview(&resp.body),
            Err(FamilyError::Cancelled) => {
                saw_cancel_ref.store(true, Ordering::Relaxed);
                None
            }
            _ => None,
        };
        (asn, name)
    });

    let cancel = ctx.cancel_token();
    let (rpki_res, resolved_names) = tokio::join!(
        rpki_fut,
        bench_api::run_bounded(ASN_LOOKUP_CONCURRENCY, &cancel, asn_tasks)
    );
    let rpki = rpki_res?;
    if cancel.is_cancelled() || saw_cancel.load(Ordering::Relaxed) {
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
        diagram_url: None,
    })
}

async fn fetch_with_fallback(
    ctx: &Context<Output>,
    url: &str,
    ip: IpAddr,
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
            if !matches!(primary_err, HttpError::NotHttps(_))
                && let Some(other) = fallback
            {
                match ctx.probe(&build(other)).await {
                    Ok(resp) => return Ok(resp),
                    Err(HttpError::Cancelled) => return Err(FamilyError::Cancelled),
                    Err(fallback_err) if matches!(primary_err, HttpError::NoAddress) => {
                        return Err(FamilyError::Failed(http_query_error(&fallback_err, ip)));
                    }
                    Err(_) => {}
                }
            }
            Err(FamilyError::Failed(http_query_error(&primary_err, ip)))
        }
    }
}

fn http_query_error(err: &HttpError, ip: IpAddr) -> QueryError {
    match err {
        HttpError::NoAddress => QueryError::NoAddress,
        HttpError::TimedOut => QueryError::TimedOut,
        HttpError::TooManyRedirects => QueryError::TooManyRedirects,
        HttpError::InvalidRedirect(why) => QueryError::InvalidRedirect(scrub_detail(why, ip)),
        HttpError::NotHttps(why) => QueryError::NotHttps(scrub_url_or_detail(why, ip)),
        HttpError::Transport(why) => QueryError::Transport(scrub_detail(why, ip)),
        HttpError::Cancelled => QueryError::NoData,
    }
}

fn scrub_query_error(err: QueryError, ip: IpAddr) -> QueryError {
    match err {
        QueryError::InvalidRedirect(why) => QueryError::InvalidRedirect(scrub_detail(&why, ip)),
        QueryError::NotHttps(why) => QueryError::NotHttps(scrub_url_or_detail(&why, ip)),
        QueryError::Transport(why) => QueryError::Transport(scrub_detail(&why, ip)),
        QueryError::InvalidJson(why) => QueryError::InvalidJson(scrub_detail(&why, ip)),
        QueryError::ApiStatus(why) => QueryError::ApiStatus(scrub_detail(&why, ip)),
        other => other,
    }
}

fn scrub_url_or_detail(raw: &str, ip: IpAddr) -> String {
    let trimmed = raw.trim();
    for scheme in ["http://", "https://"] {
        if let Some(rest) = trimmed.strip_prefix(scheme) {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            let host = authority
                .rsplit_once('@')
                .map_or(authority, |(_, h)| h)
                .trim();
            return scrub_detail(&format!("{scheme}{host}"), ip);
        }
    }
    scrub_detail(trimmed, ip)
}

fn scrub_detail(raw: &str, ip: IpAddr) -> String {
    let mut text = strip_for_url(raw);
    for scheme in ["https://", "http://"] {
        text = redact_inline_urls(&text, scheme);
    }
    let needle = ip.to_string();
    if !needle.is_empty() {
        text = replace_ascii_case_insensitive(&text, &needle, &mask_ip(ip));
    }
    let cleaned: String = text
        .chars()
        .filter(|&c| {
            !c.is_control()
                && !matches!(
                    c,
                    '\u{061c}'
                        | '\u{200e}'
                        | '\u{200f}'
                        | '\u{202a}'..='\u{202e}'
                        | '\u{2066}'..='\u{2069}'
                )
        })
        .take(256)
        .collect();
    cleaned.trim().to_owned()
}

fn strip_for_url(raw: &str) -> String {
    let Some(start) = raw.find(" for url (") else {
        return raw.to_owned();
    };
    let tail = &raw[start + " for url (".len()..];
    if let Some(end) = tail.find(')') {
        format!("{}{}", &raw[..start], &tail[end + 1..])
    } else {
        raw[..start].to_owned()
    }
}

fn redact_inline_urls(text: &str, scheme: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(scheme) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + scheme.len()..];
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | ']' | '"' | '\'' | ',' | ';'))
            .unwrap_or(after.len());
        let url_body = &after[..end];
        let authority = url_body.split(['/', '?', '#']).next().unwrap_or_default();
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        out.push_str(scheme);
        out.push_str(host);
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn replace_ascii_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    let h_bytes = haystack.as_bytes();
    let n_bytes = needle.as_bytes();
    if n_bytes.is_empty() || h_bytes.len() < n_bytes.len() {
        return haystack.to_owned();
    }
    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    let mut idx = 0;
    while idx + n_bytes.len() <= h_bytes.len() {
        if haystack.is_char_boundary(idx)
            && haystack.is_char_boundary(idx + n_bytes.len())
            && h_bytes[idx..idx + n_bytes.len()].eq_ignore_ascii_case(n_bytes)
        {
            out.push_str(&haystack[cursor..idx]);
            out.push_str(replacement);
            idx += n_bytes.len();
            cursor = idx;
        } else {
            idx += 1;
        }
    }
    out.push_str(&haystack[cursor..]);
    out
}

fn mask_ip(address: IpAddr) -> String {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, _, _] = v4.octets();
            format!("{a}.{b}.*.*")
        }
        IpAddr::V6(v6) => {
            let [first, second, ..] = v6.segments();
            format!("{first:x}:{second:x}:*")
        }
    }
}

fn found_ip(address: &Address) -> Option<IpAddr> {
    match address {
        Address::Found { address, .. } => Some(*address),
        _ => None,
    }
}
