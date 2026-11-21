//! Command-line flow: parse, select, configure, provision, run, summarize.
//! `main` adds the root check, the runtime and Ctrl-C handling around it.

use std::collections::BTreeMap;
use std::fs;
use std::fs::Permissions;
use std::future::Future;
use std::io::Write;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::IpFamily;
use bench_api::Plugin;
use bench_host::profile::HostProfile;
use plugin_cpu::bench::CpuBench;
use plugin_ip::media::Media;
use plugin_ip::quality::Quality;
use plugin_network::bgp::Bgp;
use plugin_network::iperf3::Iperf3;
use plugin_network::taier::Taier;
use plugin_network::trace::Trace;
use tokio::io::AsyncRead;

use crate::bgp_upload;
use crate::fetcher::DownloadProxy;
use crate::fetcher::GithubProxy;
use crate::fetcher::Socks5Proxy;
use crate::locale;
use crate::netprobe::NetProbers;
use crate::orchestrator;
use crate::orchestrator::Observer;
use crate::orchestrator::Planned;
use crate::orchestrator::Run;
use crate::orchestrator::Settings;
use crate::prober::Probers;
use crate::progress::Log;
use crate::provision;
use crate::provision::DownloadError;
use crate::provision::Part;
use crate::registry::Entry;
use crate::report;
use crate::report::Format;
use crate::report::terminal::Page;
use crate::results::Measured;

/// Exit code for usage, option or environment errors, and for not running as root.
pub const EXIT_USAGE: u8 = 2;
/// Exit code after Ctrl-C; the results so far were printed.
pub const EXIT_INTERRUPTED: u8 = 130;

const GRACE: Duration = Duration::from_secs(5);

/// The connections that one tool's download uses at most, unless `--download-connections` says
/// otherwise.
pub const DOWNLOAD_CONNECTIONS: u16 = 32;

/// Sub-suite selection for `cpu.bench` (`--cpu-bench`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CpuSuite {
    /// sysbench prime-sieve CPU benchmark (1-thread & N-thread).
    Sysbench,
    /// 7-Zip LZMA2 compression & decompression MIPS (1-thread & N-thread).
    #[value(name = "7zip")]
    SevenZip,
    /// Byte-UnixBench 6.0.1 12-item System Index (1-copy & N-copy).
    Unixbench,
    /// High-Performance Linpack (OpenBLAS) dense FP64 LU GFLOPS.
    Hpl,
    /// PassMark PerformanceTest Linux CPU Mark & sub-tests.
    Passmark,
    /// y-cruncher multi-threaded Pi computation (`x86_64` only).
    Ycruncher,
    /// Prime95 (`mprime`) Small-FFT torture stability test (`x86_64` only).
    Prime95,
    /// Geekbench 7 single-core & multi-core CPU benchmark.
    Geekbench,
    /// Every CPU sub-suite (auto-skipping `x86_64`-only suites on `aarch64`).
    All,
}

impl CpuSuite {
    const fn code(self) -> &'static str {
        match self {
            Self::Sysbench => "sysbench",
            Self::SevenZip => "7zip",
            Self::Unixbench => "unixbench",
            Self::Hpl => "hpl",
            Self::Passmark => "passmark",
            Self::Ycruncher => "ycruncher",
            Self::Prime95 => "prime95",
            Self::Geekbench => "geekbench",
            Self::All => "all",
        }
    }
}

/// Carrier selection for `network.taier` (`--taier-operators`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TaierOperator {
    /// China Telecom (`AS4134` / `AS4809`).
    Telecom,
    /// China Unicom (`AS4837` / `AS9929`).
    Unicom,
    /// China Mobile (`AS9808` / `AS58807`).
    Mobile,
    /// China Education and Research Network (`CERNET`).
    Cernet,
    /// China Broadnet (`CBN`).
    Cbn,
    /// Overseas or unlisted carrier.
    Other,
    /// Every carrier in the selected locations.
    All,
}

impl TaierOperator {
    const fn code(self) -> &'static str {
        match self {
            Self::Telecom => "telecom",
            Self::Unicom => "unicom",
            Self::Mobile => "mobile",
            Self::Cernet => "cernet",
            Self::Cbn => "cbn",
            Self::Other => "other",
            Self::All => "all",
        }
    }
}

/// Target group selection for `network.trace` (`--trace-groups`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TraceGroup {
    /// Nearest YouTube and Netflix cache servers.
    Services,
    /// Carrier targets in mainland China.
    China,
    /// International carrier and cloud targets.
    International,
    /// Both cache servers and every target in the list.
    All,
}

impl TraceGroup {
    const fn code(self) -> &'static str {
        match self {
            Self::Services => "services",
            Self::China => "china",
            Self::International => "international",
            Self::All => "all",
        }
    }
}

/// Regional group selection for `ip.media` (`--media-groups`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MediaGroup {
    /// Multinational streaming platforms.
    Global,
    /// Asian streaming platforms (HK/MO/TW, JP, KR, SEA, IN).
    Asia,
    /// European streaming platforms.
    Europe,
    /// North and Latin American streaming platforms.
    Americas,
    /// Australian and New Zealand streaming platforms.
    Oceania,
    /// Every regional group.
    All,
}

impl MediaGroup {
    const fn code(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Asia => "asia",
            Self::Europe => "europe",
            Self::Americas => "americas",
            Self::Oceania => "oceania",
            Self::All => "all",
        }
    }
}

// The help's words come from the catalog, as every other text that the program shows does.
#[derive(Debug, clap::Parser)]
#[command(name = "lowendaff-bench", version, about = locale::words().about())]
pub struct Cli {
    // Runs every test item in non-interactive CLI mode.
    #[arg(short, long, conflicts_with = "only", help = locale::words().help_all())]
    pub all: bool,

    // The test items to run: ids, groups, or `all`.
    #[arg(long, value_delimiter = ',', help = locale::words().help_only())]
    pub only: Vec<String>,

    // The test items to leave out: ids or groups.
    #[arg(long, value_delimiter = ',', help = locale::words().help_skip())]
    pub skip: Vec<String>,

    // Where to write the report instead of stdout.
    #[arg(short, long, help = locale::words().help_output())]
    pub output: Option<PathBuf>,

    // Which format the report takes.
    #[arg(
        short,
        long,
        value_enum,
        default_value_t = Format::Text,
        help = locale::words().help_format()
    )]
    pub format: Format,

    // Where the run directory goes.
    #[arg(long, default_value = ".", help = locale::words().help_work_dir())]
    pub work_dir: PathBuf,

    // Lists the test items instead of running them.
    #[arg(long, help = locale::words().help_list())]
    pub list: bool,

    // Logs the debug events too.
    #[arg(short, long, help = locale::words().help_verbose())]
    pub verbose: bool,

    // Shows the public addresses unmasked in the report.
    #[arg(long, help = locale::words().help_show_ip())]
    pub show_ip: bool,

    // The most connections of one file's download. At most 256: each connection also holds the
    // file open, and twice 256 descriptors stay under the usual limit of 1024.
    #[arg(
        long,
        default_value_t = DOWNLOAD_CONNECTIONS,
        value_parser = clap::value_parser!(u16).range(1..=256),
        help = locale::words().help_download_connections()
    )]
    pub download_connections: u16,

    // Optional SOCKS5 proxy (`socks5://` or `socks5h://`) for tool and list downloads only.
    #[arg(long, help = locale::words().help_socks5_proxy())]
    pub socks5_proxy: Option<String>,

    // Optional GitHub URL prefix mirror (e.g. `https://ghfast.top/`) for GitHub downloads only.
    #[arg(long, help = locale::words().help_github_proxy())]
    pub github_proxy: Option<String>,

    // Sub-suites for cpu.bench.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = locale::words().help_cpu_bench()
    )]
    pub cpu_bench: Vec<CpuSuite>,

    // The countries whose servers the network speed test uses, instead of the list's defaults.
    #[arg(long, value_delimiter = ',', help = locale::words().help_countries())]
    pub countries: Vec<String>,

    // Cities, provinces, or location IDs for China carrier speed (network.taier).
    #[arg(long, value_delimiter = ',', help = locale::words().help_taier_cities())]
    pub taier_cities: Vec<String>,

    // Carriers for China carrier speed (network.taier).
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = locale::words().help_taier_operators()
    )]
    pub taier_operators: Vec<TaierOperator>,

    // Target groups for route tracing.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = locale::words().help_trace_groups()
    )]
    pub trace_groups: Vec<TraceGroup>,

    // Regional groups for media unlock.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = locale::words().help_media_groups()
    )]
    pub media_groups: Vec<MediaGroup>,

    // Skips DNSBL blacklist lookups in ip.quality.
    #[arg(long, help = locale::words().help_no_dnsbl())]
    pub no_dnsbl: bool,

    // Skips outbound TCP port-25 mail checks in ip.quality.
    #[arg(long, help = locale::words().help_no_mail())]
    pub no_mail: bool,

    // Skips uploading BGP topology SVG diagrams to imageupload.app in text mode.
    #[arg(long, help = locale::words().help_no_bgp_upload())]
    pub no_bgp_upload: bool,

    // Inline JSON overrides used by CI loopback smoke scripts (hidden; no local config files).
    #[arg(long = "internal-overrides", hide = true)]
    pub internal_overrides: Option<String>,
}

/// Whether `os_arg_count` (`std::env::args_os().len()`) selects the default interactive TUI.
#[must_use]
pub const fn should_enter_tui(os_arg_count: usize) -> bool {
    os_arg_count <= 1
}

impl Cli {
    /// Parses the process arguments; exits with usage help on errors.
    #[must_use]
    pub fn from_env() -> Self {
        use clap::Parser;

        Self::parse()
    }
}

/// Prints every registered plugin with its id, title, platforms and pinned tools, by group in
/// the report's order, under each group's title and the name that `--only` and `--skip` take.
pub fn list(registry: &[Entry], out: &mut impl Write) {
    let _ = writeln!(out, "{}", locale::words().item_count(registry.len()));
    let group = |entry: &Entry| orchestrator::group_of(entry.id);
    let groups: Vec<&str> = registry.iter().map(group).collect();
    for (name, title) in report::terminal::sections(&groups) {
        let _ = writeln!(out, "{}", locale::words().group_line(title, name));
        let members = registry.iter().filter(|entry| group(entry) == name);
        for entry in members {
            item(entry, out);
        }
    }
}

/// One plugin's line in [`list`].
fn item(entry: &Entry, out: &mut impl Write) {
    let platforms: Vec<String> = entry
        .requires
        .platforms
        .iter()
        .map(|kind| report::terminal::kind_text(*kind).to_owned())
        .collect();
    let tools: Vec<String> = entry
        .requires
        .tools
        .iter()
        .map(|tool| format!("{} {}", tool.name, tool.version))
        .collect();
    let name = entry.title.unwrap_or(entry.id);
    let line = locale::words().item_line(entry.id, name, &platforms, entry.requires.numa, &tools);
    let _ = writeln!(out, "{line}");
}

/// Validates `--socks5-proxy` and `--github-proxy`, collecting all errors together.
pub fn download_proxy(cli: &Cli) -> Result<DownloadProxy, Vec<String>> {
    let mut errors = Vec::new();
    let socks5 = match cli
        .socks5_proxy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match Socks5Proxy::parse(raw) {
            Ok(proxy) => Some(proxy),
            Err(err) => {
                errors.push(err);
                None
            }
        },
        None => None,
    };
    let github = match cli
        .github_proxy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match GithubProxy::parse(raw) {
            Ok(proxy) => Some(proxy),
            Err(err) => {
                errors.push(err);
                None
            }
        },
        None => None,
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(DownloadProxy { socks5, github })
}

/// Builds every selected plugin; all problems are reported together and nothing runs.
pub fn prepare(cli: &Cli, registry: &[Entry]) -> Result<Vec<Planned<Measured>>, Vec<String>> {
    let mut errors = Vec::new();
    if let Err(proxy_errs) = download_proxy(cli) {
        errors.extend(proxy_errs);
    }
    let mut settings = match parse_overrides(cli.internal_overrides.as_deref()) {
        Ok(settings) => settings,
        Err(err) => {
            errors.push(err);
            return Err(errors);
        }
    };
    if !cli.cpu_bench.is_empty() {
        cpu_bench(&mut settings, &cli.cpu_bench);
    }
    if !cli.countries.is_empty() {
        countries(&mut settings, &cli.countries);
    }
    if !cli.taier_cities.is_empty() || !cli.taier_operators.is_empty() {
        taier_settings(&mut settings, &cli.taier_cities, &cli.taier_operators);
    }
    if !cli.trace_groups.is_empty() {
        trace_groups(&mut settings, &cli.trace_groups);
    }
    if !cli.media_groups.is_empty() {
        media_groups(&mut settings, &cli.media_groups);
    }
    if cli.no_dnsbl || cli.no_mail {
        ip_quality(&mut settings, !cli.no_dnsbl, !cli.no_mail);
    }
    if cli.no_bgp_upload {
        bgp_settings(&mut settings, false);
    }
    let ids = match select(registry, &cli.only, &cli.skip) {
        Ok(ids) => ids,
        Err(sel_errs) => {
            errors.extend(sel_errs);
            return Err(errors);
        }
    };
    match orchestrator::plan(registry, &ids, &settings) {
        Ok(planned) if errors.is_empty() => Ok(planned),
        Ok(_) => Err(errors),
        Err(plan_errs) => {
            errors.extend(plan_errs);
            Err(errors)
        }
    }
}

/// Registry order, narrowed by `--only` and `--skip`, which take ids, group names, or `all`;
/// unknown names are errors.
pub fn select(
    registry: &[Entry],
    only: &[String],
    skip: &[String],
) -> Result<Vec<&'static str>, Vec<String>> {
    // A name is `all`, a whole id, or a whole group, never a part of one.
    let names =
        |name: &str, id: &str| name == "all" || name == id || name == orchestrator::group_of(id);
    let known = |name: &String| registry.iter().any(|entry| names(name, entry.id));
    let errors: Vec<String> = only
        .iter()
        .chain(skip)
        .filter(|name| !known(name))
        .map(|name| locale::words().unknown_selection(name))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(registry
        .iter()
        .map(|entry| entry.id)
        .filter(|id| only.is_empty() || only.iter().any(|wanted| names(wanted, id)))
        .filter(|id| !skip.iter().any(|skipped| names(skipped, id)))
        .collect())
}

/// Sets `--cpu-bench` in `cpu.bench`'s table.
pub fn cpu_bench(settings: &mut BTreeMap<String, serde_json::Value>, suites: &[CpuSuite]) {
    let table = plugin_table(settings, CpuBench::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        let values = suites
            .iter()
            .map(|suite| serde_json::Value::String(suite.code().to_owned()))
            .collect();
        table.insert("suites".to_owned(), serde_json::Value::Array(values));
    }
}

/// Sets `--countries` in the network speed test's table, over any inline override.
/// A table that is not a JSON object is left as it is, for the plan to refuse.
pub fn countries(settings: &mut BTreeMap<String, serde_json::Value>, countries: &[String]) {
    let table = plugin_table(settings, Iperf3::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        let countries = countries
            .iter()
            .map(|country| serde_json::Value::String(country.clone()))
            .collect();
        table.insert("countries".to_owned(), serde_json::Value::Array(countries));
    }
}

/// Sets `--taier-cities` and `--taier-operators` in `network.taier`'s table.
pub fn taier_settings(
    settings: &mut BTreeMap<String, serde_json::Value>,
    cities: &[String],
    operators: &[TaierOperator],
) {
    let table = plugin_table(settings, Taier::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        if !cities.is_empty() {
            let values = cities
                .iter()
                .map(|city| serde_json::Value::String(city.clone()))
                .collect();
            table.insert("cities".to_owned(), serde_json::Value::Array(values));
        }
        if !operators.is_empty() {
            let values = operators
                .iter()
                .map(|op| serde_json::Value::String(op.code().to_owned()))
                .collect();
            table.insert("operators".to_owned(), serde_json::Value::Array(values));
        }
    }
}

/// Sets `--trace-groups` in `network.trace`'s table.
pub fn trace_groups(settings: &mut BTreeMap<String, serde_json::Value>, groups: &[TraceGroup]) {
    let table = plugin_table(settings, Trace::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        let values = groups
            .iter()
            .map(|group| serde_json::Value::String(group.code().to_owned()))
            .collect();
        table.insert("groups".to_owned(), serde_json::Value::Array(values));
    }
}

/// Sets `--media-groups` in `ip.media`'s table.
pub fn media_groups(settings: &mut BTreeMap<String, serde_json::Value>, groups: &[MediaGroup]) {
    let table = plugin_table(settings, Media::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        let values = groups
            .iter()
            .map(|group| serde_json::Value::String(group.code().to_owned()))
            .collect();
        table.insert("groups".to_owned(), serde_json::Value::Array(values));
    }
}

/// Sets `--no-dnsbl` and `--no-mail` in `ip.quality`'s table.
pub fn ip_quality(settings: &mut BTreeMap<String, serde_json::Value>, dnsbl: bool, mail: bool) {
    let table = plugin_table(settings, Quality::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        if !dnsbl {
            table.insert("dnsbl".to_owned(), serde_json::Value::Bool(false));
        }
        if !mail {
            table.insert("mail".to_owned(), serde_json::Value::Bool(false));
        }
    }
}

/// Sets `--no-bgp-upload` in `network.bgp`'s table.
pub fn bgp_settings(settings: &mut BTreeMap<String, serde_json::Value>, upload: bool) {
    let table = plugin_table(settings, Bgp::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        table.insert("upload".to_owned(), serde_json::Value::Bool(upload));
    }
}

fn plugin_table<'a>(
    settings: &'a mut BTreeMap<String, serde_json::Value>,
    id: &str,
) -> &'a mut serde_json::Value {
    settings
        .entry(id.to_owned())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
}

/// Parses inline JSON plugin overrides (`{"<plugin-id>": { ... }}`).
pub fn parse_overrides(raw: Option<&str>) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let Some(raw) = raw else {
        return Ok(BTreeMap::new());
    };
    serde_json::from_str(raw).map_err(|err| locale::words().overrides_invalid(&err))
}

/// Opens `--output` before the run starts, so an unwritable path fails at once.
pub fn create_output(path: &Path) -> Result<fs::File, String> {
    fs::File::create(path).map_err(|err| locale::words().output_unwritable(path, &err))
}

/// How the report is written.
pub struct ReportOptions<W> {
    /// Where the report goes.
    pub out: W,
    /// Which format the report takes.
    pub format: Format,
    /// The terminal's width, to which a text report's tables wrap; `None` when they must not wrap,
    /// as in a file or in Markdown or JSON.
    pub width: Option<u16>,
    /// Whether the public addresses show in full.
    pub show_ip: bool,
}

/// Everything produced by a benchmark run before the report is formatted.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The host profile collected before the run.
    pub host: Arc<HostProfile>,
    /// Titles of plugins that do not apply to this host.
    pub not_applicable: Vec<&'static str>,
    /// Every applicable plugin's run result, in run order.
    pub runs: Vec<Run<Measured>>,
    /// Total elapsed wall time of the run.
    pub total: Duration,
    /// Process exit code (`0`, or `EXIT_INTERRUPTED` if cancelled).
    pub exit_code: u8,
}

impl Outcome {
    /// Renders this outcome in `format` (`width` wraps tables in [`Format::Text`] only).
    #[must_use]
    pub fn render(&self, format: Format, width: Option<u16>, show_ip: bool) -> String {
        let page = Page {
            version: env!("CARGO_PKG_VERSION"),
            host: &self.host,
            show_ip,
            not_applicable: &self.not_applicable,
            runs: &self.runs,
            total: self.total,
        };
        report::render(&page, format, width)
    }
}

/// Runs a prepared plan on the host that `host` describes, inside a fresh run directory under
/// `work_dir`, which is removed afterwards, with the tools that `fetch` downloads from their
/// pinned addresses, over up to `connections` connections a file, and the files that the plugins
/// ask for, which it downloads too, each over the family it names (`fetch`'s first argument;
/// `None` for either), and writes the report as `report` says. Returns the process exit code.
pub async fn execute<F, Fut, R, W>(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    work_dir: &Path,
    cancel: &CancellationToken,
    fetch: F,
    connections: usize,
    report: ReportOptions<W>,
) -> Result<u8, String>
where
    F: Fn(Option<IpFamily>, &'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
    W: Write,
{
    execute_with_copy(
        planned,
        host,
        work_dir,
        cancel,
        fetch,
        connections,
        report,
        None,
    )
    .await
}

/// Like [`execute`], and optionally saves an unwrapped archive copy of the report to `copy_path`.
#[allow(clippy::too_many_arguments)]
pub async fn execute_with_copy<F, Fut, R, W>(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    work_dir: &Path,
    cancel: &CancellationToken,
    fetch: F,
    connections: usize,
    report: ReportOptions<W>,
    copy_path: Option<&Path>,
) -> Result<u8, String>
where
    F: Fn(Option<IpFamily>, &'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
    W: Write,
{
    let mut outcome = execute_with_observer(
        planned,
        host,
        work_dir,
        cancel,
        fetch,
        connections,
        &mut Log,
    )
    .await?;
    let ReportOptions {
        mut out,
        format,
        width,
        show_ip,
    } = report;
    if format == Format::Text {
        let upload_started = Instant::now();
        bgp_upload::populate_runs(&mut outcome.runs, cancel).await;
        outcome.total += upload_started.elapsed();
        if cancel.is_cancelled() {
            outcome.exit_code = EXIT_INTERRUPTED;
        }
    }
    let rendered = outcome.render(format, width, show_ip);
    // Output is best effort: a closed stdout must not change the exit code.
    let _ = out.write_all(rendered.as_bytes());
    let _ = out.flush();
    if let Some(path) = copy_path {
        let archive_text = if width.is_none() {
            rendered
        } else {
            outcome.render(format, None, show_ip)
        };
        match crate::report_path::save_copy(path, &archive_text) {
            Ok(()) => tracing::info!("{}", locale::words().tui_saved_to(path)),
            Err(err) => tracing::warn!("{err}"),
        }
    }
    Ok(outcome.exit_code)
}

/// Runs a prepared plan with a custom [`Observer`] and returns the raw [`Outcome`] without writing
/// the report, so both CLI output and the interactive TUI viewer share the same runner.
pub async fn execute_with_observer<F, Fut, R>(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    work_dir: &Path,
    cancel: &CancellationToken,
    fetch: F,
    connections: usize,
    observer: &mut dyn Observer<Measured>,
) -> Result<Outcome, String>
where
    F: Fn(Option<IpFamily>, &'static str, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    use std::os::unix::fs::PermissionsExt;

    let started = Instant::now();
    // Absolute and without `.` components: tempfile joins a relative directory to the current
    // one as it is, which left `/./` in every path under it, such as fio's test file.
    let work_dir = std::path::absolute(work_dir)
        .map_err(|err| locale::words().run_dir_failed(work_dir, &err))?;
    let run_dir = tempfile::Builder::new()
        .prefix("lowendaff-bench-")
        .tempdir_in(&work_dir)
        .map_err(|err| locale::words().run_dir_failed(&work_dir, &err))?;
    let (planned, not_applicable) = orchestrator::for_host(planned, &host);

    // Only the tools of items this host can run, so that nothing is downloaded in vain, such
    // as Geekbench on a host without glibc or IPv4.
    let runnable = || {
        planned.iter().filter(|planned| {
            orchestrator::check_host(&planned.host_requirements(&host), &host).is_ok()
        })
    };
    let tools = provision::required_tools_from_specs(
        runnable().flat_map(|planned| planned.tools_for(&host)),
    );
    let mut ready = BTreeMap::new();
    let mut failed_tools = BTreeMap::new();
    if !tools.is_empty() {
        let tools_dir = run_dir.path().join("tools");
        fs::create_dir(&tools_dir).map_err(|err| locale::words().tools_dir_failed(&err))?;
        // A tool that runs as another user starts through this directory, which that user must
        // be allowed to search, whatever the umask; it may not list it, and the run directory
        // above stays closed to others.
        fs::set_permissions(&tools_dir, Permissions::from_mode(0o711))
            .map_err(|err| locale::words().tools_dir_failed(&err))?;
        // A tool comes over either family.
        let any = |url, range| fetch(None, url, range);
        let provisioned =
            provision::provision_all(&tools, &host.arch, &tools_dir, connections, cancel, &any)
                .await;
        ready = provisioned.ready;
        failed_tools = provisioned.failed;
    }
    // Each plugin's files go into a directory of its own: two plugins may name theirs alike.
    let families: Vec<IpFamily> = [
        (IpFamily::Ipv4, host.network.ipv4.reachable()),
        (IpFamily::Ipv6, host.network.ipv6.reachable()),
    ]
    .into_iter()
    .filter_map(|(family, reachable)| reachable.then_some(family))
    .collect();
    let mut files = BTreeMap::new();
    for planned in runnable().filter(|planned| !planned.files().is_empty()) {
        let dir = run_dir.path().join("files").join(planned.id());
        let fetched =
            provision::fetch_files(planned.files(), &dir, &families, cancel, &fetch).await;
        files.insert(planned.id(), fetched);
    }

    let probers = Arc::new(Probers::try_new()?);
    let net_probers = Arc::new(NetProbers::new());
    let host = Arc::new(host);
    let settings = Settings {
        grace: GRACE,
        work_root: run_dir.path().to_path_buf(),
        origin: tokio::time::Instant::from_std(started),
        tools: Arc::new(ready),
        failed_tools,
        files,
        http: Some(probers),
        net: Some(net_probers),
        host: Arc::clone(&host),
    };
    let runs = orchestrator::run_plan(planned, cancel, &settings, observer).await;
    let total = started.elapsed();
    drop(run_dir);
    let exit_code = if cancel.is_cancelled() {
        EXIT_INTERRUPTED
    } else {
        0
    };
    Ok(Outcome {
        host,
        not_applicable,
        runs,
        total,
        exit_code,
    })
}
