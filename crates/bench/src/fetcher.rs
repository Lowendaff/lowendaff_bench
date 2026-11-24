//! HTTPS download for tool and server-list provisioning: rustls with the ring provider only,
//! bundled Mozilla roots so minimal images without `ca-certificates` still work, and zero
//! reliance on environment proxy variables (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` are ignored
//! everywhere). Integrity comes from the pinned SHA-256, not from TLS.
//!
//! File and tool downloads (`family == None`) optionally route through explicit `--github-proxy`
//! (direct prefix mirror for GitHub hosts) and/or `--socks5-proxy` (`socks5://` or `socks5h://`),
//! while host-dependent probes (`family == Some(_)`, `network::probe`, `Probers`, `NetProbers`)
//! always connect directly.

use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use bench_api::IpFamily;
use reqwest::Client;
use reqwest::ClientBuilder;
use reqwest::Proxy;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::dns::Addrs;
use reqwest::dns::Name;
use reqwest::dns::Resolve;
use reqwest::dns::Resolving;
use reqwest::header;
use reqwest::header::HeaderValue;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::crypto::ring;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::locale;
use crate::provision::DownloadError;
use crate::provision::Part;

/// Which download route a [`Fetcher`] uses, determining the hint appended on connection failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyRoute {
    /// Direct download with no proxy configured for this URL.
    Direct,
    /// Download routed through `--socks5-proxy`.
    Socks5,
    /// GitHub download rewritten to `--github-proxy`.
    GithubMirror,
    /// Family-pinned probe (`family == Some(_)`), which never uses a download proxy.
    DirectProbe,
}

/// Validated SOCKS5 proxy URL (`socks5://` for client-side DNS, `socks5h://` for proxy-side DNS).
#[derive(Clone, PartialEq, Eq)]
pub struct Socks5Proxy {
    url: String,
}

impl std::fmt::Debug for Socks5Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Socks5Proxy")
            .field("url", &"<redacted>")
            .finish()
    }
}

impl Socks5Proxy {
    /// Validates a `socks5://` or `socks5h://` proxy URL without echoing credentials on error.
    pub fn try_new(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        let invalid = || locale::words().invalid_socks5_proxy();
        let Some((scheme, rest)) = trimmed.split_once("://") else {
            return Err(invalid());
        };
        let lower = scheme.to_ascii_lowercase();
        if lower != "socks5" && lower != "socks5h" {
            return Err(invalid());
        }
        let normalized = format!("{lower}://{rest}");
        let parsed = Url::parse(&normalized).map_err(|_| invalid())?;
        if parsed.host_str().is_none_or(str::is_empty)
            || !matches!(parsed.path(), "" | "/")
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(invalid());
        }
        let _ = Proxy::all(&normalized).map_err(|_| invalid())?;
        Ok(Self { url: normalized })
    }

    fn to_reqwest_proxy(&self) -> Result<Proxy, String> {
        Proxy::all(&self.url).map_err(|_| locale::words().invalid_socks5_proxy())
    }
}

/// Validated GitHub mirror prefix URL (e.g. `https://ghfast.top`), rewriting
/// `https://github.com/...` and `https://raw.githubusercontent.com/...` to
/// `<prefix>/https://github.com/...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubProxy {
    prefix: String,
}

impl GithubProxy {
    /// Validates an `http://` or `https://` GitHub proxy base URL and strips trailing slashes.
    pub fn try_new(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        let invalid = || locale::words().invalid_github_proxy();
        let parsed = Url::parse(trimmed).map_err(|_| invalid())?;
        if !matches!(parsed.scheme(), "https" | "http")
            || parsed.host_str().is_none_or(str::is_empty)
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(invalid());
        }
        let prefix = parsed.as_str().trim_end_matches('/').to_owned();
        Ok(Self { prefix })
    }

    /// Rewrites `target_url` to `<prefix>/<target_url>` when `target_url` is an HTTPS GitHub
    /// release, archive, or raw-file URL; returns `None` for non-GitHub URLs.
    #[must_use]
    pub fn rewrite(&self, target_url: &str) -> Option<String> {
        let parsed = parse_github_download_url(target_url)?;
        Some(format!("{}/{}", self.prefix, parsed.as_str()))
    }
}

/// Whether `url` points to a GitHub download host eligible for `--github-proxy` rewriting.
#[must_use]
pub fn is_github_download_url(url: &str) -> bool {
    parse_github_download_url(url).is_some()
}

fn parse_github_download_url(url: &str) -> Option<Url> {
    let parsed = Url::parse(url.trim()).ok()?;
    let eligible = parsed.scheme() == "https"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.fragment().is_none()
        && matches!(
            parsed.host_str(),
            Some(
                "github.com"
                    | "raw.githubusercontent.com"
                    | "objects.githubusercontent.com"
                    | "release-assets.githubusercontent.com"
                    | "codeload.github.com"
            )
        );
    eligible.then_some(parsed)
}

/// Optional download-only proxy settings (`--socks5-proxy` and `--github-proxy`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadProxy {
    /// Validated `socks5://` or `socks5h://` proxy for file/tool downloads.
    pub socks5: Option<Socks5Proxy>,
    /// Validated GitHub URL prefix mirror for GitHub file/tool downloads.
    pub github: Option<GithubProxy>,
}

impl DownloadProxy {
    /// Parses optional raw `--socks5-proxy` and `--github-proxy` inputs (treating empty or
    /// whitespace-only strings as unset).
    pub fn try_new(socks5: Option<&str>, github: Option<&str>) -> Result<Self, String> {
        let socks5 = match socks5.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Some(Socks5Proxy::try_new(raw)?),
            None => None,
        };
        let github = match github.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Some(GithubProxy::try_new(raw)?),
            None => None,
        };
        Ok(Self { socks5, github })
    }
}

type BodyReader = Box<dyn AsyncRead + Send + Unpin>;

/// HTTP client for tool and file downloads.
pub struct Fetcher {
    client: Client,
    route: ProxyRoute,
}

impl Fetcher {
    /// Builds a direct client that ignores environment proxy variables. Fails only if the TLS
    /// configuration is rejected.
    pub fn try_new() -> Result<Self, String> {
        Self::build(None, ProxyRoute::Direct)
    }

    /// A client that connects over `family` only, also to an address written in a URL, and
    /// without any proxy, which would connect over its own family and answer with its own address.
    /// For files whose answer depends on the host's address of that family.
    pub fn try_new_for(family: IpFamily) -> Result<Self, String> {
        Self::build_for(family, None)
    }

    /// `proxy`, which `.no_proxy()` must override, stands in for an unwanted proxy in tests.
    fn build_for(family: IpFamily, proxy: Option<Proxy>) -> Result<Self, String> {
        let local = match family {
            IpFamily::Ipv4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpFamily::Ipv6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        };
        let mut builder = timeouts(builder()?);
        if let Some(proxy) = proxy {
            builder = builder.proxy(proxy);
        }
        let client = builder
            .no_proxy()
            .local_address(local)
            .dns_resolver(Arc::new(PublicDnsResolver::new(Some(family))))
            .redirect(safe_redirect_policy(ProxyRoute::DirectProbe))
            .build()
            .map_err(|err| locale::words().http_client_failed(&err))?;
        Ok(Self {
            client,
            route: ProxyRoute::DirectProbe,
        })
    }

    fn build(proxy: Option<Proxy>, route: ProxyRoute) -> Result<Self, String> {
        let mut builder = timeouts(builder()?).redirect(safe_redirect_policy(route));
        if route == ProxyRoute::Direct {
            builder = builder.dns_resolver(Arc::new(PublicDnsResolver::new(None)));
        }
        if let Some(proxy) = proxy {
            builder = builder.proxy(proxy);
        }
        let client = builder
            .build()
            .map_err(|err| locale::words().http_client_failed(&err))?;
        Ok(Self { client, route })
    }

    /// Opens a download: the whole file, or the byte `range` of it. Non-2xx answers are errors,
    /// and so is a range other than the one asked for; a server that ignores the range sends
    /// the whole file. The body is streamed, so the caller can stop reading as soon as it grows
    /// past the pinned size. Network errors end in [`proxy_hint`]; HTTP errors do not, because
    /// the network worked. A request that got no answer, and the answers of a server that is
    /// busy or failing for the moment (408, 429 and 5xx), may go better on another try.
    pub async fn open(
        &self,
        url: &str,
        range: Option<Range<u64>>,
    ) -> Result<Part<impl AsyncRead + Unpin + use<>>, DownloadError> {
        use futures_util::TryStreamExt;

        let route = self.route;
        if matches!(route, ProxyRoute::Direct | ProxyRoute::DirectProbe) {
            let valid = Url::parse(url)
                .ok()
                .is_some_and(|parsed| is_permitted_direct_url(&parsed, true));
            if !valid {
                return Err(DownloadError::Final(
                    locale::words().request_failed(&"non-HTTPS or non-public download URL", ""),
                ));
            }
        }
        if range.as_ref().is_some_and(Range::is_empty) {
            let empty: BodyReader = Box::new(tokio::io::empty());
            return Ok(Part::Range(empty));
        }
        let mut request = self.client.get(url);
        if let Some(range) = &range {
            let bytes = format!("bytes={}-{}", range.start, range.end.saturating_sub(1));
            request = request.header(header::RANGE, bytes);
        }
        let response = request.send().await.map_err(|err| {
            DownloadError::Transient(
                locale::words().request_failed(&causes(&err), &proxy_hint(route)),
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let reason = locale::words().http_status(status.as_u16());
            let busy = status == StatusCode::REQUEST_TIMEOUT
                || status == StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error();
            return Err(if busy {
                DownloadError::Transient(reason)
            } else {
                DownloadError::Final(reason)
            });
        }
        let partial = status == StatusCode::PARTIAL_CONTENT;
        if partial
            && !holds(
                response.headers().get(header::CONTENT_RANGE),
                range.as_ref(),
            )
        {
            return Err(DownloadError::Final(locale::words().wrong_range()));
        }
        let read_hint = proxy_hint(route);
        let body = response
            .bytes_stream()
            .map_err(move |err| io::Error::other(format!("{}{read_hint}", causes(&err))));
        let reader: BodyReader = Box::new(StreamReader::new(Box::pin(body)));
        Ok(if partial {
            Part::Range(reader)
        } else {
            Part::Whole(reader)
        })
    }
}

/// Whether a `Content-Range` such as `bytes 0-1023/4096` holds exactly the `range` that was asked
/// for; never when none was.
fn holds(content_range: Option<&HeaderValue>, range: Option<&Range<u64>>) -> bool {
    let (Some(value), Some(range)) = (content_range, range) else {
        return false;
    };
    if range.start >= range.end {
        return false;
    }
    let Some(bytes) = value
        .to_str()
        .ok()
        .and_then(|text| text.strip_prefix("bytes "))
    else {
        return false;
    };
    let Some((span, _total)) = bytes.split_once('/') else {
        return false;
    };
    let Some((first, last)) = span.split_once('-') else {
        return false;
    };
    first.parse() == Ok(range.start)
        && last
            .parse::<u64>()
            .ok()
            .and_then(|last| last.checked_add(1))
            == Some(range.end)
}

fn parse_url_ip(url: &Url) -> Option<IpAddr> {
    url.host_str()?.trim_matches(['[', ']']).parse().ok()
}

fn is_permitted_direct_url(url: &Url, allow_loopback_http: bool) -> bool {
    let Some(host) = url.host_str().filter(|h| !h.is_empty()) else {
        return false;
    };
    let clean = host.trim_matches(['[', ']']);
    let host_ip = clean.parse::<IpAddr>().ok();
    match url.scheme() {
        "https" => match host_ip {
            Some(ip) => bench_api::is_routable_endpoint_ip(ip),
            None => {
                let lower = clean.to_ascii_lowercase();
                lower != "localhost" && !lower.ends_with(".localhost")
            }
        },
        "http" => allow_loopback_http && host_ip.is_some_and(|ip| ip.is_loopback()),
        _ => false,
    }
}

fn safe_redirect_policy(route: ProxyRoute) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let next = attempt.url();
        if let Some(prev) = attempt.previous().last()
            && prev.scheme() == "https"
            && next.scheme() != "https"
        {
            return attempt.error("refusing HTTPS downgrade redirect");
        }
        if matches!(route, ProxyRoute::Direct | ProxyRoute::DirectProbe) {
            let same_loopback_http = attempt.previous().first().is_some_and(|orig| {
                orig.scheme() == "http"
                    && next.scheme() == "http"
                    && parse_url_ip(orig).is_some_and(|ip| ip.is_loopback())
                    && parse_url_ip(next) == parse_url_ip(orig)
            });
            if !same_loopback_http && !is_permitted_direct_url(next, false) {
                return attempt.error("refusing redirect to non-public target");
            }
        }
        attempt.follow()
    })
}

/// The downloads' clients:
/// - `family == None`: GitHub URLs rewrite to `--github-proxy` via a direct client when configured;
///   all remaining `family == None` URLs use `--socks5-proxy` when configured, or direct otherwise.
/// - `family == Some(Ipv4 | Ipv6)`: always direct over that IP family alone, bypassing all proxies.
pub struct Fetchers {
    direct: Fetcher,
    github_direct: Fetcher,
    socks5: Option<Fetcher>,
    github: Option<GithubProxy>,
    ipv4: Fetcher,
    ipv6: Fetcher,
}

impl Fetchers {
    /// Builds direct clients with no download proxy configured.
    pub fn try_new() -> Result<Self, String> {
        Self::try_new_with_proxy(&DownloadProxy::default())
    }

    /// Builds download clients configured with explicit `proxy` settings (ignoring environment
    /// proxy variables).
    pub fn try_new_with_proxy(proxy: &DownloadProxy) -> Result<Self, String> {
        let socks5 = match &proxy.socks5 {
            Some(spec) => Some(Fetcher::build(
                Some(spec.to_reqwest_proxy()?),
                ProxyRoute::Socks5,
            )?),
            None => None,
        };
        Ok(Self {
            direct: Fetcher::build(None, ProxyRoute::Direct)?,
            github_direct: Fetcher::build(None, ProxyRoute::GithubMirror)?,
            socks5,
            github: proxy.github.clone(),
            ipv4: Fetcher::try_new_for(IpFamily::Ipv4)?,
            ipv6: Fetcher::try_new_for(IpFamily::Ipv6)?,
        })
    }

    /// Opens a download as [`Fetcher::open`] does: family-pinned probes connect directly over
    /// `family`, while general file/tool downloads apply `--github-proxy` and/or `--socks5-proxy`.
    pub async fn open(
        &self,
        family: Option<IpFamily>,
        url: &str,
        range: Option<Range<u64>>,
    ) -> Result<Part<impl AsyncRead + Unpin + use<>>, DownloadError> {
        match family {
            Some(IpFamily::Ipv4) => self.ipv4.open(url, range).await,
            Some(IpFamily::Ipv6) => self.ipv6.open(url, range).await,
            None => {
                if let Some(mirrored) = self.github.as_ref().and_then(|gh| gh.rewrite(url)) {
                    self.github_direct.open(&mirrored, range).await
                } else if let Some(socks5) = &self.socks5 {
                    socks5.open(url, range).await
                } else {
                    self.direct.open(url, range).await
                }
            }
        }
    }
}

/// The downloads' timeouts: a connection must open within 10 seconds, and a body must not stall
/// for 30.
fn timeouts(builder: ClientBuilder) -> ClientBuilder {
    builder
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
}

/// Cached rustls client configuration with bundled Mozilla roots and the `ring` crypto provider.
fn shared_tls_config() -> Result<ClientConfig, String> {
    use std::sync::OnceLock;

    static TLS: OnceLock<Result<ClientConfig, String>> = OnceLock::new();
    TLS.get_or_init(|| {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|err| locale::words().tls_failed(&err))
            .map(|cfg| cfg.with_root_certificates(roots).with_no_client_auth())
    })
    .clone()
}

/// DNS resolver for direct benchmark and upload clients that rejects private, loopback,
/// link-local, cloud-metadata, and reserved addresses on hostname lookups (optionally restricted
/// to one [`IpFamily`]).
pub struct PublicDnsResolver {
    family: Option<IpFamily>,
}

impl PublicDnsResolver {
    /// Creates a public-IP-only DNS resolver, optionally restricted to `family`.
    #[must_use]
    pub fn new(family: Option<IpFamily>) -> Self {
        Self { family }
    }
}

impl Resolve for PublicDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let family = self.family;
        Box::pin(async move {
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|addr| {
                    let family_ok = match family {
                        Some(IpFamily::Ipv4) => addr.is_ipv4(),
                        Some(IpFamily::Ipv6) => addr.is_ipv6(),
                        None => true,
                    };
                    family_ok && bench_api::is_routable_endpoint_ip(addr.ip())
                })
                .collect();
            if resolved.is_empty() {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "DNS resolved to no public routable address",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            let addrs: Addrs = Box::new(resolved.into_iter());
            Ok(addrs)
        })
    }
}

/// A client builder with the program's TLS configuration (rustls with ring, and the bundled
/// Mozilla roots), its user agent, and `.no_proxy()` so environment proxy variables are ignored
/// across both downloads and benchmark probes.
pub fn builder() -> Result<ClientBuilder, String> {
    let tls = shared_tls_config()?;
    Ok(Client::builder()
        .no_proxy()
        .tls_backend_preconfigured(tls)
        .user_agent(concat!("lowendaff-bench/", env!("CARGO_PKG_VERSION"))))
}

/// An error with its causes, each after a colon, as in "error sending request for url (...):
/// client error (Connect): tcp connect error: Connection refused (os error 111)": reqwest's own
/// message names none of them, such as a timeout, a refused connection or a failed name lookup.
/// A cause whose text the message holds already is left out.
#[must_use]
pub fn causes(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        let said = next.to_string();
        if !text.contains(&said) {
            text.push_str(": ");
            text.push_str(&said);
        }
        cause = next.source();
    }
    text
}

/// The end of a network error's reason: which explicit proxy flag was active for this download
/// (never echoing its URL or credentials), or how to pass one.
#[must_use]
pub fn proxy_hint(route: ProxyRoute) -> String {
    match route {
        ProxyRoute::Direct => locale::words().proxy_unset(),
        ProxyRoute::Socks5 => locale::words().proxy_set("--socks5-proxy"),
        ProxyRoute::GithubMirror => locale::words().proxy_set("--github-proxy"),
        ProxyRoute::DirectProbe => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::sync::Mutex;

    use reqwest::Proxy;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    use bench_api::IpFamily;

    use super::DownloadProxy;
    use super::Fetcher;
    use super::Fetchers;
    use super::GithubProxy;
    use super::ProxyRoute;
    use super::Socks5Proxy;
    use crate::provision::DownloadError;
    use crate::provision::Part;

    /// Answers every request on a local port with `body` over plain HTTP (or its requested byte
    /// range), recording each requested HTTP path into `paths`.
    async fn serve_recording(body: &'static [u8], paths: Arc<Mutex<Vec<String>>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a local test server");
        let address = listener
            .local_addr()
            .expect("bound listener has an address");
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = vec![0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap_or(0);
                let raw_req = String::from_utf8_lossy(&request[..read]);
                if let Some(first_line) = raw_req.lines().next()
                    && let Some(path) = first_line.split_whitespace().nth(1)
                {
                    paths.lock().expect("paths").push(path.to_owned());
                }
                let lower = raw_req.to_ascii_lowercase();
                let range = lower
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .and_then(|span| span.trim().split_once('-'))
                    .and_then(|(first, last)| Some((first.parse().ok()?, last.parse().ok()?)));
                let (status, extra, part) = match range {
                    Some((first, last)) => (
                        "206 Partial Content",
                        format!("Content-Range: bytes {first}-{last}/{}\r\n", body.len()),
                        &body[first..=last],
                    ),
                    None => ("200 OK", String::new(), body),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    part.len()
                );
                let _ = socket.write_all(&[head.as_bytes(), part].concat()).await;
            }
        });
        address.to_string()
    }

    async fn serve(body: &'static [u8]) -> String {
        serve_recording(body, Arc::new(Mutex::new(Vec::new()))).await
    }

    /// A SOCKS5 proxy without authentication that records `(atyp, target)` of each CONNECT
    /// (`atyp == 1` for client-resolved IPv4 `socks5://`, `atyp == 3` for proxy-resolved domain
    /// `socks5h://`) and relays the bytes to `127.0.0.1:<port>` when `host == "localhost"`.
    async fn socks5(targets: Arc<Mutex<Vec<(u8, String)>>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a local SOCKS5 proxy");
        let address = listener
            .local_addr()
            .expect("bound listener has an address");
        tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                let mut greeting = [0_u8; 2];
                client.read_exact(&mut greeting).await.expect("greeting");
                let mut methods = vec![0_u8; usize::from(greeting[1])];
                client.read_exact(&mut methods).await.expect("methods");
                client
                    .write_all(&[5, 0])
                    .await
                    .expect("choose no authentication");
                let mut request = [0_u8; 4];
                client.read_exact(&mut request).await.expect("request");
                let atyp = request[3];
                let host = if atyp == 1 {
                    let mut ip = [0_u8; 4];
                    client.read_exact(&mut ip).await.expect("IPv4 address");
                    Ipv4Addr::from(ip).to_string()
                } else {
                    let length = client.read_u8().await.expect("name length");
                    let mut name = vec![0_u8; usize::from(length)];
                    client.read_exact(&mut name).await.expect("name");
                    String::from_utf8(name).expect("ASCII name")
                };
                let port = client.read_u16().await.expect("port");
                let recorded = format!("{host}:{port}");
                targets
                    .lock()
                    .expect("targets")
                    .push((atyp, recorded.clone()));
                let dial_host = if host == "localhost" {
                    "127.0.0.1"
                } else {
                    host.as_str()
                };
                let mut upstream = TcpStream::connect(format!("{dial_host}:{port}"))
                    .await
                    .expect("reach the target");
                client
                    .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                    .await
                    .expect("reply");
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            }
        });
        address.to_string()
    }

    #[test]
    fn socks5_and_github_proxy_parsers_validate_schemes_and_rewrite_github_urls() {
        assert!(Socks5Proxy::try_new("socks5://127.0.0.1:1080").is_ok());
        let with_secret = Socks5Proxy::try_new("SOCKS5H://user:s3cret@proxy.example.com:1080")
            .expect("valid authenticated socks5h");
        let debug_repr = format!("{with_secret:?}");
        assert!(
            !debug_repr.contains("s3cret") && debug_repr.contains("<redacted>"),
            "Debug impl must redact credentials: {debug_repr}"
        );
        for bad in [
            "http://127.0.0.1:1080",
            "https://127.0.0.1:1080",
            "socks4://127.0.0.1:1080",
            "127.0.0.1:1080",
            "socks5://127.0.0.1:1080/path",
            "socks5://user:s3cret@127.0.0.1:1080?x=1",
        ] {
            let err = Socks5Proxy::try_new(bad).expect_err("invalid SOCKS5 URL must fail");
            assert!(
                !err.contains("s3cret"),
                "validation error must never leak credentials: {err}"
            );
        }

        let gh_slash = GithubProxy::try_new("https://ghfast.top/").expect("valid trailing slash");
        let gh_bare = GithubProxy::try_new("https://ghfast.top").expect("valid bare origin");
        assert_eq!(
            gh_slash
                .rewrite("https://github.com/stilleshan/dockerfiles")
                .as_deref(),
            Some("https://ghfast.top/https://github.com/stilleshan/dockerfiles")
        );
        assert_eq!(
            gh_bare
                .rewrite("https://github.com/stilleshan/dockerfiles")
                .as_deref(),
            Some("https://ghfast.top/https://github.com/stilleshan/dockerfiles")
        );
        let raw_file = "https://raw.githubusercontent.com/stilleshan/dockerfiles/main/README.md";
        assert_eq!(
            gh_slash.rewrite(raw_file).as_deref(),
            Some(concat!(
                "https://ghfast.top/",
                "https://raw.githubusercontent.com/stilleshan/dockerfiles/main/README.md"
            ))
        );
        assert_eq!(
            gh_slash.rewrite("https://cdn.geekbench.com/Geekbench-7.1.0-Linux.tar.gz"),
            None,
            "non-GitHub URLs must never be rewritten to the GitHub mirror"
        );
        assert_eq!(
            gh_slash.rewrite("https://user@github.com/stilleshan/dockerfiles"),
            None,
            "userinfo on GitHub URLs must be rejected"
        );
        assert_eq!(
            gh_slash.rewrite("https://github.com:8443/stilleshan/dockerfiles"),
            None,
            "non-standard ports on GitHub URLs must be rejected"
        );
        let max_range = reqwest::header::HeaderValue::from_static(
            "bytes 0-18446744073709551615/18446744073709551615",
        );
        assert!(
            !super::holds(Some(&max_range), Some(&(0..1))),
            "u64::MAX Content-Range end must not overflow"
        );
        for bad in [
            "socks5://127.0.0.1:1080",
            "ghfast.top",
            "https://user:s3cret@ghfast.top/",
            "https://ghfast.top/?x=1",
        ] {
            let err = GithubProxy::try_new(bad).expect_err("invalid GitHub proxy URL must fail");
            assert!(
                !err.contains("s3cret"),
                "validation error must never leak credentials: {err}"
            );
        }
    }

    #[tokio::test]
    async fn downloads_support_both_socks5_and_socks5h_wire_modes() {
        let site = serve(b"tool").await;
        let port = site.split_once(':').expect("host:port").1.to_owned();
        let targets = Arc::new(Mutex::new(Vec::new()));
        let proxy = socks5(Arc::clone(&targets)).await;
        let url = format!("http://localhost:{port}/tool");

        // 1. socks5:// resolves `localhost` locally and sends IPv4 literal (ATYP = 1) or IPv6.
        let cfg_v5 = DownloadProxy::try_new(Some(&format!("socks5://{proxy}")), None).unwrap();
        let fetchers_v5 = Fetchers::try_new_with_proxy(&cfg_v5).expect("clients");
        let Ok(Part::Whole(mut body)) = fetchers_v5
            .open(None, &format!("http://{site}/tool"), None)
            .await
        else {
            panic!("socks5:// download should succeed")
        };
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes).await.expect("body");
        assert_eq!(bytes, b"tool");

        // 2. socks5h:// delegates hostname resolution to the SOCKS5 proxy (ATYP = 3, "localhost").
        let cfg_v5h = DownloadProxy::try_new(Some(&format!("socks5h://{proxy}")), None).unwrap();
        let fetchers_v5h = Fetchers::try_new_with_proxy(&cfg_v5h).expect("clients");
        let Ok(Part::Range(mut body)) = fetchers_v5h.open(None, &url, Some(1..3)).await else {
            panic!("socks5h:// byte-range download should succeed")
        };
        let mut range_bytes = Vec::new();
        body.read_to_end(&mut range_bytes).await.expect("body");
        assert_eq!(range_bytes, b"oo");

        let seen = targets.lock().expect("targets").clone();
        assert_eq!(
            seen,
            vec![(1, site), (3, format!("localhost:{port}"))],
            "socks5:// sends IP literal (ATYP 1) while socks5h:// sends domain (ATYP 3)"
        );
    }

    #[tokio::test]
    async fn a_client_of_one_family_connects_over_it_alone_and_through_no_proxy() {
        let site = serve(b"list").await;
        let url = format!("http://{site}/list");
        let targets = Arc::new(Mutex::new(Vec::new()));
        let proxy = socks5(Arc::clone(&targets)).await;
        let socks = Proxy::all(format!("socks5h://{proxy}")).expect("a proxy address");
        let four = Fetcher::build_for(IpFamily::Ipv4, Some(socks)).expect("client");

        let Ok(Part::Whole(mut body)) = four.open(&url, None).await else {
            panic!("the IPv4 client reaches the IPv4 address")
        };
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes).await.expect("body");
        assert_eq!(bytes, b"list");
        assert!(
            targets.lock().expect("targets").is_empty(),
            "not through the proxy"
        );

        let six = Fetcher::try_new_for(IpFamily::Ipv6).expect("client");
        assert!(
            matches!(six.open(&url, None).await, Err(DownloadError::Transient(_))),
            "the IPv6 client does not reach an IPv4 address"
        );

        let Ok(Part::Range(mut empty_range)) = four.open(&url, Some(2..2)).await else {
            panic!("empty byte range should short-circuit as empty Part::Range")
        };
        let mut empty_bytes = Vec::new();
        empty_range
            .read_to_end(&mut empty_bytes)
            .await
            .expect("empty");
        assert!(empty_bytes.is_empty());

        assert!(
            matches!(
                four.open("http://10.0.0.1/list", None).await,
                Err(DownloadError::Final(_))
            ),
            "non-loopback plain HTTP must be rejected on DirectProbe"
        );
    }

    #[tokio::test]
    async fn split_routing_keeps_github_mirror_socks5_and_direct_probes_separate() {
        let gh_paths = Arc::new(Mutex::new(Vec::new()));
        let gh_mirror = serve_recording(b"gh-asset", Arc::clone(&gh_paths)).await;

        let other_site = serve(b"vendor-asset").await;
        let probe_site = serve(b"cache-map").await;

        let socks_targets = Arc::new(Mutex::new(Vec::new()));
        let socks_addr = socks5(Arc::clone(&socks_targets)).await;

        let proxy_cfg = DownloadProxy::try_new(
            Some(&format!("socks5h://{socks_addr}")),
            Some(&format!("http://{gh_mirror}/")),
        )
        .expect("valid dual proxy config");
        let fetchers = Fetchers::try_new_with_proxy(&proxy_cfg).expect("clients");

        // 1. GitHub download (`family == None`) goes DIRECT to `gh_mirror` (NOT through SOCKS5!).
        let gh_url =
            "https://github.com/Lowendaff/bench_data/releases/download/fio-3.43-2/fio-x86_64";
        let Ok(Part::Whole(mut gh_body)) = fetchers.open(None, gh_url, None).await else {
            panic!("GitHub mirror download should succeed")
        };
        let mut gh_bytes = Vec::new();
        gh_body.read_to_end(&mut gh_bytes).await.unwrap();
        assert_eq!(gh_bytes, b"gh-asset");
        assert_eq!(
            *gh_paths.lock().unwrap(),
            vec![format!("/{gh_url}")],
            "mirror received the full upstream GitHub URL path"
        );
        assert!(
            socks_targets.lock().unwrap().is_empty(),
            "GitHub mirror traffic must bypass SOCKS5"
        );

        // 2. Non-GitHub download (`family == None`) routes through SOCKS5!
        let vendor_url = format!("http://{other_site}/vendor.zip");
        let Ok(Part::Whole(mut vendor_body)) = fetchers.open(None, &vendor_url, None).await else {
            panic!("non-GitHub download via SOCKS5 should succeed")
        };
        let mut vendor_bytes = Vec::new();
        vendor_body.read_to_end(&mut vendor_bytes).await.unwrap();
        assert_eq!(vendor_bytes, b"vendor-asset");
        assert_eq!(
            *socks_targets.lock().unwrap(),
            vec![(1, other_site)],
            "non-GitHub download went through SOCKS5"
        );

        // 3. Family-pinned probe (`family == Some(Ipv4)`) bypasses BOTH GitHub mirror and SOCKS5!
        let probe_url = format!("http://{probe_site}/report_mapping");
        let Ok(Part::Whole(mut probe_body)) =
            fetchers.open(Some(IpFamily::Ipv4), &probe_url, None).await
        else {
            panic!("IPv4 probe should connect directly")
        };
        let mut probe_bytes = Vec::new();
        probe_body.read_to_end(&mut probe_bytes).await.unwrap();
        assert_eq!(probe_bytes, b"cache-map");
        assert_eq!(
            socks_targets.lock().unwrap().len(),
            1,
            "family probe did not touch SOCKS5"
        );
        assert_eq!(
            gh_paths.lock().unwrap().len(),
            1,
            "family probe did not touch GitHub mirror"
        );

        // 4. Proxy hints reflect active route without exposing URLs or credentials.
        assert_eq!(
            super::proxy_hint(ProxyRoute::Direct),
            "; pass --socks5-proxy or --github-proxy to download through a proxy"
        );
        assert_eq!(
            super::proxy_hint(ProxyRoute::Socks5),
            "; --socks5-proxy is set: check that the proxy works"
        );
        assert_eq!(
            super::proxy_hint(ProxyRoute::GithubMirror),
            "; --github-proxy is set: check that the proxy works"
        );
        assert_eq!(super::proxy_hint(ProxyRoute::DirectProbe), "");
    }
}
