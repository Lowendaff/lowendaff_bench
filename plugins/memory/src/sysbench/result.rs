//! What `memory.sysbench` measured: the read and write bandwidth with one thread and with all
//! of them, the buffers, and why a run or the whole plugin has no result. Numbers and kinds of
//! reasons only; the reporter words them.

use bench_api::typed::Measurement;
use bench_api::typed::ToolVersion;
use bench_tool::SYSBENCH;
use bench_tool::job;
use serde::Serialize;

use crate::Retry;
use crate::Value;

/// `memory.sysbench`'s result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// The version that sysbench printed, such as `1.0.20`.
    pub version: Option<String>,
    /// The block of each single-thread run, in bytes.
    pub single: u64,
    /// The block of each thread of the multi-thread runs, in bytes; `None` with one usable
    /// thread, which leaves no multi-thread runs.
    pub per_thread: Option<u64>,
    /// The threads of the multi-thread runs; 1 when there are none.
    pub threads: usize,
    /// Four times the last-level caches, when the data of a run is smaller: a part of its reads
    /// and writes stays in the caches, and the results are high.
    pub small: Option<u64>,
    /// Whether the last-level caches were assumed, for want of their sizes.
    pub cache_unknown: bool,
    /// The runs that worked, in order: reading with one thread and with all, then writing.
    pub runs: Vec<Run>,
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

/// Reading or writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Operation {
    /// Sequential reads.
    Read,
    /// Sequential writes.
    Write,
}

/// A run that worked.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    /// What it did.
    pub operation: Operation,
    /// With how many threads.
    pub threads: usize,
    /// Its bandwidth in MiB/s.
    pub rate: Value,
    /// The faults of the attempts before the one that worked, oldest first.
    pub retries: Vec<Fault>,
}

/// Why an attempt gave no rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// sysbench printed no transfer rate.
    NoRate,
}

impl From<job::Fault> for Fault {
    fn from(fault: job::Fault) -> Self {
        Self::Tool(fault)
    }
}

impl Retry for Fault {
    fn again(&self) -> bool {
        true
    }
}

/// Why `memory.sysbench` has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The available memory cannot be read, so no buffer can be sized.
    NoMemoryInfo,
    /// Every attempt of a run failed: the run, and each attempt's fault, oldest first.
    Run {
        /// What the run did.
        operation: Operation,
        /// With how many threads.
        threads: usize,
        /// Each attempt's fault.
        faults: Vec<Fault>,
    },
    /// The plugin was cancelled.
    Cancelled,
}
