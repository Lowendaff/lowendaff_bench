//! What 硬盘性能 measured: the rates of each test, or why a test or the whole item has none.
//! Numbers and kinds of reasons only, with fio's own error line; the reporter words them in
//! YABS's formats.

use bench_api::Measurement;
use bench_api::ToolVersion;
use bench_tool::FIO;
use bench_tool::job;
use serde::Serialize;

/// 硬盘性能's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    /// The version that fio printed, such as `3.43`.
    pub version: Option<String>,
    /// The test file's size in bytes.
    pub size: u64,
    /// The free space in GiB that YABS asks for on ZFS, twice `spa_asize_inflation`, when the
    /// test directory is on ZFS and has less.
    pub zfs_needed: Option<u64>,
    /// The tests that ended, in YABS's order.
    pub tests: Vec<Test>,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        self.version
            .iter()
            .map(|version| ToolVersion {
                name: FIO.name,
                version: version.clone(),
            })
            .collect()
    }
}

/// One test, at one block size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Test {
    /// The block size, as fio spells it, such as `4k`.
    pub block_size: &'static str,
    /// What it measured, or why it measured nothing.
    pub outcome: Outcome,
}

/// What one test produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Outcome {
    /// The test reported its rates.
    Done(Rates),
    /// The test failed; a test past YABS's 35 seconds is stopped and fails as timed out.
    Failed(Fault),
}

/// Read and write rates of one test, as fio's terse output reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rates {
    /// Read bandwidth in KiB/s (terse field 7).
    pub read_kib: u64,
    /// Read operations per second (field 8).
    pub read_iops: u64,
    /// Write bandwidth in KiB/s (field 48).
    pub write_kib: u64,
    /// Write operations per second (field 49).
    pub write_iops: u64,
}

/// Why a run of fio gave no rates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The run went wrong as any tool's run can; an exit code or a signal comes with fio's
    /// error line.
    Tool(job::Fault),
    /// fio printed no result for the test.
    NoResult,
    /// fio's result reports this error number.
    Error(u64),
    /// This field of fio's result, counted from 1, is not a number.
    Field(usize),
}

/// Why 硬盘性能 has no complete result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// The free space of the test directory cannot be read.
    NoSpaceInfo,
    /// The test directory is on a RAM disk, so the test would measure memory; the item skips.
    InMemory,
    /// The test directory has less free space than the test file needs; the item skips.
    Space {
        /// The test file's size in bytes.
        needed: u64,
        /// The free space in bytes.
        available: u64,
    },
    /// The test directory's filesystem refuses direct I/O; the item skips.
    NoDirectIo,
    /// fio could not lay out the test file.
    Setup(Fault),
    /// No test gave rates; each one's reason is in the published result.
    NoResult,
    /// The plugin was cancelled.
    Cancelled,
}
