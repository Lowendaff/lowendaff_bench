//! Result and fault types for PassMark PerformanceTest Linux (`results_cpu.yml`).

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::PASSMARK;
use bench_tool::job;
use serde::Serialize;

/// PassMark CPU Mark and sub-test breakdown (`-r 1`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Reported PassMark version, such as `11.0.1004`.
    pub version: Option<String>,
    /// Overall CPU Mark (`SUMM_CPU`).
    pub cpu_mark: f64,
    /// Single-Thread Rating (`CPU_SINGLETHREAD`).
    pub single_thread: f64,
    /// Integer Math (`CPU_INTEGER_MATH`, MOps/s).
    pub integer_math: f64,
    /// Floating Point Math (`CPU_FLOATINGPOINT_MATH`, MOps/s).
    pub floating_math: f64,
    /// Prime Numbers (`CPU_PRIME`, Million Primes/s).
    pub prime: f64,
    /// Sorting (`CPU_SORTING`, Thousand Strings/s).
    pub sorting: f64,
    /// Encryption (`CPU_ENCRYPTION`, MB/s).
    pub encryption: f64,
    /// Compression (`CPU_COMPRESSION`, KB/s).
    pub compression: f64,
    /// Physics (`CPU_PHYSICS`, Frames/s).
    pub physics: f64,
    /// Extended Instructions (`CPU_MATRIX_MULT_SSE`, Million Matrices/s).
    pub extended_instructions: f64,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: PASSMARK.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// Why PassMark PerformanceTest failed to produce `results_cpu.yml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// Underlying process fault.
    Tool(job::Fault),
    /// Host lacks a required system C++ library (such as `libstdc++.so.6`).
    MissingHostLib(String),
    /// Could not create work directory or read `results_cpu.yml`.
    Io(String),
    /// `results_cpu.yml` was missing, incomplete, or lacked CPU scores.
    NoScores,
}

/// Why the PassMark sub-suite stopped short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The PassMark run failed.
    Run(Fault),
    /// Interrupted via cancellation token.
    Cancelled,
}
