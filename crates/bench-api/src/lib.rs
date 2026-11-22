//! Plugin contract for lowendaff-bench, which every test-item crate implements. A plugin
//! declares what it needs, and returns a result of its own type, or its own reason to skip or to
//! fail; it publishes the result as it goes and reports its progress, and the orchestrator and
//! the reporter do the rest.

#![warn(missing_docs)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::future::Future;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bench_host::glibc::GlibcVersion;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tokio::time::Instant;

pub use tokio_util::sync::CancellationToken;

/// Architectures every pinned tool must cover.
pub const RELEASE_ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// Declarative preconditions. Root is not listed: the whole program requires root.
#[derive(Debug, Clone, Copy)]
pub struct Requirements {
    /// Pinned tools, downloaded and verified by the orchestrator before the first test.
    pub tools: &'static [ToolSpec],
    /// Network access the test needs.
    pub net: Net,
    /// Kinds of platform the test applies to; empty means every platform. On any other
    /// platform the plugin does not run.
    pub platforms: &'static [PlatformKind],
    /// The oldest glibc that the test's tools run on, or `None` when they do not need glibc.
    /// The orchestrator checks it before downloading the tools.
    pub glibc: Option<GlibcVersion>,
    /// Whether the test needs at least two NUMA nodes that it can use, as a test between them
    /// does. On any other host the plugin does not run.
    pub numa: bool,
}

impl Requirements {
    /// No tools, no network, every platform, any C library, any NUMA layout.
    pub const NONE: Self = Self {
        tools: &[],
        net: Net::None,
        platforms: &[],
        glibc: None,
        numa: false,
    };

    /// Whether the plugin runs on `host`: on one of its platforms, and with two NUMA nodes or
    /// more when it needs them. A layout that could not be read has none.
    #[must_use]
    pub fn applies_to(&self, host: &HostProfile) -> bool {
        let platform = self.platforms.is_empty() || self.platforms.contains(&host.platform.kind());
        let numa = !self.numa || matches!(&host.numa, Read::Done(layout) if layout.usable() >= 2);
        platform && numa
    }
}

/// Network access a plugin needs; an unreachable one skips the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    /// Works offline.
    None,
    /// Either IPv4 or IPv6 must be reachable.
    Any,
    /// IPv4 must be reachable.
    Ipv4,
    /// IPv6 must be reachable.
    Ipv6,
}

/// An IP version, which a file may have to be fetched over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IpFamily {
    /// IPv4.
    Ipv4,
    /// IPv6.
    Ipv6,
}

/// Whether `address` is safe to contact as an external endpoint: rejects unspecified (`0.0.0.0/8`,
/// `::`), loopback (`127.0.0.0/8`, `::1`), RFC 1918 private (`10/8`, `172.16/12`, `192.168/16`),
/// link-local / cloud metadata (`169.254.0.0/16`, `fe80::/10`), carrier-grade NAT / shared space
/// (`100.64.0.0/10`, including `100.100.100.200`), multicast, broadcast, Class E reserved blocks,
/// and non-global IPv6 addresses (`::ffff:0:0/96`, `64:ff9b::/96`, `fc00::/7`, `ff00::/8`).
///
/// IANA documentation prefixes (`192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`,
/// `2001:db8::/32`) remain accepted so offline test fixtures can name deterministic example
/// endpoints without opening local or internal network ranges.
#[must_use]
pub fn is_routable_endpoint_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || a == 0
                || a >= 224
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            let [first, ..] = v6.segments();
            first & 0xe000 == 0x2000
        }
    }
}

/// A pinned external tool. Plugins that share a tool must declare identical specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Tool name, e.g. `fio`: the executable's file name, or the directory of an archive.
    pub name: &'static str,
    /// Upstream version the builds were made from.
    pub version: &'static str,
    /// Downloadable builds by architecture.
    pub assets: &'static [ToolAsset],
    /// Whether builds must cover every architecture in [`RELEASE_ARCHES`]; `false` only for
    /// upstream tools that support a subset of release architectures (such as `x86_64` only).
    pub all_arches: bool,
    /// What each asset is.
    pub form: ToolForm,
    /// Whether the tool sends and receives raw packets, as a tracer does. The program then gives
    /// its copy CAP_NET_RAW as a file capability, the one privilege that it needs, so that it can
    /// run as an unprivileged user; a host where that cannot work has no such tool.
    pub raw_sockets: bool,
}

impl ToolSpec {
    /// The build for `arch` (a value of `std::env::consts::ARCH`), if there is one.
    #[must_use]
    pub fn asset_for(&self, arch: &str) -> Option<&ToolAsset> {
        self.assets.iter().find(|asset| asset.arch == arch)
    }
}

/// What the download of a tool is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolForm {
    /// The download is the static executable itself.
    Executable,
    /// A gzip-compressed tar archive with one top-level directory. It is unpacked into a
    /// directory named after the tool, without that top-level directory.
    TarGz {
        /// File name of the executable in the archive's top-level directory.
        executable: &'static str,
    },
    /// A gzip-compressed tar archive whose files sit directly at the archive root, unpacked into
    /// a directory named after the tool.
    TarGzFlat {
        /// File name of the executable at the archive root.
        executable: &'static str,
    },
    /// An xz-compressed tar archive with one top-level directory, unpacked into a directory named
    /// after the tool, without that top-level directory.
    TarXz {
        /// File name of the executable in the archive's top-level directory.
        executable: &'static str,
    },
    /// A ZIP archive with one top-level directory, unpacked into a directory named after the
    /// tool, without that top-level directory.
    Zip {
        /// File name of the executable in the archive's top-level directory on `x86_64`.
        x86_64: &'static str,
        /// File name of the executable in the archive's top-level directory on `aarch64`.
        aarch64: &'static str,
    },
}

/// One downloadable build of a [`ToolSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAsset {
    /// Value of `std::env::consts::ARCH`.
    pub arch: &'static str,
    /// HTTPS download URL.
    pub url: &'static str,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: &'static str,
    /// Exact size in bytes; a download that grows past it is aborted.
    pub size: u64,
}

/// A file that a plugin needs besides its tools, such as a list of the servers that it measures
/// against, which changes more often than the program does. The orchestrator gets it before the
/// first test, and a plugin whose file it could not get does not run, unless the file is optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpec {
    /// The name the plugin asks for it by, such as `servers`.
    pub name: &'static str,
    /// Where it comes from.
    pub source: FileSource,
    /// The IP version to fetch it over, for an answer that depends on the host's address of that
    /// version; `None` takes whichever the connection does. A file of a version that the host
    /// cannot reach is not fetched.
    pub family: Option<IpFamily>,
    /// Whether the plugin runs without it; it then learns why the file is missing
    /// ([`Context::missing`]).
    pub optional: bool,
}

impl FileSpec {
    /// A file that the plugin needs, fetched over either IP version.
    #[must_use]
    pub fn new(name: &'static str, source: FileSource) -> Self {
        Self {
            name,
            source,
            family: None,
            optional: false,
        }
    }
}

/// Where a [`FileSpec`] comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSource {
    /// An HTTPS address, which the orchestrator downloads into the run directory.
    Url(String),
    /// A file on the host, which the plugin reads where it is.
    Path(PathBuf),
}

/// Validates an `https://` endpoint URL or a loopback `http://127.0.0.1` / `http://[::1]` test
/// override, rejecting credentials (`@`), query strings (`?`), fragments (`#`), backslashes,
/// control characters, empty or malformed hosts, and invalid ports. When `strip_trailing_slash` is
/// `true`, trailing `/` characters after a valid authority are removed.
#[must_use]
pub fn validate_endpoint_url(raw: &str, strip_trailing_slash: bool) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed
            .bytes()
            .any(|b| b <= b' ' || b == 0x7f || matches!(b, b'?' | b'#' | b'\\' | b'@'))
    {
        return None;
    }
    let (scheme, after_scheme) = trimmed.split_once("://")?;
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let authority = after_scheme.split('/').next().unwrap_or_default();
    if !is_valid_authority(authority, scheme == "http") {
        return None;
    }
    let normalized = if strip_trailing_slash {
        trimmed.trim_end_matches('/')
    } else {
        trimmed
    };
    Some(normalized.to_owned())
}

fn is_valid_authority(authority: &str, loopback_only: bool) -> bool {
    if authority.is_empty() {
        return false;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((v6_text, port_rest)) = rest.split_once(']') else {
            return false;
        };
        if !is_valid_port_suffix(port_rest) {
            return false;
        }
        let Ok(v6) = v6_text.parse::<std::net::Ipv6Addr>() else {
            return false;
        };
        return if loopback_only {
            v6 == std::net::Ipv6Addr::LOCALHOST && v6_text == "::1"
        } else {
            is_routable_endpoint_ip(IpAddr::V6(v6))
        };
    }
    if authority.contains(']') {
        return false;
    }
    let (host, port_rest) = match authority.find(':') {
        Some(idx) => (&authority[..idx], &authority[idx..]),
        None => (authority, ""),
    };
    if host.is_empty() || !is_valid_port_suffix(port_rest) {
        return false;
    }
    if loopback_only {
        return host == "127.0.0.1";
    }
    if let Ok(v4) = host.parse::<Ipv4Addr>() {
        return is_routable_endpoint_ip(IpAddr::V4(v4));
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return false;
    }
    (1..=253).contains(&host.len())
        && host.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn is_valid_port_suffix(port_rest: &str) -> bool {
    if port_rest.is_empty() {
        return true;
    }
    let Some(digits) = port_rest.strip_prefix(':') else {
        return false;
    };
    !digits.is_empty()
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits.parse::<u16>().is_ok_and(|port| port > 0)
}

/// Static facts about a plugin type.
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    /// Stable id `<group>.<name>`, such as `memory.stream`.
    pub id: &'static str,
    /// Preconditions the orchestrator checks before calling `run`.
    pub requires: Requirements,
    /// Hard limit for one run.
    pub timeout: Duration,
}

/// Why a plugin stopped short of a complete result, with the plugin's own reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Stop<F> {
    /// The measurement does not apply here.
    Skipped(F),
    /// The measurement went wrong.
    Failed(F),
}

/// A tool that a result came from, with the version the tool reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolVersion {
    /// The tool's name, as in its pin.
    pub name: &'static str,
    /// The version the tool reported about itself.
    pub version: String,
}

/// What a plugin reports while it runs, for the progress log on stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The step it starts.
    Step(Step),
    /// A tool it ran, which the log shows with `-v`.
    Tool(ToolRun),
}

/// A step of a plugin's run, for the progress log. It holds no words: the program writes them,
/// in its user's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A test with this many threads; with one, the single-thread test.
    Test {
        /// The threads that run it.
        threads: usize,
    },
    /// Reading memory with this many threads.
    Read {
        /// The threads that read.
        threads: usize,
    },
    /// Writing memory with this many threads.
    Write {
        /// The threads that write.
        threads: usize,
    },
    /// Creating the file that the disk test reads and writes.
    TestFile,
    /// The random read and write test at one block size.
    RandomReadWrite {
        /// The block size as fio takes it, such as `4k`.
        block_size: &'static str,
    },
    /// The latency of a cache level.
    Cache {
        /// 1 for L1, 2 for L2, 3 for L3.
        level: u8,
    },
    /// The latency of main memory.
    MainMemory,
    /// The bandwidth from the CPUs of one NUMA node to the memory of another, or of the same.
    NodeBandwidth {
        /// The node whose CPUs run the threads.
        cpus: usize,
        /// The node that holds the memory.
        memory: usize,
    },
    /// The latency from the CPUs of one NUMA node to the memory of another, or of the same.
    NodeLatency {
        /// The node whose CPUs run the threads.
        cpus: usize,
        /// The node that holds the memory.
        memory: usize,
    },
    /// A run again after the upload of the last one failed.
    RunAgain {
        /// The run's number, from 2.
        run: usize,
    },
    /// The upload to a server of the network test.
    Upload {
        /// The server's place, as its list names it.
        location: Cow<'static, str>,
        /// The country of the server's place, as its list names it.
        country: Cow<'static, str>,
        /// Whether the test runs over IPv6, else over IPv4.
        ipv6: bool,
    },
    /// The download from a server of the network test.
    Download {
        /// The server's place, as its list names it.
        location: Cow<'static, str>,
        /// The country of the server's place, as its list names it.
        country: Cow<'static, str>,
        /// Whether the test runs over IPv6, else over IPv4.
        ipv6: bool,
    },
    /// The trace of a target of route tracing.
    Trace {
        /// The target's name, as its list names it, or the cache server's service.
        target: Cow<'static, str>,
        /// Whether the trace runs over IPv6, else over IPv4.
        ipv6: bool,
    },
    /// Probing a regional group of streaming services.
    Media {
        /// The group's name, such as `Global` or `Asia`.
        group: &'static str,
        /// Whether the probes run over IPv6, else over IPv4.
        ipv6: bool,
    },
    /// Running one stage of IP reputation checking.
    IpQuality {
        /// Stage code (`intelligence`, `dnsbl`, or `mail`).
        stage: &'static str,
        /// Whether the stage evaluates the host's IPv6 address, else IPv4.
        ipv6: bool,
    },
    /// Querying global BGP route state and RPKI status for an address family.
    Bgp {
        /// Whether the query evaluates the host's IPv6 address, else IPv4.
        ipv6: bool,
    },
    /// Running one sub-suite or stage of the unified CPU benchmark (`cpu.bench`).
    CpuSuite {
        /// Suite code (`sysbench`, `7zip`, `unixbench`, `hpl`, `passmark`, `ycruncher`,
        /// `prime95`, or `geekbench`).
        suite: &'static str,
        /// Optional stage code within the suite.
        stage: Option<&'static str>,
        /// Optional thread or parallel copy count when positive.
        threads: Option<usize>,
    },
    /// Measuring a China carrier node in `network.taier`.
    Taier {
        /// Location display name (e.g. `Beijing` or `Nanjing, Jiangsu`).
        location: Cow<'static, str>,
        /// Carrier code (`telecom`, `unicom`, `mobile`, `cernet`, `cbn`, or `other`).
        operator: &'static str,
        /// Phase code (`download` or `upload`).
        stage: &'static str,
    },
    /// Running a phase of `network.cloudflare` over an IP family.
    Cloudflare {
        /// Phase code (`latency`, `download`, `upload`, or `udp`).
        stage: &'static str,
        /// Whether the phase runs over IPv6, else IPv4.
        ipv6: bool,
    },
    /// Running a phase of `network.inetspeed` (Apple CDN speedtest).
    Inetspeed {
        /// Phase code (`discover`, `latency`, `download`, or `upload`).
        stage: &'static str,
        /// Concurrent stream count (`0` for `discover` and `latency`).
        threads: u8,
    },
}

/// Default byte limit read from an HTTP probe response body (256 KiB).
pub const DEFAULT_HTTP_LIMIT: usize = 256 * 1024;

/// Default timeout of one HTTP probe request.
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Method of an [`HttpRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// HTTP `GET`.
    Get,
    /// HTTP `POST`.
    Post,
}

/// How an [`HttpClient`] treats `3xx` redirects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectPolicy {
    /// Returns the `3xx` status and headers as received.
    None,
    /// Follows up to `max` redirects over the same IP family.
    Follow {
        /// Maximum redirect hops before [`HttpError::TooManyRedirects`].
        max: u8,
    },
}

/// One direct HTTP probe over a chosen IP family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// IP family the connection must bind to and resolve.
    pub family: IpFamily,
    /// HTTP method.
    pub method: HttpMethod,
    /// Target URL (`https://`, or loopback `http://` in tests).
    pub url: String,
    /// Extra request headers (`name`, `value`).
    pub headers: Vec<(String, String)>,
    /// Optional request body, for `POST` probes.
    pub body: Option<Vec<u8>>,
    /// Redirect policy.
    pub redirect: RedirectPolicy,
    /// Maximum time for the whole probe (including redirects and body read).
    pub timeout: Duration,
    /// Most bytes of the response body to keep; further bytes are dropped.
    pub max_bytes: usize,
}

impl HttpRequest {
    /// A `GET` probe over `family` to `url`, without following redirects.
    #[must_use]
    pub fn get(family: IpFamily, url: impl Into<String>) -> Self {
        Self {
            family,
            method: HttpMethod::Get,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            redirect: RedirectPolicy::None,
            timeout: DEFAULT_HTTP_TIMEOUT,
            max_bytes: DEFAULT_HTTP_LIMIT,
        }
    }

    /// A `POST` probe over `family` to `url` with `body`, without following redirects.
    #[must_use]
    pub fn post(family: IpFamily, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            family,
            method: HttpMethod::Post,
            url: url.into(),
            headers: Vec::new(),
            body: Some(body.into()),
            redirect: RedirectPolicy::None,
            timeout: DEFAULT_HTTP_TIMEOUT,
            max_bytes: DEFAULT_HTTP_LIMIT,
        }
    }

    /// Appends a request header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Sets how `3xx` responses are handled.
    #[must_use]
    pub fn with_redirect(mut self, redirect: RedirectPolicy) -> Self {
        self.redirect = redirect;
        self
    }

    /// Sets the time limit of the probe.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the maximum response body bytes kept.
    #[must_use]
    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }
}

/// Answer of an [`HttpRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code, such as `200`, `302` or `403`.
    pub status: u16,
    /// Effective URL that answered (after any followed redirects).
    pub final_url: String,
    /// Response headers with lowercase names, in wire order.
    pub headers: Vec<(String, String)>,
    /// Response body, truncated to [`HttpRequest::max_bytes`].
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// First header value matching `name` (ASCII case-insensitive).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Every header value matching `name` (ASCII case-insensitive), such as `set-cookie`.
    #[must_use]
    pub fn header_values<'a>(&'a self, name: &str) -> Vec<&'a str> {
        self.headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// The body decoded as UTF-8, replacing invalid sequences.
    #[must_use]
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }
}

/// Why an [`HttpRequest`] did not yield an [`HttpResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HttpError {
    /// The target host has no address of [`HttpRequest::family`].
    NoAddress,
    /// The probe exceeded [`HttpRequest::timeout`].
    TimedOut,
    /// Redirect hops exceeded the limit in [`RedirectPolicy::Follow`].
    TooManyRedirects,
    /// A `3xx` response had a missing, malformed or downgraded `Location`.
    InvalidRedirect(String),
    /// The URL uses a scheme other than `https` (outside loopback tests).
    NotHttps(String),
    /// Connection, DNS lookup, TLS or read failure.
    Transport(String),
    /// Cancelled via the plugin's [`CancellationToken`].
    Cancelled,
}

/// Boxed future returned by [`HttpClient::send`].
pub type HttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'a>>;

/// Direct, proxy-free HTTP transport over a chosen IP family.
pub trait HttpClient: Send + Sync {
    /// Executes `request` over `request.family`.
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a>;
}

struct NoHttpClient;

impl HttpClient for NoHttpClient {
    fn send<'a>(&'a self, _request: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async { Err(HttpError::Transport("no HTTP client configured".to_owned())) })
    }
}

/// Answer of a DNS `A` lookup used by DNSBL checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DnsAnswer {
    /// `NXDOMAIN` or `NODATA`: the queried name has no `A` record.
    NotFound,
    /// One or more IPv4 `A` records returned for the queried name.
    Addresses(Vec<Ipv4Addr>),
}

/// Initial greeting reply returned by an SMTP server on port 25.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SmtpBanner {
    /// Three-digit SMTP reply code, such as `220`, `421` or `554`.
    pub code: u16,
}

/// Answer of a CAICT GlobalSpeed (`dovalid`) queue admission handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TaierAdmission {
    /// Admitted into the node's queue with this lease key (`uuid`).
    Admitted(String),
    /// Node queue is full or busy (`dovalid: -1`).
    Busy,
    /// Token or authentication rejected (`dovalid: 0`).
    AuthRejected,
    /// Invalid parameters or version mismatch (`dovalid: 2`).
    InvalidParams,
    /// Unexpected `dovalid` response body.
    Unexpected(String),
}

/// Parameters for a multi-stream HTTP/1.1 download or upload phase against a Taier node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaierStreamSpec {
    /// IPv4 address of the Taier speedtest node.
    pub ip: Ipv4Addr,
    /// TCP port (`65499` or `9443`, or a loopback port in tests).
    pub port: u16,
    /// Queue lease key (`uuid`) obtained from `dovalid`.
    pub key: String,
    /// Duration of the transfer window.
    pub duration: Duration,
    /// Parallel TCP connections (`1..=32`).
    pub streams: u8,
}

/// Throughput summary of a multi-stream Taier download or upload phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TaierTransfer {
    /// Total payload bytes transferred across all active streams.
    pub bytes: u64,
    /// Elapsed transfer window in microseconds.
    pub elapsed_micros: u64,
    /// Sustained throughput in bits per second.
    pub bits_per_sec: u64,
    /// Number of streams that successfully transferred payload bytes.
    pub active_streams: u8,
}

/// One cumulative throughput sample recorded during a Cloudflare transfer phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThroughputTick {
    /// Active transfer window elapsed in milliseconds when the sample was taken.
    pub elapsed_ms: u64,
    /// Cumulative payload bytes transferred up to `elapsed_ms`.
    pub bytes: u64,
}

/// Parameters for a multi-worker Cloudflare download or upload throughput phase with concurrent
/// loaded-latency probing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudflareTransferSpec {
    /// IP family the connections must bind to and resolve.
    pub family: IpFamily,
    /// Base URL (`https://speed.cloudflare.com`, or loopback `http://` in tests).
    pub base_url: String,
    /// Random measurement identifier (`measId` query parameter).
    pub meas_id: String,
    /// Number of concurrent HTTP workers (`1..=32`).
    pub concurrency: u8,
    /// Active measurement window duration.
    pub duration: Duration,
    /// Payload size requested or streamed per HTTP request.
    pub bytes_per_req: u64,
    /// Delay between loaded-latency probes.
    pub probe_interval: Duration,
    /// Per-probe timeout for loaded-latency requests.
    pub probe_timeout: Duration,
}

/// Raw observations collected during a Cloudflare download or upload throughput phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloudflareTransfer {
    /// Cumulative payload bytes transferred across all workers.
    pub bytes: u64,
    /// Active phase duration in milliseconds.
    pub duration_ms: u64,
    /// Cumulative throughput samples recorded every 200 ms.
    pub ticks: Vec<ThroughputTick>,
    /// Number of loaded-latency probes sent during the transfer window.
    pub loaded_sent: u64,
    /// Round-trip times in microseconds of successful loaded-latency probes.
    pub loaded_rtts_micros: Vec<u64>,
    /// Number of failed worker HTTP requests during the phase.
    pub failed_requests: u64,
}

/// Parameters for a Cloudflare RFC 5389 STUN UDP packet-loss and reordering probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudflareStunSpec {
    /// IP family the UDP socket must bind to and resolve.
    pub family: IpFamily,
    /// STUN server hostname or IP literal (e.g. `stun.cloudflare.com`).
    pub host: String,
    /// STUN UDP port (`3478` by default).
    pub port: u16,
    /// Total STUN Binding Request packets to send.
    pub packets: u16,
    /// Interval between consecutive UDP probes.
    pub interval: Duration,
    /// Per-probe response deadline.
    pub timeout: Duration,
}

/// Raw observations from a Cloudflare RFC 5389 STUN UDP packet-loss probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloudflareStun {
    /// Effective `stun:<host>:<port>` target probed.
    pub target: String,
    /// Number of STUN Binding Requests sent.
    pub sent: u64,
    /// Number of unique STUN Binding Success responses received.
    pub received: u64,
    /// Number of responses that arrived after a higher-sequenced probe.
    pub out_of_order: u64,
    /// Round-trip times in microseconds of received STUN responses.
    pub rtts_micros: Vec<u64>,
}

/// Reachable IP address families permitted when resolving or filtering candidate endpoints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct FamilyFilter {
    /// Whether IPv4 (`A`) addresses are permitted.
    pub ipv4: bool,
    /// Whether IPv6 (`AAAA`) addresses are permitted.
    pub ipv6: bool,
}

/// Transfer direction of an Apple CDN (`network.inetspeed`) speedtest round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InetspeedDirection {
    /// HTTP `GET` download round.
    Download,
    /// Streaming HTTP `PUT` upload round.
    Upload,
}

/// Endpoint target for an Apple CDN (`network.inetspeed`) HTTP RTT probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InetspeedTarget {
    /// Probe URL (`https://mensura.cdn-apple.com/api/v1/gm/small`, or loopback `http://`).
    pub url: String,
    /// Optional IP address to pin `url`'s hostname to while preserving TLS SNI.
    pub pin_ip: Option<IpAddr>,
}

/// Parameters for an Apple CDN (`network.inetspeed`) single-thread or multi-thread transfer round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InetspeedTransferSpec {
    /// Transfer direction (`Download` or `Upload`).
    pub direction: InetspeedDirection,
    /// Download (`GET`) or upload (`PUT`) endpoint URL.
    pub transfer_url: String,
    /// Latency probe (`GET`) endpoint URL polled concurrently during the transfer.
    pub latency_url: String,
    /// Optional IP address to pin the endpoint hostname to while preserving TLS SNI.
    pub pin_ip: Option<IpAddr>,
    /// Concurrent transfer worker count (`1..=64`).
    pub threads: u8,
    /// Per-worker byte cap.
    pub max_bytes: u64,
    /// Per-worker transfer duration limit.
    pub timeout: Duration,
}

/// Raw observations from an Apple CDN (`network.inetspeed`) transfer round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InetspeedTransfer {
    /// Total payload bytes transferred across all workers.
    pub bytes: u64,
    /// Elapsed round duration in microseconds.
    pub elapsed_micros: u64,
    /// Number of workers that encountered a transport or HTTP status fault.
    pub fault_count: u8,
    /// Round-trip times in microseconds of concurrent loaded-latency probes.
    pub loaded_rtts_micros: Vec<u64>,
}

/// Why a DNS or SMTP socket probe failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NetError {
    /// The target host has no address of the requested [`IpFamily`].
    NoAddress,
    /// The probe exceeded its timeout.
    TimedOut,
    /// No recursive DNS resolver was configured or readable on the host.
    NoResolver,
    /// Connection, DNS wire format or read failure.
    Transport(String),
    /// Cancelled via the plugin's [`CancellationToken`].
    Cancelled,
}

/// Boxed future returned by [`NetClient`] methods.
pub type NetFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, NetError>> + Send + 'a>>;

/// Direct DNS `A` and TCP port-25 SMTP banner probes implemented by `bench`.
pub trait NetClient: Send + Sync {
    /// Queries `A` records for `qname`, using `server` when set or the host's resolvers.
    fn dns_a<'a>(
        &'a self,
        qname: &'a str,
        server: Option<SocketAddr>,
        timeout: Duration,
    ) -> NetFuture<'a, DnsAnswer>;

    /// Connects over `family` to `host:port`, reads the initial SMTP reply code, sends `QUIT`
    /// and closes the connection without sending mail.
    fn smtp_banner<'a>(
        &'a self,
        family: IpFamily,
        host: &'a str,
        port: u16,
        timeout: Duration,
    ) -> NetFuture<'a, SmtpBanner>;

    /// Executes a direct IPv4 `GET /speed/dovalid?{query}` handshake against `ip:port`.
    fn taier_acquire<'a>(
        &'a self,
        _ip: Ipv4Addr,
        _port: u16,
        _query: &'a str,
        _timeout: Duration,
    ) -> NetFuture<'a, TaierAdmission> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Best-effort `POST /speed/dovalid?key={key}` releasing a node's queue slot.
    fn taier_release<'a>(
        &'a self,
        _ip: Ipv4Addr,
        _port: u16,
        _key: &'a str,
        _timeout: Duration,
    ) -> NetFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    /// Runs a multi-stream direct IPv4 HTTP/1.1 download throughput measurement.
    fn taier_download<'a>(
        &'a self,
        _spec: &'a TaierStreamSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierTransfer> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Runs a multi-stream direct IPv4 HTTP/1.1 upload throughput measurement.
    fn taier_upload<'a>(
        &'a self,
        _spec: &'a TaierStreamSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierTransfer> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Runs a Cloudflare multi-worker download throughput phase with loaded-latency probes.
    fn cloudflare_download<'a>(
        &'a self,
        _spec: &'a CloudflareTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareTransfer> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Runs a Cloudflare multi-worker upload throughput phase with loaded-latency probes.
    fn cloudflare_upload<'a>(
        &'a self,
        _spec: &'a CloudflareTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareTransfer> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Runs an RFC 5389 STUN UDP packet-loss probe against a Cloudflare STUN endpoint.
    fn cloudflare_udp_loss<'a>(
        &'a self,
        _spec: &'a CloudflareStunSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareStun> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Resolves `host` via the system resolver, filtered by `families`.
    fn inetspeed_resolve<'a>(
        &'a self,
        _host: &'a str,
        _families: FamilyFilter,
        _timeout: Duration,
    ) -> NetFuture<'a, Vec<IpAddr>> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Measures `count` sequential HTTP `GET` RTT samples against `target` (optionally IP-pinned).
    fn inetspeed_probe_rtt<'a>(
        &'a self,
        _target: &'a InetspeedTarget,
        _count: u8,
        _timeout: Duration,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, Vec<u64>> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    /// Runs an Apple CDN (`network.inetspeed`) download or upload round with loaded-latency probes.
    fn inetspeed_transfer<'a>(
        &'a self,
        _spec: &'a InetspeedTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, InetspeedTransfer> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }
}

struct NoNetClient;

impl NetClient for NoNetClient {
    fn dns_a<'a>(
        &'a self,
        _qname: &'a str,
        _server: Option<SocketAddr>,
        _timeout: Duration,
    ) -> NetFuture<'a, DnsAnswer> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }

    fn smtp_banner<'a>(
        &'a self,
        _family: IpFamily,
        _host: &'a str,
        _port: u16,
        _timeout: Duration,
    ) -> NetFuture<'a, SmtpBanner> {
        Box::pin(async {
            Err(NetError::Transport(
                "no network client configured".to_owned(),
            ))
        })
    }
}

/// A tool that a plugin ran: its command, how it ended, and what it printed last when it did
/// not exit with 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRun {
    /// The program's file name, then its arguments.
    pub command: Vec<String>,
    /// How the run ended.
    pub end: ToolEnd,
    /// Its last lines with text, oldest first, when it did not exit with 0 and its output may be
    /// quoted.
    pub tail: Vec<String>,
}

/// How a tool's run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolEnd {
    /// It exited with this code.
    Exit(i32),
    /// A signal ended it.
    Signal,
    /// It ran past this limit and was stopped.
    TimedOut(Duration),
    /// The plugin was cancelled, and the tool stopped.
    Cancelled,
    /// It could not start, for the system's reason.
    Spawn(String),
    /// What it printed could not be read, for the system's reason.
    Read(String),
}

/// What every result tells besides its own data.
pub trait Measurement: Serialize + Clone + Send + Sync + 'static {
    /// The tools used, with the versions they reported.
    fn tools(&self) -> Vec<ToolVersion>;
}

/// A test item, whose result has a type of its own.
pub trait Plugin: DeserializeOwned + Default + Send + Sync + 'static {
    /// The plugin's result.
    type Output: Measurement;
    /// Why the plugin skipped or failed.
    type Failure: Serialize + Send + 'static;
    /// Static facts about the plugin.
    const DESCRIPTOR: Descriptor;
    /// Dynamic host preconditions for this configured instance; defaults to
    /// [`Self::DESCRIPTOR`]'s requirements.
    fn host_requirements(&self, _host: &HostProfile) -> Requirements {
        Self::DESCRIPTOR.requires
    }
    /// Pinned tools needed by this configured instance on `host`; defaults to
    /// [`Self::DESCRIPTOR`]'s tools.
    fn tools_for(&self, _host: &HostProfile) -> Vec<ToolSpec> {
        Self::DESCRIPTOR.requires.tools.to_vec()
    }
    /// Whether the plugin runs when at least one tool in [`Self::tools_for`] is ready (`false` by
    /// default: every tool must be ready).
    fn partial_tools(&self) -> bool {
        false
    }
    /// The files it needs besides its tools, which may depend on its configuration; none by
    /// default. The orchestrator gets them before the first test.
    fn files(&self) -> Vec<FileSpec> {
        Vec::new()
    }
    /// Runs the measurement: the complete result, or why there is none.
    fn run(
        &self,
        ctx: &Context<Self::Output>,
    ) -> impl Future<Output = Result<Self::Output, Stop<Self::Failure>>> + Send;
}

/// What a plugin gets for one run.
pub struct Context<O> {
    host: Arc<HostProfile>,
    tools: Arc<BTreeMap<String, PathBuf>>,
    failed_tools: Arc<BTreeMap<String, String>>,
    files: Arc<Files>,
    http: Arc<dyn HttpClient>,
    net: Arc<dyn NetClient>,
    cancel: CancellationToken,
    deadline: Instant,
    work_dir: PathBuf,
    progress: UnboundedSender<Progress>,
    published: watch::Sender<Option<O>>,
}

/// The files that a plugin asked for, by name: the path of each that the orchestrator got, or why
/// it could not get it, which only an optional file can be when the plugin runs.
pub type Files = BTreeMap<String, Result<PathBuf, String>>;

/// The orchestrator's end of a [`Context`].
pub struct Channels<O> {
    /// The plugin's progress lines and tool runs, in order.
    pub progress: UnboundedReceiver<Progress>,
    /// The last result that the plugin published; `None` until it publishes one.
    pub published: watch::Receiver<Option<O>>,
}

impl<O: Measurement> Context<O> {
    /// A context for one run, with the tools that were provisioned and the files that the plugin
    /// asked for, and the orchestrator's end of its channels.
    #[must_use]
    pub fn new(
        host: Arc<HostProfile>,
        tools: Arc<BTreeMap<String, PathBuf>>,
        files: Arc<Files>,
        deadline: Instant,
        work_dir: PathBuf,
        cancel: CancellationToken,
    ) -> (Self, Channels<O>) {
        let (progress, progress_receiver) = mpsc::unbounded_channel();
        let (published, published_receiver) = watch::channel(None);
        let ctx = Self {
            host,
            tools,
            failed_tools: Arc::default(),
            files,
            http: Arc::new(NoHttpClient),
            net: Arc::new(NoNetClient),
            cancel,
            deadline,
            work_dir,
            progress,
            published,
        };
        let channels = Channels {
            progress: progress_receiver,
            published: published_receiver,
        };
        (ctx, channels)
    }

    /// The host, as the run read it before any plugin ran.
    #[must_use]
    pub fn host(&self) -> &HostProfile {
        &self.host
    }

    /// The path of a provisioned tool; `None` for a tool the plugin did not declare, or for a
    /// tool that failed to provision when [`Plugin::partial_tools`] is enabled.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&Path> {
        self.tools.get(name).map(PathBuf::as_path)
    }

    /// Why a tool failed to provision, when recorded by the orchestrator; `None` when the tool is
    /// ready or was not requested.
    #[must_use]
    pub fn missing_tool(&self, name: &str) -> Option<&str> {
        self.failed_tools.get(name).map(String::as_str)
    }

    /// Attaches the map of tool provisioning failure reasons for [`Self::missing_tool`].
    #[must_use]
    pub fn with_failed_tools(mut self, failed_tools: Arc<BTreeMap<String, String>>) -> Self {
        self.failed_tools = failed_tools;
        self
    }

    /// The path of a file that the plugin asked for, by its name; `None` for a file it did not
    /// ask for, and for an optional file that the orchestrator could not get ([`Self::missing`]
    /// says why). The orchestrator runs a plugin only when it has every file that is not optional.
    #[must_use]
    pub fn file(&self, name: &str) -> Option<&Path> {
        match self.files.get(name) {
            Some(Ok(path)) => Some(path),
            _ => None,
        }
    }

    /// Why the orchestrator could not get an optional file that the plugin asked for, by its name;
    /// `None` when it got the file, or the plugin did not ask for it.
    #[must_use]
    pub fn missing(&self, name: &str) -> Option<&str> {
        match self.files.get(name) {
            Some(Err(why)) => Some(why),
            _ => None,
        }
    }

    /// A directory of the plugin's own, removed after the run.
    #[must_use]
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Fires on a timeout and on an interrupt; a plugin stops soon after.
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// When the orchestrator stops waiting for the plugin.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// The step that the plugin starts, for the log on stderr. A step that arrives after the
    /// orchestrator stopped listening is dropped.
    pub fn progress(&self, step: Step) {
        let _ = self.progress.send(Progress::Step(step));
    }

    /// A tool that the plugin ran, for the log with `-v`; dropped like a step when nobody
    /// listens.
    pub fn ran(&self, run: ToolRun) {
        let _ = self.progress.send(Progress::Tool(run));
    }

    /// Attaches the direct HTTP client used by [`Self::probe`].
    #[must_use]
    pub fn with_http(mut self, http: Arc<dyn HttpClient>) -> Self {
        self.http = http;
        self
    }

    /// Attaches the DNS and SMTP socket client used by [`Self::dns_a`] and [`Self::smtp_banner`].
    #[must_use]
    pub fn with_net(mut self, net: Arc<dyn NetClient>) -> Self {
        self.net = net;
        self
    }

    /// Sends `request` over its IP family, stopping early when [`Self::cancel_token`] fires.
    pub async fn probe(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        if self.cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(HttpError::Cancelled),
            outcome = self.http.send(request) => outcome,
        }
    }

    /// Queries `A` records for `qname`, stopping early when [`Self::cancel_token`] fires.
    pub async fn dns_a(
        &self,
        qname: &str,
        server: Option<SocketAddr>,
        timeout: Duration,
    ) -> Result<DnsAnswer, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(NetError::Cancelled),
            outcome = self.net.dns_a(qname, server, timeout) => outcome,
        }
    }

    /// Reads the initial SMTP reply code from `host:port` over `family`, stopping early when
    /// [`Self::cancel_token`] fires.
    pub async fn smtp_banner(
        &self,
        family: IpFamily,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<SmtpBanner, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(NetError::Cancelled),
            outcome = self.net.smtp_banner(family, host, port, timeout) => outcome,
        }
    }

    /// Requests a queue lease on a Taier speedtest node, stopping early when cancelled.
    pub async fn taier_acquire(
        &self,
        ip: Ipv4Addr,
        port: u16,
        query: &str,
        timeout: Duration,
    ) -> Result<TaierAdmission, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(NetError::Cancelled),
            outcome = self.net.taier_acquire(ip, port, query, timeout) => outcome,
        }
    }

    /// Releases a queue lease on a Taier speedtest node (best effort).
    pub async fn taier_release(&self, ip: Ipv4Addr, port: u16, key: &str, timeout: Duration) {
        let _ = self.net.taier_release(ip, port, key, timeout).await;
    }

    /// Measures multi-stream download throughput from a Taier node.
    pub async fn taier_download(&self, spec: &TaierStreamSpec) -> Result<TaierTransfer, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net.taier_download(spec, &self.cancel).await
    }

    /// Measures multi-stream upload throughput to a Taier node.
    pub async fn taier_upload(&self, spec: &TaierStreamSpec) -> Result<TaierTransfer, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net.taier_upload(spec, &self.cancel).await
    }

    /// Runs a Cloudflare multi-worker download phase with concurrent loaded-latency probes.
    pub async fn cloudflare_download(
        &self,
        spec: &CloudflareTransferSpec,
    ) -> Result<CloudflareTransfer, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net.cloudflare_download(spec, &self.cancel).await
    }

    /// Runs a Cloudflare multi-worker upload phase with concurrent loaded-latency probes.
    pub async fn cloudflare_upload(
        &self,
        spec: &CloudflareTransferSpec,
    ) -> Result<CloudflareTransfer, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net.cloudflare_upload(spec, &self.cancel).await
    }

    /// Runs an RFC 5389 STUN UDP packet-loss probe for `network.cloudflare`.
    pub async fn cloudflare_udp_loss(
        &self,
        spec: &CloudflareStunSpec,
    ) -> Result<CloudflareStun, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net.cloudflare_udp_loss(spec, &self.cancel).await
    }

    /// Resolves `host` via the system DNS resolver, stopping early when cancelled.
    pub async fn inetspeed_resolve(
        &self,
        host: &str,
        families: FamilyFilter,
        timeout: Duration,
    ) -> Result<Vec<IpAddr>, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(NetError::Cancelled),
            outcome = self.net.inetspeed_resolve(host, families, timeout) => outcome,
        }
    }

    /// Measures `count` sequential HTTP RTT samples against an Apple CDN endpoint.
    pub async fn inetspeed_probe_rtt(
        &self,
        target: &InetspeedTarget,
        count: u8,
        timeout: Duration,
    ) -> Result<Vec<u64>, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net
            .inetspeed_probe_rtt(target, count, timeout, &self.cancel)
            .await
    }

    /// Runs an Apple CDN (`network.inetspeed`) download or upload transfer round.
    pub async fn inetspeed_transfer(
        &self,
        spec: &InetspeedTransferSpec,
    ) -> Result<InetspeedTransfer, NetError> {
        if self.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        self.net.inetspeed_transfer(spec, &self.cancel).await
    }

    /// Replaces the last partial result. The report shows the last one published when the
    /// plugin skips, fails, times out or is interrupted.
    pub fn publish(&self, output: &O) {
        self.published.send_replace(Some(output.clone()));
    }
}

/// Executes `tasks` with at most `limit` futures in flight at once, returning completed outputs in
/// input order. When `cancel` fires, in-flight futures are dropped immediately and remaining queued
/// tasks are never polled.
pub async fn run_bounded<'a, T, I, Fut>(
    limit: usize,
    cancel: &CancellationToken,
    tasks: I,
) -> Vec<T>
where
    I: IntoIterator<Item = Fut>,
    Fut: Future<Output = T> + Send + 'a,
{
    type BoxedTask<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

    if cancel.is_cancelled() {
        return Vec::new();
    }
    let cap = limit.max(1);
    let mut pending = tasks.into_iter().enumerate();
    let mut active: Vec<(usize, BoxedTask<'a, T>)> = Vec::with_capacity(cap);
    let mut done: Vec<(usize, T)> = Vec::new();
    loop {
        while !cancel.is_cancelled() && active.len() < cap {
            let Some((idx, fut)) = pending.next() else {
                break;
            };
            active.push((idx, Box::pin(fut)));
        }
        if active.is_empty() {
            break;
        }
        let next_done = std::future::poll_fn(|cx| {
            for (index, (_, fut)) in active.iter_mut().enumerate() {
                if let std::task::Poll::Ready(value) = fut.as_mut().poll(cx) {
                    return std::task::Poll::Ready((index, value));
                }
            }
            std::task::Poll::Pending
        });
        let (pos, value) = tokio::select! {
            biased;
            () = cancel.cancelled() => return Vec::new(),
            ready = next_done => ready,
        };
        let (orig_idx, _) = active.swap_remove(pos);
        done.push((orig_idx, value));
    }
    done.sort_by_key(|(idx, _)| *idx);
    done.into_iter().map(|(_, value)| value).collect()
}
