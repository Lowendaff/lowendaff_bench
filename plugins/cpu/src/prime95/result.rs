//! Result and fault types for Prime95 (`mprime -t -d`) Small-FFT torture test.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::PRIME95;
use bench_tool::job;
use serde::Serialize;

/// Prime95 Small-FFT torture stability result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// Reported `mprime` version, such as `30.19b20`.
    pub version: Option<String>,
    /// Number of torture worker threads started.
    pub workers: usize,
    /// Torture window duration in seconds before clean `SIGINT` stop.
    pub duration_secs: u64,
    /// Active FFT kernel description reported by workers (e.g. `FMA3 FFT length 18K`).
    pub fft_desc: Option<String>,
    /// Total Lucas-Lehmer / PRP sub-tests completed across workers during the window.
    pub tests_completed: u64,
    /// Hardware/rounding errors reported across workers.
    pub errors: u64,
    /// Warnings reported across workers.
    pub warnings: u64,
    /// `true` iff `errors == 0` and no `FATAL ERROR` / `HARDWARE FAILURE` occurred.
    pub passed: bool,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: PRIME95.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// Why `mprime` failed to complete its timed torture run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// Underlying process fault.
    Tool(job::Fault),
    /// Missing host shared library (such as `libstdc++.so.6` or `libm.so.6`).
    MissingHostLib(String),
    /// Could not create torture work directory or config files.
    Setup(String),
    /// Hardware arithmetic error (`FATAL ERROR` / `ROUND OFF` / `errors > 0`) detected!
    HardwareError(String),
    /// `mprime` exited without printing worker torture status lines.
    NoSummary,
}

/// Why the Prime95 sub-suite stopped short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Torture test failed or detected a hardware error.
    Run(Fault),
    /// Interrupted via cancellation token.
    Cancelled,
}
