//! The words of the test items' chapters: CPU, memory and disks.

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

/// The CPU chapters' words: CPU performance (sysbench) and Geekbench 7.
pub trait CpuWords {
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
    /// Geekbench's labels: single-core, multi-core and the result link.
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
