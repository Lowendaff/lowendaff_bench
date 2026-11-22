//! Measurement state machine for `network.cloudflare`: runs each reachable IP family (`IPv4` and
//! `IPv6`), resolves edge metadata, samples unloaded HTTP RTT, runs concurrent download and upload
//! phases with loaded-latency probes, runs the optional STUN UDP packet-loss probe, computes
//! Waveform bufferbloat and CV stability grades, and publishes partial results per family.

use std::time::Duration;
use std::time::Instant;

use bench_api::CloudflareStunSpec;
use bench_api::CloudflareTransferSpec;
use bench_api::Context;
use bench_api::HttpError;
use bench_api::HttpRequest;
use bench_api::IpFamily;
use bench_api::NetError;
use bench_api::Step;
use bench_api::Stop;

use super::parse;
use super::parse::ParsedMeta;
use super::result::Failure;
use super::result::FamilyCloudflare;
use super::result::FamilyIssue;
use super::result::Output;
use super::result::Settings;

const REFERER: &str = "https://speed.cloudflare.com/";
const META_TIMEOUT: Duration = Duration::from_secs(5);
const LOCATIONS_MAX_BYTES: usize = 512 * 1024;
const DEFAULT_DOWNLOAD_BYTES_PER_REQ: u64 = 10_000_000;
const DEFAULT_UPLOAD_BYTES_PER_REQ: u64 = 5_000_000;
const PROBE_INTERVAL_MS: u64 = 250;
const FAST_PROBE_INTERVAL_MS: u64 = 60;
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const UDP_INTERVAL: Duration = Duration::from_millis(80);
const UDP_TIMEOUT: Duration = Duration::from_millis(600);

/// Executes `network.cloudflare` over every reachable IP family on `ctx.host()`.
pub async fn measure(
    ctx: &Context<Output>,
    settings: Settings,
    base_url: &str,
    stun_host: &str,
    stun_port: u16,
    meas_id: &str,
) -> Result<Output, Stop<Failure>> {
    let v4_ok = ctx.host().network.ipv4.reachable();
    let v6_ok = ctx.host().network.ipv6.reachable();
    if !v4_ok && !v6_ok {
        return Err(Stop::Skipped(Failure::NoFamily));
    }

    let mut output = Output {
        settings,
        ipv4: None,
        ipv6: None,
        notes: Vec::new(),
    };
    ctx.publish(&output);

    let mut families = Vec::with_capacity(2);
    if v4_ok {
        families.push(IpFamily::Ipv4);
    }
    if v6_ok {
        families.push(IpFamily::Ipv6);
    }

    let mut last_issue: Option<FamilyIssue> = None;
    for family in families {
        if ctx.cancel_token().is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }
        let ipv6 = family == IpFamily::Ipv6;
        match run_family(
            ctx, family, &settings, base_url, stun_host, stun_port, meas_id,
        )
        .await
        {
            Ok(fam_result) => {
                if ipv6 {
                    output.ipv6 = Some(Box::new(fam_result));
                } else {
                    output.ipv4 = Some(Box::new(fam_result));
                }
                ctx.publish(&output);
            }
            Err(FamilyStop::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            Err(FamilyStop::Failed(issue)) => {
                last_issue = Some(issue.clone());
                output.notes.push((ipv6, issue));
                ctx.publish(&output);
            }
        }
    }

    if output.ipv4.is_none() && output.ipv6.is_none() {
        return Err(Stop::Failed(Failure::NoResult(last_issue)));
    }
    Ok(output)
}

enum FamilyStop {
    Cancelled,
    Failed(FamilyIssue),
}

async fn run_family(
    ctx: &Context<Output>,
    family: IpFamily,
    settings: &Settings,
    base_url: &str,
    stun_host: &str,
    stun_port: u16,
    meas_id: &str,
) -> Result<FamilyCloudflare, FamilyStop> {
    let ipv6 = family == IpFamily::Ipv6;
    ctx.progress(Step::Cloudflare {
        stage: "latency",
        ipv6,
    });

    let (mut meta, locations) = fetch_meta_and_locations(ctx, family, base_url, meas_id).await?;
    let (idle_sent, idle_rtts, header_meta) =
        measure_idle_latency(ctx, family, base_url, meas_id, settings.idle_ms).await?;
    meta.merge_from(&header_meta);
    let idle_latency = parse::compute_latency_stats(idle_sent, &idle_rtts);

    let probe_interval = if settings.seconds <= 1 {
        Duration::from_millis(FAST_PROBE_INTERVAL_MS)
    } else {
        Duration::from_millis(PROBE_INTERVAL_MS)
    };
    let dl_bytes_per_req = if settings.seconds <= 1 {
        500_000
    } else {
        DEFAULT_DOWNLOAD_BYTES_PER_REQ
    };
    let ul_bytes_per_req = if settings.seconds <= 1 {
        262_144
    } else {
        DEFAULT_UPLOAD_BYTES_PER_REQ
    };

    ctx.progress(Step::Cloudflare {
        stage: "download",
        ipv6,
    });
    let dl_spec = CloudflareTransferSpec {
        family,
        base_url: base_url.to_owned(),
        meas_id: meas_id.to_owned(),
        concurrency: settings.concurrency,
        duration: Duration::from_secs(u64::from(settings.seconds)),
        bytes_per_req: dl_bytes_per_req,
        probe_interval,
        probe_timeout: PROBE_TIMEOUT,
    };
    let dl_raw = ctx
        .cloudflare_download(&dl_spec)
        .await
        .map_err(map_net_err)?;
    if dl_raw.bytes == 0 {
        return Err(FamilyStop::Failed(FamilyIssue::EmptyDownload));
    }
    let (download, dl_steady) =
        parse::compute_throughput_stats(dl_raw.bytes, dl_raw.duration_ms, &dl_raw.ticks);
    let loaded_download_latency =
        parse::compute_latency_stats(dl_raw.loaded_sent, &dl_raw.loaded_rtts_micros);

    ctx.progress(Step::Cloudflare {
        stage: "upload",
        ipv6,
    });
    let ul_spec = CloudflareTransferSpec {
        bytes_per_req: ul_bytes_per_req,
        ..dl_spec
    };
    let ul_raw = ctx.cloudflare_upload(&ul_spec).await.map_err(map_net_err)?;
    if ul_raw.bytes == 0 {
        return Err(FamilyStop::Failed(FamilyIssue::EmptyUpload));
    }
    let (upload, ul_steady) =
        parse::compute_throughput_stats(ul_raw.bytes, ul_raw.duration_ms, &ul_raw.ticks);
    let loaded_upload_latency =
        parse::compute_latency_stats(ul_raw.loaded_sent, &ul_raw.loaded_rtts_micros);

    let (udp, udp_note) = if settings.udp {
        ctx.progress(Step::Cloudflare { stage: "udp", ipv6 });
        let stun_spec = CloudflareStunSpec {
            family,
            host: stun_host.to_owned(),
            port: stun_port,
            packets: settings.udp_packets,
            interval: if settings.seconds <= 1 {
                Duration::from_millis(15)
            } else {
                UDP_INTERVAL
            },
            timeout: UDP_TIMEOUT,
        };
        match ctx.cloudflare_udp_loss(&stun_spec).await {
            Ok(raw) => (Some(parse::compute_udp_stats(&raw)), None),
            Err(NetError::Cancelled) => return Err(FamilyStop::Cancelled),
            Err(err) => (None, net_err_issue(err)),
        }
    } else {
        (None, None)
    };

    let quality = parse::compute_quality(
        idle_latency.median_ms,
        loaded_download_latency.median_ms,
        loaded_upload_latency.median_ms,
        &dl_steady,
        &ul_steady,
    );

    let server = meta
        .colo
        .as_deref()
        .map(|colo| parse::map_colo_to_server(locations.as_deref(), colo));

    Ok(FamilyCloudflare {
        client_ip: meta.client_ip,
        colo: meta.colo,
        server,
        asn: meta.asn,
        as_org: meta.as_org,
        idle_latency,
        download,
        upload,
        loaded_download_latency,
        loaded_upload_latency,
        udp,
        udp_note,
        quality,
    })
}

async fn fetch_meta_and_locations(
    ctx: &Context<Output>,
    family: IpFamily,
    base_url: &str,
    meas_id: &str,
) -> Result<(ParsedMeta, Option<Vec<u8>>), FamilyStop> {
    let mut meta = ParsedMeta::default();

    let meta_req = HttpRequest::get(family, format!("{base_url}/meta?measId={meas_id}"))
        .with_header("Referer", REFERER)
        .with_timeout(META_TIMEOUT);
    match ctx.probe(&meta_req).await {
        Err(HttpError::Cancelled) => return Err(FamilyStop::Cancelled),
        Ok(resp) if resp.status == 200 => {
            meta = parse::parse_meta_json(&resp.body);
            meta.merge_from(&parse::parse_response_headers(&resp));
        }
        _ => {}
    }

    if meta.colo.is_none() {
        let trace_req = HttpRequest::get(family, format!("{base_url}/cdn-cgi/trace"))
            .with_header("Referer", REFERER)
            .with_timeout(META_TIMEOUT);
        match ctx.probe(&trace_req).await {
            Err(HttpError::Cancelled) => return Err(FamilyStop::Cancelled),
            Ok(resp) if resp.status == 200 => {
                meta.merge_from(&parse::parse_trace_text(&resp.text()));
                meta.merge_from(&parse::parse_response_headers(&resp));
            }
            _ => {}
        }
    }

    let loc_req = HttpRequest::get(family, format!("{base_url}/locations"))
        .with_header("Referer", REFERER)
        .with_timeout(META_TIMEOUT)
        .with_max_bytes(LOCATIONS_MAX_BYTES);
    let locations = match ctx.probe(&loc_req).await {
        Err(HttpError::Cancelled) => return Err(FamilyStop::Cancelled),
        Ok(resp) if resp.status == 200 => Some(resp.body),
        _ => None,
    };

    Ok((meta, locations))
}

async fn measure_idle_latency(
    ctx: &Context<Output>,
    family: IpFamily,
    base_url: &str,
    meas_id: &str,
    idle_ms: u16,
) -> Result<(u64, Vec<u64>, ParsedMeta), FamilyStop> {
    let total_window = Duration::from_millis(u64::from(idle_ms));
    let interval = if idle_ms <= 500 {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(PROBE_INTERVAL_MS)
    };
    let url = format!("{base_url}/__down?bytes=0&measId={meas_id}");
    let started = Instant::now();
    let mut sent = 0_u64;
    let mut rtts = Vec::new();
    let mut header_meta = ParsedMeta::default();
    let mut captured_meta = false;
    let cancel = ctx.cancel_token();

    while started.elapsed() < total_window || sent == 0 {
        if cancel.is_cancelled() {
            return Err(FamilyStop::Cancelled);
        }
        sent += 1;
        let req = HttpRequest::get(family, url.clone())
            .with_header("Referer", REFERER)
            .with_timeout(PROBE_TIMEOUT);
        let t0 = Instant::now();
        match ctx.probe(&req).await {
            Err(HttpError::Cancelled) => return Err(FamilyStop::Cancelled),
            Ok(resp) if (200..300).contains(&resp.status) => {
                let micros = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
                rtts.push(micros.max(1));
                if !captured_meta {
                    header_meta = parse::parse_response_headers(&resp);
                    captured_meta = true;
                }
            }
            Ok(_) | Err(_) => {}
        }
        if started.elapsed() >= total_window {
            break;
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(FamilyStop::Cancelled),
            () = tokio::time::sleep(interval) => {}
        }
    }
    Ok((sent, rtts, header_meta))
}

fn map_net_err(err: NetError) -> FamilyStop {
    match net_err_issue(err) {
        Some(issue) => FamilyStop::Failed(issue),
        None => FamilyStop::Cancelled,
    }
}

fn net_err_issue(err: NetError) -> Option<FamilyIssue> {
    match err {
        NetError::NoAddress => Some(FamilyIssue::NoAddress),
        NetError::TimedOut => Some(FamilyIssue::TimedOut),
        NetError::NoResolver => Some(FamilyIssue::NoResolver),
        NetError::Transport(why) => Some(FamilyIssue::Transport(why)),
        NetError::Cancelled => None,
    }
}
