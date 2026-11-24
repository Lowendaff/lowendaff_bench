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
use tokio::task::JoinSet;
use tokio::time::Instant as TokioInstant;

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
    if port == 0 || (!ip.is_loopback() && !bench_api::is_routable_endpoint_ip(IpAddr::V4(ip))) {
        return Err(NetError::Transport(
            "non-public Taier target address refused".to_owned(),
        ));
    }
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
    if !is_valid_query(query) {
        return Err(NetError::Transport(
            "invalid Taier dovalid query".to_owned(),
        ));
    }
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
    let mut is_chunked = false;
    let mut content_len: Option<usize> = None;
    let header_lines = head.lines().skip(1);
    for line in header_lines {
        let Some((name, val)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.eq_ignore_ascii_case("transfer-encoding")
            && val.to_ascii_lowercase().contains("chunked")
        {
            is_chunked = true;
        } else if name.eq_ignore_ascii_case("content-length") {
            let parsed = val
                .trim()
                .parse::<usize>()
                .map_err(|_| NetError::Transport("invalid dovalid Content-Length".to_owned()))?;
            if content_len.is_some_and(|prev| prev != parsed) {
                return Err(NetError::Transport(
                    "conflicting dovalid Content-Length".to_owned(),
                ));
            }
            content_len = Some(parsed);
        }
    }
    let raw_body = &raw[body_offset..];
    let decoded;
    let body_bytes = if is_chunked {
        decoded = decode_chunked(raw_body)?;
        decoded.as_slice()
    } else if let Some(len) = content_len {
        if raw_body.len() < len {
            return Err(NetError::Transport(
                "truncated dovalid HTTP body".to_owned(),
            ));
        }
        &raw_body[..len]
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
        let Some(end) = size.checked_add(2).filter(|&end| end <= rest.len()) else {
            return Err(NetError::Transport("truncated chunked body".to_owned()));
        };
        if &rest[size..end] != b"\r\n" {
            return Err(NetError::Transport("truncated chunked body".to_owned()));
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[end..];
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
        if is_valid_lease_key(key) {
            TaierAdmission::Admitted(key.to_owned())
        } else {
            TaierAdmission::Unexpected(trimmed.chars().take(120).collect())
        }
    } else {
        TaierAdmission::Unexpected(trimmed.chars().take(120).collect())
    }
}

fn is_valid_query(query: &str) -> bool {
    (1..=512).contains(&query.len())
        && query
            .bytes()
            .all(|b| b > b' ' && b < 0x7f && !matches!(b, b'#'))
}

fn is_valid_lease_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Best-effort `POST /speed/dovalid?key={key}` releasing a node's queue lease.
pub async fn release(ip: Ipv4Addr, port: u16, key: &str, timeout: Duration) {
    if !is_valid_lease_key(key) {
        return;
    }
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
    if !is_valid_lease_key(&spec.key) {
        return Err(NetError::Transport("invalid Taier lease key".to_owned()));
    }
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
    let first_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let stop = cancel.child_token();

    let started = Instant::now();
    let mut tasks = JoinSet::new();
    for _ in 0..streams {
        let ip = spec.ip;
        let port = spec.port;
        let req = Arc::clone(&req);
        let bytes = Arc::clone(&total_bytes);
        let active = Arc::clone(&active);
        let first_err = Arc::clone(&first_err);
        let stop = stop.clone();
        tasks.spawn(async move {
            if let Err(err) = download_one(ip, port, &req, &stop, &bytes, &active).await
                && let Ok(mut slot) = first_err.lock()
                && slot.is_none()
            {
                *slot = Some(err);
            }
        });
    }

    wait_window(spec.duration, &mut tasks, &stop, cancel).await;
    let elapsed = started.elapsed();
    stop.cancel();
    drain_or_abort(&mut tasks, Duration::from_millis(800)).await;

    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    finish_transfer(&total_bytes, &active, &first_err, elapsed)
}

async fn download_one(
    ip: Ipv4Addr,
    port: u16,
    req: &str,
    stop: &CancellationToken,
    total_bytes: &AtomicU64,
    active: &AtomicU8,
) -> Result<(), String> {
    let mut marked_active = false;
    let mut buf = vec![0_u8; 65_536];

    while !stop.is_cancelled() {
        let mut stream =
            match tokio::time::timeout(CONNECT_LIMIT, connect_direct(ip, port, true)).await {
                Ok(Ok(s)) => s,
                Ok(Err(_)) | Err(_) if marked_active => return Ok(()),
                Ok(Err(err)) => return Err(format_net_error(&err)),
                Err(_) => return Err("download connect timed out".to_owned()),
            };
        match tokio::time::timeout(CONNECT_LIMIT, stream.write_all(req.as_bytes())).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) | Err(_) if marked_active => return Ok(()),
            Ok(Err(err)) => return Err(err.to_string()),
            Err(_) => return Err("download request timed out".to_owned()),
        }

        let mut head_buf = Vec::with_capacity(1024);
        let mut scratch = [0_u8; 4096];
        let header_res = tokio::time::timeout(HEADER_LIMIT, async {
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
        .await;
        let body_offset = match header_res {
            Ok(Ok(pos)) => pos,
            Ok(Err(_)) | Err(_) if marked_active => return Ok(()),
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err("download header timed out".to_owned()),
        };

        let head_str = String::from_utf8_lossy(&head_buf[..body_offset]);
        if !head_str.starts_with("HTTP/1.1 200") && !head_str.starts_with("HTTP/1.0 200") {
            if marked_active {
                return Ok(());
            }
            let status = head_str.lines().next().unwrap_or("empty response");
            return Err(format!("download HTTP error: {status}"));
        }
        let mut req_bytes = 0_u64;
        let initial = head_buf.len().saturating_sub(body_offset);
        if initial > 0 {
            if !marked_active {
                active.fetch_add(1, Ordering::Relaxed);
                marked_active = true;
            }
            let init_u64 = initial as u64;
            req_bytes += init_u64;
            total_bytes.fetch_add(init_u64, Ordering::Relaxed);
        }

        let mut clean_eof = false;
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                read = stream.read(&mut buf) => {
                    match read {
                        Ok(0) => {
                            clean_eof = true;
                            break;
                        }
                        Ok(n) => {
                            if !marked_active {
                                active.fetch_add(1, Ordering::Relaxed);
                                marked_active = true;
                            }
                            let n_u64 = n as u64;
                            req_bytes += n_u64;
                            total_bytes.fetch_add(n_u64, Ordering::Relaxed);
                        }
                        Err(_) if req_bytes > 0 || marked_active => return Ok(()),
                        Err(err) => return Err(err.to_string()),
                    }
                }
            }
        }
        if !clean_eof {
            break;
        }
        if req_bytes == 0 {
            if marked_active {
                break;
            }
            return Err("download returned empty HTTP 200 body".to_owned());
        }
    }
    Ok(())
}

/// Multi-stream direct IPv4 HTTP/1.1 upload throughput measurement.
pub async fn upload(
    spec: &TaierStreamSpec,
    cancel: &CancellationToken,
) -> Result<TaierTransfer, NetError> {
    upload_with_post_len(spec, UPLOAD_CONTENT_LENGTH, cancel).await
}

async fn upload_with_post_len(
    spec: &TaierStreamSpec,
    post_content_len: u64,
    cancel: &CancellationToken,
) -> Result<TaierTransfer, NetError> {
    if !is_valid_lease_key(&spec.key) {
        return Err(NetError::Transport("invalid Taier lease key".to_owned()));
    }
    let streams = spec.streams.max(1);
    let ts = unix_secs();
    let preamble = format!(
        "--{BOUNDARY}\r\n\
         Content-Disposition: form-data; name=\"upload\";filename=\"SPEED_{ts}\"\r\n\r\n"
    );
    let trailer = Arc::new(format!("\r\n--{BOUNDARY}--\r\n"));
    let framing = (preamble.len() + trailer.len()) as u64;
    let payload_limit = post_content_len.saturating_sub(framing).max(1);
    let exact_content_len = payload_limit + framing;
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
         Content-Length: {exact_content_len}\r\n\r\n\
         {preamble}",
        spec.key, spec.ip, spec.port
    ));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let active = Arc::new(AtomicU8::new(0));
    let first_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let stop = cancel.child_token();

    let started = Instant::now();
    let mut tasks = JoinSet::new();
    for _ in 0..streams {
        let ip = spec.ip;
        let port = spec.port;
        let header = Arc::clone(&header);
        let trailer = Arc::clone(&trailer);
        let bytes = Arc::clone(&total_bytes);
        let active = Arc::clone(&active);
        let first_err = Arc::clone(&first_err);
        let stop = stop.clone();
        tasks.spawn(async move {
            let res = upload_one(
                ip,
                port,
                &header,
                &trailer,
                payload_limit,
                &stop,
                &bytes,
                &active,
            )
            .await;
            if let Err(err) = res
                && let Ok(mut slot) = first_err.lock()
                && slot.is_none()
            {
                *slot = Some(err);
            }
        });
    }

    wait_window(spec.duration, &mut tasks, &stop, cancel).await;
    let elapsed = started.elapsed();
    stop.cancel();
    drain_or_abort(&mut tasks, Duration::from_millis(800)).await;

    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    finish_transfer(&total_bytes, &active, &first_err, elapsed)
}

#[allow(clippy::too_many_arguments)]
async fn upload_one(
    ip: Ipv4Addr,
    port: u16,
    header: &str,
    trailer: &str,
    payload_limit: u64,
    stop: &CancellationToken,
    total_bytes: &AtomicU64,
    active: &AtomicU8,
) -> Result<(), String> {
    const UNCONFIRMED_BUF_LIMIT: u64 = (STREAM_BUF as u64) * 4;
    let mut marked_active = false;
    let chunk = upload_chunk();

    while !stop.is_cancelled() {
        let mut stream =
            match tokio::time::timeout(CONNECT_LIMIT, connect_direct(ip, port, true)).await {
                Ok(Ok(s)) => s,
                Ok(Err(_)) | Err(_) if marked_active => return Ok(()),
                Ok(Err(err)) => return Err(format_net_error(&err)),
                Err(_) => return Err("upload connect timed out".to_owned()),
            };
        match tokio::time::timeout(CONNECT_LIMIT, stream.write_all(header.as_bytes())).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) | Err(_) if marked_active => return Ok(()),
            Ok(Err(err)) => return Err(err.to_string()),
            Err(_) => return Err("upload header write timed out".to_owned()),
        }

        let mut sent_in_post = 0_u64;
        let mut stalled = false;
        let mut resp_head: Vec<u8> = Vec::new();
        let mut resp_buf = [0_u8; 256];
        let mut peer_closed = false;
        {
            let (mut rd, mut wr) = stream.split();
            while sent_in_post < payload_limit {
                let step_u64 = (payload_limit - sent_in_post).min(UPLOAD_CHUNK_BYTES as u64);
                let step = usize::try_from(step_u64).unwrap_or(UPLOAD_CHUNK_BYTES);
                tokio::select! {
                    biased;
                    () = stop.cancelled() => break,
                    read_res = rd.read(&mut resp_buf), if !peer_closed => {
                        match read_res {
                            Ok(0) => {
                                peer_closed = true;
                                if let Some(err) = check_upload_response(&resp_head, true) {
                                    if sent_in_post <= UNCONFIRMED_BUF_LIMIT {
                                        total_bytes.fetch_sub(sent_in_post, Ordering::Relaxed);
                                    }
                                    if marked_active {
                                        stalled = true;
                                        break;
                                    }
                                    return Err(err);
                                }
                            }
                            Ok(n) => {
                                let room = RESP_HEAD_LIMIT.saturating_sub(resp_head.len());
                                resp_head.extend_from_slice(&resp_buf[..n.min(room)]);
                                if let Some(err) = check_upload_response(&resp_head, false) {
                                    if sent_in_post <= UNCONFIRMED_BUF_LIMIT {
                                        total_bytes.fetch_sub(sent_in_post, Ordering::Relaxed);
                                    }
                                    if marked_active {
                                        stalled = true;
                                        break;
                                    }
                                    return Err(err);
                                }
                            }
                            Err(err) => {
                                if sent_in_post <= UNCONFIRMED_BUF_LIMIT {
                                    total_bytes.fetch_sub(sent_in_post, Ordering::Relaxed);
                                }
                                if marked_active {
                                    stalled = true;
                                    break;
                                }
                                return Err(err.to_string());
                            }
                        }
                    }
                    res = tokio::time::timeout(CHUNK_WRITE_LIMIT, wr.write(&chunk[..step])) => {
                        match res {
                            Ok(Ok(0)) => {
                                if sent_in_post <= UNCONFIRMED_BUF_LIMIT {
                                    total_bytes.fetch_sub(sent_in_post, Ordering::Relaxed);
                                }
                                if marked_active {
                                    stalled = true;
                                    break;
                                }
                                return Err("upload connection closed".to_owned());
                            }
                            Ok(Ok(n)) => {
                                let n_u64 = n as u64;
                                sent_in_post += n_u64;
                                total_bytes.fetch_add(n_u64, Ordering::Relaxed);
                                if !marked_active && sent_in_post > UNCONFIRMED_BUF_LIMIT {
                                    active.fetch_add(1, Ordering::Relaxed);
                                    marked_active = true;
                                }
                            }
                            Ok(Err(err)) => {
                                if sent_in_post <= UNCONFIRMED_BUF_LIMIT {
                                    total_bytes.fetch_sub(sent_in_post, Ordering::Relaxed);
                                }
                                if marked_active {
                                    stalled = true;
                                    break;
                                }
                                return Err(err.to_string());
                            }
                            Err(_) => {
                                if sent_in_post <= UNCONFIRMED_BUF_LIMIT {
                                    total_bytes.fetch_sub(sent_in_post, Ordering::Relaxed);
                                }
                                if marked_active {
                                    stalled = true;
                                    break;
                                }
                                return Err("upload write stalled".to_owned());
                            }
                        }
                    }
                }
            }
        }
        if !stalled && sent_in_post > 0 && !marked_active {
            active.fetch_add(1, Ordering::Relaxed);
            marked_active = true;
        }
        let _ = tokio::time::timeout(
            Duration::from_millis(250),
            stream.write_all(trailer.as_bytes()),
        )
        .await;
        let _ = stream.shutdown().await;
        if stalled || sent_in_post < payload_limit {
            break;
        }
    }
    Ok(())
}

const RESP_HEAD_LIMIT: usize = 256;

fn check_upload_response(resp_head: &[u8], eof: bool) -> Option<String> {
    if resp_head.is_empty() {
        return eof.then(|| "upload closed before accepting payload".to_owned());
    }
    let text = String::from_utf8_lossy(resp_head);
    if !eof && resp_head.len() < RESP_HEAD_LIMIT && !text.contains('\n') {
        return None;
    }
    let first = text.lines().next().unwrap_or_default().trim();
    if first.starts_with("HTTP/1.1 200")
        || first.starts_with("HTTP/1.0 200")
        || first.starts_with("HTTP/1.1 100")
    {
        return None;
    }
    let short: String = first.chars().take(60).collect();
    Some(format!("upload HTTP error: {short}"))
}

async fn wait_window(
    duration: Duration,
    tasks: &mut JoinSet<()>,
    stop: &CancellationToken,
    cancel: &CancellationToken,
) {
    let deadline = TokioInstant::now() + duration;
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                stop.cancel();
                break;
            }
            () = tokio::time::sleep_until(deadline) => break,
            joined = tasks.join_next() => {
                if joined.is_none() {
                    break;
                }
            }
        }
    }
}

async fn drain_or_abort(tasks: &mut JoinSet<()>, grace: Duration) {
    let drain = async { while tasks.join_next().await.is_some() {} };
    if tokio::time::timeout(grace, drain).await.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
}

fn finish_transfer(
    total_bytes: &AtomicU64,
    active: &AtomicU8,
    first_err: &Mutex<Option<String>>,
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

fn format_net_error(err: &NetError) -> String {
    match err {
        NetError::NoAddress => "no address for family".to_owned(),
        NetError::TimedOut => "timed out".to_owned(),
        NetError::NoResolver => "no DNS resolver".to_owned(),
        NetError::Transport(why) => why.clone(),
        NetError::Cancelled => "cancelled".to_owned(),
    }
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
        assert!(
            super::acquire(ip, port, "token=evil\r\nX-Injected: 1", limit)
                .await
                .is_err(),
            "CRLF or whitespace in dovalid query must be rejected before socket write"
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

        // Rollover test: single worker chains multiple POSTs when per-POST Content-Length is
        // exhausted mid-window, while keeping active_streams == 1.
        let rollover_spec = TaierStreamSpec {
            ip,
            port,
            key: "uuid-smoke-123".to_owned(),
            duration: Duration::from_millis(120),
            streams: 1,
        };
        let rolled = super::upload_with_post_len(&rollover_spec, 131_072, &cancel)
            .await
            .expect("rollover upload");
        assert!(
            rolled.bytes > 131_072,
            "single stream should roll over past one POST's Content-Length (got {})",
            rolled.bytes
        );
        assert_eq!(rolled.active_streams, 1);

        // Connection refusal surfaces a clean message without Rust Debug `Transport(...)` syntax.
        let closed_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let closed_port = closed_listener.local_addr().expect("addr").port();
        drop(closed_listener);
        let dead_spec = TaierStreamSpec {
            ip,
            port: closed_port,
            key: "uuid-smoke-123".to_owned(),
            duration: Duration::from_millis(100),
            streams: 1,
        };
        let dead_err = super::upload(&dead_spec, &cancel)
            .await
            .expect_err("closed port must fail");
        let rendered = super::format_net_error(&dead_err);
        assert!(
            !rendered.contains("Transport("),
            "must not leak Rust Debug wrapper into report note: {rendered}"
        );
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

        let crlf_injected =
            b"HTTP/1.1 200 OK\r\nContent-Length: 26\r\n\r\n1-uuid\r\nEvil: injected\r\n\r\n";
        assert!(matches!(
            super::parse_dovalid_http(crlf_injected),
            Ok(TaierAdmission::Unexpected(_))
        ));

        let mut long_reject = b"HTTP/1.1 403 Forbidden ".to_vec();
        long_reject.resize(super::RESP_HEAD_LIMIT, b'X');
        assert!(
            super::check_upload_response(&long_reject, false).is_some(),
            "status line filling RESP_HEAD_LIMIT without newline must fail immediately"
        );
    }
}
