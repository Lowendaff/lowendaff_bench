//! HTTPS download for tool provisioning (design §9): rustls with the ring provider only,
//! and bundled Mozilla roots so minimal images without `ca-certificates` still work.
//! Integrity comes from the pinned SHA-256, not from TLS.

use std::sync::Arc;
use std::time::Duration;

use futures_util::TryStreamExt;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

pub struct Fetcher {
    client: reqwest::Client,
}

impl Fetcher {
    pub fn new() -> Result<Self, String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|err| format!("TLS 初始化失败：{err}"))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .user_agent(concat!("lowendaff-bench/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| format!("HTTP 客户端初始化失败：{err}"))?;
        Ok(Self { client })
    }

    /// Opens a download. Non-2xx answers are errors. The body is streamed, so the caller
    /// can stop reading as soon as it grows past the pinned size.
    pub async fn open(&self, url: &str) -> Result<impl AsyncRead + Unpin + use<>, String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|err| format!("下载失败：{err}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("下载失败：HTTP {}", status.as_u16()));
        }
        let body = response.bytes_stream().map_err(std::io::Error::other);
        Ok(StreamReader::new(Box::pin(body)))
    }
}
