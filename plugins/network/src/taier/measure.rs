//! Measurement loop for `network.taier`: acquires a queue lease (`dovalid`) with automatic
//! busy/unreachable candidate fallback inside each `(location, operator)` slot, measures multi-
//! stream download and upload throughput plus optional `64 B` / `1400 B` packet-loss probes,
//! releases the lease, and publishes partial results after each slot.

use std::net::Ipv4Addr;
use std::time::Duration;

use bench_api::Context;
use bench_api::NetError;
use bench_api::Step;
use bench_api::Stop;
use bench_api::TaierAdmission;
use bench_api::TaierStreamSpec;

use super::nodes::Chosen;
use super::nodes::TargetSlot;
use super::parse;
use super::result::Failure;
use super::result::NodeMeasured;
use super::result::NodeOutcome;
use super::result::Output;
use super::result::Settings;
use super::result::TestedNode;

const ACQUIRE_TIMEOUT: Duration = Duration::from_millis(3_500);
const FALLBACK_ACQUIRE_TIMEOUT: Duration = Duration::from_millis(2_500);
const RELEASE_TIMEOUT: Duration = Duration::from_secs(3);
const PACKET_SAMPLES: u8 = 8;
const MAX_CANDIDATES_PER_SLOT: usize = 3;

/// Runs `chosen` slots sequentially, publishing partial results after every finished slot.
pub async fn measure(
    ctx: &Context<Output>,
    chosen: Chosen,
    settings: Settings,
    mut seed: u64,
    now_secs: impl Fn() -> u64,
) -> Result<Output, Stop<Failure>> {
    let mut output = Output {
        settings: settings.clone(),
        unknown: chosen.unknown,
        results: Vec::with_capacity(chosen.slots.len()),
    };
    ctx.publish(&output);

    for slot in chosen.slots {
        if ctx.cancel_token().is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let imei = parse::imei_from_seed(seed);
        let tested = run_slot(ctx, &slot, &settings, &imei, &now_secs).await?;
        output.results.push(tested);
        ctx.publish(&output);
    }

    let any_measured = output
        .results
        .iter()
        .any(|row| matches!(row.outcome, NodeOutcome::Measured(_)));
    if !any_measured {
        return Err(Stop::Failed(Failure::NoResult));
    }
    Ok(output)
}

async fn run_slot(
    ctx: &Context<Output>,
    slot: &TargetSlot,
    settings: &Settings,
    imei: &str,
    now_secs: &impl Fn() -> u64,
) -> Result<TestedNode, Stop<Failure>> {
    let place = if slot.city.eq_ignore_ascii_case(&slot.province) {
        slot.city.clone()
    } else {
        format!("{}, {}", slot.city, slot.province)
    };
    let leaked_place: &'static str = Box::leak(place.into_boxed_str());
    let op_code = slot.operator.code();

    let candidates = &slot.candidates[..slot.candidates.len().min(MAX_CANDIDATES_PER_SLOT)];
    let mut last_ip = candidates[0].0;
    let mut last_port = candidates[0].1;
    let mut last_outcome = NodeOutcome::Unreachable;
    let mut tried: u8 = 0;

    for &(ip, port) in candidates {
        tried = tried.saturating_add(1);
        last_ip = ip;
        last_port = port;

        let ts = now_secs();
        let token = parse::make_token(imei, ts, parse::DEFAULT_BANDWIDTH_MBPS);
        let query = parse::dovalid_query(imei, ts, parse::DEFAULT_BANDWIDTH_MBPS, &token);
        let first = ctx.taier_acquire(ip, port, &query, ACQUIRE_TIMEOUT).await;
        let admission = match first {
            Err(NetError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            Ok(TaierAdmission::AuthRejected) | Err(NetError::TimedOut) => {
                let empty_q = parse::dovalid_query(imei, ts, parse::DEFAULT_BANDWIDTH_MBPS, "");
                ctx.taier_acquire(ip, port, &empty_q, FALLBACK_ACQUIRE_TIMEOUT)
                    .await
            }
            other => other,
        };

        let key = match admission {
            Err(NetError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            Ok(TaierAdmission::Admitted(uuid)) => uuid,
            Ok(TaierAdmission::Busy) => {
                last_outcome = NodeOutcome::Busy;
                continue;
            }
            Ok(TaierAdmission::AuthRejected | TaierAdmission::InvalidParams) => {
                last_outcome = NodeOutcome::AuthRejected;
                continue;
            }
            Ok(TaierAdmission::Unexpected(text)) => {
                last_outcome = NodeOutcome::Failed(format!("dovalid: {text}"));
                continue;
            }
            Err(NetError::TimedOut) => {
                last_outcome = NodeOutcome::Unreachable;
                continue;
            }
            Err(NetError::NoAddress | NetError::NoResolver | NetError::Transport(_)) => {
                last_outcome = NodeOutcome::Unreachable;
                continue;
            }
        };

        let measured = measure_admitted(ctx, ip, port, &key, leaked_place, op_code, settings).await;
        ctx.taier_release(ip, port, &key, RELEASE_TIMEOUT).await;

        match measured {
            Ok(done) => {
                return Ok(TestedNode {
                    id: slot.id.clone(),
                    province: slot.province.clone(),
                    city: slot.city.clone(),
                    operator: slot.operator,
                    ip,
                    port,
                    tried_nodes: tried,
                    outcome: NodeOutcome::Measured(done),
                });
            }
            Err(SlotStop::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
            Err(SlotStop::TimedOut) => last_outcome = NodeOutcome::TimedOut,
            Err(SlotStop::Failed(why)) => last_outcome = NodeOutcome::Failed(why),
        }
    }

    Ok(TestedNode {
        id: slot.id.clone(),
        province: slot.province.clone(),
        city: slot.city.clone(),
        operator: slot.operator,
        ip: last_ip,
        port: last_port,
        tried_nodes: tried,
        outcome: last_outcome,
    })
}

enum SlotStop {
    Cancelled,
    TimedOut,
    Failed(String),
}

impl From<NetError> for SlotStop {
    fn from(err: NetError) -> Self {
        match err {
            NetError::Cancelled => Self::Cancelled,
            NetError::TimedOut => Self::TimedOut,
            NetError::NoAddress => Self::Failed("no IPv4 address".to_owned()),
            NetError::NoResolver => Self::Failed("no DNS resolver".to_owned()),
            NetError::Transport(why) => Self::Failed(why),
        }
    }
}

async fn measure_admitted(
    ctx: &Context<Output>,
    ip: Ipv4Addr,
    port: u16,
    key: &str,
    location: &'static str,
    operator: &'static str,
    settings: &Settings,
) -> Result<NodeMeasured, SlotStop> {
    let spec = TaierStreamSpec {
        ip,
        port,
        key: key.to_owned(),
        duration: Duration::from_secs(u64::from(settings.seconds)),
        streams: settings.streams,
    };

    ctx.progress(Step::Taier {
        location,
        operator,
        stage: "download",
    });
    let down = ctx.taier_download(&spec).await?;

    ctx.progress(Step::Taier {
        location,
        operator,
        stage: "upload",
    });
    let up = ctx.taier_upload(&spec).await?;

    let (small_probe, large_probe) = if settings.probes {
        ctx.progress(Step::Taier {
            location,
            operator,
            stage: "probes",
        });
        let small = match ctx
            .taier_packet_probe(ip, port, false, PACKET_SAMPLES)
            .await
        {
            Ok(stats) => Some(stats),
            Err(NetError::Cancelled) => return Err(SlotStop::Cancelled),
            Err(_) => None,
        };
        let large = match ctx.taier_packet_probe(ip, port, true, PACKET_SAMPLES).await {
            Ok(stats) => Some(stats),
            Err(NetError::Cancelled) => return Err(SlotStop::Cancelled),
            Err(_) => None,
        };
        (small, large)
    } else {
        (None, None)
    };

    Ok(NodeMeasured {
        download_bps: down.bits_per_sec,
        upload_bps: up.bits_per_sec,
        small_probe,
        large_probe,
    })
}
