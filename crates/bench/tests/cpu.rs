//! The CPU section of the report, from constructed runs: for CPU performance the tool and its
//! parameters, the runs, the ratio and the notes on retries; for Geekbench 7 its version, scores,
//! link and notes; and each way a run ends. Compared with snapshots, and with the words that the
//! items' own tests asserted before the reporter wrote them.

use std::time::Duration;

use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use std::path::PathBuf;

use bench::orchestrator::Unmet;
use bench_api::Net;
use bench_api::Stop;
use bench_host::glibc::GlibcVersion;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::Adjustment;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Memory;
use plugin_cpu::geekbench::result::NoSwap;
use plugin_cpu::geekbench::result::Scores;
use plugin_cpu::geekbench::result::SwapLeft;
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
        title: Some("CPU performance"),
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
    let start = text.find("[CPU performance]").expect("a CPU section");
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
        "  sysbench 1.0.20: prime limit 10000, 20 s each",
        "  Test           Threads  Events/s",
        "  Single thread        1  12842.07",
        "  Multi-thread         4  49847.67",
        "  Ratio               multi-thread is 3.88 times single-thread",
        "  Note                single-thread test: worked after 1 failure; the last reason: \
         sysbench exit code 1: x",
        "  Note                multi-thread test: worked after 2 failures; the last reason: \
         sysbench printed no events per second",
        "  done in 40.1 s",
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
    let ratio = "  Ratio               multi-thread is 1.50 times single-thread";
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
                    why: "download failed".to_owned(),
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
        "    sysbench: prime limit 10000, 20 s each",
        "    failed: sysbench exit code 1: boom (4 attempts)",
        "    failed: multi-thread test: still running after 60 s, stopped (4 attempts)",
        "    failed: interrupted",
        "    skipped: tool sysbench unavailable: download failed",
        "    timed out; the results until then are kept",
        "    interrupted",
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
            .find_map(|line| line.strip_prefix("  failed: "))
            .expect("a failure line")
            .to_owned()
    };
    let cases = [
        (
            Fault::Tool(job::Fault::Spawn(
                "No such file or directory (os error 2)".to_owned(),
            )),
            "cannot run sysbench: No such file or directory (os error 2) (1 attempt)",
        ),
        (
            Fault::Tool(job::Fault::Read("Broken pipe (os error 32)".to_owned())),
            "cannot read the output of sysbench: Broken pipe (os error 32) (1 attempt)",
        ),
        (timed_out(), "still running after 60 s, stopped (1 attempt)"),
        (
            exit(1, "Invalid value for the 'cpu_max_prime' option: 'abc'"),
            "sysbench exit code 1: Invalid value for the 'cpu_max_prime' option: 'abc' \
             (1 attempt)",
        ),
        (
            Fault::Tool(job::Fault::Exit {
                code: 2,
                last: None,
            }),
            "sysbench exit code 2: no output (1 attempt)",
        ),
        (
            Fault::Tool(job::Fault::Signal {
                last: Some("Killed".to_owned()),
            }),
            "sysbench ended by a signal: Killed (1 attempt)",
        ),
        (
            Fault::Tool(job::Fault::Signal { last: None }),
            "sysbench ended by a signal: no output (1 attempt)",
        ),
        (
            Fault::NoRate,
            "sysbench printed no events per second (1 attempt)",
        ),
    ];
    for (fault, words) in cases {
        assert_eq!(failed(fault), words);
    }
}

const GIB: u64 = 1 << 30;

const MIB: u64 = 1 << 20;

const UNREACHABLE: &str = "unable to resolve host browser.geekbench.com";

fn geekbench_run(
    status: Status,
    output: Option<geekbench::Output>,
    stop: Option<Stop<geekbench::Failure>>,
) -> Run<Measured> {
    Run {
        id: "cpu.geekbench",
        group: "cpu",
        title: Some("Geekbench 7"),
        started: Some(Duration::from_secs(60)),
        elapsed: Some(Duration::from_secs(300)),
        status,
        result: Some(Measured::CpuGeekbench(Report { output, stop })),
    }
}

fn scores(url: Option<&str>) -> Scores {
    Scores {
        version: Some("7.1.0".to_owned()),
        single: 1771,
        multi: 4347,
        url: url.map(str::to_owned),
    }
}

fn failed(output: geekbench::Output, failure: geekbench::Failure) -> Run<Measured> {
    geekbench_run(
        Status::Failed(None),
        Some(output),
        Some(Stop::Failed(failure)),
    )
}

#[test]
fn geekbench_shows_its_version_scores_link_and_notes() {
    let output = geekbench::Output {
        memory: Some(Memory::Swapped {
            need: 5 * GIB,
            available: 1730 * MIB,
            gib: 4,
        }),
        first_to_go: Some(Adjustment::NotSet(
            "No such file or directory (os error 2)".to_owned(),
        )),
        scores: Some(scores(Some("https://browser.geekbench.com/v7/cpu/840201"))),
        uploads: vec![UNREACHABLE.to_owned(); 2],
        swap_left: Some(SwapLeft {
            path: PathBuf::from("/root/run/swapfile"),
            error: "Device or resource busy (os error 16)".to_owned(),
        }),
    };
    let text = section(&[geekbench_run(Status::Completed, Some(output), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  Geekbench 7.1.0",
        "  Single-core         1771",
        "  Multi-core          4347",
        "  Result link         https://browser.geekbench.com/v7/cpu/840201",
        "  Note                available memory short (needs about 5 GiB, 1.6 GiB available); \
         added 4 GiB of temporary swap, so the scores may be low",
        "  Note                cannot make Geekbench the first to go when memory runs out: \
         No such file or directory (os error 2)",
        "  Note                ran again after 2 failed uploads; the reason: \
         unable to resolve host browser.geekbench.com",
        "  Note                cannot turn off the temporary swap: Device or resource busy \
         (os error 16); run swapoff /root/run/swapfile by hand, then delete the file",
        "  done in 300.0 s",
    ] {
        assert!(
            text.lines().any(|shown| shown == line),
            "{line:?} in {text}"
        );
    }
}

#[test]
fn every_way_geekbench_ends_shows_its_words() {
    let short = |why| {
        Some(Memory::Short {
            need: 2 * GIB,
            available: 600 * MIB,
            why,
        })
    };
    let swapped = Some(Memory::Swapped {
        need: 5 * GIB,
        available: GIB,
        gib: 4,
    });
    let runs = [
        geekbench_run(
            Status::Completed,
            Some(geekbench::Output {
                scores: Some(scores(None)),
                ..geekbench::Output::default()
            }),
            None,
        ),
        failed(
            geekbench::Output {
                memory: short(NoSwap::Unused),
                uploads: vec![UNREACHABLE.to_owned(); 3],
                ..geekbench::Output::default()
            },
            geekbench::Failure::Upload(UNREACHABLE.to_owned()),
        ),
        failed(
            geekbench::Output {
                memory: swapped,
                ..geekbench::Output::default()
            },
            geekbench::Failure::OutOfMemory,
        ),
        failed(
            geekbench::Output::default(),
            geekbench::Failure::Run(geekbench::Fault::Tool(job::Fault::TimedOut(
                Duration::from_secs(3600),
            ))),
        ),
        failed(
            geekbench::Output::default(),
            geekbench::Failure::Run(geekbench::Fault::NoScores),
        ),
        failed(
            geekbench::Output::default(),
            geekbench::Failure::Run(geekbench::Fault::Tool(job::Fault::Exit {
                code: 255,
                last: Some(
                    "Error: The `--no-upload` switch is only available in Geekbench 7 Pro."
                        .to_owned(),
                ),
            })),
        ),
        failed(geekbench::Output::default(), geekbench::Failure::Cancelled),
        Run {
            started: None,
            elapsed: None,
            ..geekbench_run(
                Status::Skipped(Some(Unmet::Glibc {
                    needed: GlibcVersion {
                        major: 2,
                        minor: 35,
                    },
                    found: Some(GlibcVersion {
                        major: 2,
                        minor: 31,
                    }),
                })),
                None,
                None,
            )
        },
        Run {
            started: None,
            elapsed: None,
            ..geekbench_run(Status::Skipped(Some(Unmet::Network(Net::Ipv4))), None, None)
        },
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    Result link         unavailable",
        "    Note                available memory may be short (needs about 2 GiB, 0.5 GiB \
         available); no temporary swap: vm.swappiness is 0 here, so the kernel would not swap \
         memory out",
        "    failed: the upload failed (unable to resolve host browser.geekbench.com); the free \
         version gives scores only after an upload",
        "    failed: ended by the system for want of memory while running",
        "    failed: still running after 60 minutes, stopped",
        "    failed: Geekbench printed no scores",
        "    failed: Geekbench exit code 255: Error: The `--no-upload` switch is only available \
         in Geekbench 7 Pro.",
        "    failed: interrupted",
        "    skipped: needs glibc 2.35 or newer; this host has 2.31",
        "    skipped: needs an IPv4 connection",
    ] {
        assert!(
            text.lines().any(|shown| shown == line),
            "{line:?} in {text}"
        );
    }
    assert!(
        !text.contains("ran again after"),
        "only a run that gave scores ran again with success: {text}"
    );
}

#[test]
fn each_fault_of_geekbench_has_its_words() {
    let failed_with = |fault: job::Fault| {
        let failure = geekbench::Failure::Run(geekbench::Fault::Tool(fault));
        let text = section(&[failed(geekbench::Output::default(), failure)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  failed: "))
            .expect("a failure line")
            .to_owned()
    };
    let cases = [
        (
            job::Fault::Exit {
                code: 1,
                last: Some("https://browser.geekbench.com/v7/cpu/1".to_owned()),
            },
            "Geekbench exit code 1: https://browser.geekbench.com/v7/cpu/1",
        ),
        (
            job::Fault::Signal { last: None },
            "Geekbench ended by a signal: no output",
        ),
        (
            job::Fault::Spawn("Exec format error (os error 8)".to_owned()),
            "cannot run Geekbench: Exec format error (os error 8)",
        ),
        (
            job::Fault::Read("Broken pipe (os error 32)".to_owned()),
            "cannot read the output of Geekbench: Broken pipe (os error 32)",
        ),
        (
            job::Fault::TimedOut(Duration::from_secs(3600)),
            "still running after 60 minutes, stopped",
        ),
    ];
    for (fault, words) in cases {
        assert_eq!(failed_with(fault), words);
    }
}

#[test]
fn each_note_on_memory_has_its_words() {
    let note = |output: geekbench::Output| {
        let text = section(&[geekbench_run(Status::Completed, Some(output), None)]);
        text.lines()
            .filter_map(|line| line.strip_prefix("  Note                "))
            .map(str::to_owned)
            .collect::<Vec<String>>()
    };
    let short = |why| geekbench::Output {
        memory: Some(Memory::Short {
            need: 5 * GIB,
            available: 1730 * MIB,
            why,
        }),
        ..geekbench::Output::default()
    };
    let estimate = "available memory may be short (needs about 5 GiB, 1.6 GiB available); \
                    no temporary swap: ";
    let cases = [
        (
            NoSwap::NoRoom(Some(GIB + GIB / 2)),
            "the run directory's disk has only 1.5 GiB free",
        ),
        (
            NoSwap::NoRoom(None),
            "the free space of the run directory's disk cannot be read",
        ),
        (
            NoSwap::Denied,
            "not permitted (containers usually forbid it)",
        ),
        (
            NoSwap::Unsupported,
            "the run directory's filesystem does not support swap files",
        ),
        (
            NoSwap::Error("No space left on device (os error 28)".to_owned()),
            "No space left on device (os error 28)",
        ),
    ];
    for (why, words) in cases {
        assert_eq!(note(short(why)), [format!("{estimate}{words}")]);
    }
    let kept = geekbench::Output {
        first_to_go: Some(Adjustment::NotRestored(
            "Permission denied (os error 13)".to_owned(),
        )),
        ..geekbench::Output::default()
    };
    assert_eq!(
        note(kept),
        ["cannot restore this program's oom_score_adj: Permission denied (os error 13)"]
    );
    let done = geekbench::Output {
        first_to_go: Some(Adjustment::Done),
        ..geekbench::Output::default()
    };
    assert!(note(done).is_empty(), "nothing to say when it went first");
}
