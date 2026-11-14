//! What each plugin left after a run, for the reporter: one variant per plugin of the typed
//! contract, and one for the plugins of the first contract, which leave their records.

use bench_api::Record;
use bench_api::typed::Stop;
use plugin_cpu::geekbench;
use plugin_cpu::sysbench;
use plugin_disk::fio;
use plugin_disk::health;
use plugin_memory::latency;
use plugin_memory::numa;
use plugin_memory::stream;

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
    /// Geekbench 7 (`cpu.geekbench`).
    CpuGeekbench(Report<geekbench::Output, geekbench::Failure>),
    /// 内存 sysbench (`memory.sysbench`).
    MemorySysbench(Report<plugin_memory::sysbench::Output, plugin_memory::sysbench::Failure>),
    /// 内存 STREAM (`memory.stream`).
    MemoryStream(Report<stream::Output, stream::Failure>),
    /// 内存延迟 (`memory.latency`).
    MemoryLatency(Report<latency::Output, latency::Failure>),
    /// 内存 NUMA (`memory.numa`).
    MemoryNuma(Report<numa::Output, numa::Failure>),
    /// 硬盘健康 (`disk.health`).
    DiskHealth(Report<health::Output, health::Failure>),
    /// 硬盘性能 (`disk.fio`).
    DiskFio(Report<fio::Output, fio::Failure>),
}
