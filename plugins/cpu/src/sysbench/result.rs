//! What CPU 性能 measured: the rate of each run and the attempts it took, or why a run failed.
//! Numbers and kinds of reasons only; the reporter words them.

use bench_api::typed::Measurement;
use bench_api::typed::ToolVersion;
use bench_tool::SYSBENCH;
use bench_tool::job;
use serde::Serialize;

/// Events per second as sysbench printed them, and their value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rate {
    /// The number as printed, with two decimals.
    pub text: String,
    /// The same number.
    pub value: f64,
}

/// One run that produced a rate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    /// Its threads.
    pub threads: usize,
    /// Its rate.
    pub rate: Rate,
    /// Why the attempts before the one that succeeded failed, oldest first.
    pub retries: Vec<Fault>,
}

/// CPU 性能's result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// The version that sysbench printed, such as `1.0.20`.
    pub version: Option<String>,
    /// The run with one thread.
    pub single: Run,
    /// The run with every usable thread, when there are several.
    pub multi: Option<Run>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: SYSBENCH.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// Why an attempt gave no rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// sysbench exited with 0 but printed no usable rate.
    NoRate,
}

/// Why CPU 性能 has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Every attempt of the run with one thread failed; their faults, oldest first.
    Single(Vec<Fault>),
    /// Every attempt of the run with every thread failed; their faults, oldest first. The run
    /// with one thread was published.
    Multi(Vec<Fault>),
    /// The plugin was cancelled.
    Cancelled,
}
