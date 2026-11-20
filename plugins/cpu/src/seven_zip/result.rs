//! Result and fault types for the 7-Zip (`7zz b`) CPU benchmark.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::SEVENZIP;
use bench_tool::job;
use serde::Serialize;

/// One 7-Zip benchmark run (`-mmt<threads>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Run {
    /// Threads passed via `-mmt`.
    pub threads: usize,
    /// Compression speed in KiB/s (`Avr:` row).
    pub compress_kib_s: u64,
    /// Compression rating in MIPS (`Avr:` row).
    pub compress_mips: u64,
    /// Decompression speed in KiB/s (`Avr:` row).
    pub decompress_kib_s: u64,
    /// Decompression rating in MIPS (`Avr:` row).
    pub decompress_mips: u64,
    /// Combined CPU usage percentage (`Tot:` row).
    pub usage_pct: u64,
    /// Combined rating per usage in MIPS (`Tot:` row).
    pub ru_mips: u64,
    /// Combined total rating in MIPS (`Tot:` row).
    pub total_mips: u64,
    /// Earlier failed attempts before this run succeeded.
    pub retries: Vec<Fault>,
}

/// Complete or partial 7-Zip benchmark result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// Reported 7-Zip version, such as `26.06`.
    pub version: Option<String>,
    /// Single-thread run (`-mmt1`).
    pub single: Run,
    /// Multi-thread run (`-mmt<N>`) when more than one CPU thread is usable.
    pub multi: Option<Run>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: SEVENZIP.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// Why one attempt of `7zz b` failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// Underlying process fault.
    Tool(job::Fault),
    /// `7zz b` exited with 0 without printing valid `Avr:` and `Tot:` summary lines.
    NoScores,
}

/// Why the 7-Zip sub-suite failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Single-thread benchmark failed on every attempt.
    Single(Vec<Fault>),
    /// Multi-thread benchmark failed on every attempt after single-thread succeeded.
    Multi(Vec<Fault>),
    /// Interrupted via cancellation token.
    Cancelled,
}
