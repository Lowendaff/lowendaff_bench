//! What `memory.stream` measured: the best rate of each of STREAM's kernels with one thread and
//! with all of them, the arrays, and why a run or the whole plugin has no result. Numbers and
//! kinds of reasons only, with STREAM's own line of a failed validation; the reporter words
//! them.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::STREAM;
use bench_tool::job;
use serde::Serialize;

use crate::Retry;
use crate::Value;

/// STREAM's kernels, in the order it prints them.
pub const KERNELS: [&str; 4] = ["Copy", "Scale", "Add", "Triad"];

/// `memory.stream`'s result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// The version that STREAM printed, such as `5.10`.
    pub version: Option<String>,
    /// The size of each of the three arrays, in millions of elements.
    pub millions: u64,
    /// The threads of the multi-thread run; 1 when there is none.
    pub threads: usize,
    /// Whether an array is smaller than four times the last-level caches, which STREAM's rules
    /// ask for.
    pub below_rule: bool,
    /// Whether the last-level caches were assumed, for want of their sizes.
    pub cache_unknown: bool,
    /// The runs that worked, in order: with one thread, then with all.
    pub runs: Vec<Run>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: STREAM.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// A run that worked.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Run {
    /// With how many threads.
    pub threads: usize,
    /// The best rate of each kernel in MB/s, in [`KERNELS`]' order.
    pub rates: [Value; 4],
    /// The faults of the attempts before the one that worked, oldest first.
    pub retries: Vec<Fault>,
}

/// Why an attempt gave no rates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// STREAM's results did not validate: its line that says so.
    Invalid(String),
    /// STREAM printed no results.
    NoResult,
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

/// Why `memory.stream` has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The available memory cannot be read, so no array can be sized.
    NoMemoryInfo,
    /// Not even the smallest arrays fit in half the available memory, `budget` bytes; the
    /// plugin skips.
    Short {
        /// Half the available memory, in bytes.
        budget: u64,
    },
    /// Every attempt of a run failed: its threads, and each attempt's fault, oldest first.
    Run {
        /// With how many threads.
        threads: usize,
        /// Each attempt's fault.
        faults: Vec<Fault>,
    },
    /// The plugin was cancelled.
    Cancelled,
}
