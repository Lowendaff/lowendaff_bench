//! HTTPS download for tool provisioning: rustls with the ring provider only,
//! and bundled Mozilla roots so minimal images without `ca-certificates` still work.
//! Integrity comes from the pinned SHA-256, not from TLS. Downloads go through the proxy the
//! standard environment variables name, SOCKS5 included.

use std::io;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use reqwest::ClientBuilder;
use reqwest::Proxy;
use reqwest::StatusCode;
use reqwest::header;
use reqwest::header::HeaderValue;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::crypto::ring;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::provision::Part;

/// The variables that set a proxy for `https://` addresses, in the order a hint names them.
/// `HTTP_PROXY` applies to `http://` addresses only.
const PROXY_VARIABLES: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];

/// HTTP client for tool downloads.
pub struct Fetcher {
    client: Client,
}

impl Fetcher {
    /// Builds the client, which takes its proxy from the environment variables, as reqwest
    /// does by default. Fails only if the TLS configuration is rejected.
    pub fn try_new() -> Result<Self, String> {
        Self::build(None)
    }

    /// `proxy` replaces the environment's; the tests use it.
    fn build(proxy: Option<Proxy>) -> Result<Self, String> {
        let mut builder = builder()?
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30));
        if let Some(proxy) = proxy {
            builder = builder.proxy(proxy);
        }
        let client = builder
            .build()
            .map_err(|err| format!("HTTP 客户端初始化失败：{err}"))?;
        Ok(Self { client })
    }

    /// Opens a download: the whole file, or the byte `range` of it. Non-2xx answers are errors,
    /// and so is a range other than the one asked for; a server that ignores the range sends
    /// the whole file. The body is streamed, so the caller can stop reading as soon as it grows
    /// past the pinned size. Network errors end in [`proxy_hint`]; HTTP errors do not, because
    /// the network worked.
    pub async fn open(
        &self,
        url: &str,
        range: Option<Range<u64>>,
    ) -> Result<Part<impl AsyncRead + Unpin + use<>>, String> {
        use futures_util::TryStreamExt;

        let hint = || proxy_hint(|name| std::env::var(name).ok());
        let mut request = self.client.get(url);
        if let Some(range) = &range {
            let bytes = format!("bytes={}-{}", range.start, range.end.saturating_sub(1));
            request = request.header(header::RANGE, bytes);
        }
        let response = request
            .send()
            .await
            .map_err(|err| format!("下载失败：{err}{}", hint()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("下载失败：HTTP {}", status.as_u16()));
        }
        let partial = status == StatusCode::PARTIAL_CONTENT;
        if partial
            && !holds(
                response.headers().get(header::CONTENT_RANGE),
                range.as_ref(),
            )
        {
            return Err("下载失败：服务器返回的不是所要的那一段".to_owned());
        }
        let read_hint = hint();
        let body = response
            .bytes_stream()
            .map_err(move |err| io::Error::other(format!("{err}{read_hint}")));
        let reader = StreamReader::new(Box::pin(body));
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
        && last.parse::<u64>().ok().map(|last| last + 1) == Some(range.end)
}

/// A client builder with the program's TLS configuration (rustls with ring, and the bundled
/// Mozilla roots) and its user agent, for the downloads and the network module alike.
pub fn builder() -> Result<ClientBuilder, String> {
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let tls = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|err| format!("TLS 初始化失败：{err}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Client::builder()
        .tls_backend_preconfigured(tls)
        .user_agent(concat!("lowendaff-bench/", env!("CARGO_PKG_VERSION"))))
}

/// The end of a network error's reason: which proxy variable is set, or
/// how to set one. It names the variable only, never its value, which may hold a password.
/// `lookup` reads a variable.
#[must_use]
pub fn proxy_hint(lookup: impl Fn(&str) -> Option<String>) -> String {
    let set = PROXY_VARIABLES
        .into_iter()
        .find(|name| lookup(name).is_some_and(|value| !value.is_empty()));
    match set {
        Some(name) => format!("；本机设置了 {name}，请确认这个代理可用"),
        None => "；可设置环境变量 HTTPS_PROXY 或 ALL_PROXY 使用代理".to_owned(),
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

    use super::Fetcher;
    use crate::provision::Part;

    /// Answers every request on a local port with `body` over plain HTTP, or with the byte range
    /// of it that a `Range: bytes=<first>-<last>` header asks for.
    async fn serve(body: &'static [u8]) -> String {
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
                let request = String::from_utf8_lossy(&request[..read]).to_ascii_lowercase();
                let range = request
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

    /// A SOCKS5 proxy without authentication that records the target of each CONNECT, given as
    /// an IPv4 address or a domain name, and relays the bytes.
    async fn socks5(targets: Arc<Mutex<Vec<String>>>) -> String {
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
                let host = if request[3] == 1 {
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
                let target = format!("{host}:{port}");
                targets.lock().expect("targets").push(target.clone());
                let mut upstream = TcpStream::connect(target).await.expect("reach the target");
                client
                    .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                    .await
                    .expect("reply");
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            }
        });
        address.to_string()
    }

    #[tokio::test]
    async fn downloads_go_through_a_socks5_proxy() {
        let site = serve(b"tool").await;
        let targets = Arc::new(Mutex::new(Vec::new()));
        let proxy = socks5(Arc::clone(&targets)).await;
        let socks = Proxy::all(format!("socks5h://{proxy}")).expect("a proxy address");
        let fetcher = Fetcher::build(Some(socks)).expect("client");
        let url = format!("http://{site}/tool");

        let Ok(Part::Whole(mut body)) = fetcher.open(&url, None).await else {
            panic!("the download should go through the proxy")
        };
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes).await.expect("body");
        assert_eq!(bytes, b"tool");

        let Ok(Part::Range(mut body)) = fetcher.open(&url, Some(1..3)).await else {
            panic!("a byte range should go through the proxy too")
        };
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes).await.expect("body");
        assert_eq!(bytes, b"oo");
        assert_eq!(
            *targets.lock().expect("targets"),
            [site.clone(), site],
            "the proxy relayed both downloads"
        );
    }
}
