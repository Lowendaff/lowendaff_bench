//! Direct, proxy-free HTTP probing over each IP family, for plugins that inspect HTTP status
//! codes, redirects, headers and payloads as outside services serve them to this host.

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpMethod;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::RedirectPolicy;
use reqwest::Client;
use reqwest::Proxy;
use reqwest::Url;
use reqwest::dns::Addrs;
use reqwest::dns::Name;
use reqwest::dns::Resolve;
use reqwest::dns::Resolving;
use reqwest::header;
use reqwest::redirect::Policy;

use crate::fetcher;
use crate::locale;

/// Desktop browser User-Agent sent unless the request names one of its own.
const BROWSER_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
);

/// Default `Accept-Language` sent unless the request names one of its own.
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

/// Time allowed for the TCP and TLS handshake of one hop.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Maximum cached hostname pin entries per family resolver before eviction.
const MAX_PINNED_HOSTS: usize = 256;

/// How long a validated hostname pin stays fresh before [`ensure_target`] re-queries DNS.
const PIN_TTL: Duration = Duration::from_secs(30);

/// Family-scoped DNS resolver that serves pre-validated addresses pinned by [`ensure_target`],
/// eliminating double-lookup DNS rebinding windows and repeated `getaddrinfo` latency during burst
/// probes while rejecting non-public addresses on fallback.
struct SafeFamilyResolver {
    ipv6: bool,
    pinned: Mutex<HashMap<String, (Instant, Vec<IpAddr>)>>,
}

impl SafeFamilyResolver {
    fn new(ipv6: bool) -> Self {
        Self {
            ipv6,
            pinned: Mutex::new(HashMap::new()),
        }
    }

    fn store(&self, host: &str, ips: Vec<IpAddr>) {
        if let Ok(mut guard) = self.pinned.lock() {
            if guard.len() >= MAX_PINNED_HOSTS {
                guard.clear();
            }
            guard.insert(host.to_ascii_lowercase(), (Instant::now(), ips));
        }
    }

    fn has_fresh(&self, host: &str) -> bool {
        self.pinned
            .lock()
            .ok()
            .and_then(|guard| guard.get(&host.to_ascii_lowercase()).map(|(at, _)| *at))
            .is_some_and(|at| at.elapsed() < PIN_TTL)
    }

    fn load(&self, host: &str) -> Option<Vec<IpAddr>> {
        self.pinned.lock().ok().and_then(|guard| {
            guard
                .get(&host.to_ascii_lowercase())
                .filter(|(at, _)| at.elapsed() < PIN_TTL)
                .map(|(_, ips)| ips.clone())
        })
    }
}

impl Resolve for SafeFamilyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let ipv6 = self.ipv6;
        let cached = self.load(&host);
        Box::pin(async move {
            if let Some(ips) = cached {
                let addrs: Addrs = Box::new(ips.into_iter().map(|ip| SocketAddr::new(ip, 0)));
                return Ok(addrs);
            }
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|addr| {
                    addr.is_ipv6() == ipv6 && bench_api::is_routable_endpoint_ip(addr.ip())
                })
                .collect();
            if resolved.is_empty() {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "DNS resolved to no public address for family",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            let addrs: Addrs = Box::new(resolved.into_iter());
            Ok(addrs)
        })
    }
}

/// Direct HTTP clients bound to IPv4 and IPv6, bypassing environment proxies.
pub struct Probers {
    ipv4: Client,
    ipv6: Client,
    v4_dns: Arc<SafeFamilyResolver>,
    v6_dns: Arc<SafeFamilyResolver>,
}

impl Probers {
    /// Builds both clients; fails only if the TLS configuration is rejected.
    pub fn try_new() -> Result<Self, String> {
        let v4_dns = Arc::new(SafeFamilyResolver::new(false));
        let v6_dns = Arc::new(SafeFamilyResolver::new(true));
        Ok(Self {
            ipv4: build_for(IpFamily::Ipv4, None, Arc::clone(&v4_dns))?,
            ipv6: build_for(IpFamily::Ipv6, None, Arc::clone(&v6_dns))?,
            v4_dns,
            v6_dns,
        })
    }

    /// Runs `request` over its IP family.
    pub async fn probe(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let (client, resolver) = match request.family {
            IpFamily::Ipv4 => (&self.ipv4, &self.v4_dns),
            IpFamily::Ipv6 => (&self.ipv6, &self.v6_dns),
        };
        let ipv6 = request.family == IpFamily::Ipv6;
        let method = method_text(request.method);
        let first_host = host_label(&request.url);
        let outcome =
            match tokio::time::timeout(request.timeout, execute(client, resolver, request)).await {
                Ok(result) => result,
                Err(_) => Err(HttpError::TimedOut),
            };
        let summary = match &outcome {
            Ok(response) => format!("HTTP {}", response.status),
            Err(error) => error_summary(error),
        };
        tracing::debug!(
            "{}",
            locale::words().http_probe(method, &first_host, ipv6, &summary)
        );
        outcome
    }
}

impl HttpClient for Probers {
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move { self.probe(request).await })
    }
}

/// Builds a direct client for `family`; `proxy`, which `.no_proxy()` must override, stands in for
/// the environment's in tests.
fn build_for(
    family: IpFamily,
    proxy: Option<Proxy>,
    resolver: Arc<SafeFamilyResolver>,
) -> Result<Client, String> {
    let local = match family {
        IpFamily::Ipv4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpFamily::Ipv6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    let mut builder = fetcher::builder()?
        .user_agent(BROWSER_USER_AGENT)
        .redirect(Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .dns_resolver(resolver);
    if let Some(proxy) = proxy {
        builder = builder.proxy(proxy);
    }
    builder
        .no_proxy()
        .local_address(local)
        .build()
        .map_err(|err| locale::words().http_client_failed(&err))
}

async fn execute(
    client: &Client,
    resolver: &SafeFamilyResolver,
    request: &HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let mut url = parse_allowed(&request.url, false)?;
    let initial_loopback = url.scheme() == "http" && is_loopback(&url);
    let initial_origin = (
        url.scheme().to_owned(),
        url.host_str().unwrap_or_default().to_ascii_lowercase(),
        url.port_or_known_default(),
    );
    let mut method = request.method;
    let mut body = request.body.clone();
    let mut hops: u8 = 0;
    let ipv6 = request.family == IpFamily::Ipv6;

    loop {
        let allow_loopback = initial_loopback && url.scheme() == "http";
        ensure_target(&url, ipv6, allow_loopback, hops > 0, resolver).await?;
        let mut builder = match method {
            HttpMethod::Get => client.get(url.clone()),
            HttpMethod::Post => client.post(url.clone()),
        };
        let has_accept_language = request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("accept-language"));
        if !has_accept_language {
            builder = builder.header(header::ACCEPT_LANGUAGE, ACCEPT_LANGUAGE);
        }
        let same_origin = url.scheme() == initial_origin.0
            && url
                .host_str()
                .is_some_and(|h| h.eq_ignore_ascii_case(&initial_origin.1))
            && url.port_or_known_default() == initial_origin.2;
        for (name, value) in &request.headers {
            let lower = name.to_ascii_lowercase();
            if !same_origin
                && matches!(
                    lower.as_str(),
                    "authorization" | "cookie" | "proxy-authorization" | "host"
                )
            {
                continue;
            }
            if body.is_none()
                && matches!(
                    lower.as_str(),
                    "content-type" | "content-length" | "content-encoding" | "transfer-encoding"
                )
            {
                continue;
            }
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(payload) = &body {
            builder = builder.body(payload.clone());
        }
        let mut response = builder.send().await.map_err(map_reqwest_err)?;
        let status = response.status().as_u16();
        if let RedirectPolicy::Follow { max } = request.redirect
            && is_redirect(status)
        {
            let Some(location) = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
            else {
                return Err(HttpError::InvalidRedirect(
                    "missing Location header".to_owned(),
                ));
            };
            if hops >= max {
                return Err(HttpError::TooManyRedirects);
            }
            let next = url
                .join(location)
                .map_err(|err| HttpError::InvalidRedirect(err.to_string()))?;
            let next = parse_allowed(next.as_str(), url.scheme() == "https")?;
            if matches!(status, 301..=303) && method == HttpMethod::Post {
                method = HttpMethod::Get;
                body = None;
            }
            url = next;
            hops += 1;
            continue;
        }

        let mut headers = Vec::with_capacity(response.headers().len());
        for (name, value) in response.headers() {
            if let Ok(text) = value.to_str() {
                headers.push((name.as_str().to_ascii_lowercase(), text.to_owned()));
            }
        }
        let mut bytes = Vec::new();
        while bytes.len() < request.max_bytes {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    let remaining = request.max_bytes - bytes.len();
                    let take = remaining.min(chunk.len());
                    bytes.extend_from_slice(&chunk[..take]);
                }
                Ok(None) => break,
                Err(err) => return Err(map_reqwest_err(err)),
            }
        }
        return Ok(HttpResponse {
            status,
            final_url: url.into(),
            headers,
            body: bytes,
        });
    }
}

fn map_reqwest_err(err: reqwest::Error) -> HttpError {
    if err.is_timeout() {
        return HttpError::TimedOut;
    }
    let stripped = err.without_url();
    HttpError::Transport(fetcher::causes(&stripped))
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Accepts `https://` URLs, or loopback `http://127.0.0.1` and `http://[::1]` when `from_https`
/// is `false`. Redirecting from `https://` to plain `http://` is always refused.
fn parse_allowed(raw: &str, from_https: bool) -> Result<Url, HttpError> {
    let parsed = Url::parse(raw).map_err(|err| {
        if from_https {
            HttpError::InvalidRedirect(err.to_string())
        } else {
            HttpError::Transport(err.to_string())
        }
    })?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if !from_https && is_loopback(&parsed) => Ok(parsed),
        _ if from_https => Err(HttpError::InvalidRedirect(
            "redirect to non-HTTPS URL".to_owned(),
        )),
        _ => Err(HttpError::NotHttps(host_label(raw))),
    }
}

fn is_loopback(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|addr| addr.is_loopback())
}

/// Checks that `url` resolves to at least one permitted address of `ipv6`'s family, rejecting
/// private, loopback, link-local, metadata, and reserved addresses (except explicit loopback
/// `http://` test URLs when `allow_loopback` is set) and pinning resolved host IPs into
/// `resolver`.
async fn ensure_target(
    url: &Url,
    ipv6: bool,
    allow_loopback: bool,
    is_redirect: bool,
    resolver: &SafeFamilyResolver,
) -> Result<(), HttpError> {
    let non_public_err = |why: &str| {
        if is_redirect {
            HttpError::InvalidRedirect(why.to_owned())
        } else {
            HttpError::Transport(why.to_owned())
        }
    };
    let host = url
        .host_str()
        .ok_or_else(|| non_public_err("URL has no host"))?
        .trim_matches(['[', ']']);
    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip.is_ipv6() != ipv6 {
            return Err(HttpError::NoAddress);
        }
        let permitted =
            (allow_loopback && ip.is_loopback()) || bench_api::is_routable_endpoint_ip(ip);
        return if permitted {
            Ok(())
        } else {
            Err(non_public_err("non-public target address refused"))
        };
    }
    if resolver.has_fresh(host) {
        return Ok(());
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| non_public_err("unknown port"))?;
    let family_addrs: Vec<IpAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| HttpError::Transport(err.to_string()))?
        .filter(|address| address.is_ipv6() == ipv6)
        .map(|address| address.ip())
        .collect();
    if family_addrs.is_empty() {
        return Err(HttpError::NoAddress);
    }
    let mut safe_ips = Vec::with_capacity(family_addrs.len());
    for ip in family_addrs {
        if bench_api::is_routable_endpoint_ip(ip) && !safe_ips.contains(&ip) {
            safe_ips.push(ip);
        }
    }
    if safe_ips.is_empty() {
        return Err(non_public_err("DNS resolved to non-public address"));
    }
    resolver.store(host, safe_ips);
    Ok(())
}

fn method_text(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
    }
}

fn host_label(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
        .unwrap_or_else(|| "<invalid>".to_owned())
}

fn error_summary(error: &HttpError) -> String {
    match error {
        HttpError::NoAddress => "no address for family".to_owned(),
        HttpError::TimedOut => "timed out".to_owned(),
        HttpError::TooManyRedirects => "too many redirects".to_owned(),
        HttpError::InvalidRedirect(why) => format!("invalid redirect: {why}"),
        HttpError::NotHttps(_) => "not HTTPS".to_owned(),
        HttpError::Transport(why) => why.clone(),
        HttpError::Cancelled => "interrupted".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::time::Duration;

    use bench_api::HttpError;
    use bench_api::HttpRequest;
    use bench_api::IpFamily;
    use bench_api::RedirectPolicy;
    use reqwest::Proxy;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    use super::Probers;

    async fn serve_redirects() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback server");
        let address = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0_u8; 4096];
                let read = socket.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..read]);
                let first_line = req.lines().next().unwrap_or_default();
                let reply = if first_line.starts_with("POST /step1 ") {
                    "HTTP/1.1 302 Found\r\nLocation: /step2\r\nContent-Length: 0\r\n\r\n".to_owned()
                } else if first_line.starts_with("GET /step2 ") {
                    "HTTP/1.1 200 OK\r\nX-Region: US\r\nContent-Length: 8\r\n\r\n12345678"
                        .to_owned()
                } else if first_line.starts_with("GET /loop ") {
                    "HTTP/1.1 302 Found\r\nLocation: /loop\r\nContent-Length: 0\r\n\r\n".to_owned()
                } else if first_line.starts_with("GET /ssrf-private ") {
                    "HTTP/1.1 302 Found\r\nLocation: https://10.0.0.1/secret\r\n\
                     Content-Length: 0\r\n\r\n"
                        .to_owned()
                } else if first_line.starts_with("GET /ssrf-loopback-https ") {
                    "HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:8443/admin\r\n\
                     Content-Length: 0\r\n\r\n"
                        .to_owned()
                } else if first_line.starts_with("GET /ssrf-imds ") {
                    "HTTP/1.1 302 Found\r\nLocation: https://169.254.169.254/latest/meta-data\r\n\
                     Content-Length: 0\r\n\r\n"
                        .to_owned()
                } else {
                    concat!(
                        "HTTP/1.1 403 Forbidden\r\nCF-Mitigated: challenge\r\n",
                        "Content-Length: 2\r\n\r\nno"
                    )
                    .to_owned()
                };
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        address.to_string()
    }

    #[tokio::test]
    async fn probers_bind_family_ignore_proxy_handle_redirects_and_truncate_bodies() {
        let site = serve_redirects().await;
        let proxy = Proxy::all("socks5h://127.0.0.1:9").expect("proxy URL");
        let v4_dns = std::sync::Arc::new(super::SafeFamilyResolver::new(false));
        let v6_dns = std::sync::Arc::new(super::SafeFamilyResolver::new(true));
        let probers = Probers {
            ipv4: super::build_for(IpFamily::Ipv4, Some(proxy), std::sync::Arc::clone(&v4_dns))
                .expect("v4 client"),
            ipv6: super::build_for(IpFamily::Ipv6, None, std::sync::Arc::clone(&v6_dns))
                .expect("v6 client"),
            v4_dns,
            v6_dns,
        };

        // Non-HTTPS outside loopback is refused.
        let plain = HttpRequest::get(IpFamily::Ipv4, "http://example.com/test");
        assert!(matches!(
            probers.probe(&plain).await,
            Err(HttpError::NotHttps(_))
        ));

        // IPv6 prober refuses an IPv4 address with NoAddress.
        let wrong_family = HttpRequest::get(IpFamily::Ipv6, format!("http://{site}/step2"));
        assert_eq!(
            probers.probe(&wrong_family).await,
            Err(HttpError::NoAddress)
        );

        // RedirectPolicy::None returns 302 and Location without following.
        let step1 = format!("http://{site}/step1");
        let no_follow = HttpRequest::post(IpFamily::Ipv4, &step1, b"x=1".to_vec());
        let res = probers.probe(&no_follow).await.expect("302 response");
        assert_eq!(res.status, 302);
        assert_eq!(res.header("location"), Some("/step2"));

        // RedirectPolicy::Follow follows relative Location, turns 302 POST into GET, and truncates.
        let follow = HttpRequest::post(IpFamily::Ipv4, &step1, b"x=1".to_vec())
            .with_redirect(RedirectPolicy::Follow { max: 3 })
            .with_max_bytes(4);
        let res = probers.probe(&follow).await.expect("200 response");
        assert_eq!(res.status, 200);
        assert_eq!(res.final_url, format!("http://{site}/step2"));
        assert_eq!(res.header("x-region"), Some("US"));
        assert_eq!(res.body, b"1234");

        // Redirect loop stops at TooManyRedirects.
        let looping = HttpRequest::get(IpFamily::Ipv4, format!("http://{site}/loop"))
            .with_redirect(RedirectPolicy::Follow { max: 2 })
            .with_timeout(Duration::from_secs(5));
        assert_eq!(
            probers.probe(&looping).await,
            Err(HttpError::TooManyRedirects)
        );

        // Non-2xx status (such as 403 challenge) returns Ok(HttpResponse) for parsers to inspect.
        let challenge = HttpRequest::get(IpFamily::Ipv4, format!("http://{site}/blocked"));
        let res = probers.probe(&challenge).await.expect("403 response");
        assert_eq!(res.status, 403);
        assert_eq!(res.header("cf-mitigated"), Some("challenge"));
        assert_eq!(Ipv4Addr::LOCALHOST.to_string(), "127.0.0.1");

        // Initial HTTPS probes to private, loopback, or metadata IPs are refused before connect.
        for internal in [
            "https://127.0.0.1/secret",
            "https://10.0.0.1/secret",
            "https://169.254.169.254/latest/meta-data",
            "https://100.100.100.200/latest/meta-data",
        ] {
            let req = HttpRequest::get(IpFamily::Ipv4, internal);
            assert!(
                matches!(probers.probe(&req).await, Err(HttpError::Transport(_))),
                "initial HTTPS probe to {internal} must be refused"
            );
        }
        for internal_v6 in [
            "https://[::1]/secret",
            "https://[fe80::1]/secret",
            "https://[fd00::1]/secret",
        ] {
            let req = HttpRequest::get(IpFamily::Ipv6, internal_v6);
            assert!(
                matches!(probers.probe(&req).await, Err(HttpError::Transport(_))),
                "initial HTTPS probe to {internal_v6} must be refused"
            );
        }

        // Redirects to private, metadata, or HTTPS loopback targets are rejected as SSRF.
        for path in ["/ssrf-private", "/ssrf-loopback-https", "/ssrf-imds"] {
            let req = HttpRequest::get(IpFamily::Ipv4, format!("http://{site}{path}"))
                .with_redirect(RedirectPolicy::Follow { max: 3 });
            assert!(
                matches!(
                    probers.probe(&req).await,
                    Err(HttpError::InvalidRedirect(_))
                ),
                "redirect {path} must fail with InvalidRedirect"
            );
        }
    }
}
