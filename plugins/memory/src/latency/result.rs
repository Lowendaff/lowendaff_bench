//! What `memory.latency` measured: multichase's latency in each cache level and in memory, the
//! buffers, and why a run or the whole plugin has no result. Numbers and kinds of reasons only;
//! the reporter words them.

use bench_api::typed::Measurement;
use bench_api::typed::ToolVersion;
use bench_tool::MULTICHASE;
use bench_tool::job;
use serde::Serialize;

use crate::Retry;
use crate::Value;
use crate::plan::Level;

/// `memory.latency`'s result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// Every row of the plan, memory last, each with its latency once measured.
    pub rows: Vec<Row>,
    /// The cache levels left out for want of their sizes.
    pub missing: Vec<Level>,
    /// Four times the last-level caches, when memory's buffer is smaller: a part of its accesses
    /// stays in the caches, and its latency is low.
    pub small: Option<u64>,
    /// Whether the last-level caches were assumed, for want of their sizes.
    pub cache_unknown: bool,
}

impl Measurement for Output {
    /// multichase prints no version, so the version of the pinned build stands for it.
    fn tools(&self) -> Vec<ToolVersion> {
        vec![ToolVersion {
            name: MULTICHASE.name,
            version: MULTICHASE.version.to_owned(),
        }]
    }
}

/// A row of the latency table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// A cache level, or memory.
    pub level: Level,
    /// The buffer chased through, in bytes.
    pub buffer: u64,
    /// The latency in nanoseconds, once measured.
    pub latency: Option<Value>,
    /// The faults of the attempts before the one that worked, oldest first.
    pub retries: Vec<Fault>,
}

/// Why an attempt gave no latency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// multichase printed no latency.
    NoLatency,
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

/// Why `memory.latency` has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The available memory cannot be read, so memory's buffer cannot be sized.
    NoMemoryInfo,
    /// Every attempt of a level's run failed: the level, and each attempt's fault, oldest
    /// first.
    Run {
        /// The level.
        level: Level,
        /// Each attempt's fault.
        faults: Vec<Fault>,
    },
    /// The plugin was cancelled.
    Cancelled,
}
