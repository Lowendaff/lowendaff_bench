//! What 硬盘健康 read: where each disk sits, smartctl's verdict and the disk's values, and why a
//! read or the whole item failed. Numbers and kinds of reasons only, with smartctl's and the
//! system's own messages; the reporter words them. Serial numbers are only compared, to find a
//! disk listed twice, and are never kept.

use std::path::PathBuf;

use bench_api::typed::Measurement;
use bench_api::typed::ToolVersion;
use bench_tool::SMARTCTL;
use bench_tool::job;
use serde::Serialize;

use super::location::Location;

/// 硬盘健康's result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Output {
    /// The version that smartctl printed, such as `7.5`.
    pub version: Option<String>,
    /// One entry per physical disk, in the table's order.
    pub disks: Vec<Disk>,
    /// Device nodes that smartctl created and that could not be removed.
    pub nodes_left: Vec<NodeLeft>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: SMARTCTL.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// One physical disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disk {
    /// Where it sits.
    pub location: Location,
    /// smartctl's verdict, or why there is none.
    pub health: Health,
    /// Its values.
    pub values: Values,
}

/// smartctl's verdict, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Health {
    /// SMART status passed.
    Passed,
    /// SMART status failed.
    Failed,
    /// No verdict.
    Unknown {
        /// The disk supports SMART but has it switched off.
        smart_off: bool,
    },
    /// The disk has no SMART.
    Unsupported,
    /// smartctl could not read the disk.
    ReadFailed(Fault),
}

/// A disk's values; `None` when smartctl did not report one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Values {
    /// Hours powered on.
    pub hours: Option<u64>,
    /// Temperature in °C.
    pub temperature: Option<i64>,
    /// Endurance used, in percent; NVMe reports over 100 past the rated endurance.
    pub endurance_used: Option<u64>,
    /// Bytes written.
    pub written: Option<u64>,
    /// Bad blocks, as the disk's protocol counts them.
    pub bad_blocks: BadBlocks,
    /// SATA's interface CRC errors.
    pub interface_errors: Option<u64>,
}

/// Bad blocks, which each protocol counts in its own way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum BadBlocks {
    /// SATA: reallocated and pending sectors.
    Sectors {
        /// Sectors already replaced by spares.
        reallocated: Option<u64>,
        /// Sectors waiting to be replaced.
        pending: Option<u64>,
    },
    /// NVMe media errors, or the entries of a SAS disk's grown defect list.
    Count(Option<u64>),
}

impl Default for BadBlocks {
    fn default() -> Self {
        Self::Count(None)
    }
}

/// Why smartctl gave nothing to read, for the scan or for one disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can.
    Tool(job::Fault),
    /// What it printed is not smartctl's JSON.
    Unparsable,
    /// smartctl's own error message.
    Message(String),
    /// Exit status bit 1 without a message: it could not open or identify the device.
    Unopened,
    /// Exit status bit 0 without a message: it refused its arguments.
    Refused,
}

/// A device node that smartctl created and that could not be removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeLeft {
    /// The node.
    pub path: PathBuf,
    /// The system's reason.
    pub error: String,
}

/// Why 硬盘健康 has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The scan found no disk; the item skips.
    NoDisks,
    /// The scan failed.
    Scan(Fault),
    /// No disk could be read; every one is in the published result, with its reason.
    AllFailed,
    /// The plugin was cancelled.
    Cancelled,
}
