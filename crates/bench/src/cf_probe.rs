//! Direct, proxy-free HTTP throughput and RFC 5389 STUN UDP packet-loss prober for
//! `network.cloudflare`.
//!
//! Kept in `bench` so plugins never open sockets or depend on `reqwest` directly; every socket
//! binds to the requested IP family (`0.0.0.0` or `[::]`) and ignores all download proxy settings
//! and environment variables.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::CfStunRaw;
use bench_api::CfStunSpec;
use bench_api::CfTransferRaw;
use bench_api::CfTransferSpec;
use bench_api::IpFamily;
use bench_api::NetError;
use futures_util::stream;
use reqwest::Body;
use reqwest::Client;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::header;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::redirect::Policy;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::fetcher;

const USER_AGENT: &str = "cloudflare-speed-cli/0.1.0";
const REFERER: &str = "https://speed.cloudflare.com/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TCP_KEEPALIVE: Duration = Duration::from_secs(15);
const SAMPLE_INTERVAL: Duration = Duration::from_millis(200);
const WORKER_BACKOFF: Duration = Duration::from_millis(100);
const UPLOAD_CHUNK_SIZE: u64 = 64 * 1024;
const MIN_DOWNLOAD_BYTES_PER_REQ: u64 = 100_000;
const UDP_UNREACHABLE_AFTER: u64 = 10;

/// Multi-worker Cloudflare download phase (`GET /__down`) with 200 ms sampling and concurrent
/// loaded-latency probes.
pub async fn download(
    spec: &CfTransferSpec,
    cancel: &CancellationToken,
) -> Result<CfTransferRaw, NetError> {
    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    let (client, base) = build_client(spec.family, &spec.base_url).await?;
    let down_url = base
        .join("/__down")
        .map_err(|err| NetError::Transport(err.to_string()))?;

    let stop = Arc::new(AtomicBool::new(false));
    let total = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let workers = usize::from(spec.concurrency.max(1));

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let http = client.clone();
        let url_base = down_url.clone();
        let meas_id = spec.meas_id.clone();
        let mut bytes_per_req = spec.bytes_per_req.max(1);
        let stop_w = Arc::clone(&stop);
        let total_w = Arc::clone(&total);
        let failed_w = Arc::clone(&failed);
        let cancel_w = cancel.clone();

        handles.push(tokio::spawn(async move {
            while !stop_w.load(Ordering::Relaxed) && !cancel_w.is_cancelled() {
                let mut url = url_base.clone();
                url.query_pairs_mut()
                    .append_pair("measId", &meas_id)
                    .append_pair("bytes", &bytes_per_req.to_string());

                let send_fut = http.get(url).send();
                let resp = tokio::select! {
                    biased;
                    () = cancel_w.cancelled() => break,
                    res = send_fut => match res {
                        Ok(r) => r,
                        Err(_) => {
                            failed_w.fetch_add(1, Ordering::Relaxed);
                            tokio::time::sleep(WORKER_BACKOFF).await;
                            continue;
                        }
                    }
                };

                if !resp.status().is_success() {
                    failed_w.fetch_add(1, Ordering::Relaxed);
                    if resp.status() == StatusCode::TOO_MANY_REQUESTS {
                        bytes_per_req = (bytes_per_req / 2).max(MIN_DOWNLOAD_BYTES_PER_REQ);
                    }
                    tokio::time::sleep(WORKER_BACKOFF).await;
                    continue;
                }

                let mut resp = resp;
                while !stop_w.load(Ordering::Relaxed) && !cancel_w.is_cancelled() {
                    let chunk_fut = resp.chunk();
                    let next = tokio::select! {
                        biased;
                        () = cancel_w.cancelled() => break,
                        c = chunk_fut => c,
                    };
                    match next {
                        Ok(Some(chunk)) => {
                            total_w.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        }
                        Ok(None) => break,
                        Err(_) => {
                            failed_w.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
        }));
    }

    let (lat_tx, mut lat_rx) = mpsc::channel(1);
    let lat_client = client.clone();
    let lat_url = down_url.clone();
    let lat_stop = Arc::clone(&stop);
    let lat_cancel = cancel.clone();
    let lat_duration = spec.duration;
    let lat_interval = spec.probe_interval;
    let lat_timeout = spec.probe_timeout;
    let lat_handle = tokio::spawn(async move {
        let stats = run_loaded_probes(
            &lat_client,
            &lat_url,
            "download",
            lat_duration,
            lat_interval,
            lat_timeout,
            &lat_stop,
            &lat_cancel,
        )
        .await;
        let _ = lat_tx.send(stats).await;
    });

    let ticks_res = sample_ticks(spec.duration, &total, cancel).await;
    stop.store(true, Ordering::Relaxed);
    for handle in &handles {
        handle.abort();
    }
    for handle in handles {
        let _ = handle.await;
    }

    let (active_ms, ticks) = ticks_res?;
    let (loaded_sent, loaded_rtts_micros) =
        match tokio::time::timeout(Duration::from_secs(5), lat_rx.recv()).await {
            Ok(Some(pair)) => pair,
            _ => (0, Vec::new()),
        };
    lat_handle.abort();
    let _ = lat_handle.await;

    Ok(CfTransferRaw {
        bytes: total.load(Ordering::Relaxed),
        duration_ms: active_ms,
        ticks,
        loaded_sent,
        loaded_rtts_micros,
        failed_requests: failed.load(Ordering::Relaxed),
    })
}

/// Multi-worker Cloudflare upload phase (`POST /__up`) streaming 64 KiB zero chunks with error
/// byte rollback, 200 ms sampling, and concurrent loaded-latency probes.
pub async fn upload(
    spec: &CfTransferSpec,
    cancel: &CancellationToken,
) -> Result<CfTransferRaw, NetError> {
    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    let (client, base) = build_client(spec.family, &spec.base_url).await?;
    let mut up_url = base
        .join("/__up")
        .map_err(|err| NetError::Transport(err.to_string()))?;
    up_url
        .query_pairs_mut()
        .append_pair("measId", &spec.meas_id);
    let down_url = base
        .join("/__down")
        .map_err(|err| NetError::Transport(err.to_string()))?;

    let stop = Arc::new(AtomicBool::new(false));
    let total = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let workers = usize::from(spec.concurrency.max(1));

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let http = client.clone();
        let url = up_url.clone();
        let bytes_per_req = spec.bytes_per_req.max(1);
        let stop_w = Arc::clone(&stop);
        let total_w = Arc::clone(&total);
        let failed_w = Arc::clone(&failed);
        let cancel_w = cancel.clone();

        handles.push(tokio::spawn(async move {
            while !stop_w.load(Ordering::Relaxed) && !cancel_w.is_cancelled() {
                let counted = Arc::new(AtomicU64::new(0));
                let total_s = Arc::clone(&total_w);
                let counted_s = Arc::clone(&counted);
                let stop_s = Arc::clone(&stop_w);
                let body_stream = stream::unfold(bytes_per_req, move |remaining| {
                    let total_c = Arc::clone(&total_s);
                    let counted_c = Arc::clone(&counted_s);
                    let stop_c = Arc::clone(&stop_s);
                    async move {
                        if remaining == 0 || stop_c.load(Ordering::Relaxed) {
                            return None;
                        }
                        let step = remaining.min(UPLOAD_CHUNK_SIZE);
                        let chunk_len = usize::try_from(step).unwrap_or(65_536);
                        total_c.fetch_add(step, Ordering::Relaxed);
                        counted_c.fetch_add(step, Ordering::Relaxed);
                        Some((
                            Ok::<Vec<u8>, io::Error>(vec![0_u8; chunk_len]),
                            remaining - step,
                        ))
                    }
                });

                let body = Body::wrap_stream(body_stream);
                let req_fut = async {
                    let mut resp = http.post(url.clone()).body(body).send().await?;
                    if !resp.status().is_success() {
                        return Ok(false);
                    }
                    while resp.chunk().await?.is_some() {}
                    Ok::<bool, reqwest::Error>(true)
                };

                let ok = tokio::select! {
                    biased;
                    () = cancel_w.cancelled() => false,
                    res = req_fut => matches!(res, Ok(true)),
                };

                if !ok {
                    let rolled = counted.load(Ordering::Relaxed);
                    let mut cur = total_w.load(Ordering::Relaxed);
                    while let Err(actual) = total_w.compare_exchange_weak(
                        cur,
                        cur.saturating_sub(rolled),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        cur = actual;
                    }
                    if !stop_w.load(Ordering::Relaxed) && !cancel_w.is_cancelled() {
                        failed_w.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(WORKER_BACKOFF).await;
                    }
                }
            }
        }));
    }

    let (lat_tx, mut lat_rx) = mpsc::channel(1);
    let lat_client = client.clone();
    let lat_url = down_url;
    let lat_stop = Arc::clone(&stop);
    let lat_cancel = cancel.clone();
    let lat_duration = spec.duration;
    let lat_interval = spec.probe_interval;
    let lat_timeout = spec.probe_timeout;
    let lat_handle = tokio::spawn(async move {
        let stats = run_loaded_probes(
            &lat_client,
            &lat_url,
            "upload",
            lat_duration,
            lat_interval,
            lat_timeout,
            &lat_stop,
            &lat_cancel,
        )
        .await;
        let _ = lat_tx.send(stats).await;
    });

    let ticks_res = sample_ticks(spec.duration, &total, cancel).await;
    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    let (active_ms, ticks) = ticks_res?;
    let (loaded_sent, loaded_rtts_micros) =
        match tokio::time::timeout(Duration::from_secs(5), lat_rx.recv()).await {
            Ok(Some(pair)) => pair,
            _ => (0, Vec::new()),
        };
    lat_handle.abort();
    let _ = lat_handle.await;

    Ok(CfTransferRaw {
        bytes: total.load(Ordering::Relaxed),
        duration_ms: active_ms,
        ticks,
        loaded_sent,
        loaded_rtts_micros,
        failed_requests: failed.load(Ordering::Relaxed),
    })
}

/// RFC 5389 STUN Binding Request UDP packet-loss and reordering probe.
pub async fn udp_loss(
    spec: &CfStunSpec,
    cancel: &CancellationToken,
) -> Result<CfStunRaw, NetError> {
    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    let ipv6 = spec.family == IpFamily::Ipv6;
    let candidates = resolve_stun_addrs(&spec.host, spec.port, ipv6).await?;
    let bind_addr = if ipv6 {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)
    } else {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
    };

    let mut connected: Option<UdpSocket> = None;
    let mut last_err = String::new();
    for addr in candidates {
        let sock = match UdpSocket::bind(bind_addr).await {
            Ok(s) => s,
            Err(err) => {
                last_err = err.to_string();
                continue;
            }
        };
        match sock.connect(addr).await {
            Ok(()) => {
                connected = Some(sock);
                break;
            }
            Err(err) => last_err = err.to_string(),
        }
    }
    let Some(sock) = connected else {
        return Err(NetError::Transport(if last_err.is_empty() {
            "no STUN UDP socket connected".to_owned()
        } else {
            last_err
        }));
    };

    let target = format!("stun:{}:{}", spec.host, spec.port);
    let attempts = u64::from(spec.packets.max(1));
    let mut seed = RandomState::new().hash_one(&target);

    let mut sent = 0_u64;
    let mut received = 0_u64;
    let mut rtts_micros = Vec::with_capacity(usize::from(spec.packets));
    let mut txid_to_seq: HashMap<[u8; 12], u64> = HashMap::new();
    let mut send_times: HashMap<u64, Instant> = HashMap::new();
    let mut arrived: HashSet<u64> = HashSet::new();
    let mut max_arrived_seq = 0_u64;
    let mut out_of_order = 0_u64;

    for seq in 1..=attempts {
        if cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        if received == 0 && sent >= UDP_UNREACHABLE_AFTER {
            break;
        }
        sent += 1;

        let txid = next_txid(&mut seed, seq);
        txid_to_seq.insert(txid, seq);
        let pkt = build_stun_binding_request(txid);
        let start = Instant::now();
        send_times.insert(seq, start);
        let _ = sock.send(&pkt).await;

        let deadline = start + spec.timeout;
        let mut buf = [0_u8; 1500];
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let recv_res = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(NetError::Cancelled),
                r = tokio::time::timeout(remaining, sock.recv(&mut buf)) => r,
            };
            match recv_res {
                Ok(Ok(len)) => {
                    let Some(rx_seq) = match_binding_response(&buf[..len], &txid_to_seq) else {
                        continue;
                    };
                    if !arrived.insert(rx_seq) {
                        continue;
                    }
                    if rx_seq < max_arrived_seq {
                        out_of_order += 1;
                    } else {
                        max_arrived_seq = rx_seq;
                    }
                    received += 1;
                    if let Some(t0) = send_times.get(&rx_seq) {
                        let micros = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
                        rtts_micros.push(micros.max(1));
                    }
                    if rx_seq == seq {
                        break;
                    }
                }
                Ok(Err(_)) | Err(_) => break,
            }
        }

        if seq < attempts {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(NetError::Cancelled),
                () = tokio::time::sleep(spec.interval) => {}
            }
        }
    }

    if sent > 0 && received == 0 {
        return Err(NetError::Transport(format!(
            "no STUN response from {target} after {sent} UDP probes (UDP likely blocked)"
        )));
    }

    Ok(CfStunRaw {
        target,
        sent,
        received,
        out_of_order,
        rtts_micros,
    })
}

async fn sample_ticks(
    duration: Duration,
    total: &AtomicU64,
    cancel: &CancellationToken,
) -> Result<(u64, Vec<(u64, u64)>), NetError> {
    let mut active = Duration::ZERO;
    let mut ticks = Vec::with_capacity(128);
    while active < duration {
        let tick_start = Instant::now();
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(NetError::Cancelled),
            () = tokio::time::sleep(SAMPLE_INTERVAL) => {}
        }
        active += tick_start.elapsed();
        let active_ms = u64::try_from(active.as_millis()).unwrap_or(u64::MAX);
        let now_bytes = total.load(Ordering::Relaxed);
        ticks.push((active_ms, now_bytes));
    }
    let total_ms = u64::try_from(active.as_millis()).unwrap_or(u64::MAX);
    Ok((total_ms, ticks))
}

#[allow(clippy::too_many_arguments)]
async fn run_loaded_probes(
    client: &Client,
    down_url: &Url,
    during: &str,
    duration: Duration,
    interval: Duration,
    timeout: Duration,
    stop: &AtomicBool,
    cancel: &CancellationToken,
) -> (u64, Vec<u64>) {
    let started = Instant::now();
    let mut sent = 0_u64;
    let mut rtts = Vec::new();
    let mut url = down_url.clone();
    url.query_pairs_mut()
        .append_pair("bytes", "0")
        .append_pair("during", during);

    while started.elapsed() < duration && !stop.load(Ordering::Relaxed) && !cancel.is_cancelled() {
        sent += 1;
        let t0 = Instant::now();
        let req = client.get(url.clone()).timeout(timeout).send();
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            res = req => res,
        };
        if let Ok(mut resp) = outcome
            && resp.status().is_success()
        {
            let mut ok = true;
            while let Some(chunk) = resp.chunk().await.transpose() {
                if chunk.is_err() {
                    ok = false;
                    break;
                }
            }
            if ok {
                let micros = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
                rtts.push(micros.max(1));
            }
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => break,
            () = tokio::time::sleep(interval) => {}
        }
    }
    (sent, rtts)
}

async fn build_client(family: IpFamily, raw_base: &str) -> Result<(Client, Url), NetError> {
    let base = Url::parse(raw_base).map_err(|err| NetError::Transport(err.to_string()))?;
    let scheme = base.scheme();
    let loopback = base
        .host_str()
        .and_then(|h| h.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback());
    if scheme != "https" && !(scheme == "http" && loopback) {
        return Err(NetError::Transport(format!(
            "non-HTTPS Cloudflare base URL: {raw_base}"
        )));
    }

    let ipv6 = family == IpFamily::Ipv6;
    let host = base
        .host_str()
        .ok_or_else(|| NetError::Transport("base URL has no host".to_owned()))?
        .trim_matches(['[', ']'])
        .to_owned();
    let port = base
        .port_or_known_default()
        .ok_or_else(|| NetError::Transport("base URL has unknown port".to_owned()))?;

    let local = if ipv6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::REFERER, HeaderValue::from_static(REFERER));

    let mut builder = fetcher::builder()
        .map_err(NetError::Transport)?
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .redirect(Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .tcp_keepalive(TCP_KEEPALIVE)
        .no_proxy()
        .local_address(local);

    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip.is_ipv6() != ipv6 {
            return Err(NetError::NoAddress);
        }
    } else {
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|err| NetError::Transport(err.to_string()))?
            .filter(|addr| addr.is_ipv6() == ipv6)
            .collect();
        if addrs.is_empty() {
            return Err(NetError::NoAddress);
        }
        builder = builder.resolve_to_addrs(&host, &addrs);
    }

    let client = builder
        .build()
        .map_err(|err| NetError::Transport(fetcher::causes(&err)))?;
    Ok((client, base))
}

async fn resolve_stun_addrs(
    host: &str,
    port: u16,
    ipv6: bool,
) -> Result<Vec<SocketAddr>, NetError> {
    let clean = host.trim_matches(['[', ']']);
    if let Ok(ip) = clean.parse::<IpAddr>() {
        return if ip.is_ipv6() == ipv6 {
            Ok(vec![SocketAddr::new(ip, port)])
        } else {
            Err(NetError::NoAddress)
        };
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((clean, port))
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?
        .filter(|addr| addr.is_ipv6() == ipv6)
        .collect();
    if addrs.is_empty() {
        Err(NetError::NoAddress)
    } else {
        Ok(addrs)
    }
}

fn next_txid(state: &mut u64, seq: u64) -> [u8; 12] {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(seq ^ 0x9e37_79b9_7f4a_7c15);
    let hi = *state;
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let lo = (*state >> 32) as u32;
    let mut out = [0_u8; 12];
    out[..8].copy_from_slice(&hi.to_le_bytes());
    out[8..12].copy_from_slice(&lo.to_le_bytes());
    out
}

fn build_stun_binding_request(txid: [u8; 12]) -> [u8; 20] {
    let mut pkt = [0_u8; 20];
    pkt[0] = 0x00;
    pkt[1] = 0x01;
    pkt[4] = 0x21;
    pkt[5] = 0x12;
    pkt[6] = 0xa4;
    pkt[7] = 0x42;
    pkt[8..20].copy_from_slice(&txid);
    pkt
}

fn match_binding_response(buf: &[u8], txid_to_seq: &HashMap<[u8; 12], u64>) -> Option<u64> {
    if buf.len() < 20 || buf[0] != 0x01 || buf[1] != 0x01 {
        return None;
    }
    if buf[4..8] != [0x21, 0x12, 0xa4, 0x42] {
        return None;
    }
    let mut txid = [0_u8; 12];
    txid.copy_from_slice(&buf[8..20]);
    txid_to_seq.get(&txid).copied()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use bench_api::CancellationToken;
    use bench_api::CfStunSpec;
    use bench_api::CfTransferSpec;
    use bench_api::IpFamily;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::UdpSocket;

    #[tokio::test]
    async fn cf_download_and_upload_stream_over_loopback_and_handle_429_backoff() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind tcp");
        let addr = listener.local_addr().expect("local addr");
        let hit_429 = Arc::new(AtomicUsize::new(0));
        let hit_429_srv = Arc::clone(&hit_429);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0_u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let head = String::from_utf8_lossy(&buf[..n]);
                    let first = head.lines().next().unwrap_or_default();
                    if first.starts_with("GET /__down?") && first.contains("bytes=0") {
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\n\
                                  Content-Length: 0\r\n\
                                  Connection: close\r\n\r\n",
                            )
                            .await;
                    } else if first.starts_with("GET /__down?") && first.contains("bytes=400000") {
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 429 Too Many Requests\r\n\
                                  Content-Length: 0\r\n\
                                  Connection: close\r\n\r\n",
                            )
                            .await;
                    } else if first.starts_with("GET /__down?") {
                        let body = vec![b'x'; 32_768];
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes()).await;
                        let _ = stream.write_all(&body).await;
                    } else if first.starts_with("POST /__up?") {
                        while !buf[..n].windows(5).any(|w| w == b"0\r\n\r\n") {
                            let mut more = [0_u8; 8192];
                            let r = stream.read(&mut more).await.unwrap_or(0);
                            if r == 0 || more[..r].windows(5).any(|w| w == b"0\r\n\r\n") {
                                break;
                            }
                        }
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\n\
                                  Content-Length: 2\r\n\
                                  Connection: close\r\n\r\nok",
                            )
                            .await;
                    }
                });
                let _ = &hit_429_srv;
            }
        });

        let spec = CfTransferSpec {
            family: IpFamily::Ipv4,
            base_url: format!("http://{addr}"),
            meas_id: "987654321".to_owned(),
            concurrency: 1,
            duration: Duration::from_millis(450),
            bytes_per_req: 400_000,
            probe_interval: Duration::from_millis(120),
            probe_timeout: Duration::from_secs(2),
        };
        let cancel = CancellationToken::new();
        let down = super::download(&spec, &cancel).await.expect("download");
        assert!(down.bytes >= 32_768, "backed off from 429 and downloaded");
        assert!(down.failed_requests >= 1, "recorded initial 429");
        assert!(!down.ticks.is_empty());
        assert!(!down.loaded_rtts_micros.is_empty());

        let up_spec = CfTransferSpec {
            bytes_per_req: 65_536,
            ..spec
        };
        let up = super::upload(&up_spec, &cancel).await.expect("upload");
        assert!(up.bytes >= 65_536);
        assert_eq!(hit_429.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn cf_stun_udp_measures_rtts_and_rejects_silent_blackhole() {
        let server = UdpSocket::bind("127.0.0.1:0").await.expect("bind stun");
        let stun_addr = server.local_addr().expect("stun addr");
        tokio::spawn(async move {
            let mut buf = [0_u8; 256];
            while let Ok((len, peer)) = server.recv_from(&mut buf).await {
                if len >= 20 && buf[0..2] == [0x00, 0x01] {
                    let mut resp = [0_u8; 20];
                    resp[0..2].copy_from_slice(&[0x01, 0x01]);
                    resp[4..20].copy_from_slice(&buf[4..20]);
                    let _ = server.send_to(&resp, peer).await;
                }
            }
        });

        let cancel = CancellationToken::new();
        let ok_spec = CfStunSpec {
            family: IpFamily::Ipv4,
            host: "127.0.0.1".to_owned(),
            port: stun_addr.port(),
            packets: 4,
            interval: Duration::from_millis(10),
            timeout: Duration::from_millis(200),
        };
        let res = super::udp_loss(&ok_spec, &cancel).await.expect("stun ok");
        assert_eq!(res.sent, 4);
        assert_eq!(res.received, 4);
        assert_eq!(res.out_of_order, 0);
        assert_eq!(res.rtts_micros.len(), 4);

        let blackhole = UdpSocket::bind("127.0.0.1:0").await.expect("blackhole");
        let bh_port = blackhole.local_addr().expect("bh addr").port();
        let bh_spec = CfStunSpec {
            port: bh_port,
            packets: 2,
            timeout: Duration::from_millis(30),
            ..ok_spec
        };
        assert!(super::udp_loss(&bh_spec, &cancel).await.is_err());
    }
}
