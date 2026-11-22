//! Continent-level selection and bounded-concurrency probe orchestration across IPv4 and IPv6.

use bench_api::Context;
use bench_api::IpFamily;
use bench_api::Step;
use bench_api::Stop;

use super::platforms;
use super::platforms::Platform;
use super::result::Choice;
use super::result::Failure;
use super::result::Group;
use super::result::Output;
use super::result::ServiceResult;
use super::result::Settings;

/// Concurrent platform probes active at once within a continent group.
const CONCURRENCY: usize = 10;

/// One platform scheduled in a continent's run plan.
#[derive(Clone, Copy)]
pub struct Scheduled {
    /// Continent group that orchestrates this platform.
    pub group: Group,
    /// Platform definition and probe function.
    pub platform: Platform,
}

/// Selected platforms organized in continent order, plus any unknown requested ids.
pub struct Plan {
    /// Platforms to probe, ordered by [`Group::ALL`].
    pub scheduled: Vec<Scheduled>,
    /// Requested service ids that do not exist in any continent's roster.
    pub unknown: Vec<String>,
}

/// Builds the continent-ordered [`Plan`]: `services` wins over `groups` when non-empty; empty
/// `groups` and empty `services` select every continent's roster.
#[must_use]
pub fn plan(groups: &[Choice], services: &[String]) -> Plan {
    if !services.is_empty() {
        let mut unknown: Vec<String> = Vec::new();
        for wanted in services {
            let trimmed = wanted.trim();
            let exists = Group::ALL.into_iter().any(|group| {
                platforms::in_group(group)
                    .iter()
                    .any(|item| item.id.eq_ignore_ascii_case(trimmed))
            });
            let already = unknown
                .iter()
                .any(|seen| seen.eq_ignore_ascii_case(trimmed));
            if !exists && !already {
                unknown.push(trimmed.to_owned());
            }
        }
        let mut scheduled = Vec::new();
        for group in Group::ALL {
            for &platform in platforms::in_group(group) {
                let picked = services
                    .iter()
                    .any(|wanted| platform.id.eq_ignore_ascii_case(wanted.trim()));
                if picked {
                    scheduled.push(Scheduled { group, platform });
                }
            }
        }
        return Plan { scheduled, unknown };
    }

    let mut scheduled = Vec::new();
    for group in Group::ALL {
        if groups.is_empty() || groups.iter().any(|choice| choice.matches(group)) {
            for &platform in platforms::in_group(group) {
                scheduled.push(Scheduled { group, platform });
            }
        }
    }
    Plan {
        scheduled,
        unknown: Vec::new(),
    }
}

/// Orchestrates `plan` continent by continent over IPv4 and IPv6, publishing incremental results
/// after each continent finishes.
pub async fn measure(
    ctx: &Context<Output>,
    chosen: Plan,
    settings: Settings,
    base_url: Option<&str>,
) -> Result<Output, Stop<Failure>> {
    let network = &ctx.host().network;
    let ipv4_reachable = network.ipv4.reachable();
    let ipv6_reachable = network.ipv6.reachable();
    if !ipv4_reachable && !ipv6_reachable {
        return Err(Stop::Skipped(Failure::NoFamily));
    }

    let results: Vec<ServiceResult> = chosen
        .scheduled
        .iter()
        .map(|entry| ServiceResult {
            id: entry.platform.id,
            name: entry.platform.name,
            group: entry.group,
            ipv4: None,
            ipv6: None,
        })
        .collect();
    let mut output = Output {
        settings,
        unknown: chosen.unknown,
        ipv4_reachable,
        ipv6_reachable,
        results,
    };
    ctx.publish(&output);

    let families = [
        (IpFamily::Ipv4, ipv4_reachable, false),
        (IpFamily::Ipv6, ipv6_reachable, true),
    ];
    for (family, reachable, ipv6) in families {
        if !reachable {
            continue;
        }
        for group in Group::ALL {
            let indices: Vec<(usize, Platform)> = chosen
                .scheduled
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.group == group)
                .map(|(idx, entry)| (idx, entry.platform))
                .collect();
            if indices.is_empty() {
                continue;
            }
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            ctx.progress(Step::Media {
                group: group.code(),
                ipv6,
            });
            let tasks = indices.iter().copied().map(|(idx, platform)| async move {
                (idx, (platform.probe)(ctx, family, base_url).await)
            });
            let cancel = ctx.cancel_token();
            let verdicts = bench_api::run_bounded(CONCURRENCY, &cancel, tasks).await;
            if cancel.is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            for (idx, verdict) in verdicts {
                if ipv6 {
                    output.results[idx].ipv6 = Some(verdict);
                } else {
                    output.results[idx].ipv4 = Some(verdict);
                }
            }
            ctx.publish(&output);
        }
    }

    Ok(output)
}
