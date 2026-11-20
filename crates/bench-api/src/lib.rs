//! Plugin contract for lowendaff-bench, which every test-item crate implements. A plugin
//! declares what it needs, and returns a result of its own type, or its own reason to skip or to
//! fail; it publishes the result as it goes and reports its progress, and the orchestrator and
//! the reporter do the rest.

#![warn(missing_docs)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::future::Future;
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

/// A pinned external tool. Plugins that share a tool must declare identical specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Tool name, e.g. `fio`: the executable's file name, or the directory of an archive.
    pub name: &'static str,
    /// Upstream version the builds were made from.
    pub version: &'static str,
    /// One static build per architecture in [`RELEASE_ARCHES`].
    pub assets: &'static [ToolAsset],
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        location: &'static str,
        /// The country of the server's place, as its list names it.
        country: &'static str,
        /// Whether the test runs over IPv6, else over IPv4.
        ipv6: bool,
    },
    /// The download from a server of the network test.
    Download {
        /// The server's place, as its list names it.
        location: &'static str,
        /// The country of the server's place, as its list names it.
        country: &'static str,
        /// Whether the test runs over IPv6, else over IPv4.
        ipv6: bool,
    },
    /// The trace of a target of route tracing.
    Trace {
        /// The target's name, as its list names it, or the cache server's service.
        target: &'static str,
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

    /// The path of a provisioned tool; `None` for a tool the plugin did not declare, since the
    /// orchestrator runs a plugin only when every declared tool is ready.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&Path> {
        self.tools.get(name).map(PathBuf::as_path)
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

    /// Replaces the last partial result. The report shows the last one published when the
    /// plugin skips, fails, times out or is interrupted.
    pub fn publish(&self, output: &O) {
        self.published.send_replace(Some(output.clone()));
    }
}
