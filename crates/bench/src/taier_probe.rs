//! Direct IPv4 socket client for CAICT GlobalSpeed (`network.taier`): queue admission (`dovalid`),
//! lease release, and multi-stream HTTP/1.1 download and upload throughput measurements.
//!
//! Kept in `bench` so plugins never open sockets of their own; every socket binds directly to
//! `0.0.0.0:0` and ignores all download proxy settings and environment variables.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bench_api::CancellationToken;
use bench_api::NetError;
use bench_api::TaierAdmission;
use bench_api::TaierStreamSpec;
use bench_api::TaierTransfer;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpSocket;
use tokio::net::TcpStream;

const UA_DOWN: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/100.0.4896.60 Safari/537.36"
);
const UA_UP: &str = "Dalvik/1.6.0 (Linux; U; Android 4.2.2; GT-I9505 Build/JDQ39)";
const BOUNDARY: &str = "00content0boundary00";
const STREAM_BUF: u32 = 524_288;
const CONNECT_LIMIT: Duration = Duration::from_secs(3);
const HEADER_LIMIT: Duration = Duration::from_secs(4);
const CHUNK_WRITE_LIMIT: Duration = Duration::from_secs(3);
const UPLOAD_CONTENT_LENGTH: u64 = 900_000_000;
const UPLOAD_CHUNK_BYTES: usize = 65_536;

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn upload_chunk() -> &'static [u8; UPLOAD_CHUNK_BYTES] {
    static CHUNK: OnceLock<[u8; UPLOAD_CHUNK_BYTES]> = OnceLock::new();
    CHUNK.get_or_init(|| {
        let mut buf = [0_u8; UPLOAD_CHUNK_BYTES];
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        for byte in &mut buf {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = (state & 0xff) as u8;
        }
        buf
    })
}

async fn connect_direct(ip: Ipv4Addr, port: u16, buffers: bool) -> Result<TcpStream, NetError> {
    let sock = TcpSocket::new_v4().map_err(|err| NetError::Transport(err.to_string()))?;
    sock.bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))
        .map_err(|err| NetError::Transport(err.to_string()))?;
    if buffers {
        let _ = sock.set_recv_buffer_size(STREAM_BUF);
        let _ = sock.set_send_buffer_size(STREAM_BUF);
    }
    let stream = sock
        .connect(SocketAddr::new(IpAddr::V4(ip), port))
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

/// Executes `GET /speed/dovalid?{query}` over a direct IPv4 socket.
pub async fn acquire(
    ip: Ipv4Addr,
    port: u16,
    query: &str,
    timeout: Duration,
) -> Result<TaierAdmission, NetError> {
    let run = async {
        let mut stream = connect_direct(ip, port, false).await?;
        let req = format!(
            "GET /speed/dovalid?{query} HTTP/1.1\r\n\
             Host: {ip}:{port}\r\n\
             User-Agent: Mozilla/5.0\r\n\
             Connection: close\r\n\r\n"
        );
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|err| NetError::Transport(err.to_string()))?;
        let mut raw = Vec::with_capacity(512);
        stream
            .take(4096)
            .read_to_end(&mut raw)
            .await
            .map_err(|err| NetError::Transport(err.to_string()))?;
        parse_dovalid_http(&raw)
    };
    let outcome = match tokio::time::timeout(timeout, run).await {
        Ok(res) => res,
        Err(_) => Err(NetError::TimedOut),
    };
    tracing::debug!("Taier dovalid {ip}:{port}: {outcome:?}");
    outcome
}

fn parse_dovalid_http(raw: &[u8]) -> Result<TaierAdmission, NetError> {
    let Some(body_offset) = find_header_end(raw) else {
        return Err(NetError::Transport("incomplete HTTP response".to_owned()));
    };
    let head = String::from_utf8_lossy(&raw[..body_offset - 4]);
    let first = head.lines().next().unwrap_or_default();
    if !first.starts_with("HTTP/1.1 200") && !first.starts_with("HTTP/1.0 200") {
        return Err(NetError::Transport(format!(
            "unexpected status: {}",
            first.chars().take(60).collect::<String>()
        )));
    }
    let is_chunked = head.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, val)| {
            name.trim().eq_ignore_ascii_case("transfer-encoding")
                && val.to_ascii_lowercase().contains("chunked")
        })
    });
    let raw_body = &raw[body_offset..];
    let decoded;
    let body_bytes = if is_chunked {
        decoded = decode_chunked(raw_body)?;
        decoded.as_slice()
    } else {
        raw_body
    };
    let body = String::from_utf8_lossy(body_bytes);
    Ok(parse_dovalid_body(&body))
}

fn decode_chunked(mut rest: &[u8]) -> Result<Vec<u8>, NetError> {
    let mut out = Vec::with_capacity(rest.len());
    loop {
        let Some(crlf) = rest.windows(2).position(|w| w == b"\r\n") else {
            return Err(NetError::Transport(
                "malformed chunked dovalid response".to_owned(),
            ));
        };
        let size_line = std::str::from_utf8(&rest[..crlf])
            .map_err(|_| NetError::Transport("invalid chunk size encoding".to_owned()))?;
        let hex = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(hex, 16)
            .map_err(|_| NetError::Transport("invalid chunk size hex".to_owned()))?;
        rest = &rest[crlf + 2..];
        if size == 0 {
            return Ok(out);
        }
        if rest.len() < size + 2 || &rest[size..size + 2] != b"\r\n" {
            return Err(NetError::Transport("truncated chunked body".to_owned()));
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

fn parse_dovalid_body(body: &str) -> TaierAdmission {
    let trimmed = body.trim();
    if trimmed.starts_with("-1") {
        TaierAdmission::Busy
    } else if trimmed.starts_with('0') {
        TaierAdmission::AuthRejected
    } else if trimmed.starts_with('2') {
        TaierAdmission::InvalidParams
    } else if let Some(rest) = trimmed.strip_prefix('1') {
        let key = rest.trim_start_matches('-').trim();
        if key.is_empty() {
            TaierAdmission::Unexpected(trimmed.chars().take(120).collect())
        } else {
            TaierAdmission::Admitted(key.to_owned())
        }
    } else {
        TaierAdmission::Unexpected(trimmed.chars().take(120).collect())
    }
}

/// Best-effort `POST /speed/dovalid?key={key}` releasing a node's queue lease.
pub async fn release(ip: Ipv4Addr, port: u16, key: &str, timeout: Duration) {
    let run = async {
        let Ok(mut stream) = connect_direct(ip, port, false).await else {
            return;
        };
        let req = format!(
            "POST /speed/dovalid?key={key} HTTP/1.1\r\n\
             Host: {ip}:{port}\r\n\
             Content-Length: 0\r\n\
             Connection: close\r\n\r\n"
        );
        if stream.write_all(req.as_bytes()).await.is_ok() {
            let mut sink = [0_u8; 256];
            let _ = stream.read(&mut sink).await;
            let _ = stream.shutdown().await;
        }
    };
    let _ = tokio::time::timeout(timeout, run).await;
}

/// Multi-stream direct IPv4 HTTP/1.1 download throughput measurement.
pub async fn download(
    spec: &TaierStreamSpec,
    cancel: &CancellationToken,
) -> Result<TaierTransfer, NetError> {
    let streams = spec.streams.max(1);
    let ts = unix_secs();
    let req = Arc::new(format!(
        "GET /speed/File(1G).dl?r={ts}&key={} HTTP/1.1\r\n\
         Accept: */*\r\n\
         Connection: close\r\n\
         User-Agent: {UA_DOWN}\r\n\
         Host:{}:{}\r\n\r\n",
        spec.key, spec.ip, spec.port
    ));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let active = Arc::new(AtomicU8::new(0));
    let failed = Arc::new(AtomicU8::new(0));
    let first_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let stop = cancel.child_token();

    let started = Instant::now();
    let mut handles = Vec::with_capacity(usize::from(streams));
    for _ in 0..streams {
        let ip = spec.ip;
        let port = spec.port;
        let req = Arc::clone(&req);
        let bytes = Arc::clone(&total_bytes);
        let active = Arc::clone(&active);
        let failed = Arc::clone(&failed);
        let first_err = Arc::clone(&first_err);
        let stop = stop.clone();
        handles.push(tokio::spawn(async move {
            if let Err(err) = download_one(ip, port, &req, &stop, &bytes, &active).await {
                failed.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut slot) = first_err.lock()
                    && slot.is_none()
                {
                    *slot = Some(err);
                }
            }
        }));
    }

    wait_window(spec.duration, streams, &failed, &stop, cancel).await;
    let elapsed = started.elapsed();
    stop.cancel();
    for handle in handles {
        let _ = tokio::time::timeout(Duration::from_millis(800), handle).await;
    }

    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    finish_transfer(total_bytes, active, first_err, elapsed)
}

async fn download_one(
    ip: Ipv4Addr,
    port: u16,
    req: &str,
    stop: &CancellationToken,
    total_bytes: &AtomicU64,
    active: &AtomicU8,
) -> Result<(), String> {
    let mut stream = tokio::time::timeout(CONNECT_LIMIT, connect_direct(ip, port, true))
        .await
        .map_err(|_| "download connect timed out".to_owned())?
        .map_err(|err| format!("{err:?}"))?;
    tokio::time::timeout(CONNECT_LIMIT, stream.write_all(req.as_bytes()))
        .await
        .map_err(|_| "download request timed out".to_owned())?
        .map_err(|err| err.to_string())?;

    let mut head_buf = Vec::with_capacity(1024);
    let mut scratch = [0_u8; 4096];
    let body_offset = tokio::time::timeout(HEADER_LIMIT, async {
        loop {
            let n = stream
                .read(&mut scratch)
                .await
                .map_err(|err| err.to_string())?;
            if n == 0 {
                return Err("download closed before HTTP header".to_owned());
            }
            head_buf.extend_from_slice(&scratch[..n]);
            if let Some(pos) = find_header_end(&head_buf) {
                return Ok(pos);
            }
            if head_buf.len() > 16_384 {
                return Err("download HTTP header too large".to_owned());
            }
        }
    })
    .await
    .map_err(|_| "download header timed out".to_owned())??;

    let head_str = String::from_utf8_lossy(&head_buf[..body_offset]);
    if !head_str.starts_with("HTTP/1.1 200") && !head_str.starts_with("HTTP/1.0 200") {
        let status = head_str.lines().next().unwrap_or("empty response");
        return Err(format!("download HTTP error: {status}"));
    }
    active.fetch_add(1, Ordering::Relaxed);
    let initial = head_buf.len().saturating_sub(body_offset);
    if initial > 0 {
        total_bytes.fetch_add(initial as u64, Ordering::Relaxed);
    }

    let mut buf = vec![0_u8; 65_536];
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            read = stream.read(&mut buf) => {
                match read {
                    Ok(0) => break,
                    Ok(n) => {
                        total_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    }
                    Err(err) => return Err(err.to_string()),
                }
            }
        }
    }
    Ok(())
}

/// Multi-stream direct IPv4 HTTP/1.1 upload throughput measurement.
pub async fn upload(
    spec: &TaierStreamSpec,
    cancel: &CancellationToken,
) -> Result<TaierTransfer, NetError> {
    let streams = spec.streams.max(1);
    let ts = unix_secs();
    let header = Arc::new(format!(
        "POST /speed/doAnalsLoad.do HTTP/1.1\r\n\
         Connection: close\r\n\
         Cache-Control: no-cache\r\n\
         Charset: UTF-8\r\n\
         Key: {}\r\n\
         Content-Type: multipart/form-data;boundary={BOUNDARY}\r\n\
         User-Agent: {UA_UP}\r\n\
         Host: {}:{}\r\n\
         Accept-Encoding: gzip\r\n\
         Content-Length: {UPLOAD_CONTENT_LENGTH}\r\n\r\n\
         --{BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"upload\";filename=\"SPEED_{ts}\"\r\n\r\n",
        spec.key, spec.ip, spec.port
    ));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let active = Arc::new(AtomicU8::new(0));
    let failed = Arc::new(AtomicU8::new(0));
    let first_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let stop = cancel.child_token();

    let started = Instant::now();
    let mut handles = Vec::with_capacity(usize::from(streams));
    for _ in 0..streams {
        let ip = spec.ip;
        let port = spec.port;
        let header = Arc::clone(&header);
        let bytes = Arc::clone(&total_bytes);
        let active = Arc::clone(&active);
        let failed = Arc::clone(&failed);
        let first_err = Arc::clone(&first_err);
        let stop = stop.clone();
        handles.push(tokio::spawn(async move {
            if let Err(err) = upload_one(ip, port, &header, &stop, &bytes, &active).await {
                failed.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut slot) = first_err.lock()
                    && slot.is_none()
                {
                    *slot = Some(err);
                }
            }
        }));
    }

    wait_window(spec.duration, streams, &failed, &stop, cancel).await;
    let elapsed = started.elapsed();
    stop.cancel();
    for handle in handles {
        let _ = tokio::time::timeout(Duration::from_millis(800), handle).await;
    }

    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    finish_transfer(total_bytes, active, first_err, elapsed)
}

async fn upload_one(
    ip: Ipv4Addr,
    port: u16,
    header: &str,
    stop: &CancellationToken,
    total_bytes: &AtomicU64,
    active: &AtomicU8,
) -> Result<(), String> {
    let mut stream = tokio::time::timeout(CONNECT_LIMIT, connect_direct(ip, port, true))
        .await
        .map_err(|_| "upload connect timed out".to_owned())?
        .map_err(|err| format!("{err:?}"))?;
    tokio::time::timeout(CONNECT_LIMIT, stream.write_all(header.as_bytes()))
        .await
        .map_err(|_| "upload header write timed out".to_owned())?
        .map_err(|err| err.to_string())?;
    active.fetch_add(1, Ordering::Relaxed);

    let chunk = upload_chunk();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            res = tokio::time::timeout(CHUNK_WRITE_LIMIT, stream.write_all(chunk)) => {
                match res {
                    Ok(Ok(())) => {
                        total_bytes.fetch_add(UPLOAD_CHUNK_BYTES as u64, Ordering::Relaxed);
                    }
                    Ok(Err(err)) => return Err(err.to_string()),
                    Err(_) => return Err("upload write stalled".to_owned()),
                }
            }
        }
    }
    let trailer = format!("\r\n--{BOUNDARY}--\r\n");
    let _ = tokio::time::timeout(
        Duration::from_millis(250),
        stream.write_all(trailer.as_bytes()),
    )
    .await;
    let _ = stream.shutdown().await;
    Ok(())
}

async fn wait_window(
    duration: Duration,
    streams: u8,
    failed: &AtomicU8,
    stop: &CancellationToken,
    cancel: &CancellationToken,
) {
    let deadline = tokio::time::Instant::now() + duration;
    let mut poll = tokio::time::interval(Duration::from_millis(50));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                stop.cancel();
                break;
            }
            () = tokio::time::sleep_until(deadline) => break,
            _ = poll.tick() => {
                if failed.load(Ordering::Relaxed) >= streams {
                    break;
                }
            }
        }
    }
}

fn finish_transfer(
    total_bytes: Arc<AtomicU64>,
    active: Arc<AtomicU8>,
    first_err: Arc<Mutex<Option<String>>>,
    elapsed: Duration,
) -> Result<TaierTransfer, NetError> {
    let bytes = total_bytes.load(Ordering::Relaxed);
    let active_streams = active.load(Ordering::Relaxed);
    if bytes == 0 {
        let why = first_err
            .lock()
            .ok()
            .and_then(|mut g| g.take())
            .unwrap_or_else(|| "zero bytes transferred".to_owned());
        return Err(NetError::Transport(why));
    }
    let elapsed_micros = u64::try_from(elapsed.as_micros())
        .unwrap_or(u64::MAX)
        .max(1);
    let bps_128 = u128::from(bytes) * 8 * 1_000_000 / u128::from(elapsed_micros);
    let bits_per_sec = u64::try_from(bps_128).unwrap_or(u64::MAX);
    Ok(TaierTransfer {
        bytes,
        elapsed_micros,
        bits_per_sec,
        active_streams,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|win| win == b"\r\n\r\n")
        .map(|idx| idx + 4)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use bench_api::CancellationToken;
    use bench_api::TaierAdmission;
    use bench_api::TaierStreamSpec;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn taier_prober_handles_dovalid_streams_and_release() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let released = Arc::new(AtomicBool::new(false));
        let rel_clone = Arc::clone(&released);

        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let rel = Arc::clone(&rel_clone);
                tokio::spawn(async move {
                    let mut buf = vec![0_u8; 4096];
                    let Ok(mut n) = sock.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    while !buf[..n].windows(4).any(|w| w == b"\r\n\r\n") && n < buf.len() {
                        let Ok(more) = sock.read(&mut buf[n..]).await else {
                            break;
                        };
                        if more == 0 {
                            break;
                        }
                        n += more;
                    }
                    let head = String::from_utf8_lossy(&buf[..n]);
                    if head.starts_with("GET /speed/dovalid?") {
                        if head.contains("token=busy") {
                            let _ = sock
                                .write_all(
                                    b"HTTP/1.1 200 \r\nTransfer-Encoding: chunked\r\n\r\n\
                                      3\r\n-1-\r\n0\r\n\r\n",
                                )
                                .await;
                        } else {
                            let _ = sock
                                .write_all(
                                    b"HTTP/1.1 200 \r\nTransfer-Encoding: chunked\r\n\r\n\
                                      10\r\n1-uuid-smoke-123\r\n0\r\n\r\n",
                                )
                                .await;
                        }
                    } else if head.starts_with("POST /speed/dovalid?key=uuid-smoke-123") {
                        rel.store(true, Ordering::Relaxed);
                        let _ = sock
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                            .await;
                    } else if head.starts_with("GET /speed/File(1G).dl?") {
                        let _ = sock
                            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                            .await;
                        let payload = [0x5a_u8; 32_768];
                        while sock.write_all(&payload).await.is_ok() {}
                    } else if head.starts_with("POST /speed/doAnalsLoad.do ") {
                        let mut sink = [0_u8; 32_768];
                        while let Ok(read_n) = sock.read(&mut sink).await {
                            if read_n == 0 {
                                break;
                            }
                        }
                    }
                });
            }
        });

        let ip = Ipv4Addr::LOCALHOST;
        let limit = Duration::from_secs(3);
        assert_eq!(
            super::acquire(ip, port, "token=busy", limit).await,
            Ok(TaierAdmission::Busy)
        );
        assert_eq!(
            super::acquire(ip, port, "token=good", limit).await,
            Ok(TaierAdmission::Admitted("uuid-smoke-123".to_owned()))
        );

        let cancel = CancellationToken::new();
        let spec = TaierStreamSpec {
            ip,
            port,
            key: "uuid-smoke-123".to_owned(),
            duration: Duration::from_millis(250),
            streams: 4,
        };
        let down = super::download(&spec, &cancel).await.expect("download");
        assert!(down.bytes > 0 && down.bits_per_sec > 0);
        assert_eq!(down.active_streams, 4);

        let up = super::upload(&spec, &cancel).await.expect("upload");
        assert!(up.bytes > 0 && up.bits_per_sec > 0);
        assert!(up.active_streams >= 1);

        super::release(ip, port, "uuid-smoke-123", limit).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(released.load(Ordering::Relaxed));
    }

    #[test]
    fn dovalid_http_parser_decodes_tomcat_chunked_and_content_length_bodies() {
        let chunked_ok = b"HTTP/1.1 200 \r\nCache-Control: no-cache\r\n\
            Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
            26;ext=1\r\n1-71e94fc6-f92a-490d-88ce-782ea7c0f872\r\n0\r\n\r\n";
        assert_eq!(
            super::parse_dovalid_http(chunked_ok),
            Ok(TaierAdmission::Admitted(
                "71e94fc6-f92a-490d-88ce-782ea7c0f872".to_owned()
            ))
        );

        let chunked_busy =
            b"HTTP/1.1 200 \r\nTransfer-Encoding: chunked\r\n\r\n3\r\n-1-\r\n0\r\n\r\n";
        assert_eq!(
            super::parse_dovalid_http(chunked_busy),
            Ok(TaierAdmission::Busy)
        );

        let chunked_zero =
            b"HTTP/1.1 200 \r\nTransfer-Encoding: chunked\r\n\r\n2\r\n0-\r\n0\r\n\r\n";
        assert_eq!(
            super::parse_dovalid_http(chunked_zero),
            Ok(TaierAdmission::AuthRejected)
        );

        let plain_ok = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n1-uuid-abc";
        assert_eq!(
            super::parse_dovalid_http(plain_ok),
            Ok(TaierAdmission::Admitted("uuid-abc".to_owned()))
        );
    }
}
