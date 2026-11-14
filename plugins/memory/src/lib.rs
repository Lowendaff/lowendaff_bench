//! The memory test items, the chapters of 内存性能: sysbench's read and write bandwidth
//! (`memory.sysbench`), STREAM (`memory.stream`), the latency of each cache level and of memory
//! (`memory.latency`), and on machines with several NUMA nodes the bandwidth and latency between
//! them (`memory.numa`). They share the working-set plan, the way a run is tried again, and
//! multichase, which the last two run.

pub mod chase;
pub mod latency;
pub mod numa;
pub mod plan;
pub mod stream;
pub mod sysbench;

use std::future::Future;
use std::time::Duration;

use bench_api::typed::Context;
use bench_api::typed::Measurement;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::retried::Retried;
use serde::Serialize;

/// Attempts per run: the first one and 3 retries.
pub const ATTEMPTS: usize = 4;

/// A number as the tool printed it, and its value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Value {
    /// The number as printed.
    pub text: String,
    /// The same number.
    pub value: f64,
}

impl Value {
    /// `text` as a finite, positive number; `None` when it is not one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let value: f64 = text.parse().ok()?;
        (value.is_finite() && value > 0.0).then(|| Self {
            text: text.to_owned(),
            value,
        })
    }
}

/// Why an attempt gave nothing to read, and whether another attempt may.
pub trait Retry {
    /// Whether to try again after this.
    fn again(&self) -> bool;
}

/// How a run, tried up to [`ATTEMPTS`] times, ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Tried<T, F> {
    /// An attempt gave a reading: it, and the faults of the attempts before it.
    Done(T, Vec<F>),
    /// The last attempt failed, the last of [`ATTEMPTS`] or one not worth another try: every
    /// attempt's fault, oldest first.
    Failed(Vec<F>),
    /// The plugin was cancelled.
    Cancelled,
}

/// Why an attempt gave no reading.
enum Attempt<F> {
    Failed(F),
    Cancelled,
}

/// Whether to try again after `attempt`: only after a fault that is worth it.
fn again<F: Retry>(attempt: &Attempt<F>) -> bool {
    matches!(attempt, Attempt::Failed(fault) if fault.again())
}

/// Runs `job` until `read` takes a reading from an attempt, up to [`ATTEMPTS`] times, with no
/// wait between attempts. `run` runs one attempt: the shared runner, or a recording in tests.
pub async fn tried<O, T, F, R, Fut>(
    ctx: &Context<O>,
    run: &R,
    job: &Job,
    read: fn(&job::Output) -> Result<T, F>,
) -> Tried<T, F>
where
    O: Measurement,
    F: Retry + From<job::Fault>,
    R: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let once = || async {
        match run(job.clone()).await {
            Ok(output) => read(&output).map_err(Attempt::Failed),
            Err(RunError::Fault(fault)) => Err(Attempt::Failed(F::from(fault))),
            Err(RunError::Cancelled) => Err(Attempt::Cancelled),
        }
    };
    let cancel = ctx.cancel_token();
    let retried = Retried::run(ATTEMPTS, Duration::ZERO, &cancel, again::<F>, once).await;
    // Only failures are tried again, so the earlier attempts all failed.
    let mut faults: Vec<F> = retried
        .failures
        .into_iter()
        .filter_map(|attempt| match attempt {
            Attempt::Failed(fault) => Some(fault),
            Attempt::Cancelled => None,
        })
        .collect();
    match retried.last {
        Ok(reading) => Tried::Done(reading, faults),
        Err(Attempt::Failed(fault)) => {
            faults.push(fault);
            Tried::Failed(faults)
        }
        Err(Attempt::Cancelled) => Tried::Cancelled,
    }
}
