//! The words of the test items' chapters: CPU, memory, disks and network, route tracing among it.

use std::path::Path;

/// Why no temporary swap was added for Geekbench, with sizes as the report writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoSwapWhy<'a> {
    /// `vm.swappiness` is 0, so the kernel would not swap memory out.
    Unused,
    /// The run directory's disk has this much free, in GiB, or its free space is unknown.
    NoRoom(Option<&'a str>),
    /// Not permitted, as in most containers.
    Denied,
    /// The run directory's filesystem does not support swap files.
    Unsupported,
    /// Another error, in the system's words.
    Error(&'a str),
}

/// What the latency and the NUMA chapters run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Bandwidth.
    Bandwidth,
    /// Latency.
    Latency,
}

/// The CPU chapters' words: unified `cpu.bench` and its eight sub-suites.
pub trait CpuWords {
    /// Progress log phrase for one `cpu.bench` sub-suite or stage.
    fn cpu_suite_step(&self, suite: &str, stage: Option<&str>, threads: Option<usize>) -> String;
    /// Line naming non-default `--cpu-bench` suites chosen by the user.
    fn cpu_selected_suites(&self, suites: &[String]) -> String;
    /// Subheading title of a sub-suite inside `[CPU performance]`.
    fn cpu_suite_title(&self, suite: &str) -> &'static str;
    /// Note or status line when one sub-suite inside `cpu.bench` was skipped.
    fn cpu_suite_skipped(&self, suite: &str, why: &str) -> String;
    /// Note or status line when one sub-suite inside `cpu.bench` failed.
    fn cpu_suite_failed(&self, suite: &str, why: &str) -> String;
    /// Reason when an `x86_64`-only sub-suite is skipped on `arch`.
    fn cpu_skip_arch(&self, arch: &str) -> String;
    /// Reason when a host lacks a shared runtime library such as `libstdc++.so.6`.
    fn cpu_missing_lib(&self, lib: &str) -> String;
    /// Reason when creating a sub-suite's work files failed.
    fn cpu_setup_failed(&self, reason: &str) -> String;
    /// sysbench's line: its version when it printed one, the prime limit and the seconds of each
    /// test.
    fn sysbench_parameters(&self, version: Option<&str>, max_prime: u64, seconds: u64) -> String;
    /// The headers of sysbench's table: the test, its threads, and the events per second.
    fn cpu_headers(&self) -> [&'static str; 3];
    /// A run's row: one thread, or all of them.
    fn cpu_row(&self, multi: bool) -> &'static str;
    /// A run's name in a note: the test with one thread, or with all of them.
    fn cpu_test(&self, multi: bool) -> &'static str;
    /// The label of how many times the single-thread rate the multi-thread one is.
    fn ratio_label(&self) -> &'static str;
    /// How many times the single-thread rate the multi-thread one is.
    fn ratio(&self, ratio: f64) -> String;
    /// An attempt of sysbench that printed no rate.
    fn no_rate(&self) -> String;
    /// 7-Zip's method line.
    fn seven_zip_parameters(&self, version: Option<&str>) -> String;
    /// 7-Zip's table headers (`Test`, `Threads`, `Compress (MIPS)`, `Decompress (MIPS)`,
    /// `Total (MIPS)`).
    fn seven_zip_headers(&self) -> [&'static str; 5];
    /// 7-Zip exited with 0 without printing MIPS ratings.
    fn seven_zip_no_scores(&self) -> String;
    /// UnixBench's method line.
    fn unixbench_parameters(&self, version: &str) -> String;
    /// UnixBench System Index field label for `copies` parallel copies.
    fn unixbench_index_label(&self, copies: usize) -> String;
    /// UnixBench 12-item table headers (`Test`, `1-copy`, optional `<N>-copy`, `Unit`, `1-idx`,
    /// optional `<N>-idx`).
    fn unixbench_headers(&self, multi_copies: Option<usize>) -> Vec<String>;
    /// Human-readable name of a UnixBench System Index item.
    fn unixbench_item_name(&self, id: &str) -> &'static str;
    /// UnixBench sub-item exited with 0 without printing a `COUNT|` reading.
    fn unixbench_no_count(&self, item: &str) -> String;
    /// HPL's method line (`HPL 2.3 (OpenBLAS): N=..., NB=..., ... threads`).
    fn hpl_parameters(&self, version: Option<&str>, n: u32, nb: u32, threads: usize) -> String;
    /// HPL field labels (`Throughput`, `Solve time`, `Residual check`).
    fn hpl_labels(&self) -> [&'static str; 3];
    /// HPL throughput value (`123.45 GFLOPS`).
    fn hpl_gflops(&self, gflops: f64) -> String;
    /// HPL solve duration (`4.12 s`).
    fn hpl_seconds(&self, seconds: f64) -> String;
    /// HPL residual verification cell (`passed (...)`).
    fn hpl_residual_passed(&self, residual: Option<f64>) -> String;
    /// HPL failed its scaled residual check.
    fn hpl_residual_failed(&self) -> String;
    /// PassMark's method line.
    fn passmark_parameters(&self, version: Option<&str>) -> String;
    /// PassMark top-level labels (`CPU Mark`, `Single-thread`).
    fn passmark_summary_labels(&self) -> [&'static str; 2];
    /// PassMark sub-test breakdown headers (`Subtest`, `Score`, `Unit`).
    fn passmark_headers(&self) -> [&'static str; 3];
    /// PassMark's eight CPU subtest names and units.
    fn passmark_subtests(&self) -> [(&'static str, &'static str); 8];
    /// PassMark produced no `results_cpu.yml` scores.
    fn passmark_no_scores(&self) -> String;
    /// y-cruncher's method line.
    fn ycruncher_parameters(
        &self,
        version: Option<&str>,
        digits: u64,
        tuning: Option<&str>,
    ) -> String;
    /// y-cruncher field labels (`Computation time`, `Wall time`, `CPU utilization`,
    /// `Multi-core eff.`).
    fn ycruncher_labels(&self) -> [&'static str; 4];
    /// y-cruncher failed its Pi digit spot check.
    fn ycruncher_validation_failed(&self) -> String;
    /// Prime95's method line.
    fn prime95_parameters(
        &self,
        version: Option<&str>,
        workers: usize,
        duration_secs: u64,
        fft: Option<&str>,
    ) -> String;
    /// Prime95 field labels (`Stability`, `Completed tests`).
    fn prime95_labels(&self) -> [&'static str; 2];
    /// Prime95 passed value (`passed (0 errors, 0 warnings)`).
    fn prime95_passed(&self, warnings: u64) -> String;
    /// Prime95 detected a hardware or rounding failure.
    fn prime95_hardware_error(&self, detail: &str) -> String;
    /// Prime95 exited without printing worker torture summaries.
    fn prime95_no_summary(&self) -> String;
    ///Geekbench's labels: single-core, multi-core and the result link.
    fn geekbench_labels(&self) -> [&'static str; 3];
    /// Swap added for Geekbench: the memory it needs in GiB, what was available, and the GiB
    /// of swap added.
    fn swapped(&self, need: u64, available: &str, swap: u64) -> String;
    /// Memory that may be short for Geekbench, and why no swap could help.
    fn memory_short(&self, need: u64, available: &str, why: NoSwapWhy<'_>) -> String;
    /// Geekbench could not be made the first to go when memory runs out.
    fn first_to_go_not_set(&self, error: &str) -> String;
    /// The program could not take back its own `oom_score_adj`.
    fn oom_not_restored(&self, error: &str) -> String;
    /// Geekbench gave scores after this many failed uploads; the last one's reason.
    fn uploads_retried(&self, count: usize, last: &str) -> String;
    /// The temporary swap could not be turned off.
    fn swap_left(&self, error: &str, path: &Path) -> String;
    /// The upload failed, without which the free version gives no scores.
    fn upload_failed(&self, reason: &str) -> String;
    /// Geekbench printed no scores.
    fn no_scores(&self) -> String;
    /// Geekbench ran out of memory and the kernel ended it.
    fn out_of_memory(&self) -> String;
    /// Stop reason when no CPU sub-suite was enabled.
    fn cpu_no_suites_selected(&self) -> String;
    /// Stop reason when all selected CPU sub-suites were skipped.
    fn cpu_all_skipped(&self) -> String;
    /// Stop reason when all runnable CPU sub-suites failed.
    fn cpu_all_failed(&self) -> String;
}

/// The memory chapters' words: sysbench, STREAM, latency and NUMA.
pub trait MemoryWords {
    /// sysbench's parameters: the seconds of each test and the blocks, of one thread and of each
    /// thread when there are several.
    fn memory_sysbench(
        &self,
        version: Option<&str>,
        seconds: u64,
        single: &str,
        per_thread: Option<&str>,
    ) -> String;
    /// STREAM's parameters: the elements of each array in millions, its MiB and the three
    /// arrays' MiB, and whether the threads spread over the nodes.
    fn memory_stream(
        &self,
        version: Option<&str>,
        millions: u64,
        array: u64,
        arrays: u64,
        spread: bool,
    ) -> String;
    /// The latency chapter's label.
    fn latency_label(&self) -> &'static str;
    /// What the latency chapter runs.
    fn latency_method(&self) -> String;
    /// What the NUMA chapter runs, on this many nodes.
    fn numa_method(&self, nodes: usize) -> String;
    /// A chapter's table title: the tool's results.
    fn results(&self, tool: &str) -> String;
    /// The NUMA tables' titles.
    fn numa_title(&self, probe: Probe) -> &'static str;
    /// The header of a table's first column.
    fn test_header(&self) -> &'static str;
    /// The header of a column of runs with one thread, or with `threads` of them.
    fn threads_column(&self, threads: usize) -> String;
    /// The latency table's headers: the level, the buffer and the latency.
    fn latency_headers(&self) -> [&'static str; 3];
    /// A node, in the NUMA tables.
    fn node(&self, node: usize) -> String;
    /// A row of reads, or of writes.
    fn operation(&self, write: bool) -> &'static str;
    /// The memory level of the latency table; the cache levels keep their names.
    fn memory_level(&self) -> &'static str;
    /// The note when the kernel describes no caches.
    fn cache_unknown(&self) -> String;
    /// The note when STREAM's arrays are below four times the last-level caches.
    fn stream_rule(&self) -> String;
    /// The note when sysbench's data is below four times the last-level caches.
    fn sysbench_small(&self, four: &str) -> String;
    /// The note when the memory level's buffer is below four times the last-level caches.
    fn latency_small(&self, four: &str) -> String;
    /// The note on the cache levels left out of the latency table.
    fn levels_missing(&self, levels: &[&str]) -> String;
    /// The name of a sysbench run: reading or writing, with one thread or with several.
    fn sysbench_run(&self, write: bool, multi: bool) -> String;
    /// The name of a STREAM run, with one thread or with several.
    fn stream_run(&self, multi: bool) -> String;
    /// The name of a latency run, by its level's name.
    fn latency_run(&self, level: &str) -> String;
    /// The name of a run of the NUMA matrix.
    fn numa_run(&self, row: usize, column: usize, probe: Probe) -> String;
    /// An attempt of sysbench that printed no rate.
    fn no_transfer_rate(&self) -> String;
    /// STREAM's results that failed its own check.
    fn stream_invalid(&self, line: &str) -> String;
    /// A tool that printed no result.
    fn no_result(&self, tool: &str) -> String;
    /// multichase, which printed no latency.
    fn no_latency(&self) -> String;
    /// multiload, which printed no bandwidth.
    fn no_bandwidth(&self) -> String;
    /// A container that does not allow binding memory to a node.
    fn bind_refused(&self) -> String;
    /// The available memory, which could not be read.
    fn no_memory_info(&self) -> String;
    /// Too little memory for STREAM's smallest arrays: the MiB they need, and half of the
    /// available memory.
    fn stream_short(&self, needed: u64, budget: &str) -> String;
    /// A host with fewer than two usable NUMA nodes.
    fn one_node(&self) -> String;
}

/// The disk chapters' words: disk health (smartctl) and disk performance (fio).
pub trait DiskWords {
    /// The health table's headers.
    fn health_headers(&self) -> [&'static str; 8];
    /// A disk on a RAID card, by its numbers, with the card's host when there are several.
    fn card_disk(&self, numbers: &str, host: Option<u32>) -> String;
    /// SMART's verdict: passed, failed, unknown, unsupported, or a disk that could not be read.
    fn verdict(&self, verdict: Verdict) -> &'static str;
    /// Power-on hours.
    fn hours(&self, hours: u64) -> String;
    /// The note on a disk that could not be read.
    fn disk_read_failed(&self, disk: &str, reason: &str) -> String;
    /// The note on a disk without SMART.
    fn no_smart(&self, disk: &str) -> String;
    /// The note on a disk whose SMART is off.
    fn smart_off(&self, disk: &str) -> String;
    /// A disk's read that ran out of time.
    fn disk_read_timed_out(&self, seconds: u64) -> String;
    /// smartctl's output, which could not be parsed.
    fn unparsable(&self) -> String;
    /// A device that could not be opened or identified.
    fn unopened(&self) -> String;
    /// smartctl, which refused its arguments.
    fn arguments_refused(&self) -> String;
    /// A device node that smartctl created and that could not be removed.
    fn node_left(&self, path: &Path, error: &str) -> String;
    /// smartctl, which found no disks.
    fn no_disks(&self) -> String;
    /// smartctl's scan for disks, which ran out of time.
    fn scan_timed_out(&self, seconds: u64) -> String;
    /// smartctl's scan for disks, which failed.
    fn scan_failed(&self, reason: &str) -> String;
    /// Every disk, which could not be read.
    fn all_failed(&self) -> String;
    /// fio's method: YABS's, with the test file's size.
    fn fio_method(&self, size: &str) -> (&'static str, String);
    /// YABS's note on ZFS, with the GiB it wants free.
    fn zfs_note(&self, needed: u64) -> String;
    /// The title of a table of two block sizes.
    fn block_sizes(&self, first: &str, second: &str) -> String;
    /// The header of the table's first column.
    fn block_size_header(&self) -> &'static str;
    /// A row: reads, writes, or both.
    fn fio_row(&self, row: FioRow) -> &'static str;
    /// A cell: a speed and its IOPS.
    fn rate(&self, speed: &str, iops: &str) -> String;
    /// A cell of a test that ran out of time.
    fn fio_timed_out(&self) -> &'static str;
    /// A cell of a test that failed.
    fn fio_error(&self) -> &'static str;
    /// The note on a block size whose test ran out of time.
    fn block_timed_out(&self, block_size: &str, seconds: u64) -> String;
    /// The note on a block size whose test failed.
    fn block_failed(&self, block_size: &str, reason: &str) -> String;
    /// The test directory's free space, which could not be read.
    fn no_space_info(&self) -> String;
    /// A test directory on a RAM disk.
    fn in_memory(&self) -> String;
    /// Too little free space for the test file.
    fn no_space(&self, needed: &str, available: &str) -> String;
    /// A filesystem without direct I/O.
    fn no_direct_io(&self) -> String;
    /// The test file, which could not be created.
    fn setup_failed(&self, reason: &str) -> String;
    /// No block size with a result.
    fn no_block_size(&self) -> String;
    /// A tool that printed nothing.
    fn no_output(&self, tool: &str) -> String;
    /// fio's own error code.
    fn fio_reported(&self, error: &str) -> String;
    /// A field of fio's output that could not be parsed.
    fn fio_field(&self, field: usize) -> String;
}

/// SMART's verdict on a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The disk passed.
    Passed,
    /// The disk failed.
    Failed,
    /// No verdict.
    Unknown,
    /// No SMART.
    Unsupported,
    /// The disk could not be read.
    ReadFailed,
}

/// A row of fio's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FioRow {
    /// Reads.
    Read,
    /// Writes.
    Write,
    /// Both.
    Total,
}

/// The network chapter's words: network speed (iperf3).
pub trait NetworkWords {
    /// iperf3's method: its version when it reported one, the streams, the seconds each way, and
    /// the most Mbit/s of a stream when there is a limit.
    fn iperf3_method(
        &self,
        version: Option<&str>,
        streams: u8,
        seconds: u16,
        rate: Option<u32>,
    ) -> String;
    /// The title of a family's table: IPv6's, or else IPv4's.
    fn family_title(&self, ipv6: bool) -> &'static str;
    /// The table's headers: the place, the provider, the upload, the download and the latency.
    fn network_headers(&self) -> [&'static str; 5];
    /// A server's place: its location, then its country.
    fn place(&self, location: &str, country: &str) -> String;
    /// A cell of a direction without a speed.
    fn way_cell(&self, cell: WayCell) -> &'static str;
    /// The note on a direction that needed more than one try: the download, or else the upload.
    fn tries(&self, place: &str, download: bool, tries: u32) -> String;
    /// The note on a direction whose last try failed, with the reason.
    fn way_failed(&self, place: &str, download: bool, reason: &str) -> String;
    /// iperf3, which measured nothing.
    fn nothing_measured(&self) -> String;
    /// The note on the countries or server ids asked for that no server of the list has.
    fn unknown_servers(&self, names: &[String]) -> String;
    /// The server list, which could not be read, for the system's reason.
    fn list_unreadable(&self, reason: &str) -> String;
    /// The server list, larger than `limit` bytes.
    fn list_too_large(&self, limit: usize) -> String;
    /// The server list, which is not in its format, with the parser's reason.
    fn list_format(&self, reason: &str) -> String;
    /// The server list, of a newer version of the format.
    fn list_newer(&self, version: u64) -> String;
    /// The server list, of a version of the format that never was.
    fn list_version(&self, version: u64) -> String;
    /// The server list, with more than `most` servers.
    fn list_too_many(&self, count: usize, most: usize) -> String;
    /// A server of the list, by its position from 1, which breaks a rule.
    fn list_rule(&self, position: usize, rule: ListRule) -> String;
    /// Two servers of the list with the same id.
    fn list_duplicate(&self, id: &str) -> String;
    /// No server chosen: by default, or for the countries or ids asked for.
    fn no_server(&self, unknown: &[String]) -> String;
    /// No chosen server answers over a family that the host has.
    fn no_family(&self) -> String;
    /// No server gave a speed.
    fn no_speed(&self) -> String;
}

/// A cell of the network table without a speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WayCell {
    /// The server was busy on every try.
    Busy,
    /// The server could not be reached.
    Unreachable,
    /// The last try ran out of time.
    TimedOut,
    /// The last try failed otherwise.
    Error,
    /// Not tried.
    NotTried,
}

/// Route tracing's words: its method, each target's trace and table, and its reasons.
pub trait TraceWords {
    /// NextTrace's method: its version, ICMP with 3 probes a hop and 30 hops at most, and what the
    /// configuration chose other than the default.
    fn trace_method(&self, version: &str, choice: &TraceChoice) -> String;
    /// A target's line: its name, the name traced when there is one, and what came of it.
    fn trace_heading(&self, name: &str, host: Option<&str>, outcome: &str) -> String;
    /// YouTube's nearest cache server.
    fn youtube(&self) -> &'static str;
    /// Netflix's nearest cache server, with its city and country when its lookup named them.
    fn netflix(&self, place: Option<(&str, &str)>) -> String;
    /// A trace that reached its target after `hops` hops.
    fn reached(&self, hops: usize) -> String;
    /// A trace that did not reach its target after `hops` hops.
    fn not_reached(&self, hops: usize) -> String;
    /// The table's headers: the hop, the address, the network, the place and the latency.
    fn trace_headers(&self) -> [&'static str; 5];
    /// An address's network, when the service names one, with the internet exchange where
    /// Netflix's network takes the address.
    fn at_exchange(&self, network: Option<&str>, exchange: &str) -> String;
    /// A cache server that could not be looked up, with the reason.
    fn unavailable_lookup(&self, reason: &str) -> String;
    /// An answer that names no cache server that the item can trace.
    fn lookup_problem(&self, problem: LookupProblem) -> String;
    /// An answer that could not be read, for the system's reason.
    fn answer_unreadable(&self, reason: &str) -> String;
    /// NextTrace, which exited without a trace.
    fn traced_nothing(&self) -> String;
    /// The note on the ids asked for that no target has.
    fn unknown_targets(&self, ids: &[String]) -> String;
    /// The note on a run without exchange names, with the reason.
    fn no_exchanges(&self, reason: &str) -> String;
    /// A list of the item's that cannot be used.
    fn trace_list_problem(&self, list: TraceList, problem: TraceListProblem<'_>) -> String;
    /// No target chosen: by default, or for the ids asked for.
    fn no_target(&self, unknown: &[String]) -> String;
    /// No chosen target has a name of a family that the host reaches.
    fn no_target_family(&self) -> String;
    /// No target got a route.
    fn no_target_route(&self) -> String;
}

/// What route tracing's configuration chose other than the default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceChoice {
    /// The groups chosen, by their names in the configuration, such as `china`.
    pub groups: Vec<String>,
    /// The ids chosen.
    pub targets: Vec<String>,
    /// Whether the targets came from another list than this repository's.
    pub list: bool,
    /// Whether NextTrace looked up each hop's network and place.
    pub geo: bool,
}

/// One of route tracing's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceList {
    /// The target list.
    Targets,
    /// The list of Netflix's exchange ports.
    Exchanges,
}

/// What makes one of route tracing's lists unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceListProblem<'a> {
    /// It could not be read, for the system's reason.
    Unreadable(&'a str),
    /// It is larger than this many bytes.
    TooLarge(usize),
    /// It is not in its format, for the parser's reason.
    Format(&'a str),
    /// It is of a newer version of the format.
    Newer(u64),
    /// It is of a version of the format that never was.
    Version(u64),
    /// It holds this many entries, more than the most.
    TooMany(usize, usize),
    /// An entry, by its position from 1, breaks a rule.
    Rule(usize, TraceRule),
    /// Two entries have this id or address.
    Duplicate(&'a str),
}

/// A rule of an entry of route tracing's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceRule {
    /// A target's id.
    Id,
    /// A target's group.
    Group,
    /// A target's name.
    Name,
    /// A target's AS number.
    Asn,
    /// A target's IPv4 name.
    Ipv4,
    /// A target's IPv6 name.
    Ipv6,
    /// A port's address.
    Address,
    /// A port's exchange.
    Exchange,
}

/// Why an answer names no cache server that route tracing can trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupProblem {
    /// Google's answer names no server.
    NoServer,
    /// Google's answer names a server whose id the code cannot write.
    Id,
    /// fast.com's answer is not its API's JSON.
    Format,
    /// fast.com's answer names no target.
    NoTarget,
    /// fast.com's first target is not at an HTTPS address of a name or an IP address.
    Url,
    /// fast.com's first target has a city or country that cannot be shown.
    Place,
}

/// A rule of a server of the network test's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListRule {
    /// The id.
    Id,
    /// The country.
    Country,
    /// The host.
    Host,
    /// The ports.
    Ports,
    /// The families.
    Families,
    /// The place, the provider or the link.
    Text,
}

/// Streaming media unlock's words: its method, regional subheadings, table cells and notes.
pub trait MediaWords {
    /// Method line at the top of the streaming media section.
    fn media_method(&self, choice: &MediaChoice) -> String;
    /// Heading of a regional group (`global`, `asia`, `europe`, `americas`, `oceania`).
    fn media_group(&self, code: &str) -> &'static str;
    /// Headers of a regional table (`Service`, `IPv4`, `IPv6`).
    fn media_headers(&self) -> [&'static str; 3];
    /// Cell text for an unlocked service, optionally in `region`.
    fn media_yes(&self, region: Option<&str>) -> String;
    /// Cell text when only originals are available, optionally in `region`.
    fn media_originals(&self, region: Option<&str>) -> String;
    /// Cell text for a non-unlocked outcome.
    fn media_cell(&self, cell: MediaCell, ipv6: bool) -> &'static str;
    /// Note below a regional table for an unexpected HTTP or parse error.
    fn media_unexpected(&self, service: &str, ipv6: bool, reason: &str) -> String;
    /// Note when configured `services` contain unknown ids.
    fn unknown_services(&self, ids: &[String]) -> String;
    /// Failure reason when `base_url` is neither `https://` nor loopback `http://`.
    fn invalid_base_url(&self, url: &str) -> String;
    /// Skip reason when no requested service matched the catalog.
    fn no_service(&self, unknown: &[String]) -> String;
    /// Skip reason when neither IPv4 nor IPv6 is reachable.
    fn no_media_family(&self) -> String;
}

/// Non-default settings chosen for `ip.media`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaChoice {
    /// Regional groups requested, such as `global` or `asia`.
    pub groups: Vec<String>,
    /// Specific service ids requested.
    pub services: Vec<String>,
    /// Whether `base_url` overrode the built-in endpoints.
    pub custom_base: bool,
}

/// Non-unlocked cell kinds in the streaming media table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCell {
    /// Geographically restricted.
    No,
    /// Redirected to Mainland China (`CN`).
    NoChina,
    /// Service announced as coming soon.
    ComingSoon,
    /// Proxy or VPN detected by the service.
    ProxyDetected,
    /// Blocked by an anti-bot challenge or WAF.
    BlockedWaf,
    /// Rate limited (`HTTP 429`).
    RateLimited,
    /// Domain has no DNS record for this IP family.
    NoAddress,
    /// Probe timed out.
    TimedOut,
    /// Unexpected status or response body.
    Error,
}

/// Non-default settings chosen for `ip.quality`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IpChoice {
    /// Whether DNSBL blacklist checks ran.
    pub dnsbl: bool,
    /// Whether outbound TCP port 25 checks ran.
    pub mail: bool,
    /// Whether custom loopback or HTTPS endpoints were configured.
    pub custom_endpoints: bool,
}

/// Summary row labels in `[IP reputation]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpField {
    /// Public address (masked unless `--show-ip`).
    Address,
    /// Reverse DNS or observed hostname (shown only with `--show-ip`).
    Hostname,
    /// Autonomous System Number.
    Asn,
    /// Organization or ISP.
    Organization,
    /// Regional Internet Registry and registration country.
    Registry,
    /// Native vs broadcast origin.
    Origin,
    /// Network usage classification.
    UsageType,
    /// Operational GeoIP consensus.
    GeoIp,
}

/// Subtable headings in `[IP reputation]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpTable {
    /// Identity and origin summary.
    Summary,
    /// Per-source risk scores and reconnaissance telemetry.
    Scores,
    /// Aggregated risk factors.
    Factors,
    /// DNSBL blacklists and port-25 mail connectivity.
    BlacklistsAndMail,
}

/// IP reputation's words: its method, subtable headings, cells, and failure reasons.
pub trait IpWords {
    /// Method line at the top of `[IP reputation]`.
    fn ip_method(&self, choice: &IpChoice) -> String;
    /// Progress log stage phrase (`intelligence`, `dnsbl`, `mail`).
    fn ip_stage(&self, stage: &str) -> &'static str;
    /// Heading of one subtable in `[IP reputation]`.
    fn ip_table(&self, table: IpTable) -> &'static str;
    /// Headers of one subtable (`[<first>, "IPv4", "IPv6"]`).
    fn ip_headers(&self, table: IpTable) -> [&'static str; 3];
    /// Label of a row in the summary subtable.
    fn ip_field(&self, field: IpField) -> &'static str;
    /// Native origin cell (`native (SG)`).
    fn ip_native(&self, country: &str) -> String;
    /// Broadcast origin cell (`broadcast (US -> SG)`).
    fn ip_broadcast(&self, registered: &str, located: &str) -> String;
    /// Usage classification word.
    fn ip_usage(&self, code: &str) -> &'static str;
    /// Qualitative risk level word (`low`, `medium`, `high`, `very high`).
    fn ip_risk_level(&self, code: &str) -> &'static str;
    /// Numeric score cell (`12/100 (low)`).
    fn ip_numeric_score(&self, score: u8, max: u8, level: &str) -> String;
    /// Ratio score cell (`0.0039 (low)`).
    fn ip_ratio_score(&self, ratio: &str, level: &str) -> String;
    /// GreyNoise cell.
    fn ip_greynoise(&self, riot: bool, noise: bool, class: &str) -> &'static str;
    /// Shodan InternetDB cell.
    fn ip_shodan(&self, ports: usize, vulns: usize, flagged: bool) -> String;
    /// Risk factor row label (`Proxy`, `VPN`, `Tor`, `Datacenter`, `Abuser`, `Robot`, `Bogon`).
    fn ip_factor_name(&self, code: &str) -> &'static str;
    /// Risk factor cell (`no (0/3)` or `yes (2/3: ipapi.is, Shodan)`).
    fn ip_factor_cell(&self, checked: usize, flagged: &[&str]) -> String;
    /// DNSBL row label (`DNSBL`).
    fn ip_dnsbl_label(&self) -> &'static str;
    /// DNSBL cell (`clean (0/24)`, `clean (0/23; 1 refused)`, `listed (1/23: SpamCop)`).
    fn ip_dnsbl_cell(&self, checked: usize, refused: usize, listed: &[&str]) -> String;
    /// Outbound port 25 summary row label (`Port 25`).
    fn ip_port25_label(&self) -> &'static str;
    /// Outbound port 25 summary cell (`open` or `blocked`).
    fn ip_port25_cell(&self, open: bool) -> &'static str;
    /// Mail provider deliverable cell (`yes (220)`).
    fn ip_mail_yes(&self, code: u16) -> String;
    /// Mail provider rejected cell (`rejected (554)`).
    fn ip_mail_rejected(&self, code: u16) -> String;
    /// Mail provider blocked cell (`blocked`).
    fn ip_mail_blocked(&self) -> &'static str;
    /// Note below the section when a provider returned an unexpected error.
    fn ip_unexpected(&self, provider: &str, ipv6: bool, reason: &str) -> String;
    /// Skip reason when reachable families had no discovered public IP address.
    fn ip_no_public_address(&self) -> String;
    /// Failure reason when an endpoint override is invalid.
    fn ip_invalid_endpoint(&self, raw: &str) -> String;
}

/// Summary row labels in `[BGP paths]` (`network.bgp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgpField {
    /// Public address (masked unless `--show-ip`).
    Address,
    /// Announced CIDR prefix (masked unless `--show-ip`).
    Prefix,
    /// Primary origin Autonomous System (and MOAS count when > 1).
    OriginAsn,
    /// RPKI Route Origin Authorization validity.
    Rpki,
    /// Collector peer count and distinct path count.
    Visibility,
    /// Top direct upstream ASNs immediately before the origin.
    Upstreams,
}

/// Typed `network.bgp` query or parser failure passed to [`BgpWords`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgpIssueRef<'a> {
    /// API host has no DNS address for this IP family.
    NoAddress,
    /// HTTP probe timed out.
    TimedOut,
    /// Redirect hop cap exceeded.
    TooManyRedirects,
    /// Redirect target was malformed or unsafe.
    InvalidRedirect(&'a str),
    /// Non-HTTPS endpoint rejected.
    NotHttps(&'a str),
    /// Underlying HTTP/TLS/TCP transport error.
    Transport(&'a str),
    /// Non-200 HTTP status returned by `bgp-state`.
    HttpStatus(u16),
    /// Malformed JSON payload returned by `bgp-state`.
    InvalidJson(&'a str),
    /// Non-`ok` envelope `status` field returned by `bgp-state`.
    ApiStatus(&'a str),
    /// Envelope lacked a `data` object.
    MissingData,
    /// No family returned BGP data.
    NoData,
}

/// BGP path topology words (`network.bgp`).
pub trait BgpWords {
    /// Method line at the top of `[BGP paths]`.
    fn bgp_method(&self, custom_endpoints: bool, upload: bool) -> String;
    /// Summary subtable heading.
    fn bgp_summary_title(&self) -> &'static str;
    /// Summary subtable headers (`["Field", "IPv4", "IPv6"]`).
    fn bgp_summary_headers(&self) -> [&'static str; 3];
    /// Row label in the summary subtable.
    fn bgp_field(&self, field: BgpField) -> &'static str;
    /// Cell when an IP address is not announced in global BGP tables.
    fn bgp_not_announced(&self) -> &'static str;
    /// Origin ASN cell (`AS13335 (Cloudflare)` or `AS13335 (Cloudflare) +1 MOAS`).
    fn bgp_origin_cell(&self, asn: u32, name: Option<&str>, total_origins: usize) -> String;
    /// RPKI status cell (`valid`, `invalid`, `not found`, `unknown`).
    fn bgp_rpki_cell(&self, code: &str) -> &'static str;
    /// Visibility cell (`180 peers, 14 paths`).
    fn bgp_visibility_cell(&self, peers: u32, distinct_paths: usize) -> String;
    /// Direct collector-only peering cell when all paths have length 1.
    fn bgp_direct_peering(&self) -> &'static str;
    /// Label/heading for a family's topology (`IPv4 topology` / `IPv6 topology`).
    fn bgp_topology_title(&self, ipv6: bool) -> &'static str;
    /// Heading above a family's top observed paths table (`IPv4 observed paths` /
    /// `IPv6 observed paths`).
    fn bgp_paths_title(&self, ipv6: bool) -> &'static str;
    /// Headers of the observed paths table (`["Peers", "Hops", "AS_PATH"]`).
    fn bgp_paths_headers(&self) -> [&'static str; 3];
    /// Renders a [`BgpIssueRef`] lookup or parser failure in the catalog's language.
    fn bgp_issue(&self, issue: BgpIssueRef<'_>) -> String;
    /// HTTP status + message returned by the topology image host.
    fn bgp_upload_http_error(&self, status: u16, reason: &str) -> String;
    /// Malformed JSON returned by the topology image host.
    fn bgp_upload_invalid_json(&self) -> String;
    /// Generic rejection when the topology image host returned `success: false` without a message.
    fn bgp_upload_rejected(&self) -> String;
    /// Response from the topology image host lacked a safe `imageKey`.
    fn bgp_upload_missing_key(&self) -> String;
    /// Per-family note when topology image upload failed in `text` mode.
    fn bgp_upload_failed(&self, ipv6: bool, reason: &str) -> String;
    /// Per-family note when one family's BGP lookup failed.
    fn bgp_family_failed(&self, ipv6: bool, reason: &str) -> String;
    /// Failure reason when every attempted `bgp-state` lookup failed.
    fn bgp_query_failed(&self, reason: &str) -> String;
}

/// Effective parameters and non-default filters shown in `network.taier`'s method line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaierChoice {
    /// Parallel TCP streams per direction.
    pub streams: u8,
    /// Duration per direction in seconds.
    pub seconds: u16,
    /// Non-empty when explicit `<place>[:<op>[+<op>...]]` selectors were requested.
    pub selections: Vec<String>,
    /// Non-empty when city/province filters were requested.
    pub cities: Vec<String>,
    /// Non-empty when non-default carrier filters were requested.
    pub operators: Vec<String>,
    /// Whether a custom `list` path or URL overrode the default Taier server list.
    pub custom_list: bool,
}

/// Non-measured cell kinds in `[China carrier speed]` (`network.taier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaierCell {
    /// Every candidate node in this slot was busy (`dovalid: -1`).
    Busy,
    /// No candidate node accepted TCP connections.
    Unreachable,
    /// Node rejected `dovalid` authentication (`0` or `2`).
    AuthRejected,
    /// Handshake or stream timed out.
    TimedOut,
    /// Unexpected protocol or stream error.
    Error,
}

/// What makes the Taier server list unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaierListProblem<'a> {
    /// Neither a downloaded list nor an embedded snapshot was available.
    Missing,
    /// Could not be read from disk.
    Unreadable(&'a str),
    /// Larger than `usize` bytes.
    TooLarge(usize),
    /// Invalid JSON or field types.
    Format(&'a str),
    /// Newer schema version.
    Newer(u64),
    /// Unknown/zero schema version.
    Version(u64),
    /// Empty `locations` array.
    Empty,
    /// More than `most` locations.
    TooMany(usize, usize),
    /// Location at 1-based position broke `TaierRule`.
    Rule(usize, TaierRule),
    /// Duplicate location `id`.
    Duplicate(&'a str),
}

/// Validation rule broken by an entry in the Taier server list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaierRule {
    /// Location `id`.
    Id,
    /// Location `province`.
    Province,
    /// Location `city`.
    City,
    /// Location `nodes` array length.
    Nodes,
    /// Node `operator`.
    Operator,
    /// Node `ip`.
    Ip,
    /// Node `port`.
    Port,
    /// Duplicate `ip:port` inside one location.
    DuplicateEndpoint,
}

/// Syntax problem in a `--taier` target selector passed to [`TaierWords`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaierSelectorProblem<'a> {
    /// Selector string was empty or whitespace-only.
    Empty,
    /// Malformed `<place>` or multiple `:` / `/` separators.
    Format(&'a str),
    /// Missing `<place>` before `:`.
    EmptyPlace(&'a str),
    /// Empty carrier list or trailing `+` after `:`.
    EmptyOperators(&'a str),
    /// Unknown carrier token after `:`.
    UnknownOperator {
        /// Original selector token.
        selector: &'a str,
        /// Unrecognized carrier part.
        operator: &'a str,
    },
}

/// China carrier speed's words (`network.taier`).
pub trait TaierWords {
    /// Method line at the top of `[China carrier speed]`.
    fn taier_method(&self, choice: &TaierChoice) -> String;
    /// Note when requested city/province tokens matched no location.
    fn taier_unknown_cities(&self, names: &[String]) -> String;
    /// Table headers (`Location`, `Carrier`, `Download`, `Upload`).
    fn taier_headers(&self) -> [&'static str; 4];
    /// Human-readable location cell (`Beijing` or `Guangzhou, Guangdong`).
    fn taier_place(&self, city: &str, province: &str) -> String;
    /// Human-readable carrier cell (`Telecom`, `Unicom`, `Mobile`, `CERNET`, `Broadnet`, `Other`).
    fn taier_operator(&self, code: &str) -> &'static str;
    /// Non-measured throughput cell (`busy`, `unreachable`, `auth rejected`, `timed out`, `error`).
    fn taier_cell(&self, cell: TaierCell) -> &'static str;
    /// Note when a `(location, operator)` slot fell back past busy/unreachable candidate nodes.
    fn taier_fallback(&self, place: &str, operator: &str, tried: u8) -> String;
    /// Note when a `(location, operator)` slot failed with a protocol/stream error.
    fn taier_node_failed(&self, place: &str, operator: &str, reason: &str) -> String;
    /// Failure/skip reason when the Taier server list cannot be used.
    fn taier_list_problem(&self, problem: TaierListProblem<'_>) -> String;
    /// Syntax error message for a malformed `<place>[:<op>[+<op>...]]` selector.
    fn taier_selector_problem(&self, problem: TaierSelectorProblem<'_>) -> String;
    /// Top summary line of `--taier-list` overview.
    fn taier_catalog_header(&self, regions: usize, locations: usize) -> String;
    /// Carrier short-code legend line of `--taier-list` overview.
    fn taier_catalog_carriers_legend(&self) -> &'static str;
    /// Region kind label (`municipality`, `province`, or `overseas`) in `--taier-list`.
    fn taier_region_kind(&self, code: &str) -> &'static str;
    /// Column headers of `--taier-list` overview table.
    fn taier_catalog_overview_headers(&self) -> [&'static str; 5];
    /// Footer hint of `--taier-list` overview.
    fn taier_catalog_footer_hint(&self) -> &'static str;
    /// Heading line of a region section in `--taier-list <REGION>`.
    fn taier_catalog_region_heading(&self, name: &str, municipality: bool, count: usize) -> String;
    /// Column headers of a region's city table in `--taier-list <REGION>`.
    fn taier_catalog_city_headers(&self) -> [&'static str; 3];
    /// Skip reason when no `(location, operator)` slot matched the selection.
    fn taier_no_node(&self, unknown: &[String]) -> String;
    /// Failure reason when every attempted `(location, operator)` slot failed.
    fn taier_no_result(&self) -> String;
}

/// Effective parameters shown in `network.cloudflare`'s method line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudflareChoice {
    /// Concurrent HTTP workers per throughput direction.
    pub concurrency: u8,
    /// Duration per throughput direction in seconds.
    pub seconds: u16,
    /// Whether the STUN UDP packet-loss probe is enabled.
    pub udp: bool,
    /// Number of STUN UDP probe packets when `udp` is enabled.
    pub udp_packets: u16,
    /// Whether custom HTTP or STUN endpoints were configured.
    pub custom_endpoints: bool,
}

/// Summary row labels in `[Cloudflare speed]` (`network.cloudflare`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudflareField {
    /// Edge colocation server (`SJC - San Jose, United States`).
    Server,
    /// Client IP reported by Cloudflare (masked unless `--show-ip`).
    ClientIp,
    /// Client ASN and organization reported by Cloudflare.
    ClientAsn,
    /// Pre-transfer unloaded HTTP RTT summary.
    IdleLatency,
    /// Steady-state download throughput and p50/p95 summary.
    Download,
    /// Loaded latency during download.
    DownloadLatency,
    /// Steady-state upload throughput and p50/p95 summary.
    Upload,
    /// Loaded latency during upload.
    UploadLatency,
    /// STUN UDP packet loss, out-of-order %, and ITU-T G.107 E-model MOS.
    UdpLoss,
    /// Waveform bufferbloat and CV throughput stability grades.
    Quality,
}

/// Typed Cloudflare measurement or transport issue passed to [`CloudflareWords`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudflareIssueRef<'a> {
    /// Download phase completed with zero payload bytes.
    EmptyDownload,
    /// Upload phase completed with zero payload bytes.
    EmptyUpload,
    /// Host or endpoint has no address for this IP family.
    NoAddress,
    /// Request or probe timed out.
    TimedOut,
    /// Host has no usable DNS resolver.
    NoResolver,
    /// Underlying HTTP or UDP transport error.
    Transport(&'a str),
}

/// Cloudflare speed's words (`network.cloudflare`).
pub trait CloudflareWords {
    /// Method line at the top of `[Cloudflare speed]`.
    fn cloudflare_method(&self, choice: &CloudflareChoice) -> String;
    /// Summary subtable headers (`["Metric", "IPv4", "IPv6"]` or single-family fields).
    fn cloudflare_headers(&self) -> [&'static str; 3];
    /// Label of a row in the Cloudflare summary table.
    fn cloudflare_field(&self, field: CloudflareField) -> &'static str;
    /// Latency cell (`12.4 ms median (jitter 1.2 ms, loss 0.0%)`).
    fn cloudflare_latency_cell(
        &self,
        median: &str,
        min: &str,
        max: &str,
        jitter: Option<&str>,
        loss_pct: f64,
    ) -> String;
    /// Throughput cell (`845 Mbit/s (p50 850, p95 910 Mbit/s)`).
    fn cloudflare_speed_cell(&self, speed: &str, p50_mbps: f64, p95_mbps: f64) -> String;
    /// Localized STUN UDP quality word (`Excellent`, `Good`, `Acceptable`, `Poor`, `Bad`).
    fn cloudflare_udp_quality(&self, code: &str) -> &'static str;
    /// STUN UDP packet-loss cell (`0.0% loss (50/50, 0.0% reordered, MOS 4.4 Excellent)`).
    fn cloudflare_udp_cell(
        &self,
        loss_pct: f64,
        received: u64,
        sent: u64,
        reordered_pct: f64,
        mos: Option<f64>,
        quality: &str,
    ) -> String;
    /// Connection quality cell (`bufferbloat A (+12.0 ms), stability A (CV 2.4%)`).
    fn cloudflare_quality_cell(
        &self,
        bloat_grade: &str,
        bloat_ms: Option<f64>,
        stab_grade: &str,
        stab_cv: Option<f64>,
    ) -> String;
    /// Renders a [`CloudflareIssueRef`] reason in the catalog's language.
    fn cloudflare_issue(&self, issue: CloudflareIssueRef<'_>) -> String;
    /// Per-family note when STUN UDP probing failed or was blocked.
    fn cloudflare_udp_note(&self, ipv6: bool, reason: &str) -> String;
    /// Per-family note when one family failed while the other succeeded.
    fn cloudflare_family_failed(&self, ipv6: bool, reason: &str) -> String;
    /// Failure reason when every reachable family failed.
    fn cloudflare_no_result(&self, reason: &str) -> String;
}

/// Effective parameters shown in `network.inetspeed`'s method line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InetspeedChoice {
    /// Multi-thread worker count.
    pub threads: u8,
    /// Per-thread timeout in seconds.
    pub timeout_secs: u16,
    /// Per-thread byte cap label (`2G` by default).
    pub max: String,
    /// Idle latency sample count (`20` by default).
    pub latency_count: u8,
    /// Whether `--inetspeed-endpoint` forced a specific IP.
    pub forced_endpoint: bool,
    /// Whether custom URLs overrode `mensura.cdn-apple.com`.
    pub custom_endpoints: bool,
}

/// Typed `network.inetspeed` transport fault passed to [`InetspeedWords`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InetspeedProbeFaultRef<'a> {
    /// RTT probe completed without returning a sample.
    EmptySample,
    /// Host or endpoint has no address for the requested IP family.
    NoAddress,
    /// Request or probe timed out.
    TimedOut,
    /// Host has no usable DNS resolver.
    NoResolver,
    /// Underlying HTTP/TCP transport error.
    Transport(&'a str),
}

/// Typed `network.inetspeed` round issue passed to [`InetspeedWords`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InetspeedRoundIssueRef<'a> {
    /// Round completed with zero bytes transferred.
    ZeroBytes,
    /// Some worker streams reported transport faults while others succeeded.
    WorkerFaults(u8),
    /// Entire round failed with a transport error.
    Transport(InetspeedProbeFaultRef<'a>),
}

/// Typed `network.inetspeed` warning passed to [`InetspeedWords`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InetspeedWarningRef<'a> {
    /// Download, upload, and latency URLs do not share the same hostname.
    HostMismatch,
    /// User-forced `--inetspeed-endpoint` IP failed its RTT pre-probe.
    ForcedProbeFailed(&'a str),
    /// Dual DoH returned no candidate IPs; fell back to system DNS.
    DohEmpty,
    /// Neither DoH nor system DNS yielded a candidate IP; using unpinned DNS.
    UnpinnedFallback,
    /// Unloaded latency phase returned zero RTT samples.
    NoIdleSamples,
    /// Unloaded latency phase failed with a transport error.
    IdleFailed(InetspeedProbeFaultRef<'a>),
}

/// Apple CDN speed's words (`network.inetspeed`).
pub trait InetspeedWords {
    /// Method line at the top of `[Apple CDN speed]`.
    fn inetspeed_method(&self, choice: &InetspeedChoice) -> String;
    /// Selected endpoint field label (`Selected endpoint`).
    fn inetspeed_endpoint_label(&self) -> &'static str;
    /// Selected endpoint value (`17.253.x.x (mensura.cdn-apple.com, 8.42 ms via Dual DoH)`).
    fn inetspeed_endpoint_value(
        &self,
        host: &str,
        ip: Option<&str>,
        rtt_ms: Option<f64>,
        source: &str,
    ) -> String;
    /// Human-readable endpoint discovery source (`pinned`, `Dual DoH`, `system DNS`, etc.).
    fn inetspeed_source(&self, code: &str) -> &'static str;
    /// Idle latency field label (`Idle latency`).
    fn inetspeed_idle_label(&self) -> &'static str;
    /// Formatted idle latency summary (`8.50 ms median (min ... / avg ... / max ..., jitter ...)`).
    fn inetspeed_latency_value(
        &self,
        median_ms: f64,
        min_ms: f64,
        avg_ms: f64,
        max_ms: f64,
        jitter_ms: f64,
    ) -> String;
    /// Total data transferred field label (`Data transferred`).
    fn inetspeed_total_label(&self) -> &'static str;
    /// Round table headers (`["Round", "Threads", "Speed", "Data", "Loaded latency"]`).
    fn inetspeed_headers(&self) -> [&'static str; 5];
    /// Round name cell (`Download (1T)`, `Download (4T)`, `Upload (1T)`, `Upload (4T)`).
    fn inetspeed_round_name(&self, download: bool, single: bool) -> &'static str;
    /// Loaded latency cell (`14.20 ms (jitter 1.80 ms)`).
    fn inetspeed_loaded_cell(&self, median_ms: f64, jitter_ms: f64) -> String;
    /// Renders a round degradation or failure note.
    fn inetspeed_round_issue(&self, issue: InetspeedRoundIssueRef<'_>) -> String;
    /// Renders a non-fatal discovery or idle-latency warning.
    fn inetspeed_warning(&self, warning: InetspeedWarningRef<'_>) -> String;
    /// Failure reason when `max` byte-size string is invalid.
    fn inetspeed_invalid_max(&self, raw: &str) -> String;
    /// Failure reason when every transfer round failed.
    fn inetspeed_no_result(&self) -> String;
}
