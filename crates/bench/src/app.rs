//! Command-line flow: parse, select, configure, provision, run, summarize.
//! `main` adds the root check, the runtime and Ctrl-C handling around it.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::fs::Permissions;
use std::future::Future;
use std::io;
use std::io::Write;
use std::net::IpAddr;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::IpFamily;
use bench_api::Plugin;
use bench_host::Root;
use bench_host::profile::HostProfile;
use plugin_cpu::bench::CpuBench;
use plugin_ip::media::Media;
use plugin_ip::quality::Quality;
use plugin_network::bgp::Bgp;
use plugin_network::cloudflare::Cloudflare;
use plugin_network::inetspeed::Inetspeed;
use plugin_network::iperf3::Iperf3;
use plugin_network::taier;
use plugin_network::taier::Selector as TaierSelector;
use plugin_network::taier::SelectorError as TaierSelectorError;
use plugin_network::taier::Taier;
use plugin_network::taier::nodes::Location as TaierLocation;
use plugin_network::tcping::Tcping;
use plugin_network::trace::Trace;
use tokio::io::AsyncRead;
use tokio::time::Instant as TokioInstant;

use crate::bgp_upload;
use crate::fetcher::DownloadProxy;
use crate::fetcher::Fetchers;
use crate::fetcher::GithubProxy;
use crate::fetcher::Socks5Proxy;
use crate::locale;
use crate::locale::TaierListProblem;
use crate::locale::TaierRule;
use crate::locale::TaierSelectorProblem;
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
use crate::tui;
use crate::tui::taier_picker::CityEntry;
use crate::tui::taier_picker::RegionEntry;
use crate::tui::taier_picker::RegionKind;
use crate::vm_cpu::SamplingObserver;
use crate::vm_cpu::VmCpuSample;
use crate::vm_cpu::VmCpuSampler;

/// Exit code for usage, option or environment errors, and for not running as root.
pub const EXIT_USAGE: u8 = 2;
/// Exit code after Ctrl-C; the results so far were printed.
pub const EXIT_INTERRUPTED: u8 = 130;

pub(crate) const GRACE: Duration = Duration::from_secs(5);

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

/// Target group selection for `network.tcping` (`--tcping-groups`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TcpingGroup {
    /// Mainland China provincial standard carrier routes.
    China,
    /// Mainland China premium international backbones (`CN2`, `9929`, `CMIN2`).
    #[value(name = "china-premium", alias = "china_premium")]
    ChinaPremium,
    /// Global single-region Internet hubs.
    Global,
    /// Every target group.
    All,
}

impl TcpingGroup {
    const fn code(self) -> &'static str {
        match self {
            Self::China => "china",
            Self::ChinaPremium => "china-premium",
            Self::Global => "global",
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

/// Command-line flags; user-facing help text comes from [`locale::words`] so `clap` does not
/// synthesize `long_help` from field doc comments.
#[allow(missing_docs)]
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

    // Interface and text/markdown report language (`en` by default; JSON always stays English).
    #[arg(long, value_enum, help = locale::words().help_lang())]
    pub lang: Option<locale::Language>,

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

    // Unified <place>[:<op>[+<op>...]] target selectors for China carrier speed (network.taier).
    #[arg(long, value_delimiter = ',', help = locale::words().help_taier())]
    pub taier: Vec<String>,

    // Lists available network.taier regions (or cities in PROVINCE) and exits without root.
    #[arg(
        long,
        value_name = "PROVINCE",
        num_args = 0..=1,
        default_missing_value = "",
        help = locale::words().help_taier_list()
    )]
    pub taier_list: Option<String>,

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

    // Skips the RFC 5389 STUN UDP packet-loss probe in network.cloudflare.
    #[arg(long, help = locale::words().help_no_cloudflare_udp())]
    pub no_cloudflare_udp: bool,

    // Concurrent worker threads for multi-thread rounds in network.inetspeed.
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(1..=64),
        help = locale::words().help_inetspeed_threads()
    )]
    pub inetspeed_threads: Option<u8>,

    // Optional pinned Apple CDN endpoint IP for network.inetspeed.
    #[arg(long, help = locale::words().help_inetspeed_endpoint())]
    pub inetspeed_endpoint: Option<String>,

    // Target groups for TCP handshake latency (network.tcping).
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = locale::words().help_tcping_groups()
    )]
    pub tcping_groups: Vec<TcpingGroup>,

    // Expands network.tcping to all 31 mainland China provinces and secondary global hubs.
    #[arg(long, help = locale::words().help_tcping_all())]
    pub tcping_all: bool,

    // TCP handshake probes sent per target in network.tcping (5..=50).
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(5..=50),
        help = locale::words().help_tcping_count()
    )]
    pub tcping_count: Option<u8>,

    // Target groups for route tracing.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        help = locale::words().help_trace_groups()
    )]
    pub trace_groups: Vec<TraceGroup>,

    // Skips TCP SYN large-packet route traces to mainland China targets in network.trace.
    #[arg(long, help = locale::words().help_no_trace_tcp())]
    pub no_trace_tcp: bool,

    // TCP probe packet size in bytes (64..=1500) for mainland China targets in network.trace.
    #[arg(
        long,
        value_parser = clap::value_parser!(u16).range(64..=1500),
        help = locale::words().help_trace_packet_size()
    )]
    pub trace_packet_size: Option<u16>,

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

/// Pre-scans raw process arguments for `--lang <VAL>` or `--lang=<VAL>` before `clap` runs, so
/// `--lang zh-cn --help` and `clap` usage errors render in the requested language immediately.
#[must_use]
pub fn preparse_lang<I, S>(args: I) -> Option<locale::Language>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut iter = args.into_iter().skip(1);
    let mut found = None;
    while let Some(arg) = iter.next() {
        let Some(text) = arg.as_ref().to_str() else {
            continue;
        };
        if text == "--" {
            break;
        }
        if text == "--lang" {
            let Some(next) = iter.next() else {
                break;
            };
            if let Some(val) = next.as_ref().to_str() {
                if val == "--" {
                    break;
                }
                if let Some(lang) = locale::parse_language(val) {
                    found = Some(lang);
                }
            }
        } else if let Some(val) = text.strip_prefix("--lang=")
            && let Some(lang) = locale::parse_language(val)
        {
            found = Some(lang);
        }
    }
    found
}

/// Whether `args` (starting with `argv[0]`) should launch the interactive TUI: `true` when given
/// no flags or only `--lang <VAL>` / `--lang=<VAL>`.
#[must_use]
pub fn should_enter_tui_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut iter = args.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        let Some(text) = arg.as_ref().to_str() else {
            return false;
        };
        if text == "--lang" {
            let Some(next) = iter.next() else {
                return false;
            };
            let Some(val) = next.as_ref().to_str() else {
                return false;
            };
            if val.is_empty() || val == "--" {
                return false;
            }
        } else if let Some(val) = text.strip_prefix("--lang=") {
            if val.is_empty() {
                return false;
            }
        } else {
            return false;
        }
    }
    true
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
    let name = locale::words()
        .item_name(entry.id)
        .or(entry.title)
        .unwrap_or(entry.id);
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
        Some(raw) => match Socks5Proxy::try_new(raw) {
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
        Some(raw) => match GithubProxy::try_new(raw) {
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
    prepare_with_proxy(cli, registry).map(|(planned, _)| planned)
}

/// Builds every selected plugin and validates download proxy flags in a single pass.
pub fn prepare_with_proxy(
    cli: &Cli,
    registry: &[Entry],
) -> Result<(Vec<Planned<Measured>>, DownloadProxy), Vec<String>> {
    let mut errors = Vec::new();
    let proxy = match download_proxy(cli) {
        Ok(proxy) => Some(proxy),
        Err(proxy_errs) => {
            errors.extend(proxy_errs);
            None
        }
    };
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
    let mut valid_taier = Vec::new();
    for raw_sel in &cli.taier {
        match TaierSelector::try_new(raw_sel) {
            Ok(_) => valid_taier.push(raw_sel.clone()),
            Err(err) => errors.push(format_taier_selector_error(&err)),
        }
    }
    if !cli.taier.is_empty() || !cli.taier_cities.is_empty() || !cli.taier_operators.is_empty() {
        taier_settings(
            &mut settings,
            &valid_taier,
            &cli.taier_cities,
            &cli.taier_operators,
            !cli.taier.is_empty(),
        );
    }
    if cli.no_cloudflare_udp {
        cloudflare_settings(&mut settings, false);
    }
    let inet_endpoint = match cli
        .inetspeed_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => {
            let parsed_ip = raw
                .parse::<IpAddr>()
                .ok()
                .filter(|&ip| bench_api::is_routable_endpoint_ip(ip));
            match parsed_ip {
                Some(ip) => Some(ip.to_string()),
                None => {
                    errors.push(locale::words().invalid_inetspeed_endpoint(raw));
                    None
                }
            }
        }
        None => None,
    };
    if cli.inetspeed_threads.is_some() || inet_endpoint.is_some() {
        inetspeed_settings(
            &mut settings,
            cli.inetspeed_threads,
            inet_endpoint.as_deref(),
        );
    }
    if !cli.tcping_groups.is_empty() || cli.tcping_all || cli.tcping_count.is_some() {
        tcping_settings(
            &mut settings,
            &cli.tcping_groups,
            cli.tcping_all,
            cli.tcping_count,
        );
    }
    if !cli.trace_groups.is_empty() || cli.no_trace_tcp || cli.trace_packet_size.is_some() {
        trace_settings(
            &mut settings,
            &cli.trace_groups,
            !cli.no_trace_tcp,
            cli.trace_packet_size,
        );
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
    match (orchestrator::plan(registry, &ids, &settings), proxy) {
        (Ok(planned), Some(proxy)) if errors.is_empty() => Ok((planned, proxy)),
        (Ok(_), _) => Err(errors),
        (Err(plan_errs), _) => {
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
    let known = |name: &str| registry.iter().any(|entry| names(name, entry.id));
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

/// Sets `--taier`, `--taier-cities`, and `--taier-operators` in `network.taier`'s table.
pub fn taier_settings(
    settings: &mut BTreeMap<String, serde_json::Value>,
    selections: &[String],
    cities: &[String],
    operators: &[TaierOperator],
    override_selections: bool,
) {
    let table = plugin_table(settings, Taier::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        if override_selections || !selections.is_empty() {
            let values = selections
                .iter()
                .map(|sel| serde_json::Value::String(sel.clone()))
                .collect();
            table.insert("selections".to_owned(), serde_json::Value::Array(values));
        }
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

/// Downloads and validates `servers/taier.json` using `proxy` (`--socks5-proxy` /
/// `--github-proxy`), falling back to the build-time embedded `bench_data` snapshot when live
/// retrieval fails.
pub async fn fetch_taier_locations(proxy: &DownloadProxy) -> Result<Vec<TaierLocation>, String> {
    let embedded = taier::embedded_servers();
    let timeout = if embedded.is_some() {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(20)
    };
    let live = match tokio::time::timeout(timeout, fetch_taier_bytes(proxy)).await {
        Ok(result) => result,
        Err(_) => {
            Err(locale::words().taier_list_problem(TaierListProblem::Unreadable("timed out")))
        }
    };
    resolve_taier_locations(live.as_deref(), embedded)
}

async fn fetch_taier_bytes(proxy: &DownloadProxy) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;

    let fetchers = Fetchers::try_new_with_proxy(proxy)?;
    let part = fetchers
        .open(None, taier::SERVERS_URL, None)
        .await
        .map_err(|err| match err {
            DownloadError::Transient(msg) | DownloadError::Final(msg) => msg,
        })?;
    let reader = match part {
        Part::Whole(r) | Part::Range(r) => r,
    };
    let mut buf = Vec::new();
    reader
        .take(taier::nodes::LIMIT as u64 + 1)
        .read_to_end(&mut buf)
        .await
        .map_err(|err| {
            locale::words().taier_list_problem(TaierListProblem::Unreadable(&err.to_string()))
        })?;
    Ok(buf)
}

/// Resolves validated Taier locations from a live download attempt and an optional build-time
/// embedded fallback snapshot: live valid data always wins, while network or upstream errors fall
/// back to `embedded` when available.
pub fn resolve_taier_locations(
    live: Result<&[u8], &String>,
    embedded: Option<&[u8]>,
) -> Result<Vec<TaierLocation>, String> {
    let outcome = match live {
        Ok(bytes) => match parse_taier_locations(bytes) {
            Ok(locations) => Ok(locations),
            Err(err) => match embedded {
                Some(snapshot) => {
                    tracing::debug!(
                        "live Taier list rejected ({err}); falling back to embedded snapshot"
                    );
                    parse_taier_locations(snapshot)
                }
                None => Err(err),
            },
        },
        Err(err) => match embedded {
            Some(snapshot) => {
                let clean = crate::report::terminal::strip_terminal_controls(err, false);
                tracing::debug!(
                    "live Taier list download failed ({clean}); falling back to embedded snapshot"
                );
                parse_taier_locations(snapshot)
            }
            None => Err(err.clone()),
        },
    };
    outcome.map_err(|err| crate::report::terminal::strip_terminal_controls(&err, false))
}

fn parse_taier_locations(buf: &[u8]) -> Result<Vec<TaierLocation>, String> {
    taier::nodes::parse(buf).map_err(|error| {
        use taier::nodes::ListError as E;
        use taier::nodes::Rule as R;

        let problem = match &error {
            E::TooLarge => TaierListProblem::TooLarge(taier::nodes::LIMIT),
            E::Format(reason) => TaierListProblem::Format(reason),
            E::Newer(version) => TaierListProblem::Newer(*version),
            E::Version(version) => TaierListProblem::Version(*version),
            E::Empty => TaierListProblem::Empty,
            E::TooMany(count) => TaierListProblem::TooMany(*count, taier::nodes::MOST_LOCATIONS),
            E::Location(position, rule) => {
                let mapped = match rule {
                    R::Id => TaierRule::Id,
                    R::Province => TaierRule::Province,
                    R::City => TaierRule::City,
                    R::Nodes => TaierRule::Nodes,
                    R::Operator => TaierRule::Operator,
                    R::Ip => TaierRule::Ip,
                    R::Port => TaierRule::Port,
                    R::DuplicateEndpoint => TaierRule::DuplicateEndpoint,
                };
                TaierListProblem::Rule(*position, mapped)
            }
            E::Duplicate(id) => TaierListProblem::Duplicate(id),
        };
        let rendered = locale::words().taier_list_problem(problem);
        crate::report::terminal::strip_terminal_controls(&rendered, false)
    })
}

/// Formats a [`TaierSelectorError`] through the active [`locale`] catalog.
#[must_use]
pub fn format_taier_selector_error(err: &TaierSelectorError) -> String {
    let problem = match err {
        TaierSelectorError::Empty => TaierSelectorProblem::Empty,
        TaierSelectorError::Format(raw) => TaierSelectorProblem::Format(raw),
        TaierSelectorError::EmptyPlace(raw) => TaierSelectorProblem::EmptyPlace(raw),
        TaierSelectorError::EmptyOperators(raw) => TaierSelectorProblem::EmptyOperators(raw),
        TaierSelectorError::UnknownOperator { selector, operator } => {
            TaierSelectorProblem::UnknownOperator { selector, operator }
        }
    };
    locale::words().taier_selector_problem(problem)
}

/// Formats `--taier-list [PROVINCE]` output from validated `locations`.
pub fn format_taier_list(
    locations: &[TaierLocation],
    filter: &str,
    out: &mut impl Write,
) -> Result<(), String> {
    let words = locale::words();
    let regions = tui::taier_picker::build_regions(locations);
    let trimmed = filter.trim();
    if trimmed.is_empty() {
        let _ = writeln!(
            out,
            "{}",
            words.taier_catalog_header(regions.len(), locations.len())
        );
        let _ = writeln!(out, "{}\n", words.taier_catalog_carriers_legend());
        let headers = words.taier_catalog_overview_headers();
        let mut rows = Vec::with_capacity(regions.len());
        for reg in &regions {
            let kind_code = match reg.kind {
                RegionKind::Municipality => "municipality",
                RegionKind::Province => "province",
                RegionKind::Overseas => "overseas",
            };
            let kind = words.taier_region_kind(kind_code);
            let ops: Vec<&str> = reg.operators.iter().map(|op| op.short_code()).collect();
            let example = match reg.kind {
                RegionKind::Municipality => {
                    let top_ops: Vec<&str> = reg
                        .operators
                        .iter()
                        .take(3)
                        .map(|op| op.short_code())
                        .collect();
                    format!("{}:{}", reg.slug, top_ops.join("+"))
                }
                RegionKind::Province | RegionKind::Overseas => {
                    let city = &reg.cities[0];
                    let op = city
                        .operators
                        .first()
                        .map_or("ct", |(op, _)| op.short_code());
                    format!("{}/{}:{op}", reg.slug, city.slug)
                }
            };
            rows.push((
                reg.name.as_str(),
                kind,
                reg.cities.len().to_string(),
                ops.join(", "),
                example,
            ));
        }
        let w_reg = rows
            .iter()
            .map(|r| display_width(r.0))
            .max()
            .unwrap_or(0)
            .max(display_width(headers[0]));
        let w_kind = rows
            .iter()
            .map(|r| display_width(r.1))
            .max()
            .unwrap_or(0)
            .max(display_width(headers[1]));
        let w_cities = rows
            .iter()
            .map(|r| display_width(&r.2))
            .max()
            .unwrap_or(0)
            .max(display_width(headers[2]));
        let w_ops = rows
            .iter()
            .map(|r| display_width(&r.3))
            .max()
            .unwrap_or(0)
            .max(display_width(headers[3]));
        let _ = writeln!(
            out,
            "{}  {}  {}  {}  {}",
            pad_right(headers[0], w_reg),
            pad_right(headers[1], w_kind),
            pad_left(headers[2], w_cities),
            pad_right(headers[3], w_ops),
            headers[4]
        );
        for (name, kind, cities, ops, example) in rows {
            let _ = writeln!(
                out,
                "{}  {}  {}  {}  {example}",
                pad_right(name, w_reg),
                pad_right(kind, w_kind),
                pad_left(&cities, w_cities),
                pad_right(&ops, w_ops),
            );
        }
        let _ = writeln!(out, "\n{}", words.taier_catalog_footer_hint());
        return Ok(());
    }

    let matched = filter_taier_regions(&regions, trimmed);
    if matched.is_empty() {
        return Err(words.taier_unknown_cities(&[trimmed.to_owned()]));
    }

    let city_headers = words.taier_catalog_city_headers();
    for (idx, (reg, cities)) in matched.into_iter().enumerate() {
        if idx > 0 {
            let _ = writeln!(out);
        }
        let _ = writeln!(
            out,
            "{}",
            words.taier_catalog_region_heading(
                &reg.name,
                reg.kind == RegionKind::Municipality,
                cities.len()
            )
        );
        let mut rows = Vec::with_capacity(cities.len());
        for city in cities {
            let selector = if reg.kind == RegionKind::Municipality {
                reg.slug.clone()
            } else {
                format!("{}/{}", reg.slug, city.slug)
            };
            let city_label = if city.default {
                format!("{}*", city.name)
            } else {
                city.name.clone()
            };
            let ops_parts: Vec<String> = city
                .operators
                .iter()
                .map(|(op, count)| format!("{} ({count})", op.short_code()))
                .collect();
            rows.push((selector, city_label, ops_parts.join(", ")));
        }
        let w_sel = rows
            .iter()
            .map(|r| display_width(&r.0))
            .max()
            .unwrap_or(0)
            .max(display_width(city_headers[0]));
        let w_city = rows
            .iter()
            .map(|r| display_width(&r.1))
            .max()
            .unwrap_or(0)
            .max(display_width(city_headers[1]));
        let _ = writeln!(
            out,
            "  {}  {}  {}",
            pad_right(city_headers[0], w_sel),
            pad_right(city_headers[1], w_city),
            city_headers[2]
        );
        for (selector, city_label, ops_text) in rows {
            let _ = writeln!(
                out,
                "  {}  {}  {ops_text}",
                pad_right(&selector, w_sel),
                pad_right(&city_label, w_city),
            );
        }
    }
    Ok(())
}

fn display_width(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;

    UnicodeWidthStr::width(text)
}

fn pad_right(text: &str, width: usize) -> String {
    let pad = width.saturating_sub(display_width(text));
    format!("{text}{}", " ".repeat(pad))
}

fn pad_left(text: &str, width: usize) -> String {
    let pad = width.saturating_sub(display_width(text));
    format!("{}{text}", " ".repeat(pad))
}

fn filter_taier_regions<'a>(
    regions: &'a [RegionEntry],
    query: &str,
) -> Vec<(&'a RegionEntry, Vec<&'a CityEntry>)> {
    let q_slug = taier::nodes::slug(query);
    let q_lower = query.to_ascii_lowercase();

    let mut exact_regions = Vec::new();
    for reg in regions {
        if reg.name.eq_ignore_ascii_case(query) || reg.slug == q_slug {
            let refs: Vec<&CityEntry> = reg.cities.iter().collect();
            exact_regions.push((reg, refs));
        }
    }
    if !exact_regions.is_empty() {
        return exact_regions;
    }

    let mut exact_cities = Vec::new();
    for reg in regions {
        let mut hits = Vec::new();
        for city in &reg.cities {
            let selector = format!("{}/{}", reg.slug, city.slug);
            if city.name.eq_ignore_ascii_case(query)
                || city.slug == q_slug
                || city.id.eq_ignore_ascii_case(query)
                || selector.eq_ignore_ascii_case(query)
            {
                hits.push(city);
            }
        }
        if !hits.is_empty() {
            exact_cities.push((reg, hits));
        }
    }
    if !exact_cities.is_empty() {
        return exact_cities;
    }

    let mut fuzzy = Vec::new();
    for reg in regions {
        if reg.name.to_ascii_lowercase().contains(&q_lower) || reg.slug.contains(&q_lower) {
            let refs: Vec<&CityEntry> = reg.cities.iter().collect();
            fuzzy.push((reg, refs));
            continue;
        }
        let mut hits = Vec::new();
        for city in &reg.cities {
            if city.name.to_ascii_lowercase().contains(&q_lower) || city.id.contains(&q_lower) {
                hits.push(city);
            }
        }
        if !hits.is_empty() {
            fuzzy.push((reg, hits));
        }
    }
    fuzzy
}

/// Sets `--no-cloudflare-udp` in `network.cloudflare`'s table.
pub fn cloudflare_settings(settings: &mut BTreeMap<String, serde_json::Value>, udp: bool) {
    let table = plugin_table(settings, Cloudflare::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        table.insert("udp".to_owned(), serde_json::Value::Bool(udp));
    }
}

/// Sets `--inetspeed-threads` and `--inetspeed-endpoint` in `network.inetspeed`'s table.
pub fn inetspeed_settings(
    settings: &mut BTreeMap<String, serde_json::Value>,
    threads: Option<u8>,
    endpoint: Option<&str>,
) {
    let table = plugin_table(settings, Inetspeed::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        if let Some(n) = threads {
            table.insert("threads".to_owned(), serde_json::Value::from(n));
        }
        if let Some(ip) = endpoint {
            table.insert(
                "endpoint".to_owned(),
                serde_json::Value::String(ip.to_owned()),
            );
        }
    }
}

/// Sets `--tcping-groups`, `--tcping-all`, and `--tcping-count` in `network.tcping`'s table.
pub fn tcping_settings(
    settings: &mut BTreeMap<String, serde_json::Value>,
    groups: &[TcpingGroup],
    all_targets: bool,
    count: Option<u8>,
) {
    let table = plugin_table(settings, Tcping::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        if !groups.is_empty() {
            let values = groups
                .iter()
                .map(|g| serde_json::Value::String(g.code().to_owned()))
                .collect();
            table.insert("groups".to_owned(), serde_json::Value::Array(values));
        }
        if all_targets {
            table.insert("all_targets".to_owned(), serde_json::Value::Bool(true));
        }
        if let Some(n) = count {
            table.insert("count".to_owned(), serde_json::Value::from(n));
        }
    }
}

/// Sets `--trace-groups` in `network.trace`'s table.
pub fn trace_groups(settings: &mut BTreeMap<String, serde_json::Value>, groups: &[TraceGroup]) {
    trace_settings(settings, groups, true, None);
}

/// Sets `--trace-groups`, `--no-trace-tcp`, and `--trace-packet-size` in `network.trace`'s table.
pub fn trace_settings(
    settings: &mut BTreeMap<String, serde_json::Value>,
    groups: &[TraceGroup],
    tcp: bool,
    large_packets: Option<u16>,
) {
    let table = plugin_table(settings, Trace::DESCRIPTOR.id);
    if let Some(table) = table.as_object_mut() {
        if !groups.is_empty() {
            let values = groups
                .iter()
                .map(|group| serde_json::Value::String(group.code().to_owned()))
                .collect();
            table.insert("groups".to_owned(), serde_json::Value::Array(values));
        }
        if !tcp {
            table.insert("tcp".to_owned(), serde_json::Value::Bool(false));
        }
        if let Some(size) = large_packets {
            table.insert("large_packets".to_owned(), serde_json::Value::from(size));
        }
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

/// Opens `--output` before the run starts (creating new files with `0600` permissions and refusing
/// symlinks, FIFOs, block devices, sockets, and untrusted pre-created files in world-writable
/// directories), so an unwritable or unsafe path fails at once without blocking.
pub fn create_output(path: &Path) -> Result<fs::File, String> {
    create_output_inner(path).map_err(|err| locale::words().output_unwritable(path, &err))
}

fn create_output_inner(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    use rustix::fs::Mode;
    use rustix::fs::OFlags;

    let flags =
        OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let mode = Mode::RUSR | Mode::WUSR;
    let fd = rustix::fs::openat(rustix::fs::CWD, path, flags, mode).map_err(io::Error::from)?;
    let file = fs::File::from(fd);
    let meta = file.metadata()?;
    let file_type = meta.file_type();
    if file_type.is_char_device() {
        let cur = rustix::fs::fcntl_getfl(&file).map_err(io::Error::from)?;
        rustix::fs::fcntl_setfl(&file, cur - OFlags::NONBLOCK).map_err(io::Error::from)?;
        return Ok(file);
    }
    if !file_type.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path must be a regular file",
        ));
    }
    let euid = rustix::process::geteuid().as_raw();
    let parent = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let world_writable = fs::metadata(parent)
        .is_ok_and(|dir_meta| dir_meta.is_dir() && (dir_meta.permissions().mode() & 0o002) != 0);
    if world_writable && (meta.uid() != euid || meta.nlink() > 1) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing pre-existing unowned or hardlinked output file in world-writable directory",
        ));
    }
    if meta.uid() == euid {
        rustix::fs::fchmod(&file, mode).map_err(io::Error::from)?;
    }
    file.set_len(0)?;
    let cur = rustix::fs::fcntl_getfl(&file).map_err(io::Error::from)?;
    rustix::fs::fcntl_setfl(&file, cur - OFlags::NONBLOCK).map_err(io::Error::from)?;
    Ok(file)
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
    /// 5-second `/proc/stat` `%si` and `%st` samples collected when running on a virtual machine.
    pub vm_cpu_samples: Vec<VmCpuSample>,
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
        report::render_with_vm_cpu(&page, &self.vm_cpu_samples, format, width)
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
    F: Fn(Option<IpFamily>, Cow<'static, str>, Option<Range<u64>>) -> Fut,
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
    F: Fn(Option<IpFamily>, Cow<'static, str>, Option<Range<u64>>) -> Fut,
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
        match crate::report_path::save_archive(path, &archive_text) {
            Ok(written) => tracing::info!("{}", locale::words().tui_saved_to(&written)),
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
    F: Fn(Option<IpFamily>, Cow<'static, str>, Option<Range<u64>>) -> Fut,
    Fut: Future<Output = Result<Part<R>, DownloadError>>,
    R: AsyncRead + Unpin,
{
    use std::os::unix::fs::PermissionsExt;

    let started = Instant::now();
    let vm_sampler = VmCpuSampler::start_for_host(host.platform, Root::system(), started);
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
        let any = |url: &'static str, range| fetch(None, Cow::Borrowed(url), range);
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
    let needing_files = runnable().filter(|planned| !planned.files().is_empty());
    for planned in needing_files {
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
        origin: TokioInstant::from_std(started),
        tools: Arc::new(ready),
        failed_tools,
        files,
        http: Some(probers),
        net: Some(net_probers),
        host: Arc::clone(&host),
    };
    let mut sampling_obs = SamplingObserver::new(observer, &vm_sampler);
    let runs = orchestrator::run_plan(planned, cancel, &settings, &mut sampling_obs).await;
    let vm_cpu_samples = vm_sampler.finish().await;
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
        vm_cpu_samples,
        total,
        exit_code,
    })
}
