//! What Geekbench 7 measured: its scores and the result link, what was done about memory, and
//! why it failed. Numbers and kinds of reasons only, with Geekbench's and the system's own
//! messages; the reporter words them.

use std::path::PathBuf;

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::GEEKBENCH;
use bench_tool::job;
use serde::Serialize;

/// The scores of a run that uploaded its result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scores {
    /// The version that Geekbench printed, such as `7.1.0` or `7.1.0 Preview`.
    pub version: Option<String>,
    /// The single-core score.
    pub single: u32,
    /// The multi-core score.
    pub multi: u32,
    /// The public result page, never the link that claims the result.
    pub url: Option<String>,
}

/// What was done about memory before Geekbench ran, when it may be short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Memory {
    /// Temporary swap of `gib` GiB was added.
    Swapped {
        /// Bytes Geekbench is estimated to need.
        need: u64,
        /// Bytes of memory and swap it could use.
        available: u64,
        /// The swap's size in GiB.
        gib: u64,
    },
    /// Memory may be short, and no swap was added.
    Short {
        /// Bytes Geekbench is estimated to need.
        need: u64,
        /// Bytes of memory and swap it could use.
        available: u64,
        /// Why there is no swap.
        why: NoSwap,
    },
}

/// Why no temporary swap was added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NoSwap {
    /// The kernel would leave it unused: `vm.swappiness` is 0 under the multi-gen LRU.
    Unused,
    /// The work directory's filesystem has no room for it; its free bytes, when known.
    NoRoom(Option<u64>),
    /// Swap needs a permission that the program lacks, as in most containers.
    Denied,
    /// The work directory's filesystem cannot hold a swap file.
    Unsupported,
    /// Another error, as the system words it.
    Error(String),
}

/// A temporary swap that could not be turned off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SwapLeft {
    /// The swap file.
    pub path: PathBuf,
    /// The system's reason.
    pub error: String,
}

/// Geekbench 7's result, partial until the scores arrive.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Output {
    /// What was done about memory, when it may be short.
    pub memory: Option<Memory>,
    /// Why Geekbench could not be made the kernel's first choice when memory runs out, when it
    /// could not; the first run's that could not.
    pub first_to_go: Option<job::Adjustment>,
    /// The scores of the run that uploaded its result.
    pub scores: Option<Scores>,
    /// Why each earlier run's upload failed, in Geekbench's words, oldest first.
    pub uploads: Vec<String>,
    /// A temporary swap that could not be turned off.
    pub swap_left: Option<SwapLeft>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.scores
            .iter()
            .filter_map(|scores| scores.version.clone())
            .map(|version| ToolVersion {
                name: GEEKBENCH.name,
                version,
            })
            .collect()
    }
}

/// Why a run gave no scores, apart from its upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// Geekbench exited with 0 but printed no scores.
    NoScores,
}

/// Why Geekbench 7 has no scores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The last run's upload failed too, for this reason in Geekbench's words; the free version
    /// shows scores only after an upload.
    Upload(String),
    /// The run went wrong.
    Run(Fault),
    /// The kernel killed Geekbench for lack of memory.
    OutOfMemory,
    /// The plugin was cancelled.
    Cancelled,
}
