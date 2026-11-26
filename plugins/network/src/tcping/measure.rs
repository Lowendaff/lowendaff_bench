//! Concurrent dual-stack TCP handshake probing with 800 ms primary->backup failover and
//! distribution statistics (`min`, `avg`, `median`, `p95`, `max`, `jitter`, `loss_pct`).

use std::net::SocketAddr;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::IpFamily;
use bench_api::NetError;
use bench_api::Step;
use bench_api::Stop;
use bench_api::TcpPingOutcome;

use super::result::Failure;
use super::result::Group;
use super::result::LatencyMeasured;
use super::result::Output;
use super::result::Settings;
use super::result::TargetOutcome;
use super::result::TestedTarget;
use super::targets::Endpoint;
use super::targets::Target;

/// First-probe liveness deadline on `candidates[0]` before failing over to `candidates[1]`.
pub const PRECHECK_TIMEOUT: Duration = Duration::from_millis(800);

/// Per-probe TCP handshake deadline during normal measurement.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1_000);

/// DNS lookup deadline per candidate hostname.
pub const RESOLVE_TIMEOUT: Duration = Duration::from_millis(2_500);

/// Pacing delay between consecutive probes to the same endpoint.
pub const PROBE_INTERVAL: Duration = Duration::from_millis(150);

/// Maximum targets probed concurrently within a `(family, group)` batch.
pub const CONCURRENCY: usize = 12;

/// Runs `network.tcping` across all reachable IP families and selected target groups.
///
/// # Errors
///
/// Returns [`Stop::Skipped`] when no targets match reachable IP families, or [`Stop::Failed`] when
/// interrupted or when every probed target fails without a single TCP handshake response.
pub async fn run(
    ctx: &Context<Output>,
    selected: &[Target],
    settings: Settings,
    allow_local: bool,
) -> Result<Output, Stop<Failure>> {
    let mut families = Vec::with_capacity(2);
    if ctx.host().network.ipv4.reachable() {
        families.push(IpFamily::Ipv4);
    }
    if ctx.host().network.ipv6.reachable() {
        families.push(IpFamily::Ipv6);
    }

    let has_any_pair = families.iter().any(|&fam| {
        selected
            .iter()
            .any(|target| !target.endpoints(fam).is_empty())
    });
    if !has_any_pair {
        return Err(Stop::Skipped(Failure::NoTargets));
    }

    let cancel = ctx.cancel_token();
    let count = settings.count;
    let mut output = Output {
        settings,
        targets: Vec::new(),
    };

    for family in families {
        let ipv6 = family == IpFamily::Ipv6;
        for group in [Group::China, Group::ChinaPremium, Group::Global] {
            if cancel.is_cancelled() {
                return Err(Stop::Failed(Failure::Interrupted));
            }
            let batch: Vec<&Target> = selected
                .iter()
                .filter(|t| t.group == group && !t.endpoints(family).is_empty())
                .collect();
            if batch.is_empty() {
                continue;
            }
            ctx.progress(Step::Tcping {
                group: group.as_code(),
                targets: batch.len(),
                ipv6,
            });
            let tasks: Vec<_> = batch
                .iter()
                .map(|target| probe_target(ctx, target, family, count, allow_local, &cancel))
                .collect();
            let results = bench_api::run_bounded(CONCURRENCY, &cancel, tasks).await;
            if cancel.is_cancelled() {
                return Err(Stop::Failed(Failure::Interrupted));
            }
            output.targets.extend(results);
            ctx.publish(&output);
        }
    }

    let any_received = output
        .targets
        .iter()
        .any(|t| matches!(&t.outcome, TargetOutcome::Measured(m) if m.received > 0));
    if !any_received {
        return Err(Stop::Failed(Failure::AllUnreachable));
    }
    Ok(output)
}

async fn probe_target(
    ctx: &Context<Output>,
    target: &Target,
    family: IpFamily,
    count: u8,
    allow_local: bool,
    cancel: &CancellationToken,
) -> TestedTarget {
    let outcome = probe_candidates(
        ctx,
        target.endpoints(family),
        family,
        count,
        allow_local,
        cancel,
    )
    .await;
    TestedTarget {
        id: target.id.clone(),
        group: target.group,
        region: target.region.clone(),
        city: target.city.clone(),
        name: target.name.clone(),
        operator: target.operator,
        asn: target.asn,
        ipv6: family == IpFamily::Ipv6,
        outcome,
    }
}

async fn probe_candidates(
    ctx: &Context<Output>,
    candidates: &[Endpoint],
    family: IpFamily,
    count: u8,
    allow_local: bool,
    cancel: &CancellationToken,
) -> TargetOutcome {
    let Some(primary) = candidates.first() else {
        return TargetOutcome::Failed {
            reason: "no endpoint configured".to_owned(),
        };
    };
    let backup = candidates.get(1);

    match ctx
        .resolve_endpoint(
            family,
            &primary.host,
            primary.port,
            allow_local,
            RESOLVE_TIMEOUT,
        )
        .await
    {
        Ok(addr0) => match ctx.tcp_ping(addr0, allow_local, PRECHECK_TIMEOUT).await {
            Ok(TcpPingOutcome::Connected { micros, .. }) => {
                let mut samples = Vec::with_capacity(usize::from(count));
                samples.push(micros);
                let mut sent: u8 = 1;
                for _ in 1..count {
                    if !sleep_or_cancel(PROBE_INTERVAL, cancel).await {
                        break;
                    }
                    match ctx.tcp_ping(addr0, allow_local, PROBE_TIMEOUT).await {
                        Ok(TcpPingOutcome::Connected { micros: rtt, .. }) => {
                            sent = sent.saturating_add(1);
                            samples.push(rtt);
                        }
                        Err(NetError::Cancelled) => break,
                        Ok(TcpPingOutcome::TimedOut | TcpPingOutcome::Unreachable(_)) | Err(_) => {
                            sent = sent.saturating_add(1);
                        }
                    }
                }
                summarize(primary, addr0, false, sent, &samples)
            }
            Ok(TcpPingOutcome::TimedOut) => {
                if let Some(alt) = backup {
                    probe_backup(ctx, alt, family, count, allow_local, cancel).await
                } else {
                    let mut samples = Vec::new();
                    let mut sent: u8 = 1;
                    for _ in 1..count {
                        if !sleep_or_cancel(PROBE_INTERVAL, cancel).await {
                            break;
                        }
                        match ctx.tcp_ping(addr0, allow_local, PROBE_TIMEOUT).await {
                            Ok(TcpPingOutcome::Connected { micros: rtt, .. }) => {
                                sent = sent.saturating_add(1);
                                samples.push(rtt);
                            }
                            Err(NetError::Cancelled) => break,
                            Ok(TcpPingOutcome::TimedOut | TcpPingOutcome::Unreachable(_))
                            | Err(_) => {
                                sent = sent.saturating_add(1);
                            }
                        }
                    }
                    summarize(primary, addr0, false, sent, &samples)
                }
            }
            Ok(TcpPingOutcome::Unreachable(why)) => {
                if let Some(alt) = backup {
                    probe_backup(ctx, alt, family, count, allow_local, cancel).await
                } else {
                    TargetOutcome::Failed { reason: why }
                }
            }
            Err(err) => {
                if let Some(alt) = backup
                    && err != NetError::Cancelled
                {
                    probe_backup(ctx, alt, family, count, allow_local, cancel).await
                } else {
                    TargetOutcome::Failed {
                        reason: net_err_text(&err),
                    }
                }
            }
        },
        Err(err) => {
            if let Some(alt) = backup
                && err != NetError::Cancelled
            {
                probe_backup(ctx, alt, family, count, allow_local, cancel).await
            } else {
                TargetOutcome::Failed {
                    reason: net_err_text(&err),
                }
            }
        }
    }
}

async fn probe_backup(
    ctx: &Context<Output>,
    backup: &Endpoint,
    family: IpFamily,
    count: u8,
    allow_local: bool,
    cancel: &CancellationToken,
) -> TargetOutcome {
    let addr = match ctx
        .resolve_endpoint(
            family,
            &backup.host,
            backup.port,
            allow_local,
            RESOLVE_TIMEOUT,
        )
        .await
    {
        Ok(addr) => addr,
        Err(err) => {
            return TargetOutcome::Failed {
                reason: net_err_text(&err),
            };
        }
    };
    let mut samples = Vec::with_capacity(usize::from(count));
    let mut sent: u8 = 0;
    for idx in 0..count {
        if idx > 0 && !sleep_or_cancel(PROBE_INTERVAL, cancel).await {
            break;
        }
        match ctx.tcp_ping(addr, allow_local, PROBE_TIMEOUT).await {
            Ok(TcpPingOutcome::Connected { micros, .. }) => {
                sent = sent.saturating_add(1);
                samples.push(micros);
            }
            Ok(TcpPingOutcome::Unreachable(why)) => {
                if idx == 0 {
                    return TargetOutcome::Failed { reason: why };
                }
                sent = sent.saturating_add(1);
            }
            Err(NetError::Cancelled) => break,
            Ok(TcpPingOutcome::TimedOut) | Err(_) => {
                sent = sent.saturating_add(1);
            }
        }
    }
    summarize(backup, addr, true, sent.max(1), &samples)
}

async fn sleep_or_cancel(delay: Duration, cancel: &CancellationToken) -> bool {
    if cancel.is_cancelled() {
        return false;
    }
    tokio::select! {
        biased;
        () = cancel.cancelled() => false,
        () = tokio::time::sleep(delay) => true,
    }
}

fn net_err_text(err: &NetError) -> String {
    match err {
        NetError::NoAddress => "no address for IP family".to_owned(),
        NetError::TimedOut => "DNS lookup timed out".to_owned(),
        NetError::NoResolver => "no DNS resolver".to_owned(),
        NetError::Transport(why) => why.clone(),
        NetError::Cancelled => "interrupted".to_owned(),
    }
}

/// Computes `min`, `avg`, `median`, `p95`, `max`, `jitter`, and `loss_pct` from `samples_micros`.
#[must_use]
pub fn summarize(
    endpoint: &Endpoint,
    addr: SocketAddr,
    used_backup: bool,
    sent: u8,
    samples_micros: &[u64],
) -> TargetOutcome {
    let effective_sent = sent.max(1);
    let received = u8::try_from(samples_micros.len())
        .unwrap_or(u8::MAX)
        .min(effective_sent);
    let lost = effective_sent.saturating_sub(received);
    let loss_pct = round1((f64::from(lost) * 100.0) / f64::from(effective_sent));

    if samples_micros.is_empty() {
        return TargetOutcome::Loss(LatencyMeasured {
            endpoint: endpoint.host.clone(),
            port: endpoint.port,
            resolved_ip: addr.ip().to_string(),
            used_backup,
            sent: effective_sent,
            received: 0,
            loss_pct: 100.0,
            min_ms: None,
            avg_ms: None,
            median_ms: None,
            p95_ms: None,
            max_ms: None,
            jitter_ms: None,
        });
    }

    let jitter_ms = if samples_micros.len() < 2 {
        0.0
    } else {
        let mut diff_sum: u128 = 0;
        for pair in samples_micros.windows(2) {
            diff_sum += u128::from(pair[0].abs_diff(pair[1]));
        }
        let pairs = (samples_micros.len() - 1) as f64;
        round2((diff_sum as f64) / (pairs * 1_000.0))
    };

    let mut sorted = samples_micros.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    let min_ms = round2(sorted[0] as f64 / 1_000.0);
    let max_ms = round2(sorted[len - 1] as f64 / 1_000.0);
    let sum_micros: u128 = sorted.iter().map(|&v| u128::from(v)).sum();
    let avg_ms = round2((sum_micros as f64) / ((len as f64) * 1_000.0));
    let median_ms = if len % 2 == 1 {
        round2(sorted[len / 2] as f64 / 1_000.0)
    } else {
        let mid = (u128::from(sorted[len / 2 - 1]) + u128::from(sorted[len / 2])) as f64 / 2.0;
        round2(mid / 1_000.0)
    };
    let p95_idx = (len * 95).div_ceil(100).saturating_sub(1);
    let p95_ms = round2(sorted[p95_idx] as f64 / 1_000.0);

    TargetOutcome::Measured(LatencyMeasured {
        endpoint: endpoint.host.clone(),
        port: endpoint.port,
        resolved_ip: addr.ip().to_string(),
        used_backup,
        sent: effective_sent,
        received,
        loss_pct,
        min_ms: Some(min_ms),
        avg_ms: Some(avg_ms),
        median_ms: Some(median_ms),
        p95_ms: Some(p95_ms),
        max_ms: Some(max_ms),
        jitter_ms: Some(jitter_ms),
    })
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
