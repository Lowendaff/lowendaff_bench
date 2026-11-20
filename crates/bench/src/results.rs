//! What each plugin left after a run, for the reporter: one variant per plugin.

use bench_api::Stop;
use plugin_cpu::geekbench;
use plugin_cpu::sysbench;
use plugin_disk::fio;
use plugin_disk::health;
use plugin_ip::media;
use plugin_ip::quality;
use plugin_memory::latency;
use plugin_memory::numa;
use plugin_memory::stream;
use plugin_network::bgp;
use plugin_network::iperf3;
use plugin_network::trace;

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
    /// CPU performance (`cpu.sysbench`).
    CpuSysbench(Report<sysbench::Output, sysbench::Failure>),
    /// Geekbench 7 (`cpu.geekbench`).
    CpuGeekbench(Report<geekbench::Output, geekbench::Failure>),
    /// Memory sysbench (`memory.sysbench`).
    MemorySysbench(Report<plugin_memory::sysbench::Output, plugin_memory::sysbench::Failure>),
    /// Memory STREAM (`memory.stream`).
    MemoryStream(Report<stream::Output, stream::Failure>),
    /// Memory latency (`memory.latency`).
    MemoryLatency(Report<latency::Output, latency::Failure>),
    /// Memory NUMA (`memory.numa`).
    MemoryNuma(Report<numa::Output, numa::Failure>),
    /// Disk health (`disk.health`).
    DiskHealth(Report<health::Output, health::Failure>),
    /// Disk performance (`disk.fio`).
    DiskFio(Report<fio::Output, fio::Failure>),
    /// Network speed (`network.iperf3`).
    NetworkIperf3(Report<iperf3::Output, iperf3::Failure>),
    /// Route tracing (`network.trace`).
    NetworkTrace(Report<trace::Output, trace::Failure>),
    /// BGP paths (`network.bgp`).
    NetworkBgp(Report<bgp::Output, bgp::Failure>),
    /// IP reputation (`ip.quality`).
    IpQuality(Report<quality::Output, quality::Failure>),
    /// Media unlock (`ip.media`).
    IpMedia(Report<media::Output, media::Failure>),
}
