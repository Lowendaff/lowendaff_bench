//! Result and fault types for y-cruncher Pi computation.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::YCRUNCHER;
use bench_tool::job;
use serde::Serialize;

/// y-cruncher Pi computation benchmark result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Reported y-cruncher version, such as `0.8.7.9547`.
    pub version: Option<String>,
    /// Decimal digits of Pi computed (`25_000_000` or `100_000_000`).
    pub digits: u64,
    /// Total Computation Time in seconds.
    pub computation_secs: f64,
    /// Start-to-End Wall Time in seconds.
    pub wall_secs: f64,
    /// CPU utilization percentage (`CPU Utilization`).
    pub cpu_utilization_pct: Option<f64>,
    /// Multi-core efficiency percentage (`Multi-core Efficiency`).
    pub multicore_efficiency_pct: Option<f64>,
    /// SIMD tuning profile auto-selected for the host CPU, when reported.
    pub tuning: Option<String>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: YCRUNCHER.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// Why y-cruncher failed to produce a validated Pi benchmark reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// Underlying process fault.
    Tool(job::Fault),
    /// Could not create work directory.
    Setup(String),
    /// `Spot Check:` failed or reported corrupt digits.
    ValidationFailed,
    /// y-cruncher exited with 0 without printing computation times.
    NoResult,
}

/// Why the y-cruncher sub-suite stopped short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The y-cruncher run failed.
    Run(Fault),
    /// Interrupted via cancellation token.
    Cancelled,
}
