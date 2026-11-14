//! What `memory.numa` measured: the bandwidth and latency from each node's CPUs to each node's
//! memory, and why a run or the whole plugin has no result. Numbers and kinds of reasons only;
//! the reporter words them.

use bench_api::typed::Measurement;
use bench_api::typed::ToolVersion;
use bench_tool::MULTICHASE;
use bench_tool::job;
use serde::Serialize;

use crate::Retry;
use crate::Value;

/// `memory.numa`'s result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    /// The nodes of the matrix, rows and columns counted once.
    pub nodes: usize,
    /// The nodes that the threads run on, one per row.
    pub rows: Vec<usize>,
    /// The nodes that the memory is placed on, one per column.
    pub columns: Vec<usize>,
    /// The cells, row by row, each with what was measured so far.
    pub cells: Vec<Vec<Cell>>,
    /// Whether the last-level caches were assumed, for want of their sizes.
    pub cache_unknown: bool,
}

impl Measurement for Output {
    /// multichase and multiload print no version, so the version of the pinned build stands
    /// for them.
    fn tools(&self) -> Vec<ToolVersion> {
        vec![ToolVersion {
            name: MULTICHASE.name,
            version: MULTICHASE.version.to_owned(),
        }]
    }
}

/// One cell of the matrix: threads on one node, memory on another or the same.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Cell {
    /// The bandwidth in MiB/s, once measured.
    pub bandwidth: Option<Measured>,
    /// The latency in nanoseconds, once measured.
    pub latency: Option<Measured>,
}

/// A value, and the faults of the attempts before the one that measured it, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measured {
    /// The value.
    pub value: Value,
    /// The faults of the attempts before.
    pub retries: Vec<Fault>,
}

/// The two runs of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Kind {
    /// multiload's bandwidth.
    Bandwidth,
    /// multichase's latency.
    Latency,
}

/// Why an attempt gave no value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// multiload printed no bandwidth.
    NoBandwidth,
    /// multichase printed no latency.
    NoLatency,
    /// The kernel would not bind memory to a node, as in Docker's default containers;
    /// retrying cannot help.
    Refused,
}

impl From<job::Fault> for Fault {
    fn from(fault: job::Fault) -> Self {
        Self::Tool(fault)
    }
}

impl Retry for Fault {
    fn again(&self) -> bool {
        *self != Self::Refused
    }
}

/// Why `memory.numa` has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The available memory cannot be read, so no buffer can be sized.
    NoMemoryInfo,
    /// Fewer than two NUMA nodes can be used, so there is nothing to measure between them; the
    /// plugin skips. The orchestrator does not run it on such a host, so this happens only when
    /// the layout changed since the host profile was read.
    OneNode,
    /// The kernel would not bind memory to a node; the plugin skips.
    Refused,
    /// Every attempt of a run failed: the cell and the run, and each attempt's fault, oldest
    /// first.
    Run {
        /// The node of the threads.
        row: usize,
        /// The node of the memory.
        column: usize,
        /// The run.
        kind: Kind,
        /// Each attempt's fault.
        faults: Vec<Fault>,
    },
    /// The plugin was cancelled.
    Cancelled,
}
