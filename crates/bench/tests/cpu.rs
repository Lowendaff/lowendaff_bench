//! The CPU section of the report, from constructed runs of CPU 性能: the tool and its parameters,
//! the runs, the ratio, the notes on retries, and each way a run ends. Compared with snapshots,
//! and with the words that the item's own tests asserted before the reporter wrote them.

use std::time::Duration;

use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::orchestrator::Unmet;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::typed::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use plugin_cpu::sysbench;
use plugin_cpu::sysbench::Fault;
use plugin_cpu::sysbench::result::Rate;

fn sysbench_run(
    status: Status,
    output: Option<sysbench::Output>,
    stop: Option<Stop<sysbench::Failure>>,
) -> Run<Measured> {
    Run {
        id: "cpu.sysbench",
        group: "cpu",
        title: Some("CPU 性能"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_millis(40_100)),
        status,
        result: Some(Measured::CpuSysbench(Report { output, stop })),
    }
}

/// The report of `runs` from its CPU section on, with the summary.
fn section(runs: &[Run<Measured>]) -> String {
    let host = HostProfile::default();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs,
        total: Duration::from_secs(50),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("【CPU 性能】").expect("a CPU section");
    text[start..].to_owned()
}

fn run(threads: usize, rate: &str, retries: Vec<Fault>) -> sysbench::result::Run {
    sysbench::result::Run {
        threads,
        rate: Rate {
            text: rate.to_owned(),
            value: rate.parse().expect("a number"),
        },
        retries,
    }
}

fn single(version: Option<&str>) -> sysbench::Output {
    sysbench::Output {
        version: version.map(str::to_owned),
        single: run(1, "12842.07", Vec::new()),
        multi: None,
    }
}

fn exit(code: i32, last: &str) -> Fault {
    Fault::Tool(job::Fault::Exit {
        code,
        last: Some(last.to_owned()),
    })
}

fn timed_out() -> Fault {
    Fault::Tool(job::Fault::TimedOut(Duration::from_secs(60)))
}

#[test]
fn both_runs_show_in_a_table_with_the_ratio_and_the_retries() {
    let output = sysbench::Output {
        version: Some("1.0.20".to_owned()),
        single: run(1, "12842.07", vec![exit(1, "x")]),
        multi: Some(run(4, "49847.67", vec![timed_out(), Fault::NoRate])),
    };
    let text = section(&[sysbench_run(Status::Completed, Some(output), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  sysbench 1.0.20：素数上限 10000，每项 20 秒",
        "  项目    线程数  每秒事件数",
        "  单线程       1    12842.07",
        "  多线程       4    49847.67",
        "  倍数              多线程是单线程的 3.88 倍",
        "  说明              单线程测试失败 1 次后成功，最后一次失败的原因：sysbench 退出码 1：x",
        "  说明              多线程测试失败 2 次后成功，最后一次失败的原因：\
         sysbench 没有输出每秒事件数",
        "  完成，用时 40.1 秒",
    ] {
        assert!(
            text.lines().any(|shown| shown == line),
            "{line:?} in {text}"
        );
    }
}

#[test]
fn the_ratio_has_two_decimals() {
    let output = sysbench::Output {
        version: Some("1.0.20".to_owned()),
        single: run(1, "100.00", Vec::new()),
        multi: Some(run(2, "150.00", Vec::new())),
    };
    let text = section(&[sysbench_run(Status::Completed, Some(output), None)]);
    let ratio = "  倍数              多线程是单线程的 1.50 倍";
    assert!(text.lines().any(|line| line == ratio), "{text}");
}

#[test]
fn every_way_a_run_ends_shows_its_words() {
    let single_failed = vec![
        Fault::Tool(job::Fault::Spawn(
            "No such file or directory (os error 2)".to_owned(),
        )),
        Fault::Tool(job::Fault::Read("Broken pipe (os error 32)".to_owned())),
        Fault::Tool(job::Fault::Signal { last: None }),
        exit(1, "boom"),
    ];
    let runs = [
        sysbench_run(Status::Completed, Some(single(None)), None),
        sysbench_run(
            Status::Failed(None),
            None,
            Some(Stop::Failed(sysbench::Failure::Single(single_failed))),
        ),
        sysbench_run(
            Status::Failed(None),
            Some(single(Some("1.0.20"))),
            Some(Stop::Failed(sysbench::Failure::Multi(vec![timed_out(); 4]))),
        ),
        sysbench_run(
            Status::Failed(None),
            None,
            Some(Stop::Failed(sysbench::Failure::Cancelled)),
        ),
        Run {
            started: None,
            elapsed: None,
            ..sysbench_run(
                Status::Skipped(Some(Unmet::Tool {
                    name: "sysbench",
                    why: "下载失败".to_owned(),
                })),
                None,
                None,
            )
        },
        sysbench_run(Status::TimedOut, Some(single(Some("1.0.20"))), None),
        sysbench_run(Status::Cancelled, Some(single(Some("1.0.20"))), None),
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    sysbench：素数上限 10000，每项 20 秒",
        "    失败：sysbench 退出码 1：boom（共试了 4 次）",
        "    失败：多线程测试失败：超过 60 秒仍未结束，已停止（共试了 4 次）",
        "    失败：已中断",
        "    跳过：工具 sysbench 不可用：下载失败",
        "    超时，已保留超时前的结果",
        "    已中断",
    ] {
        assert!(
            text.lines().any(|shown| shown == line),
            "{line:?} in {text}"
        );
    }
}

#[test]
fn each_fault_of_a_tool_has_its_words() {
    let failed = |fault: Fault| {
        let stop = Stop::Failed(sysbench::Failure::Single(vec![fault]));
        let text = section(&[sysbench_run(Status::Failed(None), None, Some(stop))]);
        text.lines()
            .find_map(|line| line.strip_prefix("  失败："))
            .expect("a failure line")
            .to_owned()
    };
    let cases = [
        (
            Fault::Tool(job::Fault::Spawn(
                "No such file or directory (os error 2)".to_owned(),
            )),
            "无法运行 sysbench：No such file or directory (os error 2)（共试了 1 次）",
        ),
        (
            Fault::Tool(job::Fault::Read("Broken pipe (os error 32)".to_owned())),
            "无法读取 sysbench 的输出：Broken pipe (os error 32)（共试了 1 次）",
        ),
        (timed_out(), "超过 60 秒仍未结束，已停止（共试了 1 次）"),
        (
            exit(1, "Invalid value for the 'cpu_max_prime' option: 'abc'"),
            "sysbench 退出码 1：Invalid value for the 'cpu_max_prime' option: 'abc'（共试了 1 次）",
        ),
        (
            Fault::Tool(job::Fault::Exit {
                code: 2,
                last: None,
            }),
            "sysbench 退出码 2：没有输出（共试了 1 次）",
        ),
        (
            Fault::Tool(job::Fault::Signal {
                last: Some("Killed".to_owned()),
            }),
            "sysbench 被信号终止：Killed（共试了 1 次）",
        ),
        (
            Fault::Tool(job::Fault::Signal { last: None }),
            "sysbench 被信号终止：没有输出（共试了 1 次）",
        ),
        (Fault::NoRate, "sysbench 没有输出每秒事件数（共试了 1 次）"),
    ];
    for (fault, words) in cases {
        assert_eq!(failed(fault), words);
    }
}
