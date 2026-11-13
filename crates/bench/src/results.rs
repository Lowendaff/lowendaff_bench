//! What each plugin left after a run, for the reporter: one variant per plugin of the typed
//! contract, and one for the plugins of the first contract, which leave their records.

use bench_api::Record;
use bench_api::typed::Stop;
use plugin_cpu::sysbench;

/// A plugin's result, complete or as far as it got, and its own reason when it stopped short.
#[derive(Debug, Clone, PartialEq)]
pub struct Report<O, F> {
    /// The returned result, or else the last one the plugin published.
    pub output: Option<O>,
    /// Why the plugin skipped or failed, in its own terms.
    pub stop: Option<Stop<F>>,
}

/// One variant per plugin; the reporter matches it exhaustively, so a plugin without a format
/// does not compile.
#[derive(Debug, Clone, PartialEq)]
pub enum Measured {
    /// A plugin of the first contract: the records it emitted, and its reason in Chinese.
    Legacy(Report<Vec<Record>, String>),
    /// CPU 性能 (`cpu.sysbench`).
    CpuSysbench(Report<sysbench::Output, sysbench::Failure>),
}
