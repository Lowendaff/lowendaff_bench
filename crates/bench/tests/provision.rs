//! Tool provisioning: verification before execution, cleanup on failure, per-tool
//! failure reasons, downloads in byte ranges over several connections, and a plugin running the
//! tool it was given.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Quiet;
use bench::orchestrator::Settings;
use bench::orchestrator::Status;
use bench::provision;
use bench::provision::DownloadError;
use bench::provision::MIN_RANGE;
use bench::provision::Part;
use bench::registry;
use bench::results::Measured;
use bench::results::Report;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use common::Captured;
use plugin_disk::fio;
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;

mod common;

const SCRIPT: &[u8] = b"#!/bin/sh\necho fio-3.39\n";
const SCRIPT_SHA: &str = "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8";
const SCRIPT_LEN: u64 = SCRIPT.len() as u64;
const FIO_URL: &str = "https://tools.example/fio";

const fn asset(arch: &'static str, sha256: &'static str, size: u64) -> ToolAsset {
    ToolAsset {
        arch,
        url: FIO_URL,
        sha256,
        size,
    }
}

const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.39",
    form: ToolForm::Executable,
    assets: &[
        asset("x86_64", SCRIPT_SHA, SCRIPT_LEN),
        asset("aarch64", SCRIPT_SHA, SCRIPT_LEN),
    ],
};

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read the test directory")
        .map(|entry| {
            entry
                .expect("read a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[tokio::test]
async fn install_makes_only_a_verified_file_executable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = provision::install(SCRIPT, &FIO, &FIO.assets[0], dir.path())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), SCRIPT);
    assert_ne!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
        0
    );
    assert_eq!(entries(dir.path()), ["fio"]);
}

#[tokio::test]
async fn install_rejects_a_hash_mismatch_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let wrong = asset(
        "x86_64",
        "0000000000000000000000000000000000000000000000000000000000000000",
        SCRIPT_LEN,
    );
    let err = provision::install(SCRIPT, &FIO, &wrong, dir.path())
        .await
        .unwrap_err();

    assert!(err.starts_with("SHA-256 不符：应为 0000"), "{err}");
    assert!(entries(dir.path()).is_empty());
}

#[tokio::test]
async fn install_stops_reading_past_the_declared_size() {
    let dir = tempfile::tempdir().unwrap();
    let err = provision::install(
        SCRIPT,
        &FIO,
        &asset("x86_64", SCRIPT_SHA, SCRIPT_LEN - 1),
        dir.path(),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "大小超过声明的 23 字节");
    assert!(entries(dir.path()).is_empty());
}

#[tokio::test]
async fn install_rejects_a_truncated_download() {
    let dir = tempfile::tempdir().unwrap();
    let err = provision::install(
        SCRIPT,
        &FIO,
        &asset("x86_64", SCRIPT_SHA, SCRIPT_LEN + 5),
        dir.path(),
    )
    .await
    .unwrap_err();

    assert_eq!(err, "大小不符：应为 29 字节，实际 24 字节");
    assert!(entries(dir.path()).is_empty());
}

const IPERF3_ARM_ONLY: ToolSpec = ToolSpec {
    name: "iperf3",
    version: "3.19",
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "aarch64",
        url: "https://tools.example/iperf3",
        sha256: SCRIPT_SHA,
        size: SCRIPT_LEN,
    }],
};

const SYSBENCH: ToolSpec = ToolSpec {
    name: "sysbench",
    version: "1.0.20",
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/sysbench",
        sha256: SCRIPT_SHA,
        size: SCRIPT_LEN,
    }],
};

#[tokio::test]
async fn provision_all_reports_every_failure_with_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let fetch = |url: &'static str, _range| async move {
        if url == FIO_URL {
            Ok(Part::Whole(SCRIPT))
        } else {
            Err(DownloadError::Final("HTTP 404".to_owned()))
        }
    };
    let provisioned = provision::provision_all(
        &[FIO, IPERF3_ARM_ONLY, SYSBENCH],
        "x86_64",
        dir.path(),
        32,
        &CancellationToken::new(),
        fetch,
    )
    .await;

    assert_eq!(provisioned.ready.keys().collect::<Vec<_>>(), ["fio"]);
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([
            ("iperf3".to_owned(), "没有适用于 x86_64 的版本".to_owned()),
            ("sysbench".to_owned(), "HTTP 404".to_owned()),
        ])
    );
}

#[test]
fn required_tools_are_unique_and_in_plan_order() {
    let requires = |tools: &'static [ToolSpec]| Requirements {
        tools,
        ..Requirements::NONE
    };
    let tools = provision::required_tools([
        requires(&[FIO, SYSBENCH]),
        requires(&[SYSBENCH, IPERF3_ARM_ONLY]),
    ]);

    assert_eq!(
        tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
        ["fio", "sysbench", "iperf3"]
    );
}

/// Runs the fio it was given, and returns what it printed as the version in a result of fio's
/// type.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunsFio {}

impl Plugin for RunsFio {
    type Output = fio::Output;
    type Failure = fio::Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.fio",
        requires: Requirements {
            tools: &[FIO],
            net: Net::None,
            platforms: &[],
            glibc: None,
            numa: false,
        },
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, ctx: &Context<fio::Output>) -> Result<fio::Output, Stop<fio::Failure>> {
        let program = ctx
            .tool("fio")
            .expect("the orchestrator runs a plugin only when its tools are ready");
        let output = tokio::process::Command::new(program)
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|_| Stop::Failed(fio::Failure::NoResult))?;
        Ok(printed(String::from_utf8_lossy(&output.stdout).trim()))
    }
}

/// A result of fio's type that holds only the version.
fn printed(version: &str) -> fio::Output {
    fio::Output {
        version: Some(version.to_owned()),
        size: 0,
        zfs_needed: None,
        tests: Vec::new(),
    }
}

#[tokio::test]
async fn a_plugin_runs_the_tool_it_was_provisioned_with() {
    let run_dir = tempfile::tempdir().unwrap();
    let registry = [registry::entry::<RunsFio>(Measured::DiskFio)];
    let planned = orchestrator::plan(&registry, &["disk.fio"], &BTreeMap::new()).unwrap();
    let tools = provision::required_tools(planned.iter().map(|planned| *planned.requires()));
    let provisioned = provision::provision_all(
        &tools,
        std::env::consts::ARCH,
        run_dir.path(),
        32,
        &CancellationToken::new(),
        |_, _| async { Ok::<_, DownloadError>(Part::Whole(SCRIPT)) },
    )
    .await;
    let settings = Settings {
        grace: Duration::from_secs(1),
        work_root: run_dir.path().to_path_buf(),
        origin: tokio::time::Instant::now(),
        tools: Arc::new(provisioned.ready),
        failed_tools: BTreeMap::new(),
        host: Arc::default(),
    };

    let runs =
        orchestrator::run_plan(planned, &CancellationToken::new(), &settings, &mut Quiet).await;

    assert_eq!(runs[0].status, Status::Completed);
    assert_eq!(
        runs[0].result,
        Some(Measured::DiskFio(Report {
            output: Some(printed("fio-3.39")),
            stop: None,
        }))
    );
}

/// `size` bytes that differ from place to place, so that a range at the wrong place shows.
fn contents(size: u64) -> &'static [u8] {
    let bytes: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
    Box::leak(bytes.into_boxed_slice())
}

/// An executable tool pinned to the SHA-256 and size of `body`.
fn pinned_to(body: &[u8]) -> ToolSpec {
    let mut sha256 = String::new();
    for byte in Sha256::digest(body) {
        let _ = write!(sha256, "{byte:02x}");
    }
    let assets = Box::leak(Box::new([ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/big",
        sha256: Box::leak(sha256.into_boxed_str()),
        size: body.len() as u64,
    }]));
    ToolSpec {
        name: "big",
        version: "1.0",
        form: ToolForm::Executable,
        assets,
    }
}

/// A server of one file: whole, or in the byte ranges asked for while it `serves_ranges` (a
/// number of answers), the range starting at `short` one byte short, the range starting at
/// `long` one byte long, and the range starting at `failing` not at all. Notes each request's
/// range.
struct Served {
    body: &'static [u8],
    serves_ranges: usize,
    short: Option<u64>,
    long: Option<u64>,
    failing: Option<u64>,
    asked: Mutex<Vec<Option<Range<u64>>>>,
}

impl Served {
    fn new(body: &'static [u8]) -> Self {
        Self {
            body,
            serves_ranges: usize::MAX,
            short: None,
            long: None,
            failing: None,
            asked: Mutex::new(Vec::new()),
        }
    }

    async fn open(&self, range: Option<Range<u64>>) -> Result<Part<&'static [u8]>, DownloadError> {
        let answered = {
            let mut asked = self.asked.lock().expect("no test panics while holding it");
            asked.push(range.clone());
            asked.len()
        };
        let Some(range) = range.filter(|_| answered <= self.serves_ranges) else {
            return Ok(Part::Whole(self.body));
        };
        if self.failing == Some(range.start) {
            return Err(DownloadError::Transient("下载失败：HTTP 503".to_owned()));
        }
        let start = usize::try_from(range.start).expect("in memory");
        let mut end = usize::try_from(range.end).expect("in memory");
        if self.short == Some(range.start) {
            end -= 1;
        }
        if self.long == Some(range.start) {
            end += 1;
        }
        Ok(Part::Range(&self.body[start..end]))
    }

    /// The ranges asked for, in the order of the requests.
    fn asked(&self) -> Vec<Option<Range<u64>>> {
        self.asked
            .lock()
            .expect("no test panics while holding it")
            .clone()
    }
}

/// What came of provisioning one tool: the result, the log, the installed file's bytes and the
/// names left in the tools' directory.
struct Outcome {
    provisioned: provision::Provisioned,
    log: Vec<String>,
    installed: Option<Vec<u8>>,
    left: Vec<String>,
}

/// Provisions `tool` from `served` over up to `connections` connections, logging as `-v` does.
async fn provision_from(tool: ToolSpec, served: &Served, connections: usize) -> Outcome {
    let dir = tempfile::tempdir().expect("a directory for the tools");
    let (log, capturing) = Captured::start(true);
    let provisioned = provision::provision_all(
        &[tool],
        "x86_64",
        dir.path(),
        connections,
        &CancellationToken::new(),
        |_, range| served.open(range),
    )
    .await;
    drop(capturing);
    let installed = provisioned
        .ready
        .get("big")
        .map(|path| std::fs::read(path).expect("the installed tool reads"));
    Outcome {
        provisioned,
        log: log.lines(),
        installed,
        left: entries(dir.path()),
    }
}

/// The ranges that `served` was asked for, sorted, after checking that every request asked for
/// one and that the first request asked for the start of the file.
fn sorted_ranges(served: &Served) -> Vec<Range<u64>> {
    let asked = served.asked();
    assert_eq!(
        asked.first().cloned().flatten().map(|range| range.start),
        Some(0),
        "the first range comes first: {asked:?}"
    );
    let mut ranges: Vec<Range<u64>> = asked
        .into_iter()
        .map(|range| range.expect("every request asks for a range"))
        .collect();
    ranges.sort_by_key(|range| range.start);
    ranges
}

#[tokio::test]
async fn a_large_file_comes_in_byte_ranges_one_connection_each() {
    let size = 5 * MIN_RANGE + 3;
    let body = contents(size);
    for (connections, count) in [(32_usize, 5_u64), (2, 2)] {
        let served = Served::new(body);
        let outcome = provision_from(pinned_to(body), &served, connections).await;

        assert!(
            outcome.provisioned.failed.is_empty(),
            "{:?}",
            outcome.provisioned.failed
        );
        assert_eq!(
            outcome.installed.as_deref(),
            Some(body),
            "{connections} connections"
        );
        assert_eq!(outcome.left, ["big"], "no partial file is left");
        let ranges = sorted_ranges(&served);
        assert_eq!(ranges.len() as u64, count, "{connections} connections");
        let mut next = 0;
        for range in &ranges {
            assert_eq!(range.start, next, "no gap and no overlap: {ranges:?}");
            let length = range.end - range.start;
            assert!(
                length == size / count || length == size / count + 1,
                "even ranges: {ranges:?}"
            );
            next = range.end;
        }
        assert_eq!(next, size, "the ranges cover the file");
        assert_eq!(
            outcome.log,
            [
                "下载 https://tools.example/big".to_owned(),
                format!("big 1.0 分 {count} 段同时下载"),
                "准备工具：big 1.0 就绪".to_owned(),
            ]
        );
    }
}

#[tokio::test]
async fn one_connection_or_a_file_under_two_ranges_asks_for_the_whole_file() {
    for (size, connections, whole) in [
        (5 * MIN_RANGE, 1, true),
        (2 * MIN_RANGE - 1, 32, true),
        (2 * MIN_RANGE, 32, false),
    ] {
        let body = contents(size);
        let served = Served::new(body);
        let outcome = provision_from(pinned_to(body), &served, connections).await;

        assert_eq!(outcome.installed.as_deref(), Some(body), "{size} bytes");
        if whole {
            assert_eq!(
                served.asked(),
                [None],
                "{size} bytes, {connections} connections"
            );
        } else {
            assert_eq!(
                sorted_ranges(&served),
                [0..MIN_RANGE, MIN_RANGE..2 * MIN_RANGE]
            );
        }
    }
}

#[tokio::test]
async fn a_server_that_ignores_ranges_sends_the_whole_file_over_one_connection() {
    let body = contents(3 * MIN_RANGE);
    let served = Served {
        serves_ranges: 0,
        ..Served::new(body)
    };
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(outcome.installed.as_deref(), Some(body));
    assert_eq!(
        served.asked(),
        [Some(0..MIN_RANGE)],
        "one request, for the first range"
    );
    assert_eq!(
        outcome.log[1], "big 1.0：服务器不支持分段下载，改用一个连接",
        "{:?}",
        outcome.log
    );
}

#[tokio::test]
async fn a_range_that_goes_wrong_fails_the_tool_and_leaves_nothing() {
    let body = contents(4 * MIN_RANGE);
    let wrong = [
        (
            Served {
                short: Some(2 * MIN_RANGE),
                ..Served::new(body)
            },
            format!(
                "大小不符：一段应为 {MIN_RANGE} 字节，实际 {} 字节",
                MIN_RANGE - 1
            ),
        ),
        (
            Served {
                long: Some(MIN_RANGE),
                ..Served::new(body)
            },
            format!("大小超过一段应有的 {MIN_RANGE} 字节"),
        ),
        (
            Served {
                failing: Some(MIN_RANGE),
                ..Served::new(body)
            },
            "下载失败：HTTP 503".to_owned(),
        ),
        (
            Served {
                serves_ranges: 1,
                ..Served::new(body)
            },
            "下载失败：服务器不再分段返回".to_owned(),
        ),
    ];
    for (served, reason) in wrong {
        let outcome = provision_from(pinned_to(body), &served, 32).await;

        assert_eq!(outcome.provisioned.failed["big"], reason);
        assert_eq!(outcome.left, Vec::<String>::new(), "{reason}");
    }
}

#[tokio::test]
async fn ranges_that_do_not_add_up_to_the_pinned_file_fail_and_leave_nothing() {
    let body = contents(3 * MIN_RANGE);
    let mut other = body.to_vec();
    other[MIN_RANGE as usize + 7] ^= 1;
    let served = Served::new(Box::leak(other.into_boxed_slice()));
    let tool = pinned_to(body);
    let outcome = provision_from(tool, &served, 32).await;

    let reason = &outcome.provisioned.failed["big"];
    assert!(
        reason.starts_with(&format!(
            "SHA-256 不符：应为 {}，实际 ",
            tool.assets[0].sha256
        )),
        "{reason}"
    );
    assert_eq!(outcome.left, Vec::<String>::new());
}

#[tokio::test]
async fn ctrl_c_stops_the_download_under_way_and_the_tools_after_it() {
    let body = contents(4 * MIN_RANGE);
    let big = pinned_to(body);
    let dir = tempfile::tempdir().expect("a directory for the tools");
    let cancel = CancellationToken::new();
    let asked = Mutex::new(Vec::new());
    // The first range comes; the others never do, and Ctrl-C comes meanwhile.
    let fetch = |url: &'static str, range: Option<Range<u64>>| {
        asked
            .lock()
            .expect("no test panics while holding it")
            .push(url);
        let cancel = &cancel;
        async move {
            let range = range.expect("the large file comes in ranges");
            if range.start > 0 {
                cancel.cancel();
                std::future::pending::<()>().await;
            }
            let end = usize::try_from(range.end).expect("in memory");
            Ok::<_, DownloadError>(Part::Range(&body[..end]))
        }
    };

    let provisioned =
        provision::provision_all(&[big, FIO], "x86_64", dir.path(), 32, &cancel, fetch).await;

    assert_eq!(provisioned.ready, BTreeMap::new());
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([("big".to_owned(), "下载已中断".to_owned())])
    );
    let asked = asked
        .into_inner()
        .expect("no test panicked while holding it");
    assert_eq!(
        asked.len(),
        4,
        "the four ranges of the large file, and nothing of fio"
    );
    assert!(
        asked.iter().all(|url| *url == big.assets[0].url),
        "{asked:?}"
    );
    assert_eq!(
        entries(dir.path()),
        Vec::<String>::new(),
        "the partial file is removed"
    );
}

#[tokio::test]
async fn nothing_is_downloaded_after_ctrl_c() {
    let dir = tempfile::tempdir().expect("a directory for the tools");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let fetch = |url: &'static str, _range| async move {
        Err::<Part<&[u8]>, DownloadError>(DownloadError::Final(format!("asked for {url}")))
    };

    let provisioned =
        provision::provision_all(&[FIO], "x86_64", dir.path(), 32, &cancel, fetch).await;

    assert!(
        provisioned.ready.is_empty() && provisioned.failed.is_empty(),
        "{provisioned:?}"
    );
}
