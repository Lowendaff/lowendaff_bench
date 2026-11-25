//! Command-line flow: argument parsing, selection, config file, and a full run through
//! the terminal renderer with the run directory removed afterwards.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;

use bench::app;
use bench::app::Cli;
use bench::app::CpuSuite;
use bench::app::EXIT_INTERRUPTED;
use bench::app::MediaGroup;
use bench::app::ReportOptions;
use bench::app::TaierOperator;
use bench::app::TraceGroup;
use bench::orchestrator;
use bench::orchestrator::Planned;
use bench::progress;
use bench::provision::DownloadError;
use bench::provision::MIN_RANGE;
use bench::provision::Part;
use bench::registry;
use bench::registry::Entry;
use bench::report::Format;
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
use plugin_cpu::bench as cpu_bench;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::result::Scores;
use serde::Deserialize;

mod common;

/// What the tests' plugins return: `cpu.bench`'s output, empty by default so that the report shows
/// only how a plugin ended.
type Output = cpu_bench::Output;
type Failure = cpu_bench::Failure;

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
            geekbench: Some(cpu_bench::SuiteOutcome::Done {
                output: geekbench::Output {
                    scores: Some(Scores {
                        version: None,
                        single: self.samples,
                        multi: self.samples,
                        url: None,
                    }),
                    ..geekbench::Output::default()
                },
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
    registry::entry::<P>(Measured::cpu_bench)
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
type Asked = (Option<IpFamily>, String, Option<Range<u64>>);

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
        url: Cow<'static, str>,
        range: Option<Range<u64>>,
    ) -> Result<Part<&'static [u8]>, DownloadError> {
        let url = url.into_owned();
        self.asked
            .lock()
            .expect("no test panics while holding the addresses")
            .push((family, url.clone(), range.clone()));
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
    fn asked(&self) -> Vec<String> {
        let asked = self
            .asked
            .lock()
            .expect("no test panics while holding the addresses");
        asked.iter().map(|(_, url, _)| url.clone()).collect()
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
        format: Format::Text,
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
        format: Format::Text,
        width: None,
        show_ip: false,
    };
    // Captured, so that no test logs while another listens (see `common`).
    let (_log, capturing) = Captured::start(false);
    let code = app::execute(
        vec![Planned::new(plugin, Measured::cpu_bench)],
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
        vec![Planned::new(plugin, Measured::cpu_bench)],
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
        vec![Planned::new(plugin, Measured::cpu_bench)],
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
        let planned = vec![Planned::new(plugin, Measured::cpu_bench)];
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
        assert!(
            log.iter()
                .any(|line| line.starts_with("test.optional-files done in ")),
            "it runs: {log:?}"
        );
        assert!(!text.contains("done in "), "{text}");
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
        vec![Planned::new(ReadsFilesOverIpv4 {}, Measured::cpu_bench)],
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
    assert_eq!(cli.output, None);
    assert_eq!(cli.format, Format::Text);
}

#[test]
fn output_and_format_flags_accept_short_and_long_forms_and_md_alias() {
    use clap::Parser;

    let parsed = cli(&["-o", "report.md", "-f", "md"]);
    assert_eq!(parsed.output, Some(PathBuf::from("report.md")));
    assert_eq!(parsed.format, Format::Markdown);

    let parsed = cli(&["--output", "out.json", "--format", "json"]);
    assert_eq!(parsed.output, Some(PathBuf::from("out.json")));
    assert_eq!(parsed.format, Format::Json);

    assert_eq!(cli(&["--format", "markdown"]).format, Format::Markdown);
    assert_eq!(cli(&["--format", "text"]).format, Format::Text);
    assert!(Cli::try_parse_from(["lowendaff-bench", "--format", "xml"]).is_err());
}

#[test]
fn skip_leaves_out_single_plugins_or_groups_in_the_real_registry() {
    let all = registry::all();
    let planned = app::prepare(&cli(&["--skip", "cpu.bench,ip.media"]), &all).unwrap();
    let kept: Vec<&str> = planned.iter().map(Planned::id).collect();
    assert!(!kept.contains(&"cpu.bench"), "{kept:?}");
    assert!(!kept.contains(&"ip.media"), "{kept:?}");
    assert!(kept.contains(&"memory.sysbench"), "sibling stays: {kept:?}");
    assert!(kept.contains(&"ip.quality"), "sibling stays: {kept:?}");
    assert_eq!(kept.len(), all.len() - 2);

    let repeated = app::prepare(
        &cli(&[
            "--only",
            "ip,cpu",
            "--skip",
            "cpu.bench",
            "--skip",
            "ip.media",
        ]),
        &all,
    )
    .unwrap();
    let kept: Vec<&str> = repeated.iter().map(Planned::id).collect();
    assert_eq!(kept, ["ip.quality"]);
}

#[tokio::test]
async fn execute_writes_markdown_or_json_to_an_output_file_and_rejects_missing_dirs() {
    let dir = tempfile::tempdir().expect("a temp directory");
    let missing = dir.path().join("no-such-dir").join("report.md");
    let err = app::create_output(&missing).expect_err("missing parent directory must fail");
    assert!(
        err.starts_with("cannot write the report to "),
        "unexpected error: {err}"
    );

    for (name, format) in [
        ("report.md", Format::Markdown),
        ("report.json", Format::Json),
    ] {
        let out_path = dir.path().join(name);
        let file = app::create_output(&out_path).expect("output file opens");
        let work = tempfile::tempdir().expect("a work directory");
        let downloads = Downloads::default();
        let (_log, capturing) = Captured::start(false);
        let planned = app::prepare(&cli(&["--only", "test.good"]), &plugins()).unwrap();
        let code = app::execute(
            planned,
            HostProfile::default(),
            work.path(),
            &CancellationToken::new(),
            |family, url, range| downloads.open(family, url, range),
            1,
            ReportOptions {
                out: file,
                format,
                width: None,
                show_ip: false,
            },
        )
        .await
        .expect("run finishes");
        drop(capturing);
        assert_eq!(code, 0);
        let written = std::fs::read_to_string(&out_path).expect("report was written");
        match format {
            Format::Markdown => assert!(written.starts_with("# lowendaff-bench "), "{written}"),
            Format::Json => {
                let parsed: serde_json::Value =
                    serde_json::from_str(&written).expect("valid JSON report");
                assert_eq!(parsed["summary"]["items"], 1);
            }
            Format::Text => unreachable!(),
        }
    }
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
fn config_flag_is_rejected_and_enum_parameters_configure_plugins() {
    use clap::Parser;

    assert!(
        Cli::try_parse_from(["lowendaff-bench", "--config", "bench.toml"]).is_err(),
        "--config local config file launching must not exist"
    );
    assert!(Cli::try_parse_from(["lowendaff-bench", "--cpu-bench", "pts"]).is_err());
    assert!(Cli::try_parse_from(["lowendaff-bench", "--taier-operators", "starlink"]).is_err());
    assert!(Cli::try_parse_from(["lowendaff-bench", "--trace-groups", "mars"]).is_err());
    assert!(Cli::try_parse_from(["lowendaff-bench", "--media-groups", "antarctica"]).is_err());

    let parsed = cli(&[
        "--cpu-bench",
        "sysbench,7zip,hpl",
        "--taier-cities",
        "Beijing,Jiangsu",
        "--taier-operators",
        "telecom,unicom",
        "--trace-groups",
        "services,china",
        "--media-groups",
        "global,asia",
        "--no-dnsbl",
        "--no-mail",
        "--no-bgp-upload",
    ]);
    assert_eq!(
        parsed.cpu_bench,
        [CpuSuite::Sysbench, CpuSuite::SevenZip, CpuSuite::Hpl]
    );
    assert_eq!(parsed.taier_cities, ["Beijing", "Jiangsu"]);
    assert_eq!(
        parsed.taier_operators,
        [TaierOperator::Telecom, TaierOperator::Unicom]
    );
    assert_eq!(
        parsed.trace_groups,
        [TraceGroup::Services, TraceGroup::China]
    );
    assert_eq!(parsed.media_groups, [MediaGroup::Global, MediaGroup::Asia]);
    assert!(parsed.no_dnsbl);
    assert!(parsed.no_mail);
    assert!(parsed.no_bgp_upload);

    assert!(
        app::prepare(
            &cli(&["-a", "--no-trace-tcp", "--trace-packet-size", "1280",]),
            &registry::all(),
        )
        .is_ok(),
        "--no-trace-tcp and --trace-packet-size prepare cleanly"
    );

    let planned = app::prepare(&parsed, &registry::all()).expect("valid enum flags prepare");
    assert_eq!(planned.len(), registry::all().len());

    let expected = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    assert_eq!(
        app::parse_overrides(Some(r#"{"test.good":{"samples":2}}"#)).unwrap(),
        expected
    );
    assert!(
        app::parse_overrides(Some("{bad"))
            .unwrap_err()
            .starts_with("invalid plugin overrides: ")
    );
    let Err(errors) = app::prepare(
        &cli(&[
            "--internal-overrides",
            r#"{"test.good":{"samples":"many"}}"#,
        ]),
        &plugins(),
    ) else {
        panic!("bad overrides must not plan")
    };
    assert!(
        errors[0].starts_with("invalid settings for test item test.good: "),
        "{errors:?}"
    );
}

#[test]
fn socks5_and_github_proxy_flags_validate_upfront_with_other_cli_errors() {
    let valid_dual = cli(&[
        "--only",
        "test.good",
        "--socks5-proxy",
        "socks5h://user:s3cret@127.0.0.1:1080",
        "--github-proxy",
        "https://ghfast.top/",
    ]);
    let proxy = app::download_proxy(&valid_dual).expect("valid dual proxy flags");
    assert!(proxy.socks5.is_some());
    assert!(proxy.github.is_some());
    let (planned, prepared_proxy) =
        app::prepare_with_proxy(&valid_dual, &plugins()).expect("prepare_with_proxy succeeds");
    assert_eq!(planned.len(), 1);
    assert_eq!(prepared_proxy, proxy);

    let valid_socks5 = cli(&["--socks5-proxy", "socks5://127.0.0.1:1080"]);
    assert!(app::download_proxy(&valid_socks5).is_ok());

    let invalid = cli(&[
        "--only",
        "nope",
        "--socks5-proxy",
        "http://alice:s3cret@127.0.0.1:1080",
        "--github-proxy",
        "https://bob:s3cret@ghfast.top/",
    ]);
    let Err(errors) = app::prepare(&invalid, &plugins()) else {
        panic!("invalid proxy URLs and unknown item must fail upfront")
    };
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(
        errors[0].starts_with("invalid --socks5-proxy:"),
        "{errors:?}"
    );
    assert!(
        errors[1].starts_with("invalid --github-proxy:"),
        "{errors:?}"
    );
    assert_eq!(errors[2], "unknown test item or group `nope`");
    let joined = errors.join("\n");
    assert!(
        !joined.contains("s3cret"),
        "CLI validation errors must never echo proxy credentials: {joined}"
    );
}

#[tokio::test]
async fn a_run_prints_sections_and_summary_then_removes_its_directory() {
    let planned = app::prepare(
        &cli(&[
            "--skip",
            "test.other",
            "--internal-overrides",
            r#"{"test.good":{"samples":3}}"#,
        ]),
        &plugins(),
    )
    .unwrap();

    let (code, text, log) =
        execute(planned, HostProfile::default(), &CancellationToken::new()).await;

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
    assert!(!text.contains("\n[Summary]\n"), "{text}");
    assert!(!text.contains("done in "), "{text}");
    assert!(
        log.iter()
            .any(|line| line.starts_with("test.good done in ")),
        "{log:?}"
    );
}

#[tokio::test]
async fn an_interrupted_run_exits_130_and_still_lists_every_plugin() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let planned = app::prepare(&cli(&[]), &plugins()).unwrap();

    let (code, text, log) = execute(planned, HostProfile::default(), &cancel).await;

    assert_eq!(code, EXIT_INTERRUPTED);
    assert!(
        text.contains("[test]\n  test.good\n    interrupted\n  test.other\n    interrupted\n"),
        "two plugins of one group share a section: {text}"
    );
    assert_eq!(text.matches("[test]").count(), 1, "{text}");
    assert!(!text.contains("\n[Summary]\n"), "{text}");
    assert!(
        log.contains(&"test.good interrupted".to_owned())
            && log.contains(&"test.other interrupted".to_owned()),
        "{log:?}"
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
    all_arches: true,
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

#[tokio::test]
async fn a_plugin_that_needs_numa_nodes_runs_only_where_two_can_be_used() {
    let registry = [entry::<Numa>()];
    for (count, runs) in [(1, false), (2, true)] {
        let planned = app::prepare(&cli(&[]), &registry).unwrap();
        let (code, text, log) =
            execute(planned, with_nodes(count), &CancellationToken::new()).await;
        assert_eq!(code, 0);
        assert_eq!(text.contains("\n[test]\n"), runs, "{count} nodes: {text}");
        assert_eq!(
            log.iter().any(|line| line.contains("test.numa started")),
            runs,
            "{count} nodes: {log:?}"
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
        log.iter().any(|line| line.contains("test.info-vm started")),
        "{log:?}"
    );
    for other in ["test.info-dedicated", "test.disk-health", "test.numa"] {
        assert!(
            !log.iter().any(|line| line.contains(other)),
            "{other} must not run on KVM: {log:?}"
        );
    }
    assert!(
        !log.iter().any(|line| line.starts_with("Tools: ")),
        "tools of plugins that do not apply must not be downloaded: {log:?}"
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
    all_arches: true,
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
            Vec::<String>::new(),
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
    all_arches: true,
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
            log.iter()
                .any(|line| line.starts_with("test.script done in ")),
            "the plugin ran the tool it was handed: {log:?}"
        );
        assert!(!text.contains("done in "), "{text}");
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
    assert!(text.contains("\n[test]\n  interrupted\n"), "{text}");
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
        vec![Planned::new(plugin, Measured::cpu_bench)],
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
            "cpu.bench",
            "memory.sysbench",
            "memory.stream",
            "memory.latency",
            "memory.numa",
            "disk.health",
            "disk.fio",
            "network.iperf3",
            "network.taier",
            "network.cloudflare",
            "network.inetspeed",
            "network.trace",
            "network.bgp",
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
        [concat!(
            "  cpu.bench  CPU benchmark (tools: sysbench 1.0.20, 7zip 26.06, unixbench 6.0.1, ",
            "hpl 2.3, passmark 11.0.1004, passmark-libs 1.0, ycruncher 0.8.7.9547, ",
            "prime95 30.19b20, geekbench 7.1.0)"
        )],
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
            "  network.taier  China carrier speed",
            "  network.cloudflare  Cloudflare speed",
            "  network.inetspeed  Apple CDN speed",
            "  network.trace  Route tracing (tools: nexttrace 1.7.6)",
            "  network.bgp  BGP paths"
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
    assert_eq!(names, [("cpu.bench", Some("CPU benchmark"))]);
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
    assert_eq!(applicable.len(), 13, "the others run");
}

#[test]
fn zero_args_selects_tui_flag_is_gone_and_all_runs_full_cli_suite() {
    use clap::Parser;

    assert!(app::should_enter_tui(0));
    assert!(app::should_enter_tui(1));
    assert!(!app::should_enter_tui(2));
    assert!(
        Cli::try_parse_from(["lowendaff-bench", "--tui"]).is_err(),
        "--tui flag must be removed"
    );

    let via_flag = cli(&["--all"]);
    assert!(via_flag.all);
    let all = registry::all();
    let planned_flag = app::prepare(&via_flag, &all).unwrap();
    assert_eq!(planned_flag.len(), 15);

    let planned_short = app::prepare(&cli(&["-a", "--skip", "network.trace"]), &all).unwrap();
    assert_eq!(planned_short.len(), 14);

    let planned_only_all =
        app::prepare(&cli(&["--only", "all", "--skip", "ip.media"]), &all).unwrap();
    assert_eq!(planned_only_all.len(), 14);

    let planned_skip_all = app::prepare(&cli(&["--skip", "all"]), &all).unwrap();
    assert!(planned_skip_all.is_empty());

    assert!(
        Cli::try_parse_from(["lowendaff-bench", "--all", "--only", "cpu"]).is_err(),
        "--all must conflict with --only"
    );

    let ok_cf_inet = app::prepare(
        &cli(&[
            "--only",
            "network.cloudflare,network.inetspeed",
            "--no-cloudflare-udp",
            "--inetspeed-threads",
            "8",
            "--inetspeed-endpoint",
            "198.51.100.25",
        ]),
        &all,
    )
    .expect("valid cloudflare and inetspeed flags");
    assert_eq!(ok_cf_inet.len(), 2);

    let bad_ep = app::prepare(
        &cli(&[
            "--only",
            "network.inetspeed",
            "--inetspeed-endpoint",
            "not-an-ip",
        ]),
        &all,
    );
    assert!(bad_ep.is_err(), "non-IP --inetspeed-endpoint must fail");

    for non_public_ep in ["127.0.0.1", "10.0.0.1", "169.254.169.254", "::1", "fe80::1"] {
        let res = app::prepare(
            &cli(&[
                "--only",
                "network.inetspeed",
                "--inetspeed-endpoint",
                non_public_ep,
            ]),
            &all,
        );
        assert!(
            res.is_err(),
            "non-public --inetspeed-endpoint {non_public_ep} must fail"
        );
    }

    // Unified --taier selectors and --taier-list parsing & formatting.
    let ok_taier = app::prepare(
        &cli(&[
            "--only",
            "network.taier",
            "--taier",
            "beijing:cu,guangdong/shenzhen:ct+cm,jiangsu",
            "--taier-operators",
            "telecom",
        ]),
        &all,
    )
    .expect("valid --taier selectors");
    assert_eq!(ok_taier.len(), 1);

    let bad_taier = app::prepare(
        &cli(&["--only", "network.taier", "--taier", "beijing:invalidop"]),
        &all,
    );
    assert!(bad_taier.is_err(), "invalid --taier carrier must fail");

    let list_all = cli(&["--taier-list", "--github-proxy", "https://ghfast.top/"]);
    assert_eq!(list_all.taier_list.as_deref(), Some(""));
    assert_eq!(
        list_all.github_proxy.as_deref(),
        Some("https://ghfast.top/")
    );
    let list_gd = cli(&["--taier-list", "guangdong"]);
    assert_eq!(list_gd.taier_list.as_deref(), Some("guangdong"));
    let list_eq = cli(&["--taier-list=beijing"]);
    assert_eq!(list_eq.taier_list.as_deref(), Some("beijing"));

    let sample_taier = plugin_network::taier::nodes::parse(
        br#"{
            "version": 1,
            "locations": [
                {
                    "id": "beijing",
                    "province": "Beijing",
                    "city": "Beijing",
                    "default": true,
                    "nodes": [
                        {"operator": "telecom", "ip": "198.51.100.1", "port": 65499},
                        {"operator": "cernet", "ip": "198.51.100.2", "port": 65499}
                    ]
                },
                {
                    "id": "guangdong-shenzhen",
                    "province": "Guangdong",
                    "city": "Shenzhen",
                    "nodes": [
                        {"operator": "mobile", "ip": "198.51.100.11", "port": 9443},
                        {"operator": "mobile", "ip": "198.51.100.12", "port": 9443}
                    ]
                }
            ]
        }"#,
    )
    .expect("sample taier list");
    let mut overview_buf = Vec::new();
    app::format_taier_list(&sample_taier, "", &mut overview_buf).expect("overview");
    let overview_str = String::from_utf8(overview_buf).unwrap();
    assert!(overview_str.contains("Beijing") && overview_str.contains("municipality"));
    assert!(overview_str.contains("guangdong/shenzhen:cm"));

    let mut gd_buf = Vec::new();
    app::format_taier_list(&sample_taier, "guangdong", &mut gd_buf).expect("guangdong");
    let gd_str = String::from_utf8(gd_buf).unwrap();
    assert!(gd_str.contains("guangdong/shenzhen") && gd_str.contains("cm (2)"));
    assert!(app::format_taier_list(&sample_taier, "atlantis", &mut Vec::new()).is_err());
}

#[tokio::test]
async fn default_report_archive_formats_utc_timestamps_and_writes_unwrapped_copy() {
    use bench::report_path;

    assert_eq!(report_path::format_utc_timestamp(0), "19700101-000000");
    assert_eq!(
        report_path::format_utc_timestamp(1_709_251_199),
        "20240229-235959"
    );
    let tmp = tempfile::tempdir().unwrap();
    let archive_path = report_path::path_in(tmp.path(), 1_795_204_800, Format::Markdown);
    assert!(
        archive_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("lowendaff-bench-") && n.ends_with(".md"))
    );

    let work = tempfile::tempdir().unwrap();
    let mut stdout_buf = Vec::new();
    let (log, capturing) = Captured::start(false);
    let report = ReportOptions {
        out: &mut stdout_buf,
        format: Format::Markdown,
        width: Some(40),
        show_ip: false,
    };
    let downloads = Downloads::default();
    let code = app::execute_with_copy(
        Vec::new(),
        HostProfile::default(),
        work.path(),
        &CancellationToken::new(),
        |family, url, range| downloads.open(family, url, range),
        1,
        report,
        Some(&archive_path),
    )
    .await
    .unwrap();
    drop(capturing);

    assert_eq!(code, 0);
    let stdout_text = String::from_utf8(stdout_buf).unwrap();
    let saved_text = std::fs::read_to_string(&archive_path).unwrap();
    assert_eq!(saved_text, stdout_text);
    assert!(saved_text.starts_with("# lowendaff-bench"));

    let mount_point = "/srv/a long mount point name that wraps on a narrow terminal";
    let wide_host = HostProfile {
        storage: Read::Done(StorageFacts {
            filesystems: vec![Filesystem {
                mount_point: mount_point.to_owned(),
                fs_type: "ext4".to_owned(),
                devices: Vec::new(),
                zfs_pool: None,
                disks: Vec::new(),
                space: None,
            }],
            ..StorageFacts::default()
        }),
        ..HostProfile::default()
    };
    let text_archive = report_path::path_in(tmp.path(), 1_795_204_801, Format::Text);
    let mut wrapped_stdout = Vec::new();
    let text_code = app::execute_with_copy(
        Vec::new(),
        wide_host,
        work.path(),
        &CancellationToken::new(),
        |family, url, range| downloads.open(family, url, range),
        1,
        ReportOptions {
            out: &mut wrapped_stdout,
            format: Format::Text,
            width: Some(40),
            show_ip: false,
        },
        Some(&text_archive),
    )
    .await
    .unwrap();
    assert_eq!(text_code, 0);
    let wrapped_out_str = String::from_utf8(wrapped_stdout).unwrap();
    let unwrapped_saved_str = std::fs::read_to_string(&text_archive).unwrap();
    assert!(
        !wrapped_out_str
            .lines()
            .any(|line| line.contains(mount_point)),
        "terminal stdout at width=40 must wrap long mount point: {wrapped_out_str}"
    );
    assert!(
        unwrapped_saved_str
            .lines()
            .any(|line| line.contains(mount_point)),
        "archived text copy must remain unwrapped: {unwrapped_saved_str}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(&archive_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "archive report must be owner-only (0600)");

        let collided = report_path::save_archive(&archive_path, "second run\n")
            .expect("same-second collision allocates -1 suffix");
        let stem = archive_path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(
            collided.file_name().and_then(|n| n.to_str()),
            Some(format!("{stem}-1.md").as_str())
        );
        assert_eq!(std::fs::read_to_string(&collided).unwrap(), "second run\n");

        let explicit_copy = tmp.path().join("explicit-export.md");
        let written_explicit = report_path::save_user_report(&explicit_copy, "explicit\n").unwrap();
        assert_eq!(written_explicit, explicit_copy);
        assert_eq!(
            std::fs::read_to_string(&explicit_copy).unwrap(),
            "explicit\n"
        );

        let link_target = tmp.path().join("victim.txt");
        std::fs::write(&link_target, "keep\n").unwrap();
        let symlink_path = tmp.path().join("symlink-report.md");
        std::os::unix::fs::symlink(&link_target, &symlink_path).unwrap();
        assert!(
            app::create_output(&symlink_path).is_err(),
            "create_output must refuse symlink targets with O_NOFOLLOW"
        );
        assert_eq!(std::fs::read_to_string(&link_target).unwrap(), "keep\n");

        let fifo_path = tmp.path().join("fifo-report.md");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo_path)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            app::create_output(&fifo_path).is_err(),
            "create_output must fail immediately on unread FIFOs without blocking"
        );

        let lax_file = tmp.path().join("lax-report.md");
        std::fs::write(&lax_file, "old contents\n").unwrap();
        std::fs::set_permissions(&lax_file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let reopened = app::create_output(&lax_file).unwrap();
        assert_eq!(reopened.metadata().unwrap().len(), 0);
        assert_eq!(
            reopened.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );

        let shared_dir = tmp.path().join("world-writable");
        std::fs::create_dir(&shared_dir).unwrap();
        std::fs::set_permissions(&shared_dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let hardlink_path = shared_dir.join("hardlinked.md");
        std::fs::hard_link(&link_target, &hardlink_path).unwrap();
        assert!(
            app::create_output(&hardlink_path).is_err(),
            "create_output must refuse hardlinked files in world-writable directories"
        );
        assert_eq!(std::fs::read_to_string(&link_target).unwrap(), "keep\n");

        assert!(
            app::create_output(std::path::Path::new("/dev/null")).is_ok(),
            "create_output must still permit /dev/null"
        );
    }
    assert!(
        log.lines()
            .iter()
            .any(|line| line.contains("Saved report to")),
        "progress log should confirm archive path: {:?}",
        log.lines()
    );
}

#[tokio::test]
async fn vm_cpu_sampler_collects_only_on_vms_and_ignores_dedicated_and_containers() {
    use bench::vm_cpu::VmCpuSampler;
    use bench_host::Root;
    use bench_host::platform::ContainerKind;
    use std::time::Instant;

    let dir = tempfile::tempdir().expect("temp root");
    std::fs::create_dir_all(dir.path().join("proc")).expect("proc");
    let stat_path = dir.path().join("proc/stat");
    std::fs::write(&stat_path, "cpu  100 0 50 800 0 0 10 40 0 0\n").expect("stat 0");

    for non_vm in [
        Platform::Dedicated,
        Platform::Container(ContainerKind::Docker),
    ] {
        let sampler = VmCpuSampler::start_with_interval(
            non_vm,
            Root::new(dir.path()),
            Instant::now(),
            Duration::from_millis(20),
        );
        tokio::time::sleep(Duration::from_millis(35)).await;
        assert!(sampler.finish().await.is_empty());
    }

    let sampler = VmCpuSampler::start_with_interval(
        Platform::Vm(Hypervisor::Kvm),
        Root::new(dir.path()),
        Instant::now(),
        Duration::from_millis(25),
    );
    sampler.set_active_item(Some("cpu.bench"));
    std::fs::write(&stat_path, "cpu  300 0 100 1450 0 0 30 120 0 0\n").expect("stat 1");
    tokio::time::sleep(Duration::from_millis(40)).await;
    let samples = sampler.finish().await;
    assert!(!samples.is_empty(), "VM must record /proc/stat samples");
    assert_eq!(samples[0].item, Some("cpu.bench"));
    assert!((samples[0].si_pct - 2.0).abs() < 1e-6);
    assert!((samples[0].st_pct - 8.0).abs() < 1e-6);
}
