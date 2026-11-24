//! Measurement flow for `network.inetspeed`: Dual DoH (`cloudflare-dns.com` + `dns.alidns.com`)
//! endpoint discovery with system DNS fallback, candidate RTT probing and lowest-RTT pinning,
//! idle latency measurement, and 4 transfer rounds (`1T DL`, `NT DL`, `1T UL`, `NT UL`) with
//! concurrent loaded-latency probes.

use std::net::IpAddr;
use std::time::Duration;

use bench_api::Context;
use bench_api::FamilyFilter;
use bench_api::HttpError;
use bench_api::HttpRequest;
use bench_api::InetspeedDirection;
use bench_api::InetspeedTarget;
use bench_api::InetspeedTransferSpec;
use bench_api::IpFamily;
use bench_api::NetError;
use bench_api::Step;
use bench_api::Stop;

use super::parse;
use super::result::CandidateEndpoint;
use super::result::EndpointSource;
use super::result::Failure;
use super::result::Output;
use super::result::ProbeFault;
use super::result::RoundIssue;
use super::result::RoundResult;
use super::result::SelectedEndpoint;
use super::result::Settings;
use super::result::TransferDirection;
use super::result::Warning;

const DOH_TIMEOUT: Duration = Duration::from_millis(1_500);
const SYSTEM_DNS_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PROBED_CANDIDATES: usize = 10;

/// Endpoint URLs for an `Inetspeed` run.
pub struct Endpoints<'a> {
    /// Download (`GET`) URL.
    pub dl_url: &'a str,
    /// Upload (`PUT`) URL.
    pub ul_url: &'a str,
    /// Latency probe (`GET`) URL.
    pub latency_url: &'a str,
    /// Cloudflare DoH base URL.
    pub cf_doh_url: &'a str,
    /// AliDNS DoH base URL.
    pub ali_doh_url: &'a str,
}

/// Runs `network.inetspeed` and publishes partial results after discovery, idle latency, and each
/// transfer round.
pub async fn measure(
    ctx: &Context<Output>,
    settings: Settings,
    endpoints: &Endpoints<'_>,
) -> Result<Output, Stop<Failure>> {
    let families = FamilyFilter {
        ipv4: ctx.host().network.ipv4.reachable(),
        ipv6: ctx.host().network.ipv6.reachable(),
    };
    if !families.ipv4 && !families.ipv6 {
        return Err(Stop::Skipped(Failure::NoFamily));
    }

    ctx.progress(Step::Inetspeed {
        stage: "discover",
        threads: 0,
    });

    let (selected, candidates, mut warnings) =
        discover_endpoint(ctx, &settings, endpoints, families).await?;

    let mut output = Output {
        settings: settings.clone(),
        selected,
        candidates,
        idle_latency: None,
        rounds: Vec::with_capacity(4),
        total_bytes: 0,
        warnings: Vec::new(),
    };
    output.warnings.append(&mut warnings);
    ctx.publish(&output);

    let pin_ip = output.selected.ip;

    ctx.progress(Step::Inetspeed {
        stage: "latency",
        threads: 0,
    });
    let idle_target = InetspeedTarget {
        url: endpoints.latency_url.to_owned(),
        pin_ip,
    };
    match ctx
        .inetspeed_probe_rtt(&idle_target, settings.latency_count, PROBE_TIMEOUT)
        .await
    {
        Err(NetError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
        Ok(samples) => {
            output.idle_latency = parse::compute_latency(&samples);
            if output.idle_latency.is_none() {
                output.warnings.push(Warning::NoIdleSamples);
            }
        }
        Err(err) => {
            if let Some(fault) = net_err_fault(err) {
                output.warnings.push(Warning::IdleFailed(fault));
            }
        }
    }
    ctx.publish(&output);

    let mut schedule = vec![(
        InetspeedDirection::Download,
        TransferDirection::Download,
        1_u8,
    )];
    if settings.threads > 1 {
        schedule.push((
            InetspeedDirection::Download,
            TransferDirection::Download,
            settings.threads,
        ));
    }
    schedule.push((InetspeedDirection::Upload, TransferDirection::Upload, 1_u8));
    if settings.threads > 1 {
        schedule.push((
            InetspeedDirection::Upload,
            TransferDirection::Upload,
            settings.threads,
        ));
    }

    for (api_dir, res_dir, threads) in schedule {
        if ctx.cancel_token().is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }
        let stage = match res_dir {
            TransferDirection::Download => "download",
            TransferDirection::Upload => "upload",
        };
        ctx.progress(Step::Inetspeed { stage, threads });

        let transfer_url = match res_dir {
            TransferDirection::Download => endpoints.dl_url,
            TransferDirection::Upload => endpoints.ul_url,
        };
        let spec = InetspeedTransferSpec {
            direction: api_dir,
            transfer_url: transfer_url.to_owned(),
            latency_url: endpoints.latency_url.to_owned(),
            pin_ip,
            threads,
            max_bytes: settings.max_bytes,
            timeout: Duration::from_secs(u64::from(settings.timeout_secs)),
        };

        let round = match ctx.inetspeed_transfer(&spec).await {
            Err(NetError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            Ok(raw) => {
                let micros = u128::from(raw.elapsed_micros.max(1));
                let bps_u128 = (u128::from(raw.bytes) * 8 * 1_000_000 + micros / 2) / micros;
                let bps = u64::try_from(bps_u128).unwrap_or(u64::MAX);
                let mbps = (bps as f64) / 1_000_000.0;
                let error = if raw.bytes == 0 {
                    Some(RoundIssue::ZeroBytes)
                } else if raw.fault_count > 0 {
                    Some(RoundIssue::WorkerFaults(raw.fault_count))
                } else {
                    None
                };
                RoundResult {
                    direction: res_dir,
                    threads,
                    bytes: raw.bytes,
                    duration_ms: raw.elapsed_micros / 1_000,
                    bps,
                    mbps,
                    fault_count: raw.fault_count,
                    loaded_latency: parse::compute_latency(&raw.loaded_rtts_micros),
                    error,
                }
            }
            Err(err) => RoundResult {
                direction: res_dir,
                threads,
                bytes: 0,
                duration_ms: 0,
                bps: 0,
                mbps: 0.0,
                fault_count: threads,
                loaded_latency: None,
                error: net_err_fault(err).map(RoundIssue::Transport),
            },
        };
        output.total_bytes = output.total_bytes.saturating_add(round.bytes);
        output.rounds.push(round);
        ctx.publish(&output);
    }

    if !output.rounds.iter().any(|r| r.bytes > 0) {
        return Err(Stop::Failed(Failure::NoResult));
    }
    Ok(output)
}

async fn discover_endpoint(
    ctx: &Context<Output>,
    settings: &Settings,
    endpoints: &Endpoints<'_>,
    families: FamilyFilter,
) -> Result<(SelectedEndpoint, Vec<CandidateEndpoint>, Vec<Warning>), Stop<Failure>> {
    let mut warnings = Vec::new();
    let dl_host = parse::host_from_url(endpoints.dl_url).unwrap_or_default();
    let ul_host = parse::host_from_url(endpoints.ul_url).unwrap_or_default();
    let lat_host = parse::host_from_url(endpoints.latency_url).unwrap_or_default();
    let consistent = !dl_host.is_empty() && dl_host == ul_host && dl_host == lat_host;

    if !consistent {
        warnings.push(Warning::HostMismatch);
        return Ok((
            SelectedEndpoint {
                host: dl_host,
                ip: None,
                source: EndpointSource::DefaultDns,
                rtt_ms: None,
            },
            Vec::new(),
            warnings,
        ));
    }

    // Literal IP host (such as 127.0.0.1 in smoke tests) needs no DoH lookup.
    let parsed_literal: Result<IpAddr, _> = dl_host.parse();
    if let Ok(literal_ip) = parsed_literal {
        let cand =
            probe_candidate(ctx, endpoints.latency_url, literal_ip, EndpointSource::User).await?;
        let selected = SelectedEndpoint {
            host: dl_host,
            ip: Some(literal_ip),
            source: EndpointSource::User,
            rtt_ms: cand.rtt_ms,
        };
        return Ok((selected, vec![cand], warnings));
    }

    // User-forced `--inetspeed-endpoint <IP>`.
    if let Some(forced_ip) = settings.forced_endpoint {
        let cand =
            probe_candidate(ctx, endpoints.latency_url, forced_ip, EndpointSource::User).await?;
        if !cand.ok {
            warnings.push(Warning::ForcedProbeFailed(forced_ip));
        }
        let selected = SelectedEndpoint {
            host: dl_host,
            ip: Some(forced_ip),
            source: EndpointSource::User,
            rtt_ms: cand.rtt_ms,
        };
        return Ok((selected, vec![cand], warnings));
    }

    // Dual DoH (Cloudflare + AliDNS) for reachable families.
    let doh_family = if families.ipv4 {
        IpFamily::Ipv4
    } else {
        IpFamily::Ipv6
    };
    let doh_fallback = (families.ipv4 && families.ipv6).then_some(IpFamily::Ipv6);
    let doh_ips = query_dual_doh(
        ctx,
        doh_family,
        doh_fallback,
        &dl_host,
        endpoints.cf_doh_url,
        endpoints.ali_doh_url,
        families,
    )
    .await?;

    if !doh_ips.is_empty() {
        let mut original = Vec::with_capacity(doh_ips.len().min(MAX_PROBED_CANDIDATES));
        let probed_doh = doh_ips.iter().take(MAX_PROBED_CANDIDATES);
        for &ip in probed_doh {
            original
                .push(probe_candidate(ctx, endpoints.latency_url, ip, EndpointSource::Doh).await?);
        }
        let ordered = parse::order_candidates(&original);
        if let Some(chosen) = parse::choose_auto(&original, &ordered) {
            let selected = SelectedEndpoint {
                host: dl_host,
                ip: Some(chosen.ip),
                source: EndpointSource::Doh,
                rtt_ms: chosen.rtt_ms,
            };
            return Ok((selected, ordered, warnings));
        }
    }

    // Fallback to system DNS when Dual DoH returned no usable addresses.
    warnings.push(Warning::DohEmpty);
    match ctx
        .inetspeed_resolve(&dl_host, families, SYSTEM_DNS_TIMEOUT)
        .await
    {
        Err(NetError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
        Ok(sys_ips) if !sys_ips.is_empty() => {
            let mut original = Vec::with_capacity(sys_ips.len().min(MAX_PROBED_CANDIDATES));
            let probed_sys = sys_ips.iter().take(MAX_PROBED_CANDIDATES);
            for &ip in probed_sys {
                original.push(
                    probe_candidate(ctx, endpoints.latency_url, ip, EndpointSource::SystemDns)
                        .await?,
                );
            }
            let ordered = parse::order_candidates(&original);
            if let Some(chosen) = parse::choose_auto(&original, &ordered) {
                let selected = SelectedEndpoint {
                    host: dl_host,
                    ip: Some(chosen.ip),
                    source: EndpointSource::SystemDns,
                    rtt_ms: chosen.rtt_ms,
                };
                return Ok((selected, ordered, warnings));
            }
        }
        Ok(_) | Err(_) => {}
    }

    warnings.push(Warning::UnpinnedFallback);
    Ok((
        SelectedEndpoint {
            host: dl_host,
            ip: None,
            source: EndpointSource::DefaultDns,
            rtt_ms: None,
        },
        Vec::new(),
        warnings,
    ))
}

async fn query_dual_doh(
    ctx: &Context<Output>,
    doh_family: IpFamily,
    doh_fallback: Option<IpFamily>,
    host: &str,
    cf_base: &str,
    ali_base: &str,
    families: FamilyFilter,
) -> Result<Vec<IpAddr>, Stop<Failure>> {
    let mut queries: Vec<(String, bool)> = Vec::with_capacity(4);
    if families.ipv4 {
        queries.push((format!("{cf_base}?name={host}&type=A"), true));
        queries.push((format!("{ali_base}?name={host}&type=A&short=1"), false));
    }
    if families.ipv6 {
        queries.push((format!("{cf_base}?name={host}&type=AAAA"), true));
        queries.push((format!("{ali_base}?name={host}&type=AAAA&short=1"), false));
    }

    let tasks = queries
        .into_iter()
        .map(|(url, accept_dns_json)| async move {
            let send_for = |family: IpFamily| {
                let mut req = HttpRequest::get(family, url.clone())
                    .with_header("User-Agent", "iNetSpeed-CLI")
                    .with_timeout(DOH_TIMEOUT);
                if accept_dns_json {
                    req = req.with_header("Accept", "application/dns-json");
                }
                req
            };
            match ctx.probe(&send_for(doh_family)).await {
                Err(HttpError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
                Ok(resp) if resp.status == 200 => {
                    return Ok(parse::extract_doh_ips(&resp.body, families));
                }
                Ok(_) | Err(_) => {}
            }
            if let Some(fallback) = doh_fallback {
                match ctx.probe(&send_for(fallback)).await {
                    Err(HttpError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
                    Ok(resp) if resp.status == 200 => {
                        return Ok(parse::extract_doh_ips(&resp.body, families));
                    }
                    Ok(_) | Err(_) => {}
                }
            }
            Ok(Vec::new())
        });
    let cancel = ctx.cancel_token();
    let results = bench_api::run_bounded(4, &cancel, tasks).await;
    if cancel.is_cancelled() {
        return Err(Stop::Failed(Failure::Cancelled));
    }
    let mut lists = Vec::with_capacity(results.len());
    for res in results {
        lists.push(res?);
    }
    Ok(parse::merge_unique_ips(&lists))
}

async fn probe_candidate(
    ctx: &Context<Output>,
    latency_url: &str,
    ip: IpAddr,
    source: EndpointSource,
) -> Result<CandidateEndpoint, Stop<Failure>> {
    let target = InetspeedTarget {
        url: latency_url.to_owned(),
        pin_ip: Some(ip),
    };
    match ctx.inetspeed_probe_rtt(&target, 1, PROBE_TIMEOUT).await {
        Err(NetError::Cancelled) => Err(Stop::Failed(Failure::Cancelled)),
        Ok(samples) if !samples.is_empty() => {
            let rtt_ms = ((samples[0] as f64) / 10.0).round() / 100.0;
            Ok(CandidateEndpoint {
                ip,
                source,
                rtt_ms: Some(rtt_ms.max(0.01)),
                ok: true,
                error: None,
            })
        }
        Ok(_) => Ok(CandidateEndpoint {
            ip,
            source,
            rtt_ms: None,
            ok: false,
            error: Some(ProbeFault::EmptySample),
        }),
        Err(err) => Ok(CandidateEndpoint {
            ip,
            source,
            rtt_ms: None,
            ok: false,
            error: net_err_fault(err),
        }),
    }
}

fn net_err_fault(err: NetError) -> Option<ProbeFault> {
    match err {
        NetError::NoAddress => Some(ProbeFault::NoAddress),
        NetError::TimedOut => Some(ProbeFault::TimedOut),
        NetError::NoResolver => Some(ProbeFault::NoResolver),
        NetError::Transport(why) => Some(ProbeFault::Transport(why)),
        NetError::Cancelled => None,
    }
}
