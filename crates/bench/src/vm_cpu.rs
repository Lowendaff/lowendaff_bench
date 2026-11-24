//! Periodic `/proc/stat` `%si` (`softirq`) and `%st` (hypervisor steal) sampling for virtual
//! machines, running every 5 seconds from benchmark start to finish.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::Step;
use bench_api::ToolRun;
use bench_host::Root;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::system::CpuTimes;
use tokio::task::JoinHandle;
use tokio::time::Instant as TokioInstant;
use tokio::time::MissedTickBehavior;

use crate::orchestrator::Observer;
use crate::orchestrator::Run;

/// Default sampling cadence during a benchmark run on a virtual machine.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

/// Minimum elapsed time required to record a trailing `< 5 s` run-wide sample when a short VM
/// benchmark finishes before the first 5-second tick.
const MIN_SHORT_RUN_ELAPSED: Duration = Duration::from_millis(50);

/// One `/proc/stat` delta sample recorded during a virtual-machine benchmark run.
#[derive(Debug, Clone, PartialEq)]
pub struct VmCpuSample {
    /// Wall time elapsed since the benchmark run started.
    pub elapsed: Duration,
    /// Plugin ID running when the sample ended (`None` during initial setup/provisioning).
    pub item: Option<&'static str>,
    /// Percentage of total CPU ticks spent servicing software interrupts (`%si`).
    pub si_pct: f64,
    /// Percentage of total CPU ticks stolen by the host hypervisor (`%st`).
    pub st_pct: f64,
}

/// Summary statistics for either `%si` or `%st` across all recorded samples.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricStats {
    /// Arithmetic mean percentage across all samples.
    pub avg: f64,
    /// 95th-percentile percentage across all samples.
    pub p95: f64,
    /// Peak percentage observed across all samples.
    pub max: f64,
    /// Elapsed wall time at which [`Self::max`] was observed.
    pub peak_elapsed: Duration,
    /// Plugin ID active when [`Self::max`] was observed.
    pub peak_item: Option<&'static str>,
}

/// Aggregated `%si` and `%st` statistics across a virtual-machine benchmark run.
#[derive(Debug, Clone, PartialEq)]
pub struct VmCpuSummary {
    /// Software interrupt (`%si`) statistics.
    pub si: MetricStats,
    /// Hypervisor steal (`%st`) statistics.
    pub st: MetricStats,
}

/// Computes summary statistics across `samples`; returns `None` when `samples` is empty.
#[must_use]
pub fn summarize(samples: &[VmCpuSample]) -> Option<VmCpuSummary> {
    if samples.is_empty() {
        return None;
    }
    Some(VmCpuSummary {
        si: compute_stats(samples, |s| s.si_pct),
        st: compute_stats(samples, |s| s.st_pct),
    })
}

fn compute_stats(samples: &[VmCpuSample], pick: fn(&VmCpuSample) -> f64) -> MetricStats {
    let mut sum = 0.0;
    let mut values = Vec::with_capacity(samples.len());
    let mut peak_val = -1.0_f64;
    let mut peak_elapsed = samples[0].elapsed;
    let mut peak_item = samples[0].item;

    for sample in samples {
        let val = pick(sample);
        sum += val;
        values.push(val);
        if val > peak_val {
            peak_val = val;
            peak_elapsed = sample.elapsed;
            peak_item = sample.item;
        }
    }
    values.sort_by(f64::total_cmp);
    let len = values.len();
    // Nearest-rank 95th percentile: ceil(0.95 * N) - 1.
    let p95_idx = (len.saturating_mul(95).div_ceil(100)).saturating_sub(1);
    let p95 = values[p95_idx.min(len - 1)];
    let avg = sum / (len as f64);

    MetricStats {
        avg,
        p95,
        max: peak_val.max(0.0),
        peak_elapsed,
        peak_item,
    }
}

/// Formats an elapsed duration as `MM:SS` (or `H:MM:SS` when `>= 1` hour).
#[must_use]
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let rem = secs % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{rem:02}")
    } else {
        format!("{mins:02}:{rem:02}")
    }
}

struct ActiveSampler {
    active_item: Arc<Mutex<Option<&'static str>>>,
    stop: CancellationToken,
    handle: JoinHandle<Vec<VmCpuSample>>,
}

/// Background `/proc/stat` sampler active only when `platform.kind() == PlatformKind::Vm`.
pub struct VmCpuSampler {
    inner: Option<ActiveSampler>,
}

impl VmCpuSampler {
    /// Starts sampling every 5 seconds when `platform` is a virtual machine and `/proc/stat` is
    /// readable under `root`; returns a no-op sampler on dedicated servers and containers.
    #[must_use]
    pub fn start_for_host(platform: Platform, root: Root, origin: Instant) -> Self {
        Self::start_with_interval(platform, root, origin, SAMPLE_INTERVAL)
    }

    /// Like [`Self::start_for_host`], with a configurable `interval` (for deterministic tests).
    #[must_use]
    pub fn start_with_interval(
        platform: Platform,
        root: Root,
        origin: Instant,
        interval: Duration,
    ) -> Self {
        if platform.kind() != PlatformKind::Vm {
            return Self { inner: None };
        }
        let Some(initial) = CpuTimes::read(&root) else {
            return Self { inner: None };
        };
        let active_item: Arc<Mutex<Option<&'static str>>> = Arc::new(Mutex::new(None));
        let item_for_task = Arc::clone(&active_item);
        let stop = CancellationToken::new();
        let stop_for_task = stop.clone();

        let handle = tokio::spawn(async move {
            let mut prev = initial;
            let mut samples = Vec::new();
            let mut ticker = tokio::time::interval_at(TokioInstant::now() + interval, interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    biased;
                    () = stop_for_task.cancelled() => break,
                    _ = ticker.tick() => {
                        let Some(curr) = CpuTimes::read(&root) else {
                            continue;
                        };
                        if let Some((si_pct, st_pct)) = curr.delta_si_st(&prev) {
                            let item = item_for_task.lock().ok().and_then(|guard| *guard);
                            samples.push(VmCpuSample {
                                elapsed: origin.elapsed(),
                                item,
                                si_pct,
                                st_pct,
                            });
                        }
                        prev = curr;
                    }
                }
            }

            if samples.is_empty()
                && origin.elapsed() >= MIN_SHORT_RUN_ELAPSED
                && let Some(curr) = CpuTimes::read(&root)
                && let Some((si_pct, st_pct)) = curr.delta_si_st(&prev)
            {
                let item = item_for_task.lock().ok().and_then(|guard| *guard);
                samples.push(VmCpuSample {
                    elapsed: origin.elapsed(),
                    item,
                    si_pct,
                    st_pct,
                });
            }
            samples
        });

        Self {
            inner: Some(ActiveSampler {
                active_item,
                stop,
                handle,
            }),
        }
    }

    /// Updates the currently executing plugin ID attributed to subsequent samples.
    pub fn set_active_item(&self, item: Option<&'static str>) {
        if let Some(active) = &self.inner
            && let Ok(mut guard) = active.active_item.lock()
        {
            *guard = item;
        }
    }

    /// Stops the background sampler and returns all recorded samples in chronological order.
    pub async fn finish(self) -> Vec<VmCpuSample> {
        let Some(active) = self.inner else {
            return Vec::new();
        };
        active.stop.cancel();
        active.handle.await.unwrap_or_default()
    }
}

/// [`Observer`] adapter that updates [`VmCpuSampler`] as each plugin starts while forwarding all
/// observer callbacks unchanged.
pub struct SamplingObserver<'a, R> {
    inner: &'a mut dyn Observer<R>,
    sampler: &'a VmCpuSampler,
}

impl<'a, R> SamplingObserver<'a, R> {
    /// Wraps `inner` to track the active plugin ID in `sampler`.
    pub fn new(inner: &'a mut dyn Observer<R>, sampler: &'a VmCpuSampler) -> Self {
        Self { inner, sampler }
    }
}

impl<R> Observer<R> for SamplingObserver<'_, R> {
    fn on_start(&mut self, id: &'static str) {
        self.sampler.set_active_item(Some(id));
        self.inner.on_start(id);
    }

    fn on_progress(&mut self, id: &'static str, step: &Step) {
        self.inner.on_progress(id, step);
    }

    fn on_tool(&mut self, id: &'static str, run: &ToolRun) {
        self.inner.on_tool(id, run);
    }

    fn on_finish(&mut self, run: &Run<R>) {
        self.inner.on_finish(run);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::VmCpuSample;
    use super::format_elapsed;
    use super::summarize;

    #[test]
    fn summarize_computes_avg_p95_and_peak_item() {
        let samples = vec![
            VmCpuSample {
                elapsed: Duration::from_secs(5),
                item: Some("disk.fio"),
                si_pct: 0.4,
                st_pct: 0.2,
            },
            VmCpuSample {
                elapsed: Duration::from_secs(10),
                item: Some("network.iperf3"),
                si_pct: 2.6,
                st_pct: 0.8,
            },
            VmCpuSample {
                elapsed: Duration::from_secs(15),
                item: Some("cpu.bench"),
                si_pct: 0.6,
                st_pct: 5.0,
            },
        ];
        let summary = summarize(&samples).expect("non-empty summary");
        assert!((summary.si.avg - 1.2).abs() < 1e-9);
        assert!((summary.si.max - 2.6).abs() < 1e-9);
        assert_eq!(summary.si.peak_item, Some("network.iperf3"));
        assert!((summary.st.avg - 2.0).abs() < 1e-9);
        assert!((summary.st.max - 5.0).abs() < 1e-9);
        assert_eq!(summary.st.peak_item, Some("cpu.bench"));
        assert_eq!(format_elapsed(Duration::from_secs(65)), "01:05");
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "1:01:01");
    }
}
