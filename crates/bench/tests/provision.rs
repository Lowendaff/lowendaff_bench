//! Tool provisioning: verification before execution, cleanup on failure, per-tool
//! failure reasons, downloads in byte ranges over several connections, tries again after a
//! failure that may pass, and a plugin running the tool it was given; and the files that the
//! plugins ask for.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::io;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Poll;
use std::time::Duration;

use bench::orchestrator;
use bench::orchestrator::Quiet;
use bench::orchestrator::Settings;
use bench::orchestrator::Status;
use bench::provision;
use bench::provision::DownloadError;
use bench::provision::MIN_RANGE;
use bench::provision::Part;
use bench::provision::Provisioned;
use bench::registry;
use bench::results::Measured;
use bench::results::Report;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::FileSource;
use bench_api::FileSpec;
use bench_api::IpFamily;
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
use tokio::io::AsyncRead;
use tokio::io::ReadBuf;
use tokio::time::Instant;

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
    raw_sockets: false,
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

    assert!(err.starts_with("SHA-256 mismatch: 0000"), "{err}");
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

    assert_eq!(err, "larger than the pinned 23 bytes");
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

    assert_eq!(err, "size mismatch: 29 bytes pinned, 24 bytes came");
    assert!(entries(dir.path()).is_empty());
}

const IPERF3_ARM_ONLY: ToolSpec = ToolSpec {
    name: "iperf3",
    version: "3.19",
    raw_sockets: false,
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
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/sysbench",
        sha256: SCRIPT_SHA,
        size: SCRIPT_LEN,
    }],
};

/// Provisions as the program does, over up to 32 connections, while holding a capture of the log
/// that nobody reads: tests that log take turns, so that none logs while another listens (see
/// `common`).
async fn provision_in_turn<F, Fut, R>(
    tools: &[ToolSpec],
    arch: &str,
    dir: &Path,
    cancel: &CancellationToken,
    fetch: F,
) -> Provisioned
where
    F: Fn(&'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    let (_log, capturing) = Captured::start(false);
    let provisioned = provision::provision_all(tools, arch, dir, 32, cancel, fetch).await;
    drop(capturing);
    provisioned
}

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
    let provisioned = provision_in_turn(
        &[FIO, IPERF3_ARM_ONLY, SYSBENCH],
        "x86_64",
        dir.path(),
        &CancellationToken::new(),
        fetch,
    )
    .await;

    assert_eq!(provisioned.ready.keys().collect::<Vec<_>>(), ["fio"]);
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([
            ("iperf3".to_owned(), "no build for x86_64".to_owned()),
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
    let provisioned = provision_in_turn(
        &tools,
        std::env::consts::ARCH,
        run_dir.path(),
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
        files: BTreeMap::new(),
        http: None,
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
        raw_sockets: false,
        form: ToolForm::Executable,
        assets,
    }
}

/// The body of a test download: `bytes`, then its end, or a break when `breaks` is set.
struct Body {
    bytes: &'static [u8],
    breaks: bool,
}

impl tokio::io::AsyncRead for Body {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.bytes.is_empty() {
            return Poll::Ready(if self.breaks {
                Err(io::Error::other("the connection broke off"))
            } else {
                Ok(())
            });
        }
        let count = self.bytes.len().min(buf.remaining());
        buf.put_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Poll::Ready(Ok(()))
    }
}

/// A server of one file: whole, or in the byte ranges asked for while it `serves_ranges` (a
/// number of answers). The first `busy` requests get a 503, and every request a 404 when it is
/// `gone`. The whole file comes a byte short in the first `short_whole` answers; the range
/// starting at `short` comes a byte short, the one starting at `long` a byte long, and the one
/// starting at `failing` never (503). A request whose span ends at the first number of a pair in
/// `breaks` breaks off after the pair's second number of bytes, a pair for each such request, in
/// order. Notes each request's range.
struct Served {
    body: &'static [u8],
    serves_ranges: usize,
    busy: usize,
    gone: bool,
    short_whole: usize,
    short: Option<u64>,
    long: Option<u64>,
    failing: Option<u64>,
    breaks: Mutex<Vec<(u64, u64)>>,
    asked: Mutex<Vec<Option<Range<u64>>>>,
}

impl Served {
    fn new(body: &'static [u8]) -> Self {
        Self {
            body,
            serves_ranges: usize::MAX,
            busy: 0,
            gone: false,
            short_whole: 0,
            short: None,
            long: None,
            failing: None,
            breaks: Mutex::new(Vec::new()),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// A server whose requests for the span that ends at `end` break off after each number of
    /// bytes in `after` in turn.
    fn breaking(body: &'static [u8], end: u64, after: &[u64]) -> Self {
        Self {
            breaks: Mutex::new(after.iter().map(|after| (end, *after)).collect()),
            ..Self::new(body)
        }
    }

    async fn open(&self, range: Option<Range<u64>>) -> Result<Part<Body>, DownloadError> {
        let answered = {
            let mut asked = self.asked.lock().expect("no test panics while holding it");
            asked.push(range.clone());
            asked.len()
        };
        if self.gone {
            return Err(DownloadError::Final("download failed: HTTP 404".to_owned()));
        }
        if answered <= self.busy {
            return Err(DownloadError::Transient(
                "download failed: HTTP 503".to_owned(),
            ));
        }
        let ranged = range.filter(|_| answered <= self.serves_ranges);
        let span = ranged.clone().unwrap_or(0..self.body.len() as u64);
        let mut end = span.end;
        if ranged.is_none() && answered <= self.short_whole {
            end -= 1;
        }
        if ranged.is_some() {
            if self.failing == Some(span.start) {
                return Err(DownloadError::Transient(
                    "download failed: HTTP 503".to_owned(),
                ));
            }
            if self.short == Some(span.start) {
                end -= 1;
            }
            if self.long == Some(span.start) {
                end += 1;
            }
        }
        let cut = {
            let mut breaks = self.breaks.lock().expect("no test panics while holding it");
            let next = breaks.iter().position(|(ends, _)| *ends == span.end);
            next.map(|next| breaks.remove(next).1)
        };
        if let Some(after) = cut {
            end = span.start + after;
        }
        let start = usize::try_from(span.start).expect("in memory");
        let end = usize::try_from(end).expect("in memory");
        let body = Body {
            bytes: &self.body[start..end],
            breaks: cut.is_some(),
        };
        Ok(match ranged {
            Some(_) => Part::Range(body),
            None => Part::Whole(body),
        })
    }

    /// The ranges asked for, in the order of the requests.
    fn asked(&self) -> Vec<Option<Range<u64>>> {
        self.asked
            .lock()
            .expect("no test panics while holding it")
            .clone()
    }
}

/// What came of provisioning one tool: the result, the log, the installed file's bytes, the
/// names left in the tools' directory, and the time it took, in the paused clock of a test that
/// pauses it.
struct Outcome {
    provisioned: provision::Provisioned,
    log: Vec<String>,
    installed: Option<Vec<u8>>,
    left: Vec<String>,
    took: Duration,
}

/// Provisions `tool` from `served` over up to `connections` connections, logging as `-v` does.
async fn provision_from(tool: ToolSpec, served: &Served, connections: usize) -> Outcome {
    let dir = tempfile::tempdir().expect("a directory for the tools");
    let (log, capturing) = Captured::start(true);
    let started = Instant::now();
    let provisioned = provision::provision_all(
        &[tool],
        "x86_64",
        dir.path(),
        connections,
        &CancellationToken::new(),
        |_, range| served.open(range),
    )
    .await;
    let took = started.elapsed();
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
        took,
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
                "Downloading https://tools.example/big".to_owned(),
                format!("big 1.0 in {count} byte ranges at once"),
                "Tools: big 1.0 ready".to_owned(),
            ]
        );
    }
}

#[tokio::test]
async fn a_file_under_two_ranges_comes_whole_and_one_connection_takes_one_range() {
    let both = vec![Some(0..MIN_RANGE), Some(MIN_RANGE..2 * MIN_RANGE)];
    for (size, connections, asked) in [
        (5 * MIN_RANGE, 1, vec![Some(0..5 * MIN_RANGE)]),
        (2 * MIN_RANGE - 1, 32, vec![None]),
        (2 * MIN_RANGE, 32, both),
    ] {
        let body = contents(size);
        let served = Served::new(body);
        let outcome = provision_from(pinned_to(body), &served, connections).await;

        assert_eq!(outcome.installed.as_deref(), Some(body), "{size} bytes");
        let mut seen = served.asked();
        seen.sort_by_key(|range| range.as_ref().map(|range| range.start));
        assert_eq!(seen, asked, "{size} bytes, {connections} connections");
        if asked.len() == 1 {
            assert_eq!(
                outcome.log,
                [
                    "Downloading https://tools.example/big",
                    "Tools: big 1.0 ready"
                ],
                "one range or a whole file has no line of ranges"
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
        outcome.log[1], "big 1.0: the server does not serve byte ranges; one connection instead",
        "{:?}",
        outcome.log
    );
}

/// The -v log of a whole download of the test tool that breaks off `failed` times.
fn whole_retries(failed: u32) -> Vec<String> {
    (1..=failed)
        .map(|attempt| {
            format!(
                "big 1.0: attempt {attempt} failed; trying again in {} s: download broke off: \
                 the connection broke off",
                1 << (attempt - 1)
            )
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_whole_file_that_breaks_off_starts_over_after_waits_of_1_2_and_4_seconds() {
    let body = contents(1000);
    let served = Served::breaking(body, 1000, &[100, 100, 100]);
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(outcome.installed.as_deref(), Some(body));
    assert_eq!(served.asked(), [None, None, None, None]);
    assert_eq!(outcome.took, Duration::from_secs(7));
    let mut log = vec!["Downloading https://tools.example/big".to_owned()];
    log.extend(whole_retries(3));
    log.push("Tools: big 1.0 ready".to_owned());
    assert_eq!(outcome.log, log);
}

#[tokio::test(start_paused = true)]
async fn a_whole_file_that_keeps_breaking_off_fails_after_four_attempts() {
    let body = contents(1000);
    let served = Served::breaking(body, 1000, &[100, 100, 100, 100]);
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(
        outcome.provisioned.failed["big"],
        "download broke off: the connection broke off"
    );
    assert_eq!(served.asked(), [None, None, None, None]);
    assert_eq!(outcome.took, Duration::from_secs(7));
    assert_eq!(outcome.log[1..4], whole_retries(3));
    assert_eq!(outcome.left, Vec::<String>::new());
}

#[tokio::test(start_paused = true)]
async fn a_whole_file_that_ends_short_starts_over() {
    let body = contents(1000);
    let served = Served {
        short_whole: 1,
        ..Served::new(body)
    };
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(outcome.installed.as_deref(), Some(body));
    assert_eq!(served.asked(), [None, None]);
    assert_eq!(
        outcome.log[1],
        "big 1.0: attempt 1 failed; trying again in 1 s: size mismatch: 1000 bytes pinned, \
         999 bytes came"
    );
}

#[tokio::test(start_paused = true)]
async fn failures_before_the_first_answer_count_against_what_comes_after_it() {
    let size = 2 * MIN_RANGE;
    let body = contents(size);
    // Three times busy, then the first range, or the whole file, breaks off: the fourth failure.
    let cases = [
        Served {
            busy: 3,
            ..Served::breaking(body, MIN_RANGE, &[1000])
        },
        Served {
            busy: 3,
            serves_ranges: 3,
            ..Served::breaking(body, size, &[1000])
        },
    ];
    for served in cases {
        let outcome = provision_from(pinned_to(body), &served, 32).await;

        assert_eq!(
            outcome.provisioned.failed["big"],
            "download broke off: the connection broke off"
        );
        assert_eq!(outcome.took, Duration::from_secs(7));
        assert_eq!(outcome.left, Vec::<String>::new());
    }
}

#[tokio::test(start_paused = true)]
async fn a_range_that_breaks_off_or_ends_short_goes_on_from_its_first_missing_byte() {
    let body = contents(4 * MIN_RANGE);
    let cases = [
        (
            Served::breaking(body, 2 * MIN_RANGE, &[1000]),
            MIN_RANGE + 1000..2 * MIN_RANGE,
            format!(
                "big 1.0: byte range 2, attempt 1 failed; going on from byte {} in 1 s: \
                 download broke off: the connection broke off",
                MIN_RANGE + 1000
            ),
        ),
        (
            Served {
                short: Some(2 * MIN_RANGE),
                ..Served::new(body)
            },
            3 * MIN_RANGE - 1..3 * MIN_RANGE,
            format!(
                "big 1.0: byte range 3, attempt 1 failed; going on from byte {} in 1 s: size \
                 mismatch: a byte range of {MIN_RANGE} bytes came with {} bytes",
                3 * MIN_RANGE - 1,
                MIN_RANGE - 1
            ),
        ),
    ];
    for (served, again, line) in cases {
        let outcome = provision_from(pinned_to(body), &served, 32).await;

        assert_eq!(outcome.installed.as_deref(), Some(body), "{line}");
        assert_eq!(served.asked().len(), 5, "four ranges and one again: {line}");
        assert_eq!(served.asked().last(), Some(&Some(again)), "{line}");
        assert_eq!(outcome.took, Duration::from_secs(1));
        assert!(outcome.log.contains(&line), "{:?}", outcome.log);
    }
}

#[tokio::test(start_paused = true)]
async fn a_range_that_brought_a_mib_before_breaking_off_starts_its_count_afresh() {
    let size = 6 * MIN_RANGE;
    let body = contents(size);
    let served = Served::breaking(body, size, &[MIN_RANGE; 5]);
    let outcome = provision_from(pinned_to(body), &served, 1).await;

    assert_eq!(outcome.installed.as_deref(), Some(body));
    let from: Vec<Option<Range<u64>>> = (0..6).map(|count| Some(count * MIN_RANGE..size)).collect();
    assert_eq!(served.asked(), from);
    assert_eq!(outcome.took, Duration::from_secs(5), "1 second each time");
    let retries: Vec<String> = (1..=5)
        .map(|count| {
            format!(
                "big 1.0: byte range 1, attempt 1 failed; going on from byte {} in 1 s: \
                 download broke off: the connection broke off",
                count * MIN_RANGE
            )
        })
        .collect();
    assert_eq!(outcome.log[1..6], retries);
}

#[tokio::test(start_paused = true)]
async fn a_range_that_breaks_off_before_a_mib_counts_each_break() {
    let size = 4 * MIN_RANGE;
    let body = contents(size);
    let served = Served::breaking(body, size, &[1000, 1000, 1000, 1000]);
    let outcome = provision_from(pinned_to(body), &served, 1).await;

    assert_eq!(
        outcome.provisioned.failed["big"],
        "download broke off: the connection broke off"
    );
    let from: Vec<Option<Range<u64>>> = (0..4).map(|count| Some(count * 1000..size)).collect();
    assert_eq!(served.asked(), from);
    assert_eq!(outcome.took, Duration::from_secs(7));
    assert_eq!(outcome.left, Vec::<String>::new());
}

#[tokio::test(start_paused = true)]
async fn a_busy_server_is_asked_again_for_the_first_range() {
    let body = contents(2 * MIN_RANGE);
    let first = Some(0..MIN_RANGE);
    let served = Served {
        busy: 2,
        ..Served::new(body)
    };
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(outcome.installed.as_deref(), Some(body));
    assert_eq!(
        served.asked(),
        [
            first.clone(),
            first.clone(),
            first.clone(),
            Some(MIN_RANGE..2 * MIN_RANGE)
        ]
    );
    assert_eq!(outcome.took, Duration::from_secs(3));
    assert_eq!(
        outcome.log[1..3],
        [
            "big 1.0: attempt 1 failed; trying again in 1 s: download failed: HTTP 503",
            "big 1.0: attempt 2 failed; trying again in 2 s: download failed: HTTP 503",
        ]
    );

    let served = Served {
        busy: 4,
        ..Served::new(body)
    };
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(
        outcome.provisioned.failed["big"],
        "download failed: HTTP 503"
    );
    assert_eq!(
        served.asked(),
        vec![first; 4],
        "no other range is asked for"
    );
    assert_eq!(outcome.took, Duration::from_secs(7));
}

#[tokio::test(start_paused = true)]
async fn a_range_that_keeps_failing_fails_the_tool_after_four_attempts_and_leaves_nothing() {
    let body = contents(4 * MIN_RANGE);
    let served = Served {
        failing: Some(MIN_RANGE),
        ..Served::new(body)
    };
    let outcome = provision_from(pinned_to(body), &served, 32).await;

    assert_eq!(
        outcome.provisioned.failed["big"],
        "download failed: HTTP 503"
    );
    let again = served
        .asked()
        .into_iter()
        .filter(|range| *range == Some(MIN_RANGE..2 * MIN_RANGE))
        .count();
    assert_eq!(again, 4);
    assert_eq!(outcome.took, Duration::from_secs(7));
    assert_eq!(outcome.left, Vec::<String>::new());
}

#[tokio::test(start_paused = true)]
async fn failures_that_would_end_the_same_way_are_not_tried_again() {
    let body = contents(4 * MIN_RANGE);
    let wrong = [
        (
            Served {
                gone: true,
                ..Served::new(body)
            },
            "download failed: HTTP 404".to_owned(),
        ),
        (
            Served {
                long: Some(MIN_RANGE),
                ..Served::new(body)
            },
            format!("a byte range larger than its {MIN_RANGE} bytes"),
        ),
        (
            Served {
                serves_ranges: 1,
                ..Served::new(body)
            },
            "download failed: the server stopped serving byte ranges".to_owned(),
        ),
    ];
    for (served, reason) in wrong {
        let outcome = provision_from(pinned_to(body), &served, 32).await;

        assert_eq!(outcome.provisioned.failed["big"], reason);
        let asked = served.asked();
        let mut once = asked.clone();
        once.sort_by_key(|range| range.as_ref().map(|range| range.start));
        once.dedup();
        assert_eq!(
            once.len(),
            asked.len(),
            "nothing is asked for twice: {asked:?}"
        );
        assert_eq!(outcome.took, Duration::ZERO, "{reason}");
        assert_eq!(outcome.left, Vec::<String>::new(), "{reason}");
    }
}

#[tokio::test(start_paused = true)]
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
            "SHA-256 mismatch: {} pinned, ",
            tool.assets[0].sha256
        )),
        "{reason}"
    );
    assert_eq!(served.asked().len(), 3, "no range is asked for again");
    assert_eq!(outcome.took, Duration::ZERO);
    assert_eq!(outcome.left, Vec::<String>::new());
}

#[tokio::test(start_paused = true)]
async fn ctrl_c_during_a_wait_ends_the_download_at_once() {
    let body = contents(1000);
    let served = Served::breaking(body, 1000, &[100, 100, 100, 100]);
    let dir = tempfile::tempdir().expect("a directory for the tools");
    let cancel = CancellationToken::new();
    let interrupt = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            cancel.cancel();
        })
    };
    let started = Instant::now();

    let provisioned = provision_in_turn(
        &[pinned_to(body)],
        "x86_64",
        dir.path(),
        &cancel,
        |_, range| served.open(range),
    )
    .await;

    assert_eq!(started.elapsed(), Duration::from_millis(500));
    assert_eq!(provisioned.failed["big"], "download interrupted");
    assert_eq!(served.asked(), [None], "the second attempt never starts");
    assert_eq!(entries(dir.path()), Vec::<String>::new());
    interrupt.await.expect("the interrupt runs");
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

    let provisioned = provision_in_turn(&[big, FIO], "x86_64", dir.path(), &cancel, fetch).await;

    assert_eq!(provisioned.ready, BTreeMap::new());
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([("big".to_owned(), "download interrupted".to_owned())])
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

    let provisioned = provision_in_turn(&[FIO], "x86_64", dir.path(), &cancel, fetch).await;

    assert!(
        provisioned.ready.is_empty() && provisioned.failed.is_empty(),
        "{provisioned:?}"
    );
}

/// A tool that needs raw sockets, pinned to `body`, named `name`.
fn raw(name: &'static str, body: &[u8]) -> ToolSpec {
    ToolSpec {
        name,
        raw_sockets: true,
        ..pinned_to(body)
    }
}

/// Provisions `tool` into `dir`, serving `body` and counting the downloads asked for.
async fn provision_counted(
    tool: ToolSpec,
    dir: &Path,
    body: &'static [u8],
) -> (Provisioned, usize) {
    let asked = std::sync::atomic::AtomicUsize::new(0);
    let fetch = |_url, _range| {
        asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move { Ok(Part::Whole(body)) }
    };
    let cancel = CancellationToken::new();
    let provisioned = provision_in_turn(&[tool], "x86_64", dir, &cancel, fetch).await;
    (provisioned, asked.into_inner())
}

#[tokio::test]
async fn a_tool_that_needs_raw_sockets_has_them_or_is_unavailable_with_the_reason() {
    let dir = tempfile::tempdir().expect("a directory for the tool");
    let (provisioned, asked) = provision_counted(raw("big", SCRIPT), dir.path(), SCRIPT).await;

    #[cfg(target_os = "linux")]
    {
        use rustix::fs::StatVfsMountFlags;

        let nosuid = rustix::fs::statvfs(dir.path())
            .is_ok_and(|stat| stat.f_flag.contains(StatVfsMountFlags::NOSUID));
        if rustix::thread::no_new_privs().unwrap_or(false) {
            assert!(
                provisioned.failed["big"].contains("no_new_privs"),
                "{provisioned:?}"
            );
            assert_eq!(asked, 0, "not downloaded");
        } else if nosuid {
            assert!(
                provisioned.failed["big"].contains("mounted nosuid"),
                "{provisioned:?}"
            );
            assert_eq!(asked, 0, "not downloaded");
        } else if rustix::process::geteuid().is_root() {
            let path = &provisioned.ready["big"];
            let mut capability = [0_u8; 64];
            let length = rustix::fs::getxattr(path, "security.capability", &mut capability)
                .expect("the tool has a file capability");
            assert_eq!(capability[..length], provision::RAW_SOCKETS);
        } else {
            let reason = &provisioned.failed["big"];
            assert!(
                reason
                    .starts_with("cannot give it raw sockets (the file capability CAP_NET_RAW): ")
                    && reason.contains("not permitted"),
                "only root may set a file capability: {reason}"
            );
            assert_eq!(asked, 1);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        assert_eq!(
            provisioned.failed["big"],
            "it needs raw sockets, which the program gives on Linux only"
        );
        assert_eq!(asked, 0, "not downloaded");
    }
}

/// The file capability as setcap writes `cap_net_raw+ep`: `getfattr -n security.capability`
/// shows it as `0x0100000200200000000000000000000000000000`.
#[test]
fn the_raw_sockets_capability_is_cap_net_raw_ep_as_setcap_writes_it() {
    let hex: String = provision::RAW_SOCKETS
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(hex, "0100000200200000000000000000000000000000");
}

/// /dev/shm, where Linux mounts it `nosuid` as most systems do; skipped where it is not.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_that_needs_raw_sockets_is_not_downloaded_onto_a_filesystem_mounted_nosuid() {
    use rustix::fs::StatVfsMountFlags;

    let shm = Path::new("/dev/shm");
    let Ok(stat) = rustix::fs::statvfs(shm) else {
        return;
    };
    if !stat.f_flag.contains(StatVfsMountFlags::NOSUID) {
        return;
    }
    let dir = tempfile::tempdir_in(shm).expect("a directory in /dev/shm");
    let (provisioned, asked) = provision_counted(raw("big", SCRIPT), dir.path(), SCRIPT).await;
    assert_eq!(
        provisioned.failed["big"],
        "it needs raw sockets, but the run directory's filesystem is mounted nosuid, which \
         ignores file capabilities; choose another with --work-dir"
    );
    assert_eq!(asked, 0, "not downloaded");
}

/// As root on Linux, a tool with raw sockets runs as nobody with CAP_NET_RAW and nothing else, as
/// the kernel reads the capability that the program wrote; the shell stands for the tool. Root
/// only, as in the Linux check: the tests run as an ordinary user elsewhere.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_with_raw_sockets_runs_as_nobody_with_cap_net_raw_alone() {
    use std::os::unix::fs::PermissionsExt;

    use bench_tool::job::Job;
    use bench_tool::job::User;

    if !rustix::process::geteuid().is_root() {
        return;
    }
    let shell: &'static [u8] = Box::leak(
        std::fs::read("/bin/sh")
            .expect("the system's shell")
            .into_boxed_slice(),
    );
    // As the program has it: a tools directory that others may search, in one closed to them.
    let closed = tempfile::tempdir().expect("a run directory");
    std::fs::set_permissions(closed.path(), std::fs::Permissions::from_mode(0o700))
        .expect("the run directory is closed to others");
    let tools = closed.path().join("tools");
    std::fs::create_dir(&tools).expect("a tools directory");
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o711))
        .expect("the tools directory's mode");
    // Busybox picks its program by its own name, which the copy keeps.
    let (provisioned, _) = provision_counted(raw("sh", shell), &tools, shell).await;
    let Some(tool) = provisioned.ready.get("sh").cloned() else {
        panic!("the shell has raw sockets: {provisioned:?}");
    };

    let (ctx, _channels) = Context::<fio::Output>::new(
        Arc::new(bench_host::profile::HostProfile::default()),
        Arc::default(),
        Arc::default(),
        Instant::now() + Duration::from_secs(60),
        closed.path().to_path_buf(),
        CancellationToken::new(),
    );
    // The shell's own status: grep, which it starts, has no capability of its own.
    let script = "grep -E '^Cap(Prm|Eff):' /proc/$$/status; id -u";
    let mut job = Job::new(&tool, ["-c", script], Duration::from_secs(30));
    job.user = Some(User::NOBODY);
    job.dir = Some("/".into());
    let ended = job.run(&ctx).await.ended;
    let Ok(output) = &ended else {
        panic!("the tool runs: {ended:?}");
    };
    let lines: Vec<Vec<&str>> = output
        .text
        .lines()
        .map(|line| line.split_whitespace().collect())
        .collect();
    assert_eq!(
        lines,
        [
            vec!["CapPrm:", "0000000000002000"],
            vec!["CapEff:", "0000000000002000"],
            vec!["65534"],
        ],
        "CAP_NET_RAW, bit 13, permitted and effective, as nobody: {}",
        output.text
    );
}

const LIST_URL: &str = "https://lists.example/servers.json";

/// The families of a host that reaches both.
const BOTH: [IpFamily; 2] = [IpFamily::Ipv4, IpFamily::Ipv6];

fn list(url: &str) -> FileSpec {
    FileSpec::new("servers", FileSource::Url(url.to_owned()))
}

/// What came of fetching files: the result, the log, the files left in their directory with
/// their bytes, and the time it took, in the paused clock of a test that pauses it.
struct Fetched {
    provisioned: Provisioned,
    log: Vec<String>,
    left: BTreeMap<String, Vec<u8>>,
    took: Duration,
}

/// Fetches `files` from `served` into a fresh directory, logging as `-v` does.
async fn fetch_from(files: &[FileSpec], served: &Served) -> Fetched {
    let dir = tempfile::tempdir().expect("a directory for the files");
    let (log, capturing) = Captured::start(true);
    let started = Instant::now();
    let fetch = |_, _, range| served.open(range);
    let cancel = CancellationToken::new();
    let provisioned = provision::fetch_files(files, dir.path(), &BOTH, &cancel, &fetch).await;
    let took = started.elapsed();
    drop(capturing);
    let left = entries(dir.path())
        .into_iter()
        .map(|name| {
            let bytes = std::fs::read(dir.path().join(&name)).expect("the file reads");
            (name, bytes)
        })
        .collect();
    Fetched {
        provisioned,
        log: log.lines(),
        left,
        took,
    }
}

#[tokio::test]
async fn a_file_at_an_https_address_is_downloaded_and_one_on_the_host_handed_on() {
    let served = Served::new(b"{\"version\": 1}\n");
    let own = FileSpec::new(
        "own",
        FileSource::Path(PathBuf::from("/etc/own-servers.json")),
    );

    let fetched = fetch_from(&[list(LIST_URL), own], &served).await;

    assert_eq!(fetched.provisioned.failed, BTreeMap::new());
    let ready = &fetched.provisioned.ready;
    assert_eq!(ready["own"], PathBuf::from("/etc/own-servers.json"));
    assert_eq!(ready["servers"].file_name(), Some("servers".as_ref()));
    assert_eq!(
        fetched.left,
        BTreeMap::from([("servers".to_owned(), b"{\"version\": 1}\n".to_vec())])
    );
    assert_eq!(served.asked(), [None], "one request for the whole file");
    assert_eq!(fetched.log, [format!("Downloading {LIST_URL}")]);
}

#[tokio::test]
async fn a_file_at_an_address_other_than_https_is_not_downloaded() {
    let served = Served::new(b"{}");
    let url = "http://lists.example/servers.json";

    let fetched = fetch_from(&[list(url)], &served).await;

    assert_eq!(
        fetched.provisioned.failed,
        BTreeMap::from([("servers".to_owned(), format!("not an HTTPS address: {url}"))])
    );
    assert!(served.asked().is_empty(), "nothing is asked for");
    assert_eq!(fetched.left, BTreeMap::new());
}

#[tokio::test]
async fn a_file_of_64_kib_comes_and_a_larger_one_fails_at_once_and_leaves_nothing() {
    assert_eq!(provision::FILE_LIMIT, 65_536);
    let fits = Served::new(contents(provision::FILE_LIMIT));
    let fetched = fetch_from(&[list(LIST_URL)], &fits).await;
    assert!(
        fetched.provisioned.ready.contains_key("servers"),
        "{:?}",
        fetched.provisioned
    );
    assert_eq!(fetched.left["servers"], contents(provision::FILE_LIMIT));

    let large = Served::new(contents(provision::FILE_LIMIT + 1));
    let fetched = fetch_from(&[list(LIST_URL)], &large).await;
    assert_eq!(
        fetched.provisioned.failed,
        BTreeMap::from([("servers".to_owned(), "larger than 65536 bytes".to_owned())])
    );
    assert_eq!(fetched.left, BTreeMap::new(), "the partial file is removed");
    assert_eq!(large.asked(), [None], "a file too large is not tried again");
}

#[tokio::test(start_paused = true)]
async fn a_file_is_tried_again_after_failures_that_may_pass() {
    let served = Served {
        busy: 2,
        ..Served::new(b"{}")
    };

    let fetched = fetch_from(&[list(LIST_URL)], &served).await;

    assert!(fetched.provisioned.ready.contains_key("servers"));
    assert_eq!(
        fetched.log,
        [
            format!("Downloading {LIST_URL}"),
            "file servers: attempt 1 failed; trying again in 1 s: download failed: HTTP 503"
                .to_owned(),
            "file servers: attempt 2 failed; trying again in 2 s: download failed: HTTP 503"
                .to_owned(),
        ]
    );
    assert_eq!(fetched.took, Duration::from_secs(3));

    let breaking = Served::breaking(b"{\"version\": 1}", 14, &[5]);
    let fetched = fetch_from(&[list(LIST_URL)], &breaking).await;
    assert_eq!(
        fetched.left["servers"], b"{\"version\": 1}",
        "the second try came whole"
    );
    assert_eq!(breaking.asked(), [None, None]);
    assert_eq!(
        fetched.log[1],
        "file servers: attempt 1 failed; trying again in 1 s: download broke off: the connection \
         broke off"
    );

    let gone = Served {
        gone: true,
        ..Served::new(b"{}")
    };
    let fetched = fetch_from(&[list(LIST_URL)], &gone).await;
    assert_eq!(
        fetched.provisioned.failed["servers"],
        "download failed: HTTP 404"
    );
    assert_eq!(gone.asked(), [None], "a 404 is not tried again");
}

#[tokio::test]
async fn a_file_comes_over_its_family_and_one_of_a_family_the_host_lacks_is_not_fetched() {
    let dir = tempfile::tempdir().expect("a directory for the files");
    let asked = Mutex::new(Vec::new());
    let fetch = |family: Option<IpFamily>, url: &'static str, _range| {
        asked
            .lock()
            .expect("no test panics while holding it")
            .push((family, url));
        async { Ok(Part::Whole(&b"{}\n"[..])) }
    };
    let over = |name, url: &str, family| FileSpec {
        family,
        ..FileSpec::new(name, FileSource::Url(url.to_owned()))
    };
    let files = [
        over("any", "https://lists.example/any.json", None),
        over(
            "four",
            "https://lists.example/four.json",
            Some(IpFamily::Ipv4),
        ),
        over(
            "six",
            "https://lists.example/six.json",
            Some(IpFamily::Ipv6),
        ),
    ];

    let (_log, capturing) = Captured::start(false);
    let cancel = CancellationToken::new();
    let provisioned =
        provision::fetch_files(&files, dir.path(), &[IpFamily::Ipv4], &cancel, &fetch).await;
    drop(capturing);

    assert_eq!(
        provisioned.ready,
        BTreeMap::from([
            ("any".to_owned(), dir.path().join("any")),
            ("four".to_owned(), dir.path().join("four")),
        ])
    );
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([(
            "six".to_owned(),
            "not fetched: the host has no IPv6 connection".to_owned()
        )])
    );
    assert_eq!(
        asked
            .into_inner()
            .expect("no test panicked while holding it"),
        [
            (None, "https://lists.example/any.json"),
            (Some(IpFamily::Ipv4), "https://lists.example/four.json"),
        ],
        "each over its family, and the IPv6 file not at all"
    );
}

#[tokio::test]
async fn ctrl_c_stops_a_file_download_and_the_files_after_it() {
    let dir = tempfile::tempdir().expect("a directory for the files");
    let cancel = CancellationToken::new();
    let asked = Mutex::new(Vec::new());
    // The first file never comes, and Ctrl-C comes meanwhile.
    let fetch = |_family, url: &'static str, _range| {
        asked
            .lock()
            .expect("no test panics while holding it")
            .push(url);
        let cancel = &cancel;
        async move {
            cancel.cancel();
            std::future::pending::<Result<Part<&'static [u8]>, DownloadError>>().await
        }
    };
    let other = FileSpec::new(
        "other",
        FileSource::Url("https://lists.example/other.json".to_owned()),
    );
    let files = [list(LIST_URL), other];

    let (_log, capturing) = Captured::start(false);
    let provisioned = provision::fetch_files(&files, dir.path(), &BOTH, &cancel, &fetch).await;
    drop(capturing);

    assert_eq!(provisioned.ready, BTreeMap::new());
    assert_eq!(
        provisioned.failed,
        BTreeMap::from([("servers".to_owned(), "download interrupted".to_owned())])
    );
    let asked = asked
        .into_inner()
        .expect("no test panicked while holding it");
    assert_eq!(asked, [LIST_URL], "the second file is not asked for");
    assert_eq!(entries(dir.path()), Vec::<String>::new());
}
