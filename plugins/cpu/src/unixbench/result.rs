//! Result and fault types for the 12-item Byte-UnixBench System Index.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::UNIXBENCH;
use bench_tool::job;
use serde::Serialize;

/// One sub-benchmark inside an [`IndexRun`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IndexItem {
    /// Canonical UnixBench test id, such as `dhry2reg` or `shell8`.
    pub id: &'static str,
    /// Measured aggregate rate across all parallel copies.
    pub rate: f64,
    /// Unit reported by UnixBench (`lps`, `MWIPS`, `KBps`, or `lpm`).
    pub unit: &'static str,
    /// Baseline rate from `index.base` (`SPARCstation 20-61 = 10.0`).
    pub baseline: f64,
    /// Indexed score (`rate / baseline * 10.0`).
    pub index: f64,
}

/// One complete 12-item System Index pass at `copies` parallel processes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IndexRun {
    /// Number of parallel copies executed per sub-benchmark.
    pub copies: usize,
    /// Geometric-mean System Index score (`exp(mean(ln(rate_i / baseline_i))) * 10.0`).
    pub index: f64,
    /// Per-item rates and index scores in canonical UnixBench order.
    pub items: Vec<IndexItem>,
}

/// Byte-UnixBench 6.0.1 System Index result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Pinned UnixBench version (`6.0.1`).
    pub version: String,
    /// Single-copy System Index (`1` parallel copy).
    pub single: IndexRun,
    /// Multi-copy System Index (`N` parallel copies) when more than one CPU thread is usable.
    pub multi: Option<IndexRun>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        vec![ToolVersion {
            name: UNIXBENCH.name,
            version: self.version.clone(),
        }]
    }
}

/// Why one sub-benchmark inside UnixBench failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// Underlying process fault for `item`.
    Tool {
        /// Canonical UnixBench item id.
        item: &'static str,
        /// Process fault.
        fault: job::Fault,
    },
    /// Could not create temporary working directories or copy `sort.src`.
    Setup(String),
    /// Sub-benchmark exited with 0 without printing a valid `COUNT|...` line.
    NoCount {
        /// Canonical UnixBench item id.
        item: &'static str,
    },
}

/// Why the UnixBench sub-suite stopped short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// Single-copy System Index failed.
    Single(Fault),
    /// Multi-copy System Index failed after single-copy succeeded.
    Multi(Fault),
    /// Interrupted via cancellation token.
    Cancelled,
}
