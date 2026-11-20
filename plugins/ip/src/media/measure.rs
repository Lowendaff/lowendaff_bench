//! Continent-level selection and bounded-concurrency probe orchestration across IPv4 and IPv6.

use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

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
        let mut unknown = Vec::new();
        for wanted in services {
            let trimmed = wanted.trim();
            let exists = Group::ALL.into_iter().any(|group| {
                platforms::in_group(group)
                    .iter()
                    .any(|item| item.id.eq_ignore_ascii_case(trimmed))
            });
            let already = unknown
                .iter()
                .any(|seen: &String| seen.eq_ignore_ascii_case(trimmed));
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
            let verdicts = run_bounded(CONCURRENCY, tasks).await;
            if ctx.cancel_token().is_cancelled() {
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

type BoxedTask<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Executes `tasks` with at most `limit` in flight at once, returning results in input order.
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use super::run_bounded;

    #[tokio::test]
    async fn run_bounded_keeps_input_order_without_exceeding_the_limit() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let tasks = (0..25).map(|index| {
            let in_flight = Arc::clone(&in_flight);
            let peak = Arc::clone(&peak);
            async move {
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::task::yield_now().await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
                index
            }
        });
        let order = run_bounded(4, tasks).await;
        assert_eq!(order, (0..25).collect::<Vec<_>>());
        assert_eq!(peak.load(Ordering::SeqCst), 4);
    }
}
