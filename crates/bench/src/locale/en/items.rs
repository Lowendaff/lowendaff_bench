//! English: the test items' chapters.

use std::path::Path;

use super::English;
use crate::locale::BgpField;
use crate::locale::BgpWords;
use crate::locale::CloudflareChoice;
use crate::locale::CloudflareField;
use crate::locale::CloudflareIssueRef;
use crate::locale::CloudflareWords;
use crate::locale::CpuWords;
use crate::locale::DiskWords;
use crate::locale::FioRow;
use crate::locale::InetspeedChoice;
use crate::locale::InetspeedProbeFaultRef;
use crate::locale::InetspeedRoundIssueRef;
use crate::locale::InetspeedWarningRef;
use crate::locale::InetspeedWords;
use crate::locale::IpChoice;
use crate::locale::IpField;
use crate::locale::IpTable;
use crate::locale::IpWords;
use crate::locale::ListRule;
use crate::locale::LookupProblem;
use crate::locale::MediaCell;
use crate::locale::MediaChoice;
use crate::locale::MediaWords;
use crate::locale::MemoryWords;
use crate::locale::NetworkWords;
use crate::locale::NoSwapWhy;
use crate::locale::Probe;
use crate::locale::TaierCell;
use crate::locale::TaierChoice;
use crate::locale::TaierListProblem;
use crate::locale::TaierRule;
use crate::locale::TaierWords;
use crate::locale::TraceChoice;
use crate::locale::TraceList;
use crate::locale::TraceListProblem;
use crate::locale::TraceRule;
use crate::locale::TraceWords;
use crate::locale::Verdict;
use crate::locale::WayCell;

/// A tool's version followed by the colon that leads its parameters; nothing without a version.
fn version(version: Option<&str>) -> String {
    version.map_or_else(String::new, |version| format!("{version}: "))
}

impl CpuWords for English {
    fn cpu_suite_step(&self, suite: &str, stage: Option<&str>, threads: Option<usize>) -> String {
        let name = self.cpu_suite_title(suite);
        match (stage, threads) {
            (Some(stage), Some(1)) => format!("{name}: {stage} (1 copy)"),
            (Some(stage), Some(n)) => format!("{name}: {stage} ({n} threads/copies)"),
            (Some(stage), None) => format!("{name}: {stage}"),
            (None, Some(1)) => format!("{name}: single-thread test"),
            (None, Some(n)) => format!("{name}: multi-thread test ({n} threads)"),
            (None, None) => format!("{name}: running"),
        }
    }

    fn cpu_selected_suites(&self, suites: &[String]) -> String {
        format!("Selected suites: {}", suites.join(", "))
    }

    fn cpu_suite_title(&self, suite: &str) -> &'static str {
        match suite {
            "sysbench" => "sysbench",
            "7zip" => "7-Zip",
            "unixbench" => "UnixBench",
            "hpl" => "HPL (Linpack)",
            "passmark" => "PassMark",
            "ycruncher" => "y-cruncher",
            "prime95" => "Prime95",
            "geekbench" => "Geekbench 7",
            _ => "CPU suite",
        }
    }

    fn cpu_suite_skipped(&self, suite: &str, why: &str) -> String {
        format!("{}: skipped ({why})", self.cpu_suite_title(suite))
    }

    fn cpu_suite_failed(&self, suite: &str, why: &str) -> String {
        format!("{}: failed ({why})", self.cpu_suite_title(suite))
    }

    fn cpu_skip_arch(&self, arch: &str) -> String {
        format!("x86_64 only; this host is {arch}")
    }

    fn cpu_missing_lib(&self, lib: &str) -> String {
        format!("missing host shared library {lib}")
    }

    fn cpu_setup_failed(&self, reason: &str) -> String {
        format!("cannot prepare work directory: {reason}")
    }

    fn sysbench_parameters(&self, version: Option<&str>, max_prime: u64, seconds: u64) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("sysbench{version}: prime limit {max_prime}, {seconds} s each")
    }

    fn cpu_headers(&self) -> [&'static str; 3] {
        ["Test", "Threads", "Events/s"]
    }

    fn cpu_row(&self, multi: bool) -> &'static str {
        if multi {
            "Multi-thread"
        } else {
            "Single thread"
        }
    }

    fn cpu_test(&self, multi: bool) -> &'static str {
        if multi {
            "multi-thread test"
        } else {
            "single-thread test"
        }
    }

    fn ratio_label(&self) -> &'static str {
        "Ratio"
    }

    fn ratio(&self, ratio: f64) -> String {
        format!("multi-thread is {ratio:.2} times single-thread")
    }

    fn no_rate(&self) -> String {
        "sysbench printed no events per second".to_owned()
    }

    fn seven_zip_parameters(&self, version: Option<&str>) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("7-Zip{version}: LZMA2 integer compression & decompression")
    }

    fn seven_zip_headers(&self) -> [&'static str; 5] {
        [
            "Test",
            "Threads",
            "Compress (MIPS)",
            "Decompress (MIPS)",
            "Total (MIPS)",
        ]
    }

    fn seven_zip_no_scores(&self) -> String {
        "7-Zip printed no benchmark ratings".to_owned()
    }

    fn unixbench_parameters(&self, version: &str) -> String {
        format!("Byte-UnixBench {version}: 12-item System Index (SPARCstation 20-61 = 10.0)")
    }

    fn unixbench_index_label(&self, copies: usize) -> String {
        if copies <= 1 {
            "Index (1 copy)".to_owned()
        } else {
            format!("Index ({copies} copies)")
        }
    }

    fn unixbench_headers(&self, multi_copies: Option<usize>) -> Vec<String> {
        match multi_copies {
            Some(n) => vec![
                "Test".to_owned(),
                "1-copy".to_owned(),
                format!("{n}-copy"),
                "Unit".to_owned(),
                "1-idx".to_owned(),
                format!("{n}-idx"),
            ],
            None => vec![
                "Test".to_owned(),
                "1-copy".to_owned(),
                "Unit".to_owned(),
                "1-idx".to_owned(),
            ],
        }
    }

    fn unixbench_item_name(&self, id: &str) -> &'static str {
        match id {
            "dhry2reg" => "Dhrystone 2",
            "whetstone-double" => "Double-Precision Whetstone",
            "execl" => "Execl Throughput",
            "fstime" => "File Copy (1024 B, 2000 blk)",
            "fsbuffer" => "File Copy (256 B, 500 blk)",
            "fsdisk" => "File Copy (4096 B, 8000 blk)",
            "pipe" => "Pipe Throughput",
            "context1" => "Pipe-based Context Switching",
            "spawn" => "Process Creation",
            "shell1" => "Shell Scripts (1 concurrent)",
            "shell8" => "Shell Scripts (8 concurrent)",
            "syscall" => "System Call Overhead",
            _ => "UnixBench item",
        }
    }

    fn unixbench_no_count(&self, item: &str) -> String {
        format!("UnixBench {item} printed no COUNT reading")
    }

    fn hpl_parameters(&self, version: Option<&str>, n: u32, nb: u32, threads: usize) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let th = super::count(threads, "thread", "threads");
        format!("HPL{ver} (OpenBLAS): dense FP64 LU, N={n}, NB={nb}, {th}")
    }

    fn hpl_labels(&self) -> [&'static str; 3] {
        ["Throughput", "Solve time", "Residual check"]
    }

    fn hpl_gflops(&self, gflops: f64) -> String {
        format!("{gflops:.2} GFLOPS")
    }

    fn hpl_seconds(&self, seconds: f64) -> String {
        format!("{seconds:.2} s")
    }

    fn hpl_residual_passed(&self, residual: Option<f64>) -> String {
        match residual {
            Some(res) => format!("passed ({res:.2e})"),
            None => "passed".to_owned(),
        }
    }

    fn hpl_residual_failed(&self) -> String {
        "HPL scaled residual check failed".to_owned()
    }

    fn passmark_parameters(&self, version: Option<&str>) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("PassMark PerformanceTest{ver}: Linux CPU suite")
    }

    fn passmark_summary_labels(&self) -> [&'static str; 2] {
        ["CPU Mark", "Single-thread"]
    }

    fn passmark_headers(&self) -> [&'static str; 3] {
        ["Subtest", "Score", "Unit"]
    }

    fn passmark_subtests(&self) -> [(&'static str, &'static str); 8] {
        [
            ("Integer Math", "MOps/s"),
            ("Floating Point Math", "MOps/s"),
            ("Prime Numbers", "MPrimes/s"),
            ("Sorting", "KStrings/s"),
            ("Encryption", "MB/s"),
            ("Compression", "KB/s"),
            ("Physics", "Frames/s"),
            ("Extended Instructions", "MMatrices/s"),
        ]
    }

    fn passmark_no_scores(&self) -> String {
        "PassMark produced no CPU results".to_owned()
    }

    fn ycruncher_parameters(
        &self,
        version: Option<&str>,
        digits: u64,
        tuning: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let millions = digits / 1_000_000;
        let tune = tuning.map_or_else(String::new, |t| format!(" ({t})"));
        format!("y-cruncher{ver}: Pi to {millions}M decimal digits{tune}")
    }

    fn ycruncher_labels(&self) -> [&'static str; 4] {
        [
            "Computation time",
            "Wall time",
            "CPU utilization",
            "Multi-core eff.",
        ]
    }

    fn ycruncher_validation_failed(&self) -> String {
        "y-cruncher Pi digit spot check failed".to_owned()
    }

    fn prime95_parameters(
        &self,
        version: Option<&str>,
        workers: usize,
        duration_secs: u64,
        fft: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let w = super::count(workers, "worker", "workers");
        let kernel = fft.map_or_else(String::new, |f| format!(", {f}"));
        format!("Prime95{ver}: Small-FFT torture, {w}, {duration_secs} s{kernel}")
    }

    fn prime95_labels(&self) -> [&'static str; 2] {
        ["Stability", "Completed tests"]
    }

    fn prime95_passed(&self, warnings: u64) -> String {
        format!(
            "passed (0 errors, {})",
            super::count(
                usize::try_from(warnings).unwrap_or(usize::MAX),
                "warning",
                "warnings"
            )
        )
    }

    fn prime95_hardware_error(&self, detail: &str) -> String {
        format!("Prime95 detected a hardware/rounding error: {detail}")
    }

    fn prime95_no_summary(&self) -> String {
        "Prime95 exited without worker torture summaries".to_owned()
    }

    fn geekbench_labels(&self) -> [&'static str; 3] {
        ["Single-core", "Multi-core", "Result link"]
    }

    fn swapped(&self, need: u64, available: &str, swap: u64) -> String {
        format!(
            "available memory short (needs about {need} GiB, {available} GiB available); \
             added {swap} GiB of temporary swap, so the scores may be low"
        )
    }

    fn memory_short(&self, need: u64, available: &str, why: NoSwapWhy<'_>) -> String {
        let why = match why {
            NoSwapWhy::Unused => {
                "vm.swappiness is 0 here, so the kernel would not swap memory out".to_owned()
            }
            NoSwapWhy::NoRoom(Some(free)) => {
                format!("the run directory's disk has only {free} GiB free")
            }
            NoSwapWhy::NoRoom(None) => {
                "the free space of the run directory's disk cannot be read".to_owned()
            }
            NoSwapWhy::Denied => "not permitted (containers usually forbid it)".to_owned(),
            NoSwapWhy::Unsupported => {
                "the run directory's filesystem does not support swap files".to_owned()
            }
            NoSwapWhy::Error(error) => error.to_owned(),
        };
        format!(
            "available memory may be short (needs about {need} GiB, {available} GiB available); \
             no temporary swap: {why}"
        )
    }

    fn first_to_go_not_set(&self, error: &str) -> String {
        format!("cannot make Geekbench the first to go when memory runs out: {error}")
    }

    fn oom_not_restored(&self, error: &str) -> String {
        format!("cannot restore this program's oom_score_adj: {error}")
    }

    fn uploads_retried(&self, count: usize, last: &str) -> String {
        format!(
            "ran again after {}; the reason: {last}",
            super::count(count, "failed upload", "failed uploads")
        )
    }

    fn swap_left(&self, error: &str, path: &Path) -> String {
        format!(
            "cannot turn off the temporary swap: {error}; run swapoff {} by hand, then delete \
             the file",
            path.display()
        )
    }

    fn upload_failed(&self, reason: &str) -> String {
        format!("the upload failed ({reason}); the free version gives scores only after an upload")
    }

    fn no_scores(&self) -> String {
        "Geekbench printed no scores".to_owned()
    }

    fn out_of_memory(&self) -> String {
        "ended by the system for want of memory while running".to_owned()
    }

    fn cpu_no_suites_selected(&self) -> String {
        "no CPU benchmark suite selected".to_owned()
    }

    fn cpu_all_skipped(&self) -> String {
        "every selected CPU benchmark suite was skipped on this host".to_owned()
    }

    fn cpu_all_failed(&self) -> String {
        "every runnable CPU benchmark suite failed".to_owned()
    }
}

impl MemoryWords for English {
    fn memory_sysbench(
        &self,
        version: Option<&str>,
        seconds: u64,
        single: &str,
        per_thread: Option<&str>,
    ) -> String {
        let mut text = format!(
            "{}sequential reads and writes, {seconds} s each; one block of {single} for one \
             thread",
            self::version(version)
        );
        if let Some(per_thread) = per_thread {
            text.push_str(&format!(", one of {per_thread} per thread for several"));
        }
        text
    }

    fn memory_stream(
        &self,
        version: Option<&str>,
        millions: u64,
        array: u64,
        arrays: u64,
        spread: bool,
    ) -> String {
        let spread = if spread {
            "; the threads spread evenly over the nodes"
        } else {
            ""
        };
        format!(
            "{}{millions} million elements per array ({array} MiB, {arrays} MiB for the three), \
             the fastest of 10 passes each{spread}",
            self::version(version)
        )
    }

    fn latency_label(&self) -> &'static str {
        "Latency"
    }

    fn latency_method(&self) -> String {
        "multichase: random pointer chasing, 2.5 s per level".to_owned()
    }

    fn numa_method(&self, nodes: usize) -> String {
        format!(
            "{}; bandwidth by multiload's stream-triad, the fastest of 3 samples per cell; \
             latency as above",
            super::count(nodes, "node", "nodes")
        )
    }

    fn results(&self, tool: &str) -> String {
        format!("{tool} results")
    }

    fn numa_title(&self, probe: Probe) -> &'static str {
        match probe {
            Probe::Bandwidth => {
                "NUMA bandwidth (MiB/s; rows: the threads' node, columns: the memory's node)"
            }
            Probe::Latency => "NUMA latency (ns)",
        }
    }

    fn test_header(&self) -> &'static str {
        "Test"
    }

    fn threads_column(&self, threads: usize) -> String {
        if threads > 1 {
            format!("Multi-thread ({threads} threads)")
        } else {
            "Single thread".to_owned()
        }
    }

    fn latency_headers(&self) -> [&'static str; 3] {
        ["Level", "Buffer", "Latency"]
    }

    fn node(&self, node: usize) -> String {
        format!("Node {node}")
    }

    fn operation(&self, write: bool) -> &'static str {
        if write { "Write" } else { "Read" }
    }

    fn memory_level(&self) -> &'static str {
        "Memory"
    }

    fn cache_unknown(&self) -> String {
        "no word on the last-level caches; 32 MiB assumed".to_owned()
    }

    fn stream_rule(&self) -> String {
        "not quite within STREAM's run rules (each array at least 4 times the last-level caches)"
            .to_owned()
    }

    fn sysbench_small(&self, four: &str) -> String {
        format!(
            "available memory short: the test data is below 4 times the last-level caches \
             ({four}), so some reads and writes hit the caches and the results run high"
        )
    }

    fn latency_small(&self, four: &str) -> String {
        format!(
            "available memory short: the memory level's buffer is below 4 times the last-level \
             caches ({four}), so some reads hit the caches and the latency runs low"
        )
    }

    fn levels_missing(&self, levels: &[&str]) -> String {
        match levels {
            [level] => format!("no word on {level}; that level is left out"),
            [others @ .., last] => format!(
                "no word on {} or {last}; those levels are left out",
                others.join(", ")
            ),
            // The reporter writes this note only when a level is left out.
            [] => String::new(),
        }
    }

    fn sysbench_run(&self, write: bool, multi: bool) -> String {
        let threads = if multi {
            "multi-thread"
        } else {
            "single-thread"
        };
        let operation = if write { "write" } else { "read" };
        format!("{threads} {operation}")
    }

    fn stream_run(&self, multi: bool) -> String {
        if multi {
            "multi-thread test".to_owned()
        } else {
            "single-thread test".to_owned()
        }
    }

    fn latency_run(&self, level: &str) -> String {
        format!("{level} test")
    }

    fn numa_run(&self, row: usize, column: usize, probe: Probe) -> String {
        let test = match probe {
            Probe::Bandwidth => "bandwidth",
            Probe::Latency => "latency",
        };
        format!("{test} test of threads on node {row} and memory on node {column}")
    }

    fn no_transfer_rate(&self) -> String {
        "sysbench printed no transfer rate".to_owned()
    }

    fn stream_invalid(&self, line: &str) -> String {
        format!("STREAM's results failed their check: {line}")
    }

    fn no_result(&self, tool: &str) -> String {
        format!("{tool} printed no result")
    }

    fn no_latency(&self) -> String {
        "multichase printed no latency".to_owned()
    }

    fn no_bandwidth(&self) -> String {
        "multiload printed no bandwidth".to_owned()
    }

    fn bind_refused(&self) -> String {
        "the container does not allow binding memory to a node".to_owned()
    }

    fn no_memory_info(&self) -> String {
        "cannot read the available memory (/proc/meminfo)".to_owned()
    }

    fn stream_short(&self, needed: u64, budget: &str) -> String {
        format!(
            "available memory short: the three arrays need at least {needed} MiB, and half of \
             the available memory is only {budget}"
        )
    }

    fn one_node(&self) -> String {
        "needs several NUMA nodes".to_owned()
    }
}

impl DiskWords for English {
    fn health_headers(&self) -> [&'static str; 8] {
        [
            "Device",
            "Health",
            "Power-on",
            "Temp.",
            "Worn",
            "Written",
            "Bad blocks",
            "Link errors",
        ]
    }

    fn card_disk(&self, numbers: &str, host: Option<u32>) -> String {
        match host {
            Some(host) => format!("disk {numbers} of RAID card host{host}"),
            None => format!("RAID card disk {numbers}"),
        }
    }

    fn verdict(&self, verdict: Verdict) -> &'static str {
        match verdict {
            Verdict::Passed => "passed",
            Verdict::Failed => "failed",
            Verdict::Unknown => "unknown",
            Verdict::Unsupported => "unsupported",
            Verdict::ReadFailed => "read failed",
        }
    }

    fn hours(&self, hours: u64) -> String {
        format!("{hours} h")
    }

    fn disk_read_failed(&self, disk: &str, reason: &str) -> String {
        format!("{disk}: read failed: {reason}")
    }

    fn no_smart(&self, disk: &str) -> String {
        format!("{disk} does not support SMART")
    }

    fn smart_off(&self, disk: &str) -> String {
        format!("{disk} has SMART turned off, and this program does not turn it on")
    }

    fn disk_read_timed_out(&self, seconds: u64) -> String {
        format!("still reading after {seconds} s, stopped")
    }

    fn unparsable(&self) -> String {
        "smartctl's output cannot be parsed".to_owned()
    }

    fn unopened(&self) -> String {
        "cannot open the device, or read its identity".to_owned()
    }

    fn arguments_refused(&self) -> String {
        "smartctl refused its arguments".to_owned()
    }

    fn node_left(&self, path: &Path, error: &str) -> String {
        format!("cannot delete {}: {error}", path.display())
    }

    fn no_disks(&self) -> String {
        "smartctl found no disks".to_owned()
    }

    fn scan_timed_out(&self, seconds: u64) -> String {
        format!("smartctl's scan for disks failed: still running after {seconds} s")
    }

    fn scan_failed(&self, reason: &str) -> String {
        format!("smartctl's scan for disks failed: {reason}")
    }

    fn all_failed(&self) -> String {
        "no disk could be read".to_owned()
    }

    fn fio_method(&self, size: &str) -> (&'static str, String) {
        (
            "Method",
            format!("as YABS: random reads and writes, half each, on a test file of {size}"),
        )
    }

    fn zfs_note(&self, needed: u64) -> String {
        format!(
            "the test directory is on ZFS with less than {needed} GiB available, so the results \
             may be off (as YABS warns)"
        )
    }

    fn block_sizes(&self, first: &str, second: &str) -> String {
        format!("{first} and {second}")
    }

    fn block_size_header(&self) -> &'static str {
        "Block size"
    }

    fn fio_row(&self, row: FioRow) -> &'static str {
        match row {
            FioRow::Read => "Read",
            FioRow::Write => "Write",
            FioRow::Total => "Total",
        }
    }

    fn rate(&self, speed: &str, iops: &str) -> String {
        format!("{speed} ({iops})")
    }

    fn fio_timed_out(&self) -> &'static str {
        "timed out"
    }

    fn fio_error(&self) -> &'static str {
        "error"
    }

    fn block_timed_out(&self, block_size: &str, seconds: u64) -> String {
        format!("{block_size} still running after {seconds} s, stopped")
    }

    fn block_failed(&self, block_size: &str, reason: &str) -> String {
        format!("{block_size} failed: {reason}")
    }

    fn no_space_info(&self) -> String {
        "cannot read the free space of the test directory".to_owned()
    }

    fn in_memory(&self) -> String {
        "the test directory is on a RAM disk, which would measure memory; run in a directory on \
         a disk, or name one with --work-dir"
            .to_owned()
    }

    fn no_space(&self, needed: &str, available: &str) -> String {
        format!("the test directory has less than {needed} free (only {available})")
    }

    fn no_direct_io(&self) -> String {
        "the test directory's filesystem does not support direct I/O".to_owned()
    }

    fn setup_failed(&self, reason: &str) -> String {
        format!("cannot create the test file: {reason}")
    }

    fn no_block_size(&self) -> String {
        "none of the four block sizes has a result".to_owned()
    }

    fn no_output(&self, tool: &str) -> String {
        format!("{tool} printed nothing")
    }

    fn fio_reported(&self, error: &str) -> String {
        format!("fio reported error {error}")
    }

    fn fio_field(&self, field: usize) -> String {
        format!("cannot parse field {field} of fio's output")
    }
}

impl NetworkWords for English {
    fn iperf3_method(
        &self,
        version: Option<&str>,
        streams: u8,
        seconds: u16,
        rate: Option<u32>,
    ) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        let streams = super::count(usize::from(streams), "stream", "streams");
        let rate = rate.map_or_else(String::new, |rate| {
            format!(", at most {rate} Mbit/s a stream")
        });
        format!("iperf3{version}: {streams}, {seconds} s each way, TCP{rate}")
    }

    fn family_title(&self, ipv6: bool) -> &'static str {
        if ipv6 { "IPv6" } else { "IPv4" }
    }

    fn network_headers(&self) -> [&'static str; 5] {
        ["Location", "Provider", "Upload", "Download", "Latency"]
    }

    fn place(&self, location: &str, country: &str) -> String {
        format!("{location}, {country}")
    }

    fn way_cell(&self, cell: WayCell) -> &'static str {
        match cell {
            WayCell::Busy => "busy",
            WayCell::Unreachable => "unreachable",
            WayCell::TimedOut => "timed out",
            WayCell::Error => "error",
            WayCell::NotTried => "not tried",
        }
    }

    fn tries(&self, place: &str, download: bool, tries: u32) -> String {
        let direction = if download { "download" } else { "upload" };
        format!("{place}: the {direction} took {tries} tries")
    }

    fn way_failed(&self, place: &str, download: bool, reason: &str) -> String {
        let direction = if download { "download" } else { "upload" };
        format!("{place}: the {direction} failed: {reason}")
    }

    fn nothing_measured(&self) -> String {
        "iperf3 measured nothing".to_owned()
    }

    fn unknown_servers(&self, names: &[String]) -> String {
        format!("no server in the list for {}", names.join(", "))
    }

    fn list_unreadable(&self, reason: &str) -> String {
        format!("cannot read the server list: {reason}")
    }

    fn list_too_large(&self, limit: usize) -> String {
        format!("the server list is larger than {limit} bytes")
    }

    fn list_format(&self, reason: &str) -> String {
        format!("the server list is not in its format: {reason}")
    }

    fn list_newer(&self, version: u64) -> String {
        format!("the server list needs a newer lowendaff-bench (format {version})")
    }

    fn list_version(&self, version: u64) -> String {
        format!("the server list names a format that does not exist: {version}")
    }

    fn list_too_many(&self, count: usize, most: usize) -> String {
        format!("the server list has {count} servers, more than {most}")
    }

    fn list_rule(&self, position: usize, rule: ListRule) -> String {
        let rule = match rule {
            ListRule::Id => "its id must be 1 to 40 characters of a to z, digits and -",
            ListRule::Country => "its country must be two capital letters",
            ListRule::Host => "its host must be a DNS name or an IP address",
            ListRule::Ports => "its ports must be one or a range from 1024 to 65535, at most 64",
            ListRule::Families => "its families must be ipv4, ipv6 or both",
            ListRule::Text => {
                "its location, provider and link must be 1 to 40 characters that print"
            }
        };
        format!("server {position} of the server list breaks a rule: {rule}")
    }

    fn list_duplicate(&self, id: &str) -> String {
        format!("two servers of the server list have the id {id}")
    }

    fn no_server(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "no server of the list is chosen".to_owned()
        } else {
            format!("no server in the list for {}", unknown.join(", "))
        }
    }

    fn no_family(&self) -> String {
        "no chosen server answers over a network family of this host".to_owned()
    }

    fn no_speed(&self) -> String {
        "no server gave a speed".to_owned()
    }
}

impl TraceWords for English {
    fn trace_method(&self, version: &str, choice: &TraceChoice) -> String {
        let mut method = format!("NextTrace {version}: ICMP, 3 probes a hop, at most 30 hops");
        if !choice.groups.is_empty() {
            method.push_str(&format!("; groups {}", choice.groups.join(", ")));
        }
        if !choice.targets.is_empty() {
            method.push_str(&format!("; targets {}", choice.targets.join(", ")));
        }
        if choice.list {
            method.push_str("; another target list");
        }
        if !choice.geo {
            method.push_str("; without networks and places");
        }
        method
    }

    fn trace_heading(&self, name: &str, host: Option<&str>, outcome: &str) -> String {
        match host {
            Some(host) => format!("{name} ({host}): {outcome}"),
            None => format!("{name}: {outcome}"),
        }
    }

    fn youtube(&self) -> &'static str {
        "YouTube"
    }

    fn netflix(&self, place: Option<(&str, &str)>) -> String {
        match place {
            Some((city, country)) => format!("Netflix, {city}, {country}"),
            None => "Netflix".to_owned(),
        }
    }

    fn reached(&self, hops: usize) -> String {
        format!("reached in {}", super::count(hops, "hop", "hops"))
    }

    fn not_reached(&self, hops: usize) -> String {
        format!("not reached after {}", super::count(hops, "hop", "hops"))
    }

    fn trace_headers(&self) -> [&'static str; 5] {
        ["Hop", "Address", "Network", "Place", "Latency"]
    }

    fn at_exchange(&self, network: Option<&str>, exchange: &str) -> String {
        match network {
            Some(network) => format!("{network} [{exchange}]"),
            None => format!("[{exchange}]"),
        }
    }

    fn unavailable_lookup(&self, reason: &str) -> String {
        format!("unavailable: {reason}")
    }

    fn lookup_problem(&self, problem: LookupProblem) -> String {
        match problem {
            LookupProblem::NoServer => "Google's answer names no server",
            LookupProblem::Id => {
                "Google's answer names a server whose id cannot be written as a host name"
            }
            LookupProblem::Format => "fast.com's answer is not its API's",
            LookupProblem::NoTarget => "fast.com's answer names no server",
            LookupProblem::Url => {
                "fast.com's answer names a server at an address that cannot be traced"
            }
            LookupProblem::Place => "fast.com's answer names a server's place that cannot be shown",
        }
        .to_owned()
    }

    fn answer_unreadable(&self, reason: &str) -> String {
        format!("cannot read the answer: {reason}")
    }

    fn traced_nothing(&self) -> String {
        "NextTrace traced nothing".to_owned()
    }

    fn unknown_targets(&self, ids: &[String]) -> String {
        format!("no target in the list for {}", ids.join(", "))
    }

    fn no_exchanges(&self, reason: &str) -> String {
        format!("no exchange names: {reason}")
    }

    fn trace_list_problem(&self, list: TraceList, problem: TraceListProblem<'_>) -> String {
        let (name, entry, entries) = match list {
            TraceList::Targets => ("the target list", "target", "targets"),
            TraceList::Exchanges => ("the list of exchange ports", "port", "ports"),
        };
        match problem {
            TraceListProblem::Unreadable(reason) => format!("cannot read {name}: {reason}"),
            TraceListProblem::TooLarge(limit) => format!("{name} is larger than {limit} bytes"),
            TraceListProblem::Format(reason) => format!("{name} is not in its format: {reason}"),
            TraceListProblem::Newer(version) => {
                format!("{name} needs a newer lowendaff-bench (format {version})")
            }
            TraceListProblem::Version(version) => {
                format!("{name} names a format that does not exist: {version}")
            }
            TraceListProblem::TooMany(count, most) => {
                format!("{name} has {count} {entries}, more than {most}")
            }
            TraceListProblem::Rule(position, rule) => {
                let rule = match rule {
                    TraceRule::Id => {
                        "its id must be 1 to 40 characters of a to z, digits and -, other than \
                         youtube and netflix"
                    }
                    TraceRule::Group => "its group must be china or international",
                    TraceRule::Name => "its name must be 1 to 40 characters that print",
                    TraceRule::Asn => "its AS number must be from 1 to 4294967295",
                    TraceRule::Ipv4 => "its ipv4 must be a DNS name or an IPv4 address",
                    TraceRule::Ipv6 => "its ipv6 must be a DNS name or an IPv6 address",
                    TraceRule::Address => {
                        "its address must be an IP address as it prints, an IPv6 address \
                         without an IPv4 part"
                    }
                    TraceRule::Exchange => "its exchange must be 1 to 60 characters that print",
                };
                format!("{entry} {position} of {name} breaks a rule: {rule}")
            }
            TraceListProblem::Duplicate(key) => match list {
                TraceList::Targets => format!("two targets of {name} have the id {key}"),
                TraceList::Exchanges => format!("two ports of {name} have the address {key}"),
            },
        }
    }

    fn no_target(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "no target of the list is chosen".to_owned()
        } else {
            format!("no target in the list for {}", unknown.join(", "))
        }
    }

    fn no_target_family(&self) -> String {
        "no chosen target has a name of a network family of this host".to_owned()
    }

    fn no_target_route(&self) -> String {
        "no target got a route".to_owned()
    }
}

impl MediaWords for English {
    fn media_method(&self, choice: &MediaChoice) -> String {
        let mut method = if choice.custom_base {
            "probes via custom base URL, direct (no proxy)".to_owned()
        } else {
            "HTTPS probes, direct (no proxy)".to_owned()
        };
        if !choice.groups.is_empty() {
            method.push_str(&format!("; groups {}", choice.groups.join(", ")));
        }
        if !choice.services.is_empty() {
            method.push_str(&format!("; services {}", choice.services.join(", ")));
        }
        method
    }

    fn media_group(&self, code: &str) -> &'static str {
        match code {
            "global" => "Global",
            "asia" => "Asia",
            "europe" => "Europe",
            "americas" => "Americas",
            "oceania" => "Oceania",
            _ => "Other",
        }
    }

    fn media_headers(&self) -> [&'static str; 3] {
        ["Service", "IPv4", "IPv6"]
    }

    fn media_yes(&self, region: Option<&str>) -> String {
        match region {
            Some(region) => format!("yes ({region})"),
            None => "yes".to_owned(),
        }
    }

    fn media_originals(&self, region: Option<&str>) -> String {
        match region {
            Some(region) => format!("originals only ({region})"),
            None => "originals only".to_owned(),
        }
    }

    fn media_cell(&self, cell: MediaCell, ipv6: bool) -> &'static str {
        match cell {
            MediaCell::No => "no",
            MediaCell::NoChina => "no (CN)",
            MediaCell::ComingSoon => "no (coming soon)",
            MediaCell::ProxyDetected => "no (proxy detected)",
            MediaCell::BlockedWaf => "blocked (WAF)",
            MediaCell::RateLimited => "rate limited",
            MediaCell::NoAddress => {
                if ipv6 {
                    "no IPv6 address"
                } else {
                    "no IPv4 address"
                }
            }
            MediaCell::TimedOut => "timed out",
            MediaCell::Error => "error",
        }
    }

    fn media_unexpected(&self, service: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{service} over {family}: {reason}")
    }

    fn unknown_services(&self, ids: &[String]) -> String {
        format!("no streaming service in the catalog for {}", ids.join(", "))
    }

    fn invalid_base_url(&self, url: &str) -> String {
        format!(
            "base_url must start with https:// or loopback http://127.0.0.1 or http://[::1]: {url}"
        )
    }

    fn no_service(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "no streaming service is chosen".to_owned()
        } else {
            format!(
                "no streaming service in the catalog for {}",
                unknown.join(", ")
            )
        }
    }

    fn no_media_family(&self) -> String {
        "the host reaches neither IPv4 nor IPv6".to_owned()
    }
}

impl IpWords for English {
    fn ip_method(&self, choice: &IpChoice) -> String {
        let mut parts = vec!["9 intelligence sources"];
        if choice.dnsbl {
            parts.push("DNSBLs");
        }
        if choice.mail {
            parts.push("port-25 mail");
        }
        let mut method = format!("Direct checks (no proxy): {}", parts.join(", "));
        if choice.custom_endpoints {
            method.push_str("; custom endpoints");
        }
        method
    }

    fn ip_stage(&self, stage: &str) -> &'static str {
        match stage {
            "intelligence" => "intelligence sources",
            "dnsbl" => "DNSBL blacklists",
            "mail" => "port-25 mail connectivity",
            _ => "reputation",
        }
    }

    fn ip_table(&self, table: IpTable) -> &'static str {
        match table {
            IpTable::Summary => "Summary",
            IpTable::Scores => "Risk scores",
            IpTable::Factors => "Risk factors",
            IpTable::BlacklistsAndMail => "Blacklists & mail (port 25)",
        }
    }

    fn ip_headers(&self, table: IpTable) -> [&'static str; 3] {
        let first = match table {
            IpTable::Summary => "Field",
            IpTable::Scores => "Source",
            IpTable::Factors => "Factor",
            IpTable::BlacklistsAndMail => "Check",
        };
        [first, "IPv4", "IPv6"]
    }

    fn ip_field(&self, field: IpField) -> &'static str {
        match field {
            IpField::Address => "Address",
            IpField::Hostname => "Hostname",
            IpField::Asn => "ASN",
            IpField::Organization => "Organization",
            IpField::Registry => "Registry",
            IpField::Origin => "Origin",
            IpField::UsageType => "Usage type",
            IpField::GeoIp => "GeoIP",
        }
    }

    fn ip_native(&self, country: &str) -> String {
        format!("native ({country})")
    }

    fn ip_broadcast(&self, registered: &str, located: &str) -> String {
        format!("broadcast ({registered} -> {located})")
    }

    fn ip_usage(&self, code: &str) -> &'static str {
        match code {
            "residential" => "residential",
            "hosting" => "hosting",
            "business" => "business",
            "mobile" => "mobile",
            "education" => "education",
            "government" => "government",
            "cdn" => "CDN",
            _ => "unknown",
        }
    }

    fn ip_risk_level(&self, code: &str) -> &'static str {
        match code {
            "low" => "low",
            "medium" => "medium",
            "high" => "high",
            "very_high" => "very high",
            _ => "unknown",
        }
    }

    fn ip_numeric_score(&self, score: u8, max: u8, level: &str) -> String {
        format!("{score}/{max} ({})", self.ip_risk_level(level))
    }

    fn ip_ratio_score(&self, ratio: &str, level: &str) -> String {
        format!("{ratio} ({})", self.ip_risk_level(level))
    }

    fn ip_greynoise(&self, riot: bool, noise: bool, class: &str) -> &'static str {
        if riot {
            return "benign service (RIOT)";
        }
        match class {
            "not_observed" if !noise => "clean (not seen scanning)",
            "benign" => "benign scanner",
            "malicious" => "malicious scanner",
            _ if noise => "observed scanning",
            _ => "clean (not seen scanning)",
        }
    }

    fn ip_shodan(&self, ports: usize, vulns: usize, flagged: bool) -> String {
        let label = if flagged || vulns > 0 {
            "flagged"
        } else {
            "clean"
        };
        let ports_text = super::count(ports, "port", "ports");
        let vulns_text = super::count(vulns, "CVE", "CVEs");
        format!("{label} ({ports_text}, {vulns_text})")
    }

    fn ip_factor_name(&self, code: &str) -> &'static str {
        match code {
            "proxy" => "Proxy",
            "vpn" => "VPN",
            "tor" => "Tor",
            "datacenter" => "Datacenter",
            "abuser" => "Abuser",
            "robot" => "Robot",
            "bogon" => "Bogon",
            _ => "Other",
        }
    }

    fn ip_factor_cell(&self, checked: usize, flagged: &[&str]) -> String {
        if flagged.is_empty() {
            format!("no (0/{checked})")
        } else {
            format!("yes ({}/{checked}: {})", flagged.len(), flagged.join(", "))
        }
    }

    fn ip_dnsbl_label(&self) -> &'static str {
        "DNSBL"
    }

    fn ip_dnsbl_cell(&self, checked: usize, refused: usize, listed: &[&str]) -> String {
        if listed.is_empty() {
            if refused > 0 {
                format!("clean (0/{checked}; {refused} refused)")
            } else {
                format!("clean (0/{checked})")
            }
        } else {
            format!("listed ({}/{checked}: {})", listed.len(), listed.join(", "))
        }
    }

    fn ip_port25_label(&self) -> &'static str {
        "Port 25"
    }

    fn ip_port25_cell(&self, open: bool) -> &'static str {
        if open { "open" } else { "blocked" }
    }

    fn ip_mail_yes(&self, code: u16) -> String {
        format!("yes ({code})")
    }

    fn ip_mail_rejected(&self, code: u16) -> String {
        format!("rejected ({code})")
    }

    fn ip_mail_blocked(&self) -> &'static str {
        "blocked"
    }

    fn ip_unexpected(&self, provider: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{provider} over {family}: {reason}")
    }

    fn ip_no_public_address(&self) -> String {
        "no public IP address was found for any reachable network family".to_owned()
    }

    fn ip_invalid_endpoint(&self, raw: &str) -> String {
        format!(
            "endpoint override must be https:// or a loopback address (127.0.0.1 / [::1]): {raw}"
        )
    }
}

impl BgpWords for English {
    fn bgp_method(&self, custom_endpoints: bool, upload: bool) -> String {
        let mut line = "Direct RIPEstat RIS lookup: BGP state, RPKI and AS-path".to_owned();
        if !upload {
            line.push_str("; without topology image upload");
        }
        if custom_endpoints {
            line.push_str("; custom endpoints");
        }
        line
    }

    fn bgp_summary_title(&self) -> &'static str {
        "Summary"
    }

    fn bgp_summary_headers(&self) -> [&'static str; 3] {
        ["Field", "IPv4", "IPv6"]
    }

    fn bgp_field(&self, field: BgpField) -> &'static str {
        match field {
            BgpField::Address => "Address",
            BgpField::Prefix => "Announced prefix",
            BgpField::OriginAsn => "Origin ASN",
            BgpField::Rpki => "RPKI status",
            BgpField::Visibility => "RIS visibility",
            BgpField::Upstreams => "Direct upstreams",
        }
    }

    fn bgp_not_announced(&self) -> &'static str {
        "not announced"
    }

    fn bgp_origin_cell(&self, asn: u32, name: Option<&str>, total_origins: usize) -> String {
        let mut cell = match name.filter(|s| !s.is_empty()) {
            Some(label) => format!("AS{asn} ({label})"),
            None => format!("AS{asn}"),
        };
        if total_origins > 1 {
            cell.push_str(&format!(" (+{} MOAS)", total_origins - 1));
        }
        cell
    }

    fn bgp_rpki_cell(&self, code: &str) -> &'static str {
        match code {
            "valid" => "valid",
            "invalid" => "invalid",
            "not_found" => "not found (no ROA)",
            _ => "unknown",
        }
    }

    fn bgp_visibility_cell(&self, peers: u32, distinct_paths: usize) -> String {
        let peers_text = super::count(peers as usize, "peer", "peers");
        let paths_text = super::count(distinct_paths, "distinct path", "distinct paths");
        format!("{peers_text}, {paths_text}")
    }

    fn bgp_direct_peering(&self) -> &'static str {
        "direct collector peering"
    }

    fn bgp_topology_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 topology"
        } else {
            "IPv4 topology"
        }
    }

    fn bgp_paths_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 observed paths"
        } else {
            "IPv4 observed paths"
        }
    }

    fn bgp_paths_headers(&self) -> [&'static str; 3] {
        ["Peers", "Hops", "AS_PATH"]
    }

    fn bgp_upload_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} topology image upload failed: {reason}")
    }

    fn bgp_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} BGP lookup failed: {reason}")
    }

    fn bgp_query_failed(&self, reason: &str) -> String {
        format!("BGP route-state lookup failed: {reason}")
    }
}

impl TaierWords for English {
    fn taier_method(&self, choice: &TaierChoice) -> String {
        let streams = super::count(usize::from(choice.streams), "stream", "streams");
        let mut method = format!(
            "CAICT GlobalSpeed (Taier): {streams}, {} s each way, HTTP/1.1",
            choice.seconds
        );
        if !choice.selections.is_empty() {
            const MAX_SHOWN: usize = 5;
            if choice.selections.len() <= MAX_SHOWN {
                method.push_str(&format!("; targets {}", choice.selections.join(", ")));
            } else {
                let head = choice.selections[..MAX_SHOWN].join(", ");
                let rest = choice.selections.len() - MAX_SHOWN;
                method.push_str(&format!("; targets {head} (+{rest} more)"));
            }
        } else {
            if !choice.cities.is_empty() {
                method.push_str(&format!("; cities {}", choice.cities.join(", ")));
            }
            if !choice.operators.is_empty() {
                method.push_str(&format!("; carriers {}", choice.operators.join(", ")));
            }
        }
        if choice.custom_list {
            method.push_str("; custom server list");
        }
        method
    }

    fn taier_unknown_cities(&self, names: &[String]) -> String {
        format!("no location in the Taier list for {}", names.join(", "))
    }

    fn taier_headers(&self) -> [&'static str; 4] {
        ["Location", "Carrier", "Download", "Upload"]
    }

    fn taier_place(&self, city: &str, province: &str) -> String {
        if city.eq_ignore_ascii_case(province) {
            city.to_owned()
        } else {
            format!("{city}, {province}")
        }
    }

    fn taier_operator(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "Telecom",
            "unicom" => "Unicom",
            "mobile" => "Mobile",
            "cernet" => "CERNET",
            "cbn" => "Broadnet",
            _ => "Other",
        }
    }

    fn taier_cell(&self, cell: TaierCell) -> &'static str {
        match cell {
            TaierCell::Busy => "busy",
            TaierCell::Unreachable => "unreachable",
            TaierCell::AuthRejected => "auth rejected",
            TaierCell::TimedOut => "timed out",
            TaierCell::Error => "error",
        }
    }

    fn taier_fallback(&self, place: &str, operator: &str, tried: u8) -> String {
        format!("{place} ({operator}): connected on candidate {tried} after busy/unreachable nodes")
    }

    fn taier_node_failed(&self, place: &str, operator: &str, reason: &str) -> String {
        format!("{place} ({operator}): failed: {reason}")
    }

    fn taier_list_problem(&self, problem: TaierListProblem<'_>) -> String {
        match problem {
            TaierListProblem::Missing => "the Taier server list is missing".to_owned(),
            TaierListProblem::Unreadable(reason) => {
                format!("cannot read the Taier server list: {reason}")
            }
            TaierListProblem::TooLarge(limit) => {
                format!("the Taier server list is larger than {limit} bytes")
            }
            TaierListProblem::Format(reason) => {
                format!("the Taier server list is not in its format: {reason}")
            }
            TaierListProblem::Newer(version) => {
                format!("the Taier server list needs a newer lowendaff-bench (format {version})")
            }
            TaierListProblem::Version(version) => {
                format!("the Taier server list names a format that does not exist: {version}")
            }
            TaierListProblem::Empty => "the Taier server list has no locations".to_owned(),
            TaierListProblem::TooMany(count, most) => {
                format!("the Taier server list has {count} locations, more than {most}")
            }
            TaierListProblem::Rule(position, rule) => {
                let detail = match rule {
                    TaierRule::Id => {
                        "its id must be 1 to 40 characters of a to z, digits and - (not all)"
                    }
                    TaierRule::Province => {
                        "its province must be 1 to 40 printable ASCII characters"
                    }
                    TaierRule::City => "its city must be 1 to 40 printable ASCII characters",
                    TaierRule::Nodes => "its nodes must hold 1 to 32 entries",
                    TaierRule::Operator => {
                        "a node's operator must be telecom, unicom, mobile, cernet, cbn or other"
                    }
                    TaierRule::Ip => "a node's ip must be a canonical IPv4 address",
                    TaierRule::Port => "a node's port must be from 1024 to 65535",
                    TaierRule::DuplicateEndpoint => {
                        "two nodes in the location share the same ip and port"
                    }
                };
                format!("location {position} of the Taier server list breaks a rule: {detail}")
            }
            TaierListProblem::Duplicate(id) => {
                format!("two locations of the Taier server list have the id {id}")
            }
        }
    }

    fn taier_no_node(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "no carrier node in the Taier list matches the selection".to_owned()
        } else {
            format!(
                "no carrier node in the Taier list for {}",
                unknown.join(", ")
            )
        }
    }

    fn taier_no_result(&self) -> String {
        "no China carrier node gave a speed result".to_owned()
    }
}

impl CloudflareWords for English {
    fn cloudflare_method(&self, choice: &CloudflareChoice) -> String {
        let workers = super::count(usize::from(choice.concurrency), "worker", "workers");
        let mut method = format!(
            "Cloudflare Speed: {workers}, {} s each way, HTTPS",
            choice.seconds
        );
        if choice.udp {
            method.push_str(&format!(" + STUN UDP ({} probes)", choice.udp_packets));
        } else {
            method.push_str("; without STUN UDP probe");
        }
        if choice.custom_endpoints {
            method.push_str("; custom endpoints");
        }
        method
    }

    fn cloudflare_headers(&self) -> [&'static str; 3] {
        ["Metric", "IPv4", "IPv6"]
    }

    fn cloudflare_field(&self, field: CloudflareField) -> &'static str {
        match field {
            CloudflareField::Server => "Edge server",
            CloudflareField::ClientIp => "Client IP",
            CloudflareField::ClientAsn => "Client ASN",
            CloudflareField::IdleLatency => "Idle latency",
            CloudflareField::Download => "Download",
            CloudflareField::DownloadLatency => "Download latency",
            CloudflareField::Upload => "Upload",
            CloudflareField::UploadLatency => "Upload latency",
            CloudflareField::UdpLoss => "UDP packet loss",
            CloudflareField::Quality => "Connection quality",
        }
    }

    fn cloudflare_latency_cell(
        &self,
        median: &str,
        min: &str,
        max: &str,
        jitter: Option<&str>,
        loss_pct: f64,
    ) -> String {
        let mut text = format!("{median} (min {min}, max {max}");
        if let Some(jit) = jitter {
            text.push_str(&format!(", jitter {jit}"));
        }
        if loss_pct > 0.0 {
            text.push_str(&format!(", loss {loss_pct:.1}%"));
        }
        text.push(')');
        text
    }

    fn cloudflare_speed_cell(&self, speed: &str, p50_mbps: f64, p95_mbps: f64) -> String {
        format!("{speed} (p50 {p50_mbps:.1}, p95 {p95_mbps:.1} Mbit/s)")
    }

    fn cloudflare_udp_quality(&self, code: &str) -> &'static str {
        match code {
            "excellent" => "Excellent",
            "good" => "Good",
            "acceptable" => "Acceptable",
            "poor" => "Poor",
            "bad" => "Bad",
            _ => "Unknown",
        }
    }

    fn cloudflare_udp_cell(
        &self,
        loss_pct: f64,
        received: u64,
        sent: u64,
        reordered_pct: f64,
        mos: Option<f64>,
        quality: &str,
    ) -> String {
        let mut text = format!("{loss_pct:.1}% ({received}/{sent}");
        if reordered_pct > 0.0 {
            text.push_str(&format!(", {reordered_pct:.1}% reordered"));
        }
        if let Some(m) = mos {
            text.push_str(&format!(", MOS {m:.2}"));
        }
        text.push_str(&format!(", {quality})"));
        text
    }

    fn cloudflare_quality_cell(
        &self,
        bloat_grade: &str,
        bloat_ms: Option<f64>,
        stab_grade: &str,
        stab_cv: Option<f64>,
    ) -> String {
        let bloat = match bloat_ms {
            Some(ms) => format!("bufferbloat {bloat_grade} (+{ms:.1} ms)"),
            None => format!("bufferbloat {bloat_grade}"),
        };
        let stab = match stab_cv {
            Some(cv) => format!("stability {stab_grade} (CV {cv:.1}%)"),
            None => format!("stability {stab_grade}"),
        };
        format!("{bloat}, {stab}")
    }

    fn cloudflare_issue(&self, issue: CloudflareIssueRef<'_>) -> String {
        match issue {
            CloudflareIssueRef::EmptyDownload => "download measured 0 bytes".to_owned(),
            CloudflareIssueRef::EmptyUpload => "upload measured 0 bytes".to_owned(),
            CloudflareIssueRef::NoAddress => "no address for family".to_owned(),
            CloudflareIssueRef::TimedOut => "timed out".to_owned(),
            CloudflareIssueRef::NoResolver => "no DNS resolver".to_owned(),
            CloudflareIssueRef::Transport(why) => why.to_owned(),
        }
    }

    fn cloudflare_udp_note(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} STUN UDP probe unavailable: {reason}")
    }

    fn cloudflare_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} Cloudflare speed test failed: {reason}")
    }

    fn cloudflare_no_result(&self, reason: &str) -> String {
        if reason.is_empty() {
            "Cloudflare speed test produced no result".to_owned()
        } else {
            format!("Cloudflare speed test failed: {reason}")
        }
    }
}

impl InetspeedWords for English {
    fn inetspeed_method(&self, choice: &InetspeedChoice) -> String {
        let rounds = if choice.threads <= 1 {
            "1 thread".to_owned()
        } else {
            format!("1 & {} threads", choice.threads)
        };
        let mut method = format!(
            "Apple CDN (iNetSpeed): {rounds}, {} / {} s cap, {} idle probes",
            choice.max, choice.timeout_secs, choice.latency_count
        );
        if choice.forced_endpoint {
            method.push_str("; pinned endpoint");
        }
        if choice.custom_endpoints {
            method.push_str("; custom endpoints");
        }
        method
    }

    fn inetspeed_endpoint_label(&self) -> &'static str {
        "Selected endpoint"
    }

    fn inetspeed_endpoint_value(
        &self,
        host: &str,
        ip: Option<&str>,
        rtt_ms: Option<f64>,
        source: &str,
    ) -> String {
        let src = self.inetspeed_source(source);
        match (ip, rtt_ms) {
            (Some(addr), Some(rtt)) => format!("{addr} ({host}, {rtt:.2} ms via {src})"),
            (Some(addr), None) => format!("{addr} ({host}, via {src})"),
            (None, _) if host.is_empty() => format!("unpinned ({src})"),
            (None, _) => format!("{host} (unpinned, {src})"),
        }
    }

    fn inetspeed_source(&self, code: &str) -> &'static str {
        match code {
            "user" => "pinned",
            "doh" => "Dual DoH",
            "system_dns" => "system DNS",
            _ => "default DNS",
        }
    }

    fn inetspeed_idle_label(&self) -> &'static str {
        "Idle latency"
    }

    fn inetspeed_latency_value(
        &self,
        median_ms: f64,
        min_ms: f64,
        avg_ms: f64,
        max_ms: f64,
        jitter_ms: f64,
    ) -> String {
        format!(
            "{median_ms:.2} ms median (min {min_ms:.2} / avg {avg_ms:.2} / max {max_ms:.2} ms, \
             jitter {jitter_ms:.2} ms)"
        )
    }

    fn inetspeed_total_label(&self) -> &'static str {
        "Data transferred"
    }

    fn inetspeed_headers(&self) -> [&'static str; 5] {
        ["Round", "Threads", "Speed", "Data", "Loaded latency"]
    }

    fn inetspeed_round_name(&self, download: bool, single: bool) -> &'static str {
        match (download, single) {
            (true, true) => "Download (single)",
            (true, false) => "Download (multi)",
            (false, true) => "Upload (single)",
            (false, false) => "Upload (multi)",
        }
    }

    fn inetspeed_loaded_cell(&self, median_ms: f64, jitter_ms: f64) -> String {
        format!("{median_ms:.2} ms (jitter {jitter_ms:.2} ms)")
    }

    fn inetspeed_round_issue(&self, issue: InetspeedRoundIssueRef<'_>) -> String {
        match issue {
            InetspeedRoundIssueRef::ZeroBytes => "transfer completed with 0 bytes".to_owned(),
            InetspeedRoundIssueRef::WorkerFaults(count) => format!(
                "{} reported transport faults",
                super::count(usize::from(count), "worker", "workers")
            ),
            InetspeedRoundIssueRef::Transport(fault) => inetspeed_fault_text(fault),
        }
    }

    fn inetspeed_warning(&self, warning: InetspeedWarningRef<'_>) -> String {
        match warning {
            InetspeedWarningRef::HostMismatch => {
                "download, upload, and latency hosts differ; shared endpoint pinning disabled"
                    .to_owned()
            }
            InetspeedWarningRef::ForcedProbeFailed(ip) => {
                format!("forced endpoint {ip} probe failed; continuing with requested IP")
            }
            InetspeedWarningRef::DohEmpty => {
                "Dual DoH returned no candidate IPs; falling back to system DNS".to_owned()
            }
            InetspeedWarningRef::UnpinnedFallback => {
                "endpoint discovery found no IP; continuing with unpinned DNS".to_owned()
            }
            InetspeedWarningRef::NoIdleSamples => "no idle latency samples collected".to_owned(),
            InetspeedWarningRef::IdleFailed(fault) => {
                format!("idle latency failed: {}", inetspeed_fault_text(fault))
            }
        }
    }

    fn inetspeed_invalid_max(&self, raw: &str) -> String {
        format!("invalid transfer cap {raw:?}: expected a size such as 2G, 500M, or 1GiB")
    }

    fn inetspeed_no_result(&self) -> String {
        "Apple CDN speed test completed no transfer round".to_owned()
    }
}

fn inetspeed_fault_text(fault: InetspeedProbeFaultRef<'_>) -> String {
    match fault {
        InetspeedProbeFaultRef::EmptySample => "probe returned no sample".to_owned(),
        InetspeedProbeFaultRef::NoAddress => "no address for family".to_owned(),
        InetspeedProbeFaultRef::TimedOut => "timed out".to_owned(),
        InetspeedProbeFaultRef::NoResolver => "no DNS resolver".to_owned(),
        InetspeedProbeFaultRef::Transport(why) => why.to_owned(),
    }
}
