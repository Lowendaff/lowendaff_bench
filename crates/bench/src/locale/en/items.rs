//! English: the test items' chapters.

use std::path::Path;

use super::English;
use super::count;
use crate::locale::BgpField;
use crate::locale::BgpWords;
use crate::locale::CpuWords;
use crate::locale::DiskWords;
use crate::locale::FioRow;
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
        format!("reached in {}", count(hops, "hop", "hops"))
    }

    fn not_reached(&self, hops: usize) -> String {
        format!("not reached after {}", count(hops, "hop", "hops"))
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
        let mut method = "HTTPS probes, direct (no proxy)".to_owned();
        if !choice.groups.is_empty() {
            method.push_str(&format!("; groups {}", choice.groups.join(", ")));
        }
        if !choice.services.is_empty() {
            method.push_str(&format!("; services {}", choice.services.join(", ")));
        }
        if choice.custom_base {
            method.push_str("; custom base URL");
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
        let ports_text = count(ports, "port", "ports");
        let vulns_text = count(vulns, "CVE", "CVEs");
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
    fn bgp_method(&self, custom_endpoints: bool) -> String {
        let mut line = "Direct RIPEstat RIS lookup: BGP state, RPKI and AS-path".to_owned();
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
        let peers_text = count(peers as usize, "peer", "peers");
        let paths_text = count(distinct_paths, "distinct path", "distinct paths");
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

    fn bgp_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} BGP lookup failed: {reason}")
    }

    fn bgp_query_failed(&self, reason: &str) -> String {
        format!("BGP route-state lookup failed: {reason}")
    }
}
