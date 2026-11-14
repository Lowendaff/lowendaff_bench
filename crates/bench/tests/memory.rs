//! The memory section of the report, from constructed runs: each chapter of memory performance,
//! sysbench, STREAM, latency and NUMA, with its parameters, tables and notes, and each way it
//! ends.
//! Compared with snapshots, and with the words that the memory item's own tests asserted before
//! the reporter wrote them.

use std::time::Duration;

use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use plugin_memory::Value;
use plugin_memory::latency;
use plugin_memory::latency::result::Row;
use plugin_memory::numa;
use plugin_memory::numa::result::Cell;
use plugin_memory::numa::result::Kind;
use plugin_memory::plan::GIB;
use plugin_memory::plan::KIB;
use plugin_memory::plan::Level;
use plugin_memory::plan::MIB;
use plugin_memory::stream;
use plugin_memory::sysbench;
use plugin_memory::sysbench::result::Operation;

/// The report of `runs` from its memory section on, with the summary.
fn section(runs: &[Run<Measured>]) -> String {
    let host = HostProfile::default();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs,
        total: Duration::from_secs(5),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("[Memory performance]").expect("a memory section");
    text[start..].to_owned()
}

/// Whether `text` holds `line` as a whole line.
fn shows(text: &str, line: &str) -> bool {
    text.lines().any(|shown| shown == line)
}

/// The cells of each line that starts with `name`, as two spaces or more split them.
fn row(text: &str, name: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|line| line.trim_start().starts_with(name))
        .map(|line| {
            line.split("  ")
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .collect()
}

fn value(text: &str) -> Value {
    Value::parse(text).expect("a number")
}

fn run(id: &'static str, title: &'static str, status: Status, result: Measured) -> Run<Measured> {
    Run {
        id,
        group: "memory",
        title: Some(title),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_millis(41_200)),
        status,
        result: Some(result),
    }
}

fn sysbench_run(
    status: Status,
    output: Option<sysbench::Output>,
    stop: Option<Stop<sysbench::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemorySysbench(Report { output, stop });
    run("memory.sysbench", "Memory sysbench", status, result)
}

fn stream_run(
    status: Status,
    output: Option<stream::Output>,
    stop: Option<Stop<stream::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemoryStream(Report { output, stop });
    run("memory.stream", "Memory STREAM", status, result)
}

fn latency_run(
    status: Status,
    output: Option<latency::Output>,
    stop: Option<Stop<latency::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemoryLatency(Report { output, stop });
    run("memory.latency", "Memory latency", status, result)
}

fn numa_run(
    status: Status,
    output: Option<numa::Output>,
    stop: Option<Stop<numa::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemoryNuma(Report { output, stop });
    run("memory.numa", "Memory NUMA", status, result)
}

/// sysbench's runs on four threads, as the memory item's tests had them.
fn sysbench_output() -> sysbench::Output {
    let runs = [
        (Operation::Read, 1, "100.00"),
        (Operation::Read, 4, "400.00"),
        (Operation::Write, 1, "50.00"),
        (Operation::Write, 4, "200.00"),
    ]
    .map(|(operation, threads, rate)| sysbench::result::Run {
        operation,
        threads,
        rate: value(rate),
        retries: Vec::new(),
    })
    .to_vec();
    sysbench::Output {
        version: Some("1.0.20".to_owned()),
        single: 32 * MIB,
        per_thread: Some(8 * MIB),
        threads: 4,
        small: None,
        cache_unknown: false,
        runs,
    }
}

fn stream_output() -> stream::Output {
    let rates = |rates: [&str; 4]| rates.map(value);
    stream::Output {
        version: Some("5.10".to_owned()),
        millions: 8,
        threads: 4,
        below_rule: false,
        cache_unknown: false,
        runs: vec![
            stream::result::Run {
                threads: 1,
                rates: rates(["10000.1", "9000.2", "11000.3", "11500.4"]),
                retries: Vec::new(),
            },
            stream::result::Run {
                threads: 4,
                rates: rates(["40000.1", "36000.2", "44000.3", "46000.4"]),
                retries: Vec::new(),
            },
        ],
    }
}

fn latency_output() -> latency::Output {
    let rows = [
        (Level::L1, 16 * KIB, "1.500"),
        (Level::L2, 512 * KIB, "4.200"),
        (Level::L3, 4 * MIB, "12.000"),
        (Level::Memory, 32 * MIB, "90.500"),
    ]
    .map(|(level, buffer, latency)| Row {
        level,
        buffer,
        latency: Some(value(latency)),
        retries: Vec::new(),
    })
    .to_vec();
    latency::Output {
        rows,
        missing: Vec::new(),
        small: None,
        cache_unknown: false,
    }
}

fn measured(text: &str) -> Option<numa::result::Measured> {
    Some(numa::result::Measured {
        value: value(text),
        retries: Vec::new(),
    })
}

fn numa_output() -> numa::Output {
    let cell = |bandwidth, latency| Cell {
        bandwidth: measured(bandwidth),
        latency: measured(latency),
    };
    numa::Output {
        nodes: 2,
        rows: vec![0, 1],
        columns: vec![0, 1],
        cells: vec![
            vec![cell("52000", "95.000"), cell("31000", "140.1")],
            vec![cell("31000", "140.1"), cell("52000", "95.000")],
        ],
        cache_unknown: false,
    }
}

/// The four chapters as the memory item showed them on a machine of two nodes.
fn four_chapters() -> Vec<Run<Measured>> {
    vec![
        sysbench_run(Status::Completed, Some(sysbench_output()), None),
        stream_run(Status::Completed, Some(stream_output()), None),
        latency_run(Status::Completed, Some(latency_output()), None),
        numa_run(Status::Completed, Some(numa_output()), None),
    ]
}

#[test]
fn the_four_chapters_show_their_parameters_and_tables_in_the_items_words() {
    let text = section(&four_chapters());
    insta::assert_snapshot!(text);
    for line in [
        "  Memory sysbench",
        "    sysbench            1.0.20: sequential reads and writes, 10 s each; one block of 32 \
         MiB for one thread, one of 8 MiB per thread for several",
        "    sysbench results",
        "  Memory STREAM",
        "    STREAM              5.10: 8 million elements per array (61 MiB, 183 MiB for the \
         three), the fastest of 10 passes each; the threads spread evenly over the nodes",
        "    STREAM results",
        "  Memory latency",
        "    Latency             multichase: random pointer chasing, 2.5 s per level",
        "    Latency results",
        "  Memory NUMA",
        "    NUMA                2 nodes; bandwidth by multiload's stream-triad, the fastest of \
         3 samples per cell; latency as above",
        "    NUMA bandwidth (MiB/s; rows: the threads' node, columns: the memory's node)",
        "    NUMA latency (ns)",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(
        row(&text, "Test"),
        [
            ["Test", "Single thread", "Multi-thread (4 threads)"],
            ["Test", "Single thread", "Multi-thread (4 threads)"]
        ]
    );
    assert_eq!(
        row(&text, "Read"),
        [["Read", "100.00 MiB/s", "400.00 MiB/s"]]
    );
    assert_eq!(
        row(&text, "Write"),
        [["Write", "50.00 MiB/s", "200.00 MiB/s"]]
    );
    assert_eq!(
        row(&text, "Copy"),
        [["Copy", "10000.1 MB/s", "40000.1 MB/s"]]
    );
    assert_eq!(
        row(&text, "Triad"),
        [["Triad", "11500.4 MB/s", "46000.4 MB/s"]]
    );
    assert_eq!(row(&text, "Level"), [["Level", "Buffer", "Latency"]]);
    assert_eq!(row(&text, "L1"), [["L1", "16 KiB", "1.500 ns"]]);
    assert_eq!(row(&text, "L2"), [["L2", "512 KiB", "4.200 ns"]]);
    assert_eq!(row(&text, "L3"), [["L3", "4 MiB", "12.000 ns"]]);
    assert_eq!(row(&text, "Memory  "), [["Memory", "32 MiB", "90.500 ns"]]);
    assert_eq!(
        row(&text, "Node 0"),
        [
            vec!["Node 0", "Node 1"],
            vec!["Node 0", "52000", "31000"],
            vec!["Node 0", "Node 1"],
            vec!["Node 0", "95.000", "140.1"]
        ]
    );
    assert_eq!(
        text.matches("done in 41.2 s").count(),
        8,
        "each chapter ends on its own, in the section and in the summary"
    );
}

#[test]
fn one_thread_shows_the_single_thread_column_only() {
    let mut sysbench = sysbench_output();
    sysbench.per_thread = None;
    sysbench.threads = 1;
    sysbench.runs.retain(|run| run.threads == 1);
    let mut stream = stream_output();
    stream.threads = 1;
    stream.runs.truncate(1);
    let text = section(&[
        sysbench_run(Status::Completed, Some(sysbench), None),
        stream_run(Status::Completed, Some(stream), None),
    ]);
    insta::assert_snapshot!(text);
    for line in [
        "    sysbench            1.0.20: sequential reads and writes, 10 s each; one block of 32 \
         MiB for one thread",
        "    STREAM              5.10: 8 million elements per array (61 MiB, 183 MiB for the \
         three), the fastest of 10 passes each",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(
        row(&text, "Test"),
        [["Test", "Single thread"], ["Test", "Single thread"]]
    );
    assert_eq!(row(&text, "Read"), [["Read", "100.00 MiB/s"]]);
    assert_eq!(row(&text, "Copy"), [["Copy", "10000.1 MB/s"]]);
}

#[test]
fn notes_follow_each_chapters_table() {
    let timed_out = job::Fault::TimedOut(Duration::from_secs(60));
    let mut sysbench = sysbench_output();
    sysbench.small = Some(32 * MIB);
    sysbench.cache_unknown = true;
    sysbench.runs[3].retries = vec![sysbench::Fault::Tool(timed_out.clone()); 2];
    let mut stream = stream_output();
    stream.below_rule = true;
    stream.runs[1].retries = vec![stream::Fault::Invalid(
        "Failed Validation on array a[], AvgRelAbsErr > epsilon (1.000000e-13)".to_owned(),
    )];
    let mut latency = latency_output();
    latency.rows.remove(2);
    latency.missing = vec![Level::L3];
    latency.small = Some(32 * MIB);
    latency.rows[0].retries = vec![latency::Fault::NoLatency];
    let mut numa = numa_output();
    if let Some(bandwidth) = &mut numa.cells[1][0].bandwidth {
        bandwidth.retries = vec![numa::Fault::Tool(timed_out)];
    }
    let text = section(&[
        sysbench_run(Status::Completed, Some(sysbench), None),
        stream_run(Status::Completed, Some(stream), None),
        latency_run(Status::Completed, Some(latency), None),
        numa_run(Status::Completed, Some(numa), None),
    ]);
    insta::assert_snapshot!(text);
    for line in [
        "    Note                no word on the last-level caches; 32 MiB assumed",
        "    Note                available memory short: the test data is below 4 times the \
         last-level caches (32 MiB), so some reads and writes hit the caches and the results \
         run high",
        "    Note                multi-thread write: worked after 2 failures; the last reason: \
         still running after 60 s, stopped",
        "    Note                not quite within STREAM's run rules (each array at least 4 \
         times the last-level caches)",
        "    Note                multi-thread test: worked after 1 failure; the last reason: \
         STREAM's results failed their check: Failed Validation on array a[], AvgRelAbsErr > \
         epsilon (1.000000e-13)",
        "    Note                no word on L3; that level is left out",
        "    Note                available memory short: the memory level's buffer is below 4 \
         times the last-level caches (32 MiB), so some reads hit the caches and the latency \
         runs low",
        "    Note                L1 test: worked after 1 failure; the last reason: multichase \
         printed no latency",
        "    Note                bandwidth test of threads on node 1 and memory on node 0: \
         worked after 1 failure; the last reason: still running after 60 s, stopped",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    let sysbench_notes = text
        .lines()
        .skip_while(|line| !line.starts_with("    sysbench results"))
        .take_while(|line| !line.starts_with("  Memory STREAM"))
        .filter(|line| line.starts_with("    Note "))
        .count();
    assert_eq!(
        sysbench_notes, 3,
        "the cache note first, then the others: {text}"
    );
}

#[test]
fn every_way_a_chapter_ends_shows_its_words() {
    let boom = || job::Fault::Exit {
        code: 1,
        last: Some("boom".to_owned()),
    };
    let failed = Status::Failed(None);
    let skipped = Status::Skipped(None);
    let mut partial = stream_output();
    partial.runs.truncate(1);
    let mut first_cell = numa_output();
    first_cell.cells[0][1] = Cell::default();
    first_cell.cells[1] = vec![Cell::default(), Cell::default()];
    let runs = [
        sysbench_run(
            failed.clone(),
            None,
            Some(Stop::Failed(sysbench::Failure::Run {
                operation: Operation::Read,
                threads: 1,
                faults: vec![sysbench::Fault::Tool(boom()); 4],
            })),
        ),
        sysbench_run(
            failed.clone(),
            None,
            Some(Stop::Failed(sysbench::Failure::NoMemoryInfo)),
        ),
        stream_run(
            skipped.clone(),
            None,
            Some(Stop::Skipped(stream::Failure::Short { budget: 16 * MIB })),
        ),
        stream_run(
            failed.clone(),
            None,
            Some(Stop::Failed(stream::Failure::Run {
                threads: 1,
                faults: vec![stream::Fault::Tool(boom()); 4],
            })),
        ),
        stream_run(Status::Cancelled, Some(partial), None),
        latency_run(
            failed.clone(),
            None,
            Some(Stop::Failed(latency::Failure::Run {
                level: Level::L3,
                faults: vec![latency::Fault::NoLatency; 4],
            })),
        ),
        numa_run(
            skipped.clone(),
            Some(numa_output()),
            Some(Stop::Skipped(numa::Failure::Refused)),
        ),
        numa_run(
            failed,
            Some(first_cell),
            Some(Stop::Failed(numa::Failure::Run {
                row: 0,
                column: 1,
                kind: Kind::Bandwidth,
                faults: vec![numa::Fault::NoBandwidth; 4],
            })),
        ),
        numa_run(skipped, None, Some(Stop::Skipped(numa::Failure::OneNode))),
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    failed: single-thread read: sysbench exit code 1: boom (4 attempts)",
        "    failed: cannot read the available memory (/proc/meminfo)",
        "    skipped: available memory short: the three arrays need at least 46 MiB, and half of \
         the available memory is only 16 MiB",
        "    failed: single-thread test: STREAM exit code 1: boom (4 attempts)",
        "    interrupted",
        "    failed: L3 test: multichase printed no latency (4 attempts)",
        "    skipped: the container does not allow binding memory to a node",
        "    failed: bandwidth test of threads on node 0 and memory on node 1: multiload printed \
         no bandwidth (4 attempts)",
        "    skipped: needs several NUMA nodes",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(
        row(&text, "Copy"),
        [["Copy", "10000.1 MB/s", "—"]],
        "the interrupted STREAM keeps its first run"
    );
    let numa_tables = text.matches("NUMA bandwidth").count();
    assert_eq!(numa_tables, 1, "no table of a refused matrix: {text}");
    assert_eq!(
        row(&text, "Node 1"),
        [["Node 1", "—", "—"], ["Node 1", "—", "—"]],
        "the failed matrix shows the cells it measured"
    );
}

#[test]
fn each_fault_of_a_run_has_its_words() {
    let note = |fault: numa::Fault, kind: Kind| {
        let mut output = numa_output();
        let slot = match kind {
            Kind::Bandwidth => &mut output.cells[0][0].bandwidth,
            Kind::Latency => &mut output.cells[0][0].latency,
        };
        if let Some(measured) = slot {
            measured.retries = vec![fault];
        }
        let text = section(&[numa_run(Status::Completed, Some(output), None)]);
        text.lines()
            .find_map(|line| line.split_once("the last reason: ").map(|(_, why)| why))
            .expect("a note")
            .to_owned()
    };
    let spawn = job::Fault::Spawn("Text file busy (os error 26)".to_owned());
    let read = job::Fault::Read("Broken pipe (os error 32)".to_owned());
    let signal = job::Fault::Signal { last: None };
    for (fault, kind, words) in [
        (
            numa::Fault::Tool(spawn.clone()),
            Kind::Bandwidth,
            "cannot run multiload: Text file busy (os error 26)",
        ),
        (
            numa::Fault::Tool(spawn),
            Kind::Latency,
            "cannot run multichase: Text file busy (os error 26)",
        ),
        (
            numa::Fault::Tool(read),
            Kind::Latency,
            "cannot read the output of multichase: Broken pipe (os error 32)",
        ),
        (
            numa::Fault::Tool(signal),
            Kind::Bandwidth,
            "multiload ended by a signal: no output",
        ),
        (
            numa::Fault::NoBandwidth,
            Kind::Bandwidth,
            "multiload printed no bandwidth",
        ),
        (
            numa::Fault::NoLatency,
            Kind::Latency,
            "multichase printed no latency",
        ),
        (
            numa::Fault::Refused,
            Kind::Latency,
            "the container does not allow binding memory to a node",
        ),
    ] {
        assert_eq!(note(fault, kind), words);
    }

    let mut sysbench = sysbench_output();
    sysbench.runs[0].retries = vec![sysbench::Fault::NoRate];
    sysbench.runs[1].retries = vec![sysbench::Fault::Tool(job::Fault::Exit {
        code: 1,
        last: Some("FATAL: Invalid value for memory-block-size: abc".to_owned()),
    })];
    sysbench.runs[2].retries = vec![sysbench::Fault::Tool(job::Fault::Signal {
        last: Some("Threads started!".to_owned()),
    })];
    let mut stream = stream_output();
    stream.runs[0].retries = vec![stream::Fault::NoResult];
    let mut latency = latency_output();
    latency.rows[0].retries = vec![latency::Fault::Tool(job::Fault::Exit {
        code: 1,
        last: Some("mbind: Invalid argument".to_owned()),
    })];
    let text = section(&[
        sysbench_run(Status::Completed, Some(sysbench), None),
        stream_run(Status::Completed, Some(stream), None),
        latency_run(Status::Completed, Some(latency), None),
    ]);
    for line in [
        "    Note                single-thread read: worked after 1 failure; the last reason: \
         sysbench printed no transfer rate",
        "    Note                multi-thread read: worked after 1 failure; the last reason: \
         sysbench exit code 1: FATAL: Invalid value for memory-block-size: abc",
        "    Note                single-thread write: worked after 1 failure; the last reason: \
         sysbench ended by a signal: Threads started!",
        "    Note                single-thread test: worked after 1 failure; the last reason: \
         STREAM printed no result",
        "    Note                L1 test: worked after 1 failure; the last reason: multichase \
         exit code 1: mbind: Invalid argument",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}

#[test]
fn sizes_read_in_the_largest_whole_unit_and_arrays_in_mebibytes() {
    let buffers = |buffers: &[u64]| {
        let mut output = latency_output();
        output.rows = buffers
            .iter()
            .map(|&buffer| Row {
                level: Level::Memory,
                buffer,
                latency: Some(value("90.500")),
                retries: Vec::new(),
            })
            .collect();
        let text = section(&[latency_run(Status::Completed, Some(output), None)]);
        row(&text, "Memory  ")
            .into_iter()
            .map(|cells| cells[1].clone())
            .collect::<Vec<String>>()
    };
    assert_eq!(
        buffers(&[
            256 * MIB,
            GIB,
            3 * GIB / 2,
            16 * KIB,
            11 * MIB + MIB / 2,
            300_000_000,
            1000,
            1
        ]),
        [
            "256 MiB",
            "1 GiB",
            "1536 MiB",
            "16 KiB",
            "11.5 MiB",
            "286.1 MiB",
            "1000 bytes",
            "1 byte"
        ]
    );

    let arrays = |millions: u64, threads: usize| {
        let mut output = stream_output();
        output.millions = millions;
        output.threads = threads;
        let text = section(&[stream_run(Status::Completed, Some(output), None)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  STREAM              "))
            .expect("the parameters")
            .to_owned()
    };
    assert_eq!(
        arrays(32, 4),
        "5.10: 32 million elements per array (244 MiB, 732 MiB for the three), the fastest of 10 \
         passes each; the threads spread evenly over the nodes"
    );
    assert_eq!(
        arrays(2, 1),
        "5.10: 2 million elements per array (15 MiB, 46 MiB for the three), the fastest of 10 \
         passes each"
    );
    assert!(
        arrays(64, 1).contains("64 million elements per array (488 MiB, 1465 MiB for the three)")
    );
    let short = |budget: u64| {
        let stop = Some(Stop::Skipped(stream::Failure::Short { budget }));
        let text = section(&[stream_run(Status::Skipped(None), None, stop)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  skipped: "))
            .expect("the reason")
            .to_owned()
    };
    assert_eq!(
        short(30 * MIB),
        "available memory short: the three arrays need at least 46 MiB, and half of the \
         available memory is only 30 MiB"
    );
}

#[test]
fn notes_count_the_missing_levels() {
    let missing = |levels: Vec<Level>| {
        let mut output = latency_output();
        output.missing = levels;
        let text = section(&[latency_run(Status::Completed, Some(output), None)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  Note                "))
            .expect("the note")
            .to_owned()
    };
    assert_eq!(
        missing(vec![Level::L3]),
        "no word on L3; that level is left out"
    );
    assert_eq!(
        missing(vec![Level::L2, Level::L3]),
        "no word on L2 or L3; those levels are left out"
    );
    assert_eq!(
        missing(vec![Level::L1, Level::L2, Level::L3]),
        "no word on L1, L2 or L3; those levels are left out"
    );
}

#[test]
fn a_version_that_was_not_printed_is_left_out() {
    let mut sysbench = sysbench_output();
    sysbench.version = None;
    let mut stream = stream_output();
    stream.version = None;
    let text = section(&[
        sysbench_run(Status::Completed, Some(sysbench), None),
        stream_run(Status::Completed, Some(stream), None),
    ]);
    for line in [
        "    sysbench            sequential reads and writes, 10 s each; one block of 32 MiB for \
         one thread, one of 8 MiB per thread for several",
        "    STREAM              8 million elements per array (61 MiB, 183 MiB for the three), \
         the fastest of 10 passes each; the threads spread evenly over the nodes",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}
