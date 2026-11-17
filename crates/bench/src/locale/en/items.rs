//! English: the test items' chapters.

use std::path::Path;

use super::English;
use super::count;
use crate::locale::CpuWords;
use crate::locale::DiskWords;
use crate::locale::FioRow;
use crate::locale::ListRule;
use crate::locale::MemoryWords;
use crate::locale::NetworkWords;
use crate::locale::NoSwapWhy;
use crate::locale::Probe;
use crate::locale::Verdict;
use crate::locale::WayCell;

/// A tool's version followed by the colon that leads its parameters; nothing without a version.
fn version(version: Option<&str>) -> String {
    version.map_or_else(String::new, |version| format!("{version}: "))
}

impl CpuWords for English {
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
            count(nodes, "node", "nodes")
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
        let streams = count(usize::from(streams), "stream", "streams");
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
