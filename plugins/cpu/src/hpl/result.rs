//! Result and fault types for High-Performance Linpack (`xhpl`).

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::HPL;
use bench_tool::job;
use serde::Serialize;

/// High-Performance Linpack result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Reported HPL version, such as `2.3`.
    pub version: Option<String>,
    /// OpenMP/OpenBLAS worker threads used.
    pub threads: usize,
    /// Matrix order `N`.
    pub n: u32,
    /// Block size `NB`.
    pub nb: u32,
    /// Solve time in seconds.
    pub seconds: f64,
    /// Achieved rate in GFLOPS.
    pub gflops: f64,
    /// Scaled residual value verified `< 16.0`.
    pub residual: Option<f64>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: HPL.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// Why an `xhpl` run gave no verified GFLOPS reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// Underlying process fault.
    Tool(job::Fault),
    /// Could not create `HPL.dat` in the work directory.
    Setup(String),
    /// `xhpl` failed its scaled residual check (`||Ax-b||_oo/... FAILED`).
    ResidualFailed,
    /// `xhpl` exited with 0 without printing a valid result table row.
    NoResult,
}

/// Why the HPL sub-suite stopped short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The HPL run failed.
    Run(Fault),
    /// Interrupted via cancellation token.
    Cancelled,
}
