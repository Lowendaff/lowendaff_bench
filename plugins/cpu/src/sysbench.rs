//! sysbench's CPU test: its command, its result and retries (design cpu, §5.2, §5.4).

use std::ffi::OsString;
use std::future::Future;

/// Seconds each run lasts, as in the owner's `linux_bench` (CP-04).
pub const SECONDS: u32 = 20;

/// The largest number tested for primality, as in `linux_bench` (CP-04).
pub const MAX_PRIME: u32 = 10_000;

/// Attempts per run: the first one and 3 retries (CP-14).
pub const ATTEMPTS: usize = 4;

/// Events per second as sysbench printed them, and their value.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    /// The number as printed, with two decimals.
    pub text: String,
    /// The same number.
    pub value: f64,
}

/// Why an attempt ended without a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptError {
    /// The attempt failed for this reason; it may be retried.
    Failed(String),
    /// The run was cancelled; nothing is retried.
    Cancelled,
}

/// A run that produced a rate, possibly after failed attempts.
#[derive(Debug, Clone, PartialEq)]
pub struct Success {
    /// The rate of the attempt that succeeded.
    pub rate: Rate,
    /// The reasons of the attempts that failed before it, oldest first.
    pub failures: Vec<String>,
}

/// A run that produced no rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// Every attempt failed; the reasons, oldest first.
    Failed(Vec<String>),
    /// The run was cancelled.
    Cancelled,
}

/// The arguments of one run with `threads` threads.
#[must_use]
pub fn args(threads: usize) -> Vec<OsString> {
    [
        "cpu".to_owned(),
        format!("--cpu-max-prime={MAX_PRIME}"),
        format!("--threads={threads}"),
        format!("--time={SECONDS}"),
        "run".to_owned(),
    ]
    .map(OsString::from)
    .to_vec()
}

/// The rate in sysbench's output, given its exit code (`None` after a signal) and its standard
/// output followed by its standard error.
pub fn outcome(code: Option<i32>, output: &str) -> Result<Rate, String> {
    match code {
        Some(0) => rate(output).ok_or_else(|| "sysbench 没有输出每秒事件数".to_owned()),
        Some(code) => Err(format!("sysbench 退出码 {code}：{}", last_line(output))),
        None => Err(format!("sysbench 被信号终止：{}", last_line(output))),
    }
}

/// Runs `attempt` until it produces a rate, at most [`ATTEMPTS`] times. A cancellation ends
/// the run at once.
pub async fn retried<F, Fut>(mut attempt: F) -> Result<Success, RunError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Rate, AttemptError>>,
{
    let mut failures = Vec::new();
    loop {
        match attempt().await {
            Ok(rate) => return Ok(Success { rate, failures }),
            Err(AttemptError::Cancelled) => return Err(RunError::Cancelled),
            Err(AttemptError::Failed(reason)) => {
                failures.push(reason);
                if failures.len() == ATTEMPTS {
                    return Err(RunError::Failed(failures));
                }
            }
        }
    }
}

/// The value of the line `events per second: 12842.07`.
fn rate(output: &str) -> Option<Rate> {
    let text = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("events per second:"))?
        .trim();
    let value: f64 = text.parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| Rate {
        text: text.to_owned(),
        value,
    })
}

/// The last line with text, for a reason that quotes sysbench.
fn last_line(output: &str) -> &str {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("没有输出")
}
