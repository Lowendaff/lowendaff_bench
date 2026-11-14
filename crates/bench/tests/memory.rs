//! The memory section of the report, from constructed runs: each chapter of 内存性能, sysbench,
//! STREAM, latency and NUMA, with its parameters, tables and notes, and each way it ends.
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
    let start = text.find("【内存性能】").expect("a memory section");
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
    run("memory.sysbench", "内存 sysbench", status, result)
}

fn stream_run(
    status: Status,
    output: Option<stream::Output>,
    stop: Option<Stop<stream::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemoryStream(Report { output, stop });
    run("memory.stream", "内存 STREAM", status, result)
}

fn latency_run(
    status: Status,
    output: Option<latency::Output>,
    stop: Option<Stop<latency::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemoryLatency(Report { output, stop });
    run("memory.latency", "内存延迟", status, result)
}

fn numa_run(
    status: Status,
    output: Option<numa::Output>,
    stop: Option<Stop<numa::Failure>>,
) -> Run<Measured> {
    let result = Measured::MemoryNuma(Report { output, stop });
    run("memory.numa", "内存 NUMA", status, result)
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
        "  内存 sysbench",
        "    sysbench          1.0.20：顺序读写，每项 10 秒；单线程一块 32 MiB，\
         多线程每个线程一块 8 MiB",
        "    sysbench 结果",
        "  内存 STREAM",
        "    STREAM            5.10：每个数组 800 万个元素（61 MiB，三个合计 183 MiB），\
         每项取 10 遍中最快的一遍；多线程按节点均匀绑定",
        "    STREAM 结果",
        "  内存延迟",
        "    延迟              multichase：随机指针追逐，每档 2.5 秒",
        "    延迟结果",
        "  内存 NUMA",
        "    NUMA              2 个节点；带宽用 multiload 的 stream-triad，每格 3 次采样取最快；\
         延迟同上",
        "    NUMA 带宽（MiB/s；行是线程所在节点，列是内存所在节点）",
        "    NUMA 延迟（ns）",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(
        row(&text, "项目"),
        [
            ["项目", "单线程", "多线程（4 线程）"],
            ["项目", "单线程", "多线程（4 线程）"]
        ]
    );
    assert_eq!(
        row(&text, "读取"),
        [["读取", "100.00 MiB/s", "400.00 MiB/s"]]
    );
    assert_eq!(
        row(&text, "写入"),
        [["写入", "50.00 MiB/s", "200.00 MiB/s"]]
    );
    assert_eq!(
        row(&text, "Copy"),
        [["Copy", "10000.1 MB/s", "40000.1 MB/s"]]
    );
    assert_eq!(
        row(&text, "Triad"),
        [["Triad", "11500.4 MB/s", "46000.4 MB/s"]]
    );
    assert_eq!(row(&text, "层级"), [["层级", "缓冲区", "延迟"]]);
    assert_eq!(row(&text, "L1"), [["L1", "16 KiB", "1.500 ns"]]);
    assert_eq!(row(&text, "L2"), [["L2", "512 KiB", "4.200 ns"]]);
    assert_eq!(row(&text, "L3"), [["L3", "4 MiB", "12.000 ns"]]);
    assert_eq!(row(&text, "内存  "), [["内存", "32 MiB", "90.500 ns"]]);
    assert_eq!(
        row(&text, "节点 0"),
        [
            vec!["节点 0", "节点 1"],
            vec!["节点 0", "52000", "31000"],
            vec!["节点 0", "节点 1"],
            vec!["节点 0", "95.000", "140.1"]
        ]
    );
    assert_eq!(
        text.matches("完成，用时 41.2 秒").count(),
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
        "    sysbench          1.0.20：顺序读写，每项 10 秒；单线程一块 32 MiB",
        "    STREAM            5.10：每个数组 800 万个元素（61 MiB，三个合计 183 MiB），\
         每项取 10 遍中最快的一遍",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(row(&text, "项目"), [["项目", "单线程"], ["项目", "单线程"]]);
    assert_eq!(row(&text, "读取"), [["读取", "100.00 MiB/s"]]);
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
        "    说明              没有末级缓存的信息，按 32 MiB 计算",
        "    说明              可用内存不足，测试数据小于末级缓存总和的 4 倍（32 MiB），\
         一部分读写落在缓存里，结果偏高",
        "    说明              多线程写入失败 2 次后成功，最后一次失败的原因：\
         超过 60 秒仍未结束，已停止",
        "    说明              未完全符合 STREAM 运行规则（每个数组至少为末级缓存总和的 4 倍）",
        "    说明              多线程测试失败 1 次后成功，最后一次失败的原因：\
         STREAM 的结果没有通过校验：Failed Validation on array a[], \
         AvgRelAbsErr > epsilon (1.000000e-13)",
        "    说明              没有 L3 的信息，跳过这一档",
        "    说明              可用内存不足，内存一档的缓冲区小于末级缓存总和的 4 倍（32 MiB），\
         一部分访问落在缓存里，延迟偏低",
        "    说明              L1 测试失败 1 次后成功，最后一次失败的原因：multichase 没有输出延迟",
        "    说明              线程在节点 1、内存在节点 0 的带宽测试失败 1 次后成功，\
         最后一次失败的原因：超过 60 秒仍未结束，已停止",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    let sysbench_notes = text
        .lines()
        .skip_while(|line| !line.starts_with("    sysbench 结果"))
        .take_while(|line| !line.starts_with("  内存 STREAM"))
        .filter(|line| line.contains("说明"))
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
        "    失败：单线程读取：sysbench 退出码 1：boom（共试了 4 次）",
        "    失败：读不到可用内存（/proc/meminfo）",
        "    跳过：可用内存不足：三个数组最少要 46 MiB，可用内存的一半只有 16 MiB",
        "    失败：单线程测试：STREAM 退出码 1：boom（共试了 4 次）",
        "    已中断",
        "    失败：L3 测试：multichase 没有输出延迟（共试了 4 次）",
        "    跳过：容器不允许把内存绑定到指定节点",
        "    失败：线程在节点 0、内存在节点 1 的带宽测试：multiload 没有输出带宽（共试了 4 次）",
        "    跳过：需要多个 NUMA 节点",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(
        row(&text, "Copy"),
        [["Copy", "10000.1 MB/s", "—"]],
        "the interrupted STREAM keeps its first run"
    );
    let numa_tables = text.matches("NUMA 带宽").count();
    assert_eq!(numa_tables, 1, "no table of a refused matrix: {text}");
    assert_eq!(
        row(&text, "节点 1"),
        [["节点 1", "—", "—"], ["节点 1", "—", "—"]],
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
            .find_map(|line| line.split_once("最后一次失败的原因：").map(|(_, why)| why))
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
            "无法运行 multiload：Text file busy (os error 26)",
        ),
        (
            numa::Fault::Tool(spawn),
            Kind::Latency,
            "无法运行 multichase：Text file busy (os error 26)",
        ),
        (
            numa::Fault::Tool(read),
            Kind::Latency,
            "无法读取 multichase 的输出：Broken pipe (os error 32)",
        ),
        (
            numa::Fault::Tool(signal),
            Kind::Bandwidth,
            "multiload 被信号终止：没有输出",
        ),
        (
            numa::Fault::NoBandwidth,
            Kind::Bandwidth,
            "multiload 没有输出带宽",
        ),
        (
            numa::Fault::NoLatency,
            Kind::Latency,
            "multichase 没有输出延迟",
        ),
        (
            numa::Fault::Refused,
            Kind::Latency,
            "容器不允许把内存绑定到指定节点",
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
        "    说明              单线程读取失败 1 次后成功，最后一次失败的原因：\
         sysbench 没有输出传输速度",
        "    说明              多线程读取失败 1 次后成功，最后一次失败的原因：\
         sysbench 退出码 1：FATAL: Invalid value for memory-block-size: abc",
        "    说明              单线程写入失败 1 次后成功，最后一次失败的原因：\
         sysbench 被信号终止：Threads started!",
        "    说明              单线程测试失败 1 次后成功，最后一次失败的原因：STREAM 没有输出结果",
        "    说明              L1 测试失败 1 次后成功，最后一次失败的原因：\
         multichase 退出码 1：mbind: Invalid argument",
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
        row(&text, "内存  ")
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
            1000
        ]),
        [
            "256 MiB",
            "1 GiB",
            "1536 MiB",
            "16 KiB",
            "11.5 MiB",
            "286.1 MiB",
            "1000 字节"
        ]
    );

    let arrays = |millions: u64, threads: usize| {
        let mut output = stream_output();
        output.millions = millions;
        output.threads = threads;
        let text = section(&[stream_run(Status::Completed, Some(output), None)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  STREAM            "))
            .expect("the parameters")
            .to_owned()
    };
    assert_eq!(
        arrays(32, 4),
        "5.10：每个数组 3200 万个元素（244 MiB，三个合计 732 MiB），每项取 10 遍中最快的一遍；\
         多线程按节点均匀绑定"
    );
    assert_eq!(
        arrays(2, 1),
        "5.10：每个数组 200 万个元素（15 MiB，三个合计 46 MiB），每项取 10 遍中最快的一遍"
    );
    assert!(arrays(64, 1).contains("6400 万个元素（488 MiB，三个合计 1465 MiB）"));
    let short = |budget: u64| {
        let stop = Some(Stop::Skipped(stream::Failure::Short { budget }));
        let text = section(&[stream_run(Status::Skipped(None), None, stop)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  跳过："))
            .expect("the reason")
            .to_owned()
    };
    assert_eq!(
        short(30 * MIB),
        "可用内存不足：三个数组最少要 46 MiB，可用内存的一半只有 30 MiB"
    );
}

#[test]
fn notes_count_the_missing_levels() {
    let missing = |levels: Vec<Level>| {
        let mut output = latency_output();
        output.missing = levels;
        let text = section(&[latency_run(Status::Completed, Some(output), None)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  说明              "))
            .expect("the note")
            .to_owned()
    };
    assert_eq!(missing(vec![Level::L3]), "没有 L3 的信息，跳过这一档");
    assert_eq!(
        missing(vec![Level::L2, Level::L3]),
        "没有 L2、L3 的信息，跳过这两档"
    );
    assert_eq!(
        missing(vec![Level::L1, Level::L2, Level::L3]),
        "没有 L1、L2、L3 的信息，跳过这三档"
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
        "    sysbench          顺序读写，每项 10 秒；单线程一块 32 MiB，多线程每个线程一块 8 MiB",
        "    STREAM            每个数组 800 万个元素（61 MiB，三个合计 183 MiB），\
         每项取 10 遍中最快的一遍；多线程按节点均匀绑定",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}
