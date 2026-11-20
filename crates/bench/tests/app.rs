//! Command-line flow: argument parsing, selection, config file, and a full run through
//! the terminal renderer with the run directory removed afterwards.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;

use bench::app;
use bench::app::Cli;
use bench::app::EXIT_INTERRUPTED;
use bench::app::ReportOptions;
use bench::orchestrator;
use bench::orchestrator::Planned;
use bench::progress;
use bench::provision::DownloadError;
use bench::provision::MIN_RANGE;
use bench::provision::Part;
use bench::registry;
use bench::registry::Entry;
use bench::results::Measured;
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
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::numa::Layout;
use bench_host::numa::Node;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;
use bench_host::profile::StorageFacts;
use bench_host::storage::Filesystem;
use common::Captured;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Scores;
use serde::Deserialize;

mod common;

/// What the tests' plugins return, Geekbench 7's result being the simplest of the program's to
/// build: empty, so that the report shows only how a plugin ended.
type Output = geekbench::Output;
type Failure = geekbench::Failure;

const fn descriptor(id: &'static str) -> Descriptor {
    Descriptor {
        id,
        requires: Requirements::NONE,
        timeout: Duration::from_secs(10),
    }
}

/// A plugin that stands in for one of the program's and leaves an empty result.
macro_rules! stand_in {
    ($name:ident, $descriptor:expr) => {
        #[derive(Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct $name {}

        impl Plugin for $name {
            type Output = Output;
            type Failure = Failure;
            const DESCRIPTOR: Descriptor = $descriptor;

            async fn run(&self, _ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
                Ok(Output::default())
            }
        }
    };
}

/// Its config names the single-core score that its result shows.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Good {
    samples: u32,
}

impl Plugin for Good {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.good");

    async fn run(&self, _ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        Ok(Output {
            scores: Some(Scores {
                version: None,
                single: self.samples,
                multi: self.samples,
                url: None,
            }),
            ..Output::default()
        })
    }
}

stand_in!(Other, descriptor("test.other"));
// A group of one plugin, whose id is the group's name.
stand_in!(Lone, descriptor("lone"));

/// The registry's entry of a plugin of these tests.
fn entry<P: Plugin<Output = Output, Failure = Failure>>() -> Entry {
    registry::entry::<P>(Measured::CpuGeekbench)
}

fn plugins() -> Vec<Entry> {
    vec![entry::<Good>(), entry::<Other>()]
}

fn groups() -> Vec<Entry> {
    vec![entry::<Good>(), entry::<Lone>(), entry::<Other>()]
}

fn cli(args: &[&str]) -> Cli {
    use clap::Parser;

    Cli::try_parse_from(std::iter::once("lowendaff-bench").chain(args.iter().copied()))
        .expect("test arguments must parse")
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

/// A download asked for: the family it must come over or `None` for either, the address, and the
/// byte range or `None` for the whole file.
type Asked = (Option<IpFamily>, &'static str, Option<Range<u64>>);

/// The downloads of a test run: every address gets `body`, whole or the byte range asked for, or
/// fails when there is none, and the addresses and ranges asked for are noted. A file comes over
/// up to `connections` connections, or as many as the program uses by default. With `interrupt`,
/// a download instead never ends, and Ctrl-C comes meanwhile.
#[derive(Default)]
struct Downloads {
    body: Option<&'static [u8]>,
    connections: Option<usize>,
    interrupt: Option<CancellationToken>,
    asked: Mutex<Vec<Asked>>,
}

impl Downloads {
    async fn open(
        &self,
        family: Option<IpFamily>,
        url: &'static str,
        range: Option<Range<u64>>,
    ) -> Result<Part<&'static [u8]>, DownloadError> {
        self.asked
            .lock()
            .expect("no test panics while holding the addresses")
            .push((family, url, range.clone()));
        if let Some(cancel) = &self.interrupt {
            cancel.cancel();
            std::future::pending::<()>().await;
        }
        let body = self
            .body
            .ok_or_else(|| DownloadError::Final(format!("no download in this test: {url}")))?;
        Ok(match range {
            Some(range) => {
                let start = usize::try_from(range.start).expect("in memory");
                let end = usize::try_from(range.end).expect("in memory");
                Part::Range(&body[start..end])
            }
            None => Part::Whole(body),
        })
    }

    /// The addresses asked for, in order.
    fn asked(&self) -> Vec<&'static str> {
        let asked = self
            .asked
            .lock()
            .expect("no test panics while holding the addresses");
        asked.iter().map(|(_, url, _)| *url).collect()
    }

    /// The families that the addresses were asked over, in order.
    fn families(&self) -> Vec<Option<IpFamily>> {
        let asked = self
            .asked
            .lock()
            .expect("no test panics while holding the addresses");
        asked.iter().map(|(family, _, _)| *family).collect()
    }

    /// The byte ranges asked for, in order; `None` for a whole file.
    fn ranges(&self) -> Vec<Option<Range<u64>>> {
        let asked = self
            .asked
            .lock()
            .expect("no test panics while holding the addresses");
        asked.iter().map(|(_, _, range)| range.clone()).collect()
    }
}

/// Runs the plan as the program does, downloading through `downloads`, with the report's tables
/// wrapped to `width` and the public addresses in full when `show_ip` is set, and returns the
/// exit code, the report and the progress log, once the run directory is gone. The log starts
/// as the program's does, with the host and its network. Every run is captured, so that no test
/// logs while another listens (see `common`).
async fn run(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    cancel: &CancellationToken,
    downloads: &Downloads,
    width: Option<u16>,
    show_ip: bool,
) -> (u8, String, Vec<String>) {
    let work = tempfile::tempdir().expect("a work directory for the run");
    let mut out = Vec::new();
    let (log, capturing) = Captured::start(true);
    progress::host(&host);
    progress::network(&host.network);
    let report = ReportOptions {
        out: &mut out,
        width,
        show_ip,
    };
    let fetch = |family, url, range| downloads.open(family, url, range);
    let connections = downloads
        .connections
        .unwrap_or(usize::from(app::DOWNLOAD_CONNECTIONS));
    let code = app::execute(
        planned,
        host,
        work.path(),
        cancel,
        fetch,
        connections,
        report,
    )
    .await
    .expect("the run starts");
    drop(capturing);
    let left = std::fs::read_dir(work.path())
        .expect("the work directory stays")
        .count();
    assert_eq!(left, 0, "run directory must be removed");
    let report = String::from_utf8(out).expect("the report is UTF-8");
    (code, report, log.lines())
}

/// Notes the work directory that its context hands it.
#[derive(Default, Deserialize)]
struct ShowsWorkDir {
    #[serde(skip)]
    seen: Arc<Mutex<Option<PathBuf>>>,
}

impl Plugin for ShowsWorkDir {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.work-dir");

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let dir = ctx.work_dir().to_path_buf();
        *self.seen.lock().expect("nothing panics while holding it") = Some(dir);
        Ok(Output::default())
    }
}

#[tokio::test]
async fn a_work_directory_given_with_dots_leaves_none_in_the_paths_under_it() {
    let work = tempfile::tempdir().expect("a work directory for the run");
    let seen = Arc::default();
    let plugin = ShowsWorkDir {
        seen: Arc::clone(&seen),
    };
    let downloads = Downloads::default();
    let mut out = Vec::new();
    let report = ReportOptions {
        out: &mut out,
        width: None,
        show_ip: false,
    };
    // Captured, so that no test logs while another listens (see `common`).
    let (_log, capturing) = Captured::start(false);
    let code = app::execute(
        vec![Planned::new(plugin, Measured::CpuGeekbench)],
        HostProfile::default(),
        &work.path().join(".").join("."),
        &CancellationToken::new(),
        |family, url, range| downloads.open(family, url, range),
        1,
        report,
    )
    .await
    .expect("the run starts");
    drop(capturing);

    assert_eq!(code, 0);
    let seen = seen
        .lock()
        .expect("nothing panics while holding it")
        .clone()
        .expect("the plugin ran");
    let shown = seen.to_string_lossy();
    assert!(!shown.contains("/./"), "{shown}");
    let run_dir = seen
        .parent()
        .expect("the plugin's directory is in the run directory");
    assert_eq!(run_dir.parent(), Some(work.path()), "{shown}");
}

/// A file that [`ReadsFiles`] read: its name, where its context put it, and what it held.
type FileRead = (&'static str, PathBuf, String);

/// Asks for the files that it was built with, and notes what each holds and where its context
/// put it.
#[derive(Default, Deserialize)]
struct ReadsFiles {
    #[serde(skip)]
    files: Vec<FileSpec>,
    #[serde(skip)]
    seen: Arc<Mutex<Vec<FileRead>>>,
}

impl Plugin for ReadsFiles {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.files");

    fn files(&self) -> Vec<FileSpec> {
        self.files.clone()
    }

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let mut seen = Vec::new();
        for file in &self.files {
            let path = ctx.file(file.name).expect("it runs with its files");
            let text = std::fs::read_to_string(path).expect("the file reads");
            seen.push((file.name, path.to_path_buf(), text));
        }
        *self.seen.lock().expect("nothing panics while holding it") = seen;
        Ok(Output::default())
    }
}

#[tokio::test]
async fn a_plugin_gets_its_files_downloaded_into_the_run_directory_or_where_they_are() {
    let dir = tempfile::tempdir().expect("a directory for the plugin's own file");
    let own = dir.path().join("own.json");
    std::fs::write(&own, "on the host\n").expect("the file is written");
    let seen = Arc::default();
    let plugin = ReadsFiles {
        files: vec![
            FileSpec::new(
                "servers",
                FileSource::Url("https://lists.example/servers.json".to_owned()),
            ),
            FileSpec::new("own", FileSource::Path(own.clone())),
        ],
        seen: Arc::clone(&seen),
    };
    let downloads = Downloads {
        body: Some(b"downloaded\n"),
        ..Downloads::default()
    };

    let (code, text, _log) = run(
        vec![Planned::new(plugin, Measured::CpuGeekbench)],
        HostProfile::default(),
        &CancellationToken::new(),
        &downloads,
        None,
        false,
    )
    .await;

    assert_eq!(code, 0, "{text}");
    assert_eq!(downloads.asked(), ["https://lists.example/servers.json"]);
    assert_eq!(downloads.families(), [None], "over either family");
    let seen = seen
        .lock()
        .expect("nothing panics while holding it")
        .clone();
    let [(servers, path, downloaded), mine] = seen.as_slice() else {
        panic!("the plugin read both files: {seen:?}");
    };
    assert_eq!((*servers, downloaded.as_str()), ("servers", "downloaded\n"));
    assert!(
        path.ends_with("files/test.files/servers"),
        "in a directory of the plugin's own in the run directory: {}",
        path.display()
    );
    assert_eq!(mine, &("own", own, "on the host\n".to_owned()));
}

#[tokio::test]
async fn a_plugin_whose_file_cannot_be_had_is_skipped_with_the_reason() {
    let url = "https://lists.example/servers.json";
    let plugin = ReadsFiles {
        files: vec![FileSpec::new("servers", FileSource::Url(url.to_owned()))],
        seen: Arc::default(),
    };

    let (code, text, log) = execute(
        vec![Planned::new(plugin, Measured::CpuGeekbench)],
        HostProfile::default(),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(code, 0);
    let reason = format!("file servers unavailable: no download in this test: {url}");
    assert!(text.contains(&format!("\n  skipped: {reason}\n")), "{text}");
    assert_eq!(log.last(), Some(&format!("test.files skipped: {reason}")));
}

/// What a plugin saw of one optional file: its name, and whether it had the file or why not.
type OptionalSeen = (&'static str, Result<String, String>);

/// Asks for optional files, and notes for each the text it had or why it is missing.
#[derive(Default, Deserialize)]
struct ReadsOptionalFiles {
    #[serde(skip)]
    files: Vec<FileSpec>,
    #[serde(skip)]
    seen: Arc<Mutex<Vec<OptionalSeen>>>,
}

impl Plugin for ReadsOptionalFiles {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.optional-files");

    fn files(&self) -> Vec<FileSpec> {
        self.files.clone()
    }

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let seen = self
            .files
            .iter()
            .map(|file| {
                let had = match ctx.file(file.name) {
                    Some(path) => Ok(std::fs::read_to_string(path).expect("the file reads")),
                    None => Err(ctx.missing(file.name).expect("it says why").to_owned()),
                };
                (file.name, had)
            })
            .collect();
        *self.seen.lock().expect("nothing panics while holding it") = seen;
        Ok(Output::default())
    }
}

/// An optional file of `family`, at the address `url`.
fn optional(name: &'static str, url: &str, family: IpFamily) -> FileSpec {
    FileSpec {
        family: Some(family),
        optional: true,
        ..FileSpec::new(name, FileSource::Url(url.to_owned()))
    }
}

#[tokio::test]
async fn optional_files_come_over_their_family_and_a_missing_one_leaves_the_plugin_to_run() {
    let four = "https://lookup.example/four";
    let six = "https://lookup.example/six";
    let read = |body: Option<&'static [u8]>| async move {
        let seen = Arc::default();
        let plugin = ReadsOptionalFiles {
            files: vec![
                optional("four", four, IpFamily::Ipv4),
                optional("six", six, IpFamily::Ipv6),
            ],
            seen: Arc::clone(&seen),
        };
        let host = HostProfile {
            network: ipv4_only(),
            ..HostProfile::default()
        };
        let downloads = Downloads {
            body,
            ..Downloads::default()
        };
        let planned = vec![Planned::new(plugin, Measured::CpuGeekbench)];
        let (code, text, _) = run(
            planned,
            host,
            &CancellationToken::new(),
            &downloads,
            None,
            false,
        )
        .await;
        assert_eq!(code, 0);
        assert!(
            summary_line(&text, "test.optional-files").is_some_and(|line| line.contains("done")),
            "it runs: {text}"
        );
        let seen = seen
            .lock()
            .expect("no test panics while holding it")
            .clone();
        (seen, downloads.asked(), downloads.families())
    };

    let (seen, asked, families) = read(Some(b"cache-1\n")).await;
    let unreachable = "not fetched: the host has no IPv6 connection".to_owned();
    assert_eq!(
        seen,
        [
            ("four", Ok("cache-1\n".to_owned())),
            ("six", Err(unreachable.clone()))
        ]
    );
    assert_eq!(
        asked,
        [four],
        "the file of IPv6, which the host lacks, is not asked for"
    );
    assert_eq!(families, [Some(IpFamily::Ipv4)], "over IPv4 only");

    let (seen, ..) = read(None).await;
    assert_eq!(
        seen,
        [
            ("four", Err(format!("no download in this test: {four}"))),
            ("six", Err(unreachable)),
        ],
        "a failed download leaves its reason, and the plugin runs"
    );
}

/// Asks for a file, and needs IPv4.
#[derive(Default, Deserialize)]
struct ReadsFilesOverIpv4 {}

impl Plugin for ReadsFilesOverIpv4 {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        requires: Requirements {
            net: Net::Ipv4,
            ..Requirements::NONE
        },
        ..descriptor("test.files-ipv4")
    };

    fn files(&self) -> Vec<FileSpec> {
        vec![FileSpec::new(
            "servers",
            FileSource::Url("https://lists.example/servers.json".to_owned()),
        )]
    }

    async fn run(&self, _ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        Ok(Output::default())
    }
}

#[tokio::test]
async fn the_files_of_a_plugin_that_cannot_run_here_are_not_downloaded() {
    let downloads = Downloads {
        body: Some(b"{}"),
        ..Downloads::default()
    };

    let (code, text, _log) = run(
        vec![Planned::new(ReadsFilesOverIpv4 {}, Measured::CpuGeekbench)],
        HostProfile::default(),
        &CancellationToken::new(),
        &downloads,
        None,
        false,
    )
    .await;

    assert_eq!(code, 0);
    assert!(
        text.contains("\n  skipped: needs an IPv4 connection\n"),
        "{text}"
    );
    assert!(downloads.asked().is_empty(), "{:?}", downloads.asked());
}

/// [`run`] without downloads, with the report as in a file and the addresses masked.
async fn execute(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    cancel: &CancellationToken,
) -> (u8, String, Vec<String>) {
    run(planned, host, cancel, &Downloads::default(), None, false).await
}

#[test]
fn ids_are_comma_separated_and_work_dir_defaults_to_cwd() {
    let cli = cli(&["--only", "test.good,test.other", "--skip", "test.other"]);
    assert_eq!(cli.only, ["test.good", "test.other"]);
    assert_eq!(cli.skip, ["test.other"]);
    assert_eq!(cli.work_dir, PathBuf::from("."));
}

#[test]
fn a_download_uses_32_connections_unless_told_another_number_from_1_to_256() {
    use clap::Parser;

    assert_eq!(cli(&[]).download_connections, 32);
    for number in [1, 7, 256] {
        let text = number.to_string();
        assert_eq!(
            cli(&["--download-connections", &text]).download_connections,
            number
        );
    }
    for wrong in ["0", "257", "-1", "many"] {
        let parsed = Cli::try_parse_from(["lowendaff-bench", "--download-connections", wrong]);
        assert!(parsed.is_err(), "{wrong}");
    }
}

#[test]
fn selection_keeps_registry_order_and_rejects_unknown_ids() {
    assert_eq!(
        app::select(&plugins(), &[], &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(&plugins(), &ids(&["test.other", "test.good"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(&plugins(), &[], &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(&plugins(), &ids(&["nope"]), &ids(&["nada"])).unwrap_err(),
        [
            "unknown test item or group `nope`",
            "unknown test item or group `nada`"
        ]
    );
}

#[test]
fn a_group_name_selects_every_plugin_of_the_group() {
    assert_eq!(
        app::select(&groups(), &ids(&["test"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(&groups(), &[], &ids(&["test"])).unwrap(),
        ["lone"]
    );
    assert_eq!(
        app::select(&groups(), &ids(&["test.other", "lone"]), &[]).unwrap(),
        ["lone", "test.other"]
    );
    assert_eq!(
        app::select(&groups(), &ids(&["test"]), &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(&groups(), &ids(&["tes", "test."]), &ids(&["good", "lon"])).unwrap_err(),
        [
            "unknown test item or group `tes`",
            "unknown test item or group `test.`",
            "unknown test item or group `good`",
            "unknown test item or group `lon`",
        ],
        "a name is a whole id or group"
    );
}

#[test]
fn config_file_holds_plugin_tables_only() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        path
    };

    let good = write("good.toml", "[plugins.\"test.good\"]\nsamples = 2\n");
    let expected = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    assert_eq!(app::load_config(Some(&good)).unwrap(), expected);

    let stray = write("stray.toml", "timeout = 3\n");
    assert_eq!(
        app::load_config(Some(&stray)).unwrap_err(),
        "unknown key `timeout` in the config file; only [plugins.\"<id>\"] tables are supported"
    );

    let broken = write("broken.toml", "[plugins\n");
    assert!(
        app::load_config(Some(&broken))
            .unwrap_err()
            .starts_with("cannot parse the config file")
    );

    let bad_value = write("bad.toml", "[plugins.\"test.good\"]\nsamples = \"many\"\n");
    let config = bad_value.to_string_lossy().into_owned();
    let Err(errors) = app::prepare(&cli(&["--config", &config]), &plugins()) else {
        panic!("bad config must not plan")
    };
    assert!(
        errors[0].starts_with("invalid settings for test item test.good: "),
        "{errors:?}"
    );
}

#[tokio::test]
async fn a_run_prints_sections_and_summary_then_removes_its_directory() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("bench.toml");
    std::fs::write(&config, "[plugins.\"test.good\"]\nsamples = 3\n").unwrap();
    let planned = app::prepare(
        &cli(&[
            "--skip",
            "test.other",
            "--config",
            &config.to_string_lossy(),
        ]),
        &plugins(),
    )
    .unwrap();

    let (code, text, _) = execute(planned, HostProfile::default(), &CancellationToken::new()).await;

    assert_eq!(code, 0);
    assert!(
        text.starts_with(&format!(
            "lowendaff-bench {}\nEnvironment: no virtualization detected\n\n[System]\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(text.contains("\n\n[test]\n"), "{text}");
    assert!(
        text.contains("  Single-core         3\n"),
        "the score that the config gave: {text}"
    );
    assert!(
        text.contains(
            "\n1 item: 1 done, 0 skipped, 0 failed, 0 timed out, 0 interrupted. Total time "
        ),
        "{text}"
    );
}

#[tokio::test]
async fn an_interrupted_run_exits_130_and_still_lists_every_plugin() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let planned = app::prepare(&cli(&[]), &plugins()).unwrap();

    let (code, text, _) = execute(planned, HostProfile::default(), &cancel).await;

    assert_eq!(code, EXIT_INTERRUPTED);
    assert!(
        text.contains("[test]\n  test.good\n    interrupted\n  test.other\n    interrupted\n"),
        "two plugins of one group share a section: {text}"
    );
    assert_eq!(text.matches("[test]").count(), 1, "{text}");
    assert!(
        text.contains("2 items: 0 done, 0 skipped, 0 failed, 0 timed out, 2 interrupted."),
        "{text}"
    );
}

stand_in!(
    InfoDedicated,
    Descriptor {
        requires: Requirements {
            platforms: &[PlatformKind::Dedicated],
            ..Requirements::NONE
        },
        ..descriptor("test.info-dedicated")
    }
);
stand_in!(
    InfoVm,
    Descriptor {
        requires: Requirements {
            platforms: &[PlatformKind::Vm],
            ..Requirements::NONE
        },
        ..descriptor("test.info-vm")
    }
);

const SMARTCTL: ToolSpec = ToolSpec {
    name: "smartctl",
    version: "7.4",
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://example.invalid/smartctl-x86_64",
            sha256: "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8",
            size: 24,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://example.invalid/smartctl-aarch64",
            sha256: "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8",
            size: 24,
        },
    ],
};

stand_in!(
    DiskHealth,
    Descriptor {
        requires: Requirements {
            tools: &[SMARTCTL],
            net: Net::None,
            platforms: &[PlatformKind::Dedicated],
            glibc: None,
            numa: false,
        },
        ..descriptor("test.disk-health")
    }
);
// A test between NUMA nodes, which needs two of them.
stand_in!(
    Numa,
    Descriptor {
        requires: Requirements {
            numa: true,
            ..Requirements::NONE
        },
        ..descriptor("test.numa")
    }
);

fn platform_plugins() -> Vec<Entry> {
    vec![
        entry::<InfoDedicated>(),
        entry::<InfoVm>(),
        entry::<DiskHealth>(),
        entry::<Numa>(),
    ]
}

/// A host whose NUMA layout has `count` nodes, each with a CPU and memory.
fn with_nodes(count: usize) -> HostProfile {
    HostProfile {
        platform: Platform::Dedicated,
        numa: Read::Done(Layout {
            nodes: (0..count)
                .map(|id| Node {
                    id,
                    cpus: vec![id],
                    total: Some(1 << 30),
                    free: Some(1 << 29),
                })
                .collect(),
            cpus: Some((0..count).collect()),
            mems: None,
        }),
        ..HostProfile::default()
    }
}

/// The summary's line of the plugin `id`, which a plugin that ran has.
fn summary_line<'a>(text: &'a str, id: &str) -> Option<&'a str> {
    text.lines().find(|line| {
        line.strip_prefix("  ")
            .is_some_and(|rest| rest.starts_with(&format!("{id} ")))
    })
}

#[tokio::test]
async fn a_plugin_that_needs_numa_nodes_runs_only_where_two_can_be_used() {
    let registry = [entry::<Numa>()];
    for (count, runs) in [(1, false), (2, true)] {
        let planned = app::prepare(&cli(&[]), &registry).unwrap();
        let (code, text, _) = execute(planned, with_nodes(count), &CancellationToken::new()).await;
        assert_eq!(code, 0);
        assert_eq!(
            summary_line(&text, "test.numa").is_some(),
            runs,
            "{count} nodes: {text}"
        );
        assert_eq!(
            text.contains("  Not applicable here: test.numa\n"),
            !runs,
            "{count} nodes: {text}"
        );
        let total = if runs {
            "1 item: 1 done,"
        } else {
            "0 items: 0 done,"
        };
        assert!(
            text.contains(total),
            "{count} nodes, not counted elsewhere: {text}"
        );
    }
}

#[tokio::test]
async fn only_plugins_for_the_detected_platform_run() {
    let planned = app::prepare(&cli(&[]), &platform_plugins()).unwrap();
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        ..HostProfile::default()
    };

    let (code, text, log) = execute(planned, host, &CancellationToken::new()).await;

    assert_eq!(code, 0);
    assert!(
        text.starts_with(&format!(
            "lowendaff-bench {}\nEnvironment: KVM virtual machine\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(text.contains("\n\n[test]\n"), "{text}");
    assert!(
        text.contains("  Not applicable here: test.info-dedicated, test.disk-health, test.numa\n"),
        "{text}"
    );
    assert!(
        summary_line(&text, "test.info-vm").is_some_and(|line| line.contains("done")),
        "{text}"
    );
    assert!(
        !log.iter().any(|line| line.starts_with("Tools: ")),
        "tools of plugins that do not apply must not be downloaded: {log:?}"
    );
    assert!(
        text.contains("\n1 item: 1 done, 0 skipped, 0 failed, 0 timed out, 0 interrupted."),
        "{text}"
    );
}

stand_in!(
    NeedsSmartctl,
    Descriptor {
        requires: Requirements {
            tools: &[SMARTCTL],
            ..Requirements::NONE
        },
        ..descriptor("test.smartctl")
    }
);

#[tokio::test]
async fn a_tool_that_cannot_be_provisioned_skips_its_plugins_with_the_reason() {
    let registry = [entry::<NeedsSmartctl>()];
    let planned = app::prepare(&cli(&[]), &registry).unwrap();
    // No download is tried: the tool has no build for this architecture.
    let host = HostProfile {
        arch: "riscv64".to_owned(),
        ..HostProfile::default()
    };

    let (code, text, log) = execute(planned, host, &CancellationToken::new()).await;

    assert_eq!(code, 0);
    let reason = "tool smartctl unavailable: no build for riscv64";
    assert!(text.contains(&format!("\n  skipped: {reason}\n")), "{text}");
    assert_eq!(
        log,
        [
            "Host: no virtualization detected, riscv64".to_owned(),
            "Network: IPv4 unavailable".to_owned(),
            "Network: IPv6 unavailable".to_owned(),
            "Tools: smartctl 7.4 failed: no build for riscv64".to_owned(),
            format!("test.smartctl skipped: {reason}"),
        ]
    );
}

const GEEKBENCH: ToolSpec = ToolSpec {
    name: "geekbench",
    version: "7.1.0",
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    assets: SMARTCTL.assets,
};

stand_in!(
    NeedsGlibcAndIpv4,
    Descriptor {
        requires: Requirements {
            tools: &[GEEKBENCH],
            net: Net::Ipv4,
            platforms: &[],
            glibc: Some(GlibcVersion {
                major: 2,
                minor: 35,
            }),
            numa: false,
        },
        ..descriptor("test.geekbench")
    }
);

/// A network whose IPv4 side reached a target, with no public address asked for.
fn ipv4_only() -> Network {
    Network {
        ipv4: Family {
            targets: vec![("1.1.1.1:443".parse().expect("an address"), Reach::Connected)],
            address: Address::NotAsked,
        },
        ipv6: Family::default(),
    }
}

#[tokio::test]
async fn tools_of_items_the_host_cannot_run_are_not_downloaded() {
    let registry = [entry::<NeedsGlibcAndIpv4>()];
    let without_glibc = HostProfile {
        libc: Libc::NoGlibc,
        network: ipv4_only(),
        ..HostProfile::default()
    };
    let without_ipv4 = HostProfile {
        libc: Libc::GlibcUnknown,
        ..HostProfile::default()
    };
    let cases = [
        (
            without_glibc,
            "needs glibc 2.35 or newer; this host has no glibc",
        ),
        (without_ipv4, "needs an IPv4 connection"),
    ];
    for (host, reason) in cases {
        let planned = app::prepare(&cli(&[]), &registry).unwrap();
        let downloads = Downloads::default();
        let (code, text, log) = run(
            planned,
            host,
            &CancellationToken::new(),
            &downloads,
            None,
            false,
        )
        .await;
        assert_eq!(code, 0);
        assert_eq!(
            downloads.asked(),
            Vec::<&str>::new(),
            "nothing is downloaded for an item that is skipped anyway"
        );
        assert!(
            !log.iter().any(|line| line.starts_with("Tools: ")),
            "{log:?}"
        );
        assert!(text.contains(&format!("skipped: {reason}")), "{text}");
    }
}

/// The size of [`script`]: two byte ranges.
const SCRIPT_SIZE: u64 = 2 * MIN_RANGE;

/// A shell script that these tests serve as a tool. It says ok, and a comment makes it as large as
/// two byte ranges, so that it comes in ranges when the run may use two connections.
fn script() -> &'static [u8] {
    static SCRIPT: OnceLock<Vec<u8>> = OnceLock::new();
    SCRIPT.get_or_init(|| {
        let mut script = b"#!/bin/sh\necho ok\nexit 0\n".to_vec();
        let size = usize::try_from(SCRIPT_SIZE).expect("in memory");
        script.resize(size - 1, b'#');
        script.push(b'\n');
        script
    })
}

const SCRIPT_TOOL: ToolSpec = ToolSpec {
    name: "script",
    version: "1.0",
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://example.invalid/script-x86_64",
            sha256: "ce21818e73a94088130a9c7d7ff04775c27a9a04d96b3b68540f01b49cb1a0df",
            size: SCRIPT_SIZE,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://example.invalid/script-aarch64",
            sha256: "ce21818e73a94088130a9c7d7ff04775c27a9a04d96b3b68540f01b49cb1a0df",
            size: SCRIPT_SIZE,
        },
    ],
};

/// Runs the tool that it was handed, and fails unless the tool says ok and its directory is one
/// that other users may search but not list: a tool that runs as another user starts through it.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunsScript {}

impl Plugin for RunsScript {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        requires: Requirements {
            tools: &[SCRIPT_TOOL],
            ..Requirements::NONE
        },
        ..descriptor("test.script")
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        use std::os::unix::fs::PermissionsExt;

        let program = ctx
            .tool(SCRIPT_TOOL.name)
            .expect("the orchestrator runs a plugin only when its tools are ready");
        let dir_mode = program
            .parent()
            .and_then(|dir| std::fs::metadata(dir).ok())
            .map(|metadata| metadata.permissions().mode() & 0o777);
        let said = tokio::process::Command::new(program)
            .kill_on_drop(true)
            .output()
            .await
            .map(|output| output.stdout);
        match said {
            Ok(said) if said == b"ok\n" && dir_mode == Some(0o711) => Ok(Output::default()),
            _ => Err(Stop::Failed(Failure::Cancelled)),
        }
    }
}

#[tokio::test]
async fn the_program_hands_its_plugins_the_tools_it_downloaded() {
    let registry = [entry::<RunsScript>()];
    let host = || HostProfile {
        arch: "x86_64".to_owned(),
        ..HostProfile::default()
    };
    let url = "https://example.invalid/script-x86_64";
    let in_ranges = vec![Some(0..MIN_RANGE), Some(MIN_RANGE..SCRIPT_SIZE)];
    let one_range = vec![Some(0..SCRIPT_SIZE)];
    for (connections, ranges) in [(None, in_ranges), (Some(1), one_range)] {
        let planned = app::prepare(&cli(&[]), &registry).unwrap();
        let downloads = Downloads {
            body: Some(script()),
            connections,
            ..Downloads::default()
        };

        let cancel = CancellationToken::new();
        let (code, text, log) = run(planned, host(), &cancel, &downloads, None, false).await;

        assert_eq!(code, 0);
        assert_eq!(downloads.asked(), vec![url; ranges.len()]);
        assert_eq!(downloads.ranges(), ranges, "{connections:?} connections");
        assert_eq!(
            downloads.families(),
            vec![None; ranges.len()],
            "a tool comes over either family, through a proxy when one is set"
        );
        assert!(
            log.contains(&"Tools: script 1.0 ready".to_owned()),
            "{log:?}"
        );
        assert!(
            summary_line(&text, "test.script").is_some_and(|line| line.contains("done in")),
            "the plugin ran the tool it was handed: {text}"
        );
    }
}

#[tokio::test]
async fn ctrl_c_during_the_downloads_ends_the_run_at_once() {
    let registry = [entry::<RunsScript>()];
    let planned = app::prepare(&cli(&[]), &registry).unwrap();
    let host = HostProfile {
        arch: "x86_64".to_owned(),
        ..HostProfile::default()
    };
    let cancel = CancellationToken::new();
    let downloads = Downloads {
        body: Some(script()),
        interrupt: Some(cancel.clone()),
        ..Downloads::default()
    };

    let (code, text, log) = run(planned, host, &cancel, &downloads, None, false).await;

    assert_eq!(code, EXIT_INTERRUPTED);
    assert_eq!(downloads.asked(), ["https://example.invalid/script-x86_64"]);
    assert!(
        log.contains(&"Tools: script 1.0 failed: download interrupted".to_owned()),
        "{log:?}"
    );
    assert!(
        summary_line(&text, "test.script").is_some_and(|line| line.ends_with("interrupted")),
        "{text}"
    );
}

#[tokio::test]
async fn the_report_wraps_its_tables_to_the_width_it_is_given() {
    let mount_point = "/srv/a long mount point name that wraps on a narrow terminal";
    let filesystem = Filesystem {
        mount_point: mount_point.to_owned(),
        fs_type: "ext4".to_owned(),
        devices: Vec::new(),
        zfs_pool: None,
        disks: Vec::new(),
        space: None,
    };
    let host = || HostProfile {
        storage: Read::Done(StorageFacts {
            filesystems: vec![filesystem.clone()],
            ..StorageFacts::default()
        }),
        ..HostProfile::default()
    };
    let downloads = Downloads::default();
    let cancel = CancellationToken::new();
    for (width, wraps) in [(None, false), (Some(40), true)] {
        let (_, text, _) = run(Vec::new(), host(), &cancel, &downloads, width, false).await;
        assert_eq!(
            !text.lines().any(|line| line.contains(mount_point)),
            wraps,
            "{width:?}: {text}"
        );
    }
}

/// What a plugin saw of its host: the platform, the architecture, the C library and whether
/// IPv4 reached the Internet.
type Seen = (Platform, String, Libc, bool);

/// Notes the host that its context hands it.
#[derive(Default, Deserialize)]
struct ShowsHost {
    #[serde(skip)]
    seen: Arc<Mutex<Option<Seen>>>,
}

impl Plugin for ShowsHost {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = descriptor("test.host");

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let host = ctx.host();
        let seen = (
            host.platform,
            host.arch.clone(),
            host.libc,
            host.network.ipv4.reachable(),
        );
        *self.seen.lock().expect("nothing panics while holding it") = Some(seen);
        Ok(Output::default())
    }
}

#[tokio::test]
async fn plugins_see_the_host_as_the_run_read_it() {
    let seen = Arc::default();
    let plugin = ShowsHost {
        seen: Arc::clone(&seen),
    };
    let host = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        arch: "riscv64".to_owned(),
        libc: Libc::NoGlibc,
        network: ipv4_only(),
        ..HostProfile::default()
    };

    let (code, _, _) = execute(
        vec![Planned::new(plugin, Measured::CpuGeekbench)],
        host,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(code, 0);
    assert_eq!(
        *seen.lock().unwrap(),
        Some((
            Platform::Vm(Hypervisor::Kvm),
            "riscv64".to_owned(),
            Libc::NoGlibc,
            true
        ))
    );
}

#[tokio::test]
async fn the_hosts_addresses_appear_in_full_only_in_the_report_and_only_on_request() {
    let host = || HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![("1.1.1.1:443".parse().expect("a target"), Reach::Connected)],
                address: Address::Found {
                    address: "203.0.113.7".parse().expect("an address"),
                    source: "Cloudflare",
                },
            },
            ipv6: Family {
                targets: vec![(
                    "[2606:4700:4700::1111]:443".parse().expect("a target"),
                    Reach::Connected,
                )],
                address: Address::Found {
                    address: "2001:db8::7".parse().expect("an address"),
                    source: "ipify",
                },
            },
        },
        ..HostProfile::default()
    };
    let full = ["203.0.113.7", "2001:db8::7"];
    for show_ip in [false, true] {
        let planned = app::prepare(&cli(&[]), &plugins()).unwrap();
        let downloads = Downloads::default();
        let cancel = CancellationToken::new();
        let (_, text, log) = run(planned, host(), &cancel, &downloads, None, show_ip).await;
        let log = log.join("\n");
        assert!(
            log.contains("203.0.*.*") && log.contains("2001:db8:*"),
            "{log}"
        );
        for address in full {
            assert!(!log.contains(address), "the log masks {address}: {log}");
            assert_eq!(text.contains(address), show_ip, "{address}: {text}");
        }
    }
}

#[test]
fn the_list_shows_where_each_plugin_applies() {
    let mut out = Vec::new();
    app::list(&platform_plugins(), &mut out);

    assert_eq!(
        String::from_utf8(out).unwrap(),
        "4 test items\n\
         [test] (group: test)\n\
         \x20 test.info-dedicated  test.info-dedicated (applies to: dedicated server)\n\
         \x20 test.info-vm  test.info-vm (applies to: virtual machine)\n\
         \x20 test.disk-health  test.disk-health (applies to: dedicated server) \
         (tools: smartctl 7.4)\n\
         \x20 test.numa  test.numa (applies to: several NUMA nodes)\n"
    );
}

#[test]
fn the_list_keeps_each_group_together_in_the_reports_order() {
    let mut out = Vec::new();
    app::list(&groups(), &mut out);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "3 test items\n\
         [test] (group: test)\n\
         \x20 test.good  test.good\n\
         \x20 test.other  test.other\n\
         [lone] (group: lone)\n\
         \x20 lone  lone\n"
    );

    let mut out = Vec::new();
    app::list(&registry::all(), &mut out);
    let text = String::from_utf8(out).unwrap();
    let headings: Vec<&str> = text.lines().filter(|line| line.starts_with('[')).collect();
    assert_eq!(
        headings,
        [
            "[CPU performance] (group: cpu)",
            "[Memory performance] (group: memory)",
            "[Disks] (group: disk)",
            "[Network] (group: network)",
            "[IP] (group: ip)"
        ]
    );
    let ids: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(
        ids,
        [
            "cpu.sysbench",
            "cpu.geekbench",
            "memory.sysbench",
            "memory.stream",
            "memory.latency",
            "memory.numa",
            "disk.health",
            "disk.fio",
            "network.iperf3",
            "network.trace",
            "ip.quality",
            "ip.media"
        ]
    );
    let cpu: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  cpu."))
        .collect();
    assert_eq!(
        cpu,
        [
            "  cpu.sysbench  CPU performance (tools: sysbench 1.0.20)",
            "  cpu.geekbench  Geekbench 7 (tools: geekbench 7.1.0)"
        ],
        "the reporter names the plugins"
    );
    let memory: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  memory."))
        .collect();
    assert_eq!(
        memory,
        [
            "  memory.sysbench  Memory sysbench (tools: sysbench 1.0.20)",
            "  memory.stream  Memory STREAM (tools: stream 5.10)",
            "  memory.latency  Memory latency (tools: multichase 2026.10.01)",
            "  memory.numa  Memory NUMA (applies to: several NUMA nodes) \
             (tools: multichase 2026.10.01)"
        ]
    );
    let disk: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  disk."))
        .collect();
    assert_eq!(
        disk,
        [
            "  disk.health  Disk health (applies to: dedicated server) (tools: smartctl 7.5)",
            "  disk.fio  Disk performance (tools: fio 3.43)"
        ]
    );
    let network: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  network."))
        .collect();
    assert_eq!(
        network,
        [
            "  network.iperf3  Network speed (tools: iperf3 3.22)",
            "  network.trace  Route tracing (tools: nexttrace 1.7.6)"
        ]
    );
    let ip: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("  ip."))
        .collect();
    assert_eq!(
        ip,
        ["  ip.quality  IP reputation", "  ip.media  Media unlock"]
    );
}

#[test]
fn the_plan_keeps_the_reporters_names() {
    let planned = app::prepare(&cli(&["--only", "ip"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("ip.quality", Some("IP reputation")),
            ("ip.media", Some("Media unlock"))
        ]
    );
    let planned = app::prepare(&cli(&["--only", "cpu"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("cpu.sysbench", Some("CPU performance")),
            ("cpu.geekbench", Some("Geekbench 7"))
        ]
    );
    let planned = app::prepare(&cli(&["--only", "disk"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("disk.health", Some("Disk health")),
            ("disk.fio", Some("Disk performance"))
        ]
    );
    let planned = app::prepare(&cli(&["--only", "memory"]), &registry::all()).unwrap();
    let names: Vec<(&str, Option<&str>)> = planned
        .iter()
        .map(|planned| (planned.id(), planned.title()))
        .collect();
    assert_eq!(
        names,
        [
            ("memory.sysbench", Some("Memory sysbench")),
            ("memory.stream", Some("Memory STREAM")),
            ("memory.latency", Some("Memory latency")),
            ("memory.numa", Some("Memory NUMA"))
        ],
        "the group selects its four plugins"
    );
}

#[test]
fn the_items_that_do_not_apply_here_are_named_as_the_report_names_them() {
    let planned = app::prepare(&cli(&[]), &registry::all()).unwrap();
    let vm = HostProfile {
        platform: Platform::Vm(Hypervisor::Kvm),
        ..with_nodes(1)
    };

    let (applicable, names) = orchestrator::for_host(planned, &vm);

    assert_eq!(names, ["Disk health", "Memory NUMA"]);
    assert_eq!(applicable.len(), 10, "the others run");
}
