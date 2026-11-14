//! The measurement with recorded runs instead of fio: the space it needs, the setup run, the four
//! tests in YABS's order, what it returns, publishes and logs, its failures, a cancellation, and
//! the test file, which is removed however the item ends.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::typed::Context;
use bench_api::typed::Progress;
use bench_api::typed::Stop;
use bench_host::profile::HostProfile;
use bench_host::storage::FilesystemKind;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_disk::fio::Failure;
use plugin_disk::fio::Fault;
use plugin_disk::fio::Output;
use plugin_disk::fio::measure;
use plugin_disk::fio::measure::FileSize;
use plugin_disk::fio::measure::Host;
use plugin_disk::fio::result::Outcome;
use plugin_disk::fio::result::Rates;
use tokio::time::Instant;

const FIO: &str = "/tools/fio";

const FILE: &str = "/run/work/test.fio";

const MIB: u64 = 1024 * 1024;

const GIB: u64 = 1024 * MIB;

struct Measured {
    result: Result<Output, Stop<Failure>>,
    published: Option<Output>,
    progress: Vec<String>,
    jobs: Vec<Job>,
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fio")
        .join(name);
    fs::read_to_string(&path).expect("fixture is readable")
}

fn finished(code: i32, text: &str) -> Result<job::Output, RunError> {
    Ok(job::Output {
        code: Some(code),
        text: text.to_owned(),
    })
}

/// A setup run that went well: the start of its terse line, as far as the version.
fn set_up() -> Result<job::Output, RunError> {
    finished(0, "3;fio-3.43;setup;0;0\n")
}

fn tested(block_size: &str) -> Result<job::Output, RunError> {
    finished(0, &fixture(&format!("rand_rw_{block_size}-x86_64.txt")))
}

fn host(available: Option<u64>) -> Host {
    Host {
        kind: None,
        available,
        inflation: None,
    }
}

/// Measures on `host` of architecture `arch`; the runs return `script` in order.
async fn measure(host: &Host, arch: &str, script: Vec<Result<job::Output, RunError>>) -> Measured {
    let profile = HostProfile {
        arch: arch.to_owned(),
        ..HostProfile::default()
    };
    let (ctx, mut channels) = Context::<Output>::new(
        Arc::new(profile),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(600),
        "/run/work".into(),
        CancellationToken::new(),
    );
    let script = Mutex::new(VecDeque::from(script));
    let jobs = Mutex::new(Vec::new());
    let result = measure::measure(&ctx, Path::new(FIO), Path::new(FILE), host, |job| {
        jobs.lock().expect("the jobs").push(job);
        let next = script
            .lock()
            .expect("the script")
            .pop_front()
            .expect("every run is scripted");
        async move { next }
    })
    .await;
    assert!(
        script.lock().expect("the script").is_empty(),
        "every scripted run happened"
    );
    let mut progress = Vec::new();
    while let Ok(item) = channels.progress.try_recv() {
        if let Progress::Line(line) = item {
            progress.push(line);
        }
    }
    let published = channels.published.borrow().clone();
    Measured {
        result,
        published,
        progress,
        jobs: jobs.into_inner().expect("the jobs"),
    }
}

fn two_gib() -> FileSize {
    FileSize::for_arch("x86_64")
}

fn outcomes(output: &Output) -> Vec<(&str, Outcome)> {
    output
        .tests
        .iter()
        .map(|test| (test.block_size, test.outcome.clone()))
        .collect()
}

fn done(read_kib: u64, read_iops: u64, write_kib: u64, write_iops: u64) -> Outcome {
    Outcome::Done(Rates {
        read_kib,
        read_iops,
        write_kib,
        write_iops,
    })
}

#[tokio::test]
async fn the_setup_then_four_tests_in_yabss_order_and_each_is_published() {
    let script = vec![
        set_up(),
        tested("4k"),
        tested("64k"),
        tested("512k"),
        tested("1m"),
    ];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    let Ok(output) = &measured.result else {
        panic!("the item completes: {:?}", measured.result);
    };
    let program = Path::new(FIO);
    let file = Path::new(FILE);
    assert_eq!(
        measured.jobs,
        [
            measure::setup(program, file, two_gib()),
            measure::test(program, file, two_gib(), "4k"),
            measure::test(program, file, two_gib(), "64k"),
            measure::test(program, file, two_gib(), "512k"),
            measure::test(program, file, two_gib(), "1m"),
        ]
    );
    assert_eq!(measured.jobs[0].limit, Duration::from_secs(600));
    assert_eq!(
        measured.jobs[1].limit,
        Duration::from_secs(35),
        "YABS's timeout 35"
    );
    assert!(
        measured.jobs.iter().all(|job| job.quotable),
        "fio's error lines help"
    );
    assert_eq!(output.version.as_deref(), Some("3.43"));
    assert_eq!(output.size, 2 * GIB);
    assert_eq!(output.zfs_needed, None);
    assert_eq!(
        outcomes(output),
        [
            ("4k", done(597_651, 149_412, 604_844, 151_211)),
            ("64k", done(4_189_866, 65_466, 4_548_266, 71_066)),
            ("512k", done(8_850_285, 17_285, 9_874_285, 19_285)),
            ("1m", done(5_306_181, 5_181, 6_609_454, 6_454)),
        ]
    );
    assert_eq!(measured.published.as_ref(), Some(output));
    assert_eq!(
        measured.progress,
        [
            "生成测试文件",
            "4K 随机读写测试",
            "64K 随机读写测试",
            "512K 随机读写测试",
            "1M 随机读写测试"
        ]
    );
}

#[tokio::test]
async fn tests_without_a_result_keep_why_and_the_others_run() {
    let timed_out = job::Fault::TimedOut(Duration::from_secs(35));
    let busy = job::Fault::Spawn("Text file busy (os error 26)".to_owned());
    let script = vec![
        set_up(),
        tested("4k"),
        Err(RunError::Fault(timed_out.clone())),
        finished(
            1,
            "fio: io_u error on file /run/work/test.fio: Input/output error\n",
        ),
        Err(RunError::Fault(busy.clone())),
    ];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    let Ok(output) = &measured.result else {
        panic!("one test gave rates: {:?}", measured.result);
    };
    let io_error = "fio: io_u error on file /run/work/test.fio: Input/output error";
    assert_eq!(
        outcomes(output)[1..],
        [
            ("64k", Outcome::Failed(Fault::Tool(timed_out))),
            (
                "512k",
                Outcome::Failed(Fault::Tool(job::Fault::Exit {
                    code: 1,
                    last: Some(io_error.to_owned()),
                }))
            ),
            ("1m", Outcome::Failed(Fault::Tool(busy))),
        ]
    );
}

#[tokio::test]
async fn no_test_with_rates_fails_the_item_and_keeps_each_reason() {
    let script = vec![
        set_up(),
        finished(0, ""),
        finished(0, ""),
        finished(0, ""),
        finished(0, ""),
    ];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoResult)));
    let published = measured.published.expect("the tests were published");
    assert_eq!(published.tests.len(), 4);
    assert!(
        published
            .tests
            .iter()
            .all(|test| test.outcome == Outcome::Failed(Fault::NoResult))
    );
}

#[tokio::test]
async fn the_test_directory_decides_before_anything_runs() {
    let measured = measure(&host(None), "x86_64", Vec::new()).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoSpaceInfo)));

    let in_memory = Host {
        kind: Some(FilesystemKind::Memory),
        ..host(Some(100 * GIB))
    };
    let measured = measure(&in_memory, "x86_64", Vec::new()).await;
    assert_eq!(
        measured.result,
        Err(Stop::Skipped(Failure::InMemory)),
        "whatever its free space"
    );

    for (arch, needed) in [("x86_64", 2 * GIB), ("aarch64", 512 * MIB)] {
        for kind in [None, Some(FilesystemKind::Other), Some(FilesystemKind::Zfs)] {
            let short = Host {
                kind,
                ..host(Some(needed - 1))
            };
            let measured = measure(&short, arch, Vec::new()).await;
            assert_eq!(
                measured.result,
                Err(Stop::Skipped(Failure::Space {
                    needed,
                    available: needed - 1,
                })),
                "{arch}"
            );
            assert!(measured.jobs.is_empty(), "nothing runs");
            // Exactly enough passes, as in YABS.
            let enough = Host {
                kind,
                ..host(Some(needed))
            };
            let measured = measure(&enough, arch, vec![Err(RunError::Cancelled)]).await;
            assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
            assert_eq!(
                measured.jobs,
                [measure::setup(
                    Path::new(FIO),
                    Path::new(FILE),
                    FileSize::for_arch(arch)
                )],
                "{arch}: the setup ran, with the architecture's size"
            );
        }
    }
}

#[test]
fn zfs_with_little_free_space_needs_twice_its_inflation() {
    let zfs = |inflation| Host {
        kind: Some(FilesystemKind::Zfs),
        available: None,
        inflation,
    };
    assert_eq!(measure::zfs_needed(&zfs(Some(24)), 48 * GIB - 1), Some(48));
    assert_eq!(measure::zfs_needed(&zfs(Some(24)), 48 * GIB), None);
    assert_eq!(measure::zfs_needed(&zfs(None), 0), None, "no zfs module");
    let other = Host {
        kind: Some(FilesystemKind::Other),
        ..zfs(Some(24))
    };
    assert_eq!(measure::zfs_needed(&other, 0), None);
    assert_eq!(measure::zfs_needed(&host(None), 0), None);
}

#[tokio::test]
async fn zfs_with_little_free_space_is_kept_in_the_result() {
    let zfs = Host {
        kind: Some(FilesystemKind::Zfs),
        available: Some(10 * GIB),
        inflation: Some(24),
    };
    let measured = measure(&zfs, "x86_64", vec![Err(RunError::Cancelled)]).await;
    let published = measured.published.expect("published before the setup");
    assert_eq!(published.zfs_needed, Some(48));
}

#[tokio::test]
async fn a_filesystem_without_direct_io_skips_the_item() {
    let script = vec![finished(1, &fixture("setup-ramfs.txt"))];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    assert_eq!(measured.result, Err(Stop::Skipped(Failure::NoDirectIo)));
}

#[tokio::test]
async fn a_failed_setup_fails_the_item_with_fios_error_line() {
    let script = vec![finished(1, &fixture("setup-full.txt"))];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    let error =
        "fio: pid=0, err=28/file:filesetup.c:241, func=write, error=No space left on device";
    assert_eq!(
        measured.result,
        Err(Stop::Failed(Failure::Setup(Fault::Tool(
            job::Fault::Exit {
                code: 1,
                last: Some(error.to_owned()),
            }
        ))))
    );

    let busy = job::Fault::Spawn("Text file busy (os error 26)".to_owned());
    let script = vec![Err(RunError::Fault(busy.clone()))];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    assert_eq!(
        measured.result,
        Err(Stop::Failed(Failure::Setup(Fault::Tool(busy))))
    );
}

#[tokio::test]
async fn a_cancellation_ends_the_item_and_keeps_the_tests_that_ended() {
    let script = vec![
        set_up(),
        tested("4k"),
        tested("64k"),
        Err(RunError::Cancelled),
    ];
    let measured = measure(&host(Some(100 * GIB)), "x86_64", script).await;
    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    let published = measured.published.expect("two tests were published");
    let ended: Vec<&str> = published.tests.iter().map(|test| test.block_size).collect();
    assert_eq!(ended, ["4k", "64k"]);
}

#[test]
fn the_file_size_follows_the_architecture_as_in_yabs() {
    let size = |arch| {
        let size = FileSize::for_arch(arch);
        (size.bytes, size.fio)
    };
    assert_eq!(size("x86_64"), (2 * GIB, "2G"));
    assert_eq!(size("aarch64"), (512 * MIB, "512M"));
    assert_eq!(size("arm"), (512 * MIB, "512M"));
    assert_eq!(size("riscv64"), (512 * MIB, "512M"));
    assert_eq!(size("powerpc64"), (2 * GIB, "2G"));
}

#[tokio::test]
async fn the_test_file_is_removed_however_the_item_ends() {
    let scripts = [
        vec![
            set_up(),
            tested("4k"),
            tested("64k"),
            tested("512k"),
            tested("1m"),
        ],
        vec![finished(1, &fixture("setup-full.txt"))],
        vec![set_up(), tested("4k"), Err(RunError::Cancelled)],
    ];
    for script in scripts {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let file = dir.path().join("test.fio");
        let (ctx, _channels) = Context::<Output>::new(
            Arc::new(HostProfile::default()),
            Arc::new(BTreeMap::new()),
            Instant::now() + Duration::from_secs(600),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        );
        let script = Mutex::new(VecDeque::from(script));
        let result = measure::measure(&ctx, Path::new(FIO), &file, &host(Some(100 * GIB)), |_| {
            // fio lays out the file in its setup run.
            fs::write(&file, "laid out").expect("the directory is writable");
            let next = script
                .lock()
                .expect("the script")
                .pop_front()
                .expect("every run is scripted");
            async move { next }
        })
        .await;
        assert!(!file.exists(), "removed after {result:?}");
    }
}
