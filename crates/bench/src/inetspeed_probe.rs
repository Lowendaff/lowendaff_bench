//! Direct, proxy-free HTTP/HTTPS prober and transfer engine for Apple CDN (`network.inetspeed`),
//! implementing `iNetSpeed-CLI`'s `mensura.cdn-apple.com` / `networkQuality` protocol with
//! SNI-preserving endpoint IP pinning.

use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::FamilyFilter;
use bench_api::InetspeedDirection;
use bench_api::InetspeedTarget;
use bench_api::InetspeedTransfer;
use bench_api::InetspeedTransferSpec;
use bench_api::NetError;
use futures_util::stream;
use reqwest::Body;
use reqwest::Client;
use reqwest::Url;
use reqwest::header;
use reqwest::redirect::Policy;
use tokio::task::JoinSet;

use crate::fetcher;

const USER_AGENT: &str = "networkQuality/194.80.3 CFNetwork/3860.400.51 Darwin/25.3.0";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const IDLE_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const UPLOAD_CHUNK_SIZE: u64 = 64 * 1024;

/// System DNS fallback resolving `host` and filtering by reachable IP families (IPv4 first, then
/// IPv6, matching `iNetSpeed-CLI`).
pub async fn resolve_system(
    host: &str,
    families: FamilyFilter,
    timeout: Duration,
) -> Result<Vec<IpAddr>, NetError> {
    let clean = host.trim_matches(['[', ']']);
    let parsed_ip: Result<IpAddr, _> = clean.parse();
    if let Ok(ip) = parsed_ip {
        let allowed = ((ip.is_ipv4() && families.ipv4) || (ip.is_ipv6() && families.ipv6))
            && bench_api::is_routable_endpoint_ip(ip);
        return if allowed {
            Ok(vec![ip])
        } else {
            Err(NetError::NoAddress)
        };
    }
    let lookup = tokio::net::lookup_host((clean, 443));
    let resolved = match tokio::time::timeout(timeout, lookup).await {
        Ok(Ok(addrs)) => addrs,
        Ok(Err(err)) => return Err(NetError::Transport(err.to_string())),
        Err(_) => return Err(NetError::TimedOut),
    };

    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    for addr in resolved {
        if !bench_api::is_routable_endpoint_ip(addr.ip()) {
            continue;
        }
        match addr.ip() {
            IpAddr::V4(ip) if families.ipv4 => {
                let ip = IpAddr::V4(ip);
                if !v4.contains(&ip) {
                    v4.push(ip);
                }
            }
            IpAddr::V6(ip) if families.ipv6 => {
                let ip = IpAddr::V6(ip);
                if !v6.contains(&ip) {
                    v6.push(ip);
                }
            }
            _ => {}
        }
    }
    v4.extend(v6);
    if v4.is_empty() {
        Err(NetError::NoAddress)
    } else {
        Ok(v4)
    }
}

/// Measures `count` sequential `GET` RTT probes in microseconds against `target` (pinned to
/// `target.pin_ip` when set).
pub async fn probe_rtt(
    target: &InetspeedTarget,
    count: u8,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Vec<u64>, NetError> {
    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    let url = parse_allowed_url(&target.url)?;
    let client = build_pinned_client(&url, target.pin_ip, timeout.saturating_add(CONNECT_TIMEOUT))?;
    let total = usize::from(count.max(1));
    let mut samples = Vec::with_capacity(total);

    for _ in 0..total {
        if cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        let sample = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(NetError::Cancelled),
            res = single_probe(&client, url.clone(), timeout) => res,
        };
        if let Some(micros) = sample {
            samples.push(micros);
        }
    }
    if samples.is_empty() && count == 1 {
        return Err(NetError::Transport("endpoint RTT probe failed".to_owned()));
    }
    Ok(samples)
}

/// Runs a 1-thread or multi-thread Apple CDN download (`GET`) or upload (`PUT`) round with a
/// concurrent background loaded-latency probe loop.
pub async fn transfer(
    spec: &InetspeedTransferSpec,
    cancel: &CancellationToken,
) -> Result<InetspeedTransfer, NetError> {
    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }
    let transfer_url = parse_allowed_url(&spec.transfer_url)?;
    let latency_url = parse_allowed_url(&spec.latency_url)?;
    let client_timeout = spec.timeout.saturating_add(Duration::from_secs(5));
    let client = build_pinned_client(&transfer_url, spec.pin_ip, client_timeout)?;
    let lat_client = if transfer_url.host_str() == latency_url.host_str()
        && transfer_url.port_or_known_default() == latency_url.port_or_known_default()
    {
        client.clone()
    } else {
        build_pinned_client(&latency_url, spec.pin_ip, client_timeout)?
    };

    let stop_loaded = Arc::new(AtomicBool::new(false));
    let loaded_samples: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let loaded_samples_bg = Arc::clone(&loaded_samples);
    let stop_loaded_bg = Arc::clone(&stop_loaded);
    let cancel_loaded = cancel.clone();
    let mut lat_tasks = JoinSet::new();
    lat_tasks.spawn(async move {
        while !stop_loaded_bg.load(Ordering::Relaxed) && !cancel_loaded.is_cancelled() {
            let sample = tokio::select! {
                biased;
                () = cancel_loaded.cancelled() => break,
                res = single_probe(&lat_client, latency_url.clone(), IDLE_PROBE_TIMEOUT) => res,
            };
            if let Some(micros) = sample
                && let Ok(mut guard) = loaded_samples_bg.lock()
            {
                guard.push(micros);
            }
        }
    });

    let total_bytes = Arc::new(AtomicU64::new(0));
    let fault_count = Arc::new(AtomicU8::new(0));
    let workers = usize::from(spec.threads.max(1));
    let started = Instant::now();

    let mut worker_tasks = JoinSet::new();
    for _ in 0..workers {
        let http = client.clone();
        let url = transfer_url.clone();
        let direction = spec.direction;
        let max_bytes = spec.max_bytes.max(1);
        let timeout = spec.timeout;
        let bytes_ref = Arc::clone(&total_bytes);
        let faults_ref = Arc::clone(&fault_count);
        let cancel_w = cancel.clone();

        worker_tasks.spawn(async move {
            let fault = match direction {
                InetspeedDirection::Download => {
                    run_download_worker(&http, url, max_bytes, timeout, &bytes_ref, &cancel_w).await
                }
                InetspeedDirection::Upload => {
                    run_upload_worker(&http, url, max_bytes, timeout, &bytes_ref, &cancel_w).await
                }
            };
            if fault && !cancel_w.is_cancelled() {
                faults_ref.fetch_add(1, Ordering::Relaxed);
            }
        });
    }

    let round_cap = spec.timeout.saturating_add(Duration::from_secs(6));
    let wait_all = async { while worker_tasks.join_next().await.is_some() {} };
    if tokio::time::timeout(round_cap, wait_all).await.is_err() {
        let remaining = u8::try_from(worker_tasks.len()).unwrap_or(u8::MAX);
        fault_count.fetch_add(remaining.max(1), Ordering::Relaxed);
        worker_tasks.abort_all();
        while worker_tasks.join_next().await.is_some() {}
    }

    let elapsed_micros = u64::try_from(started.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1);
    stop_loaded.store(true, Ordering::Relaxed);
    lat_tasks.abort_all();
    while lat_tasks.join_next().await.is_some() {}

    if cancel.is_cancelled() {
        return Err(NetError::Cancelled);
    }

    let loaded_rtts_micros = loaded_samples
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();

    Ok(InetspeedTransfer {
        bytes: total_bytes.load(Ordering::Relaxed),
        elapsed_micros,
        fault_count: fault_count.load(Ordering::Relaxed),
        loaded_rtts_micros,
    })
}

async fn single_probe(client: &Client, url: Url, timeout: Duration) -> Option<u64> {
    let started = Instant::now();
    let req = client
        .get(url)
        .header(header::USER_AGENT, USER_AGENT)
        .header(header::ACCEPT, "*/*")
        .header(header::ACCEPT_ENCODING, "identity")
        .timeout(timeout)
        .send();
    let Ok(Ok(mut resp)) = tokio::time::timeout(timeout, req).await else {
        return None;
    };
    if resp.status().as_u16() >= 400 {
        return None;
    }
    loop {
        match tokio::time::timeout(timeout, resp.chunk()).await {
            Ok(Ok(Some(_))) => {}
            Ok(Ok(None)) => break,
            Ok(Err(_)) | Err(_) => return None,
        }
    }
    let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    Some(micros.max(10))
}

async fn run_download_worker(
    client: &Client,
    url: Url,
    max_bytes: u64,
    timeout: Duration,
    shared_bytes: &AtomicU64,
    cancel: &CancellationToken,
) -> bool {
    let started = Instant::now();
    let req = client
        .get(url)
        .header(header::USER_AGENT, USER_AGENT)
        .header(header::ACCEPT, "*/*")
        .header(header::ACCEPT_ENCODING, "identity")
        .send();
    let resp = tokio::select! {
        biased;
        () = cancel.cancelled() => return false,
        res = tokio::time::timeout(timeout, req) => match res {
            Ok(Ok(r)) => r,
            Ok(Err(_)) | Err(_) => return true,
        }
    };
    if resp.status().as_u16() >= 400 {
        return true;
    }

    let mut resp = resp;
    let mut worker_total = 0_u64;
    while worker_total < max_bytes && started.elapsed() < timeout && !cancel.is_cancelled() {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            break;
        };
        let next = tokio::select! {
            biased;
            () = cancel.cancelled() => return false,
            res = tokio::time::timeout(remaining, resp.chunk()) => res,
        };
        match next {
            Ok(Ok(Some(chunk))) => {
                let len = chunk.len() as u64;
                worker_total = worker_total.saturating_add(len);
                shared_bytes.fetch_add(len, Ordering::Relaxed);
            }
            Ok(Ok(None)) | Err(_) => break,
            Ok(Err(_)) => return worker_total == 0,
        }
    }
    worker_total == 0
}

async fn run_upload_worker(
    client: &Client,
    url: Url,
    max_bytes: u64,
    timeout: Duration,
    shared_bytes: &Arc<AtomicU64>,
    cancel: &CancellationToken,
) -> bool {
    let started = Instant::now();
    let counted = Arc::new(AtomicU64::new(0));
    let shared_s = Arc::clone(shared_bytes);
    let counted_s = Arc::clone(&counted);
    let cancel_s = cancel.clone();

    let body_stream = stream::unfold(max_bytes, move |remaining| {
        let shared_c = Arc::clone(&shared_s);
        let counted_c = Arc::clone(&counted_s);
        let cancel_c = cancel_s.clone();
        async move {
            if remaining == 0 || started.elapsed() >= timeout || cancel_c.is_cancelled() {
                return None;
            }
            let step = remaining.min(UPLOAD_CHUNK_SIZE);
            let chunk_len = usize::try_from(step).unwrap_or(65_536);
            shared_c.fetch_add(step, Ordering::Relaxed);
            counted_c.fetch_add(step, Ordering::Relaxed);
            Some((
                Ok::<Vec<u8>, io::Error>(vec![0_u8; chunk_len]),
                remaining - step,
            ))
        }
    });

    let body = Body::wrap_stream(body_stream);
    let hard_limit = timeout.saturating_add(Duration::from_secs(5));
    let req_fut = async {
        let mut resp = client
            .put(url)
            .header(header::USER_AGENT, USER_AGENT)
            .header(header::ACCEPT, "*/*")
            .header(header::ACCEPT_ENCODING, "identity")
            .header("Upload-Draft-Interop-Version", "6")
            .header("Upload-Complete", "?1")
            .body(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        while resp.chunk().await?.is_some() {}
        Ok::<u16, reqwest::Error>(status)
    };

    let outcome = tokio::select! {
        biased;
        () = cancel.cancelled() => return false,
        res = tokio::time::timeout(hard_limit, req_fut) => res,
    };

    match outcome {
        Ok(Ok(status)) if status < 400 => counted.load(Ordering::Relaxed) == 0,
        _ => {
            let sent = counted.load(Ordering::Relaxed);
            let mut cur = shared_bytes.load(Ordering::Relaxed);
            while let Err(actual) = shared_bytes.compare_exchange_weak(
                cur,
                cur.saturating_sub(sent),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                cur = actual;
            }
            true
        }
    }
}

fn parse_allowed_url(raw: &str) -> Result<Url, NetError> {
    let parsed = Url::parse(raw).map_err(|err| NetError::Transport(err.to_string()))?;
    let loopback = parsed
        .host_str()
        .and_then(|h| h.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback());
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if loopback => Ok(parsed),
        _ => Err(NetError::Transport(format!(
            "non-HTTPS Apple CDN endpoint URL: {raw}"
        ))),
    }
}

fn build_pinned_client(
    url: &Url,
    pin_ip: Option<IpAddr>,
    timeout: Duration,
) -> Result<Client, NetError> {
    let host = url
        .host_str()
        .ok_or_else(|| NetError::Transport("endpoint URL has no host".to_owned()))?
        .trim_matches(['[', ']']);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| NetError::Transport("endpoint URL has unknown port".to_owned()))?;

    let mut builder = fetcher::builder()
        .map_err(NetError::Transport)?
        .user_agent(USER_AGENT)
        .redirect(Policy::none())
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT)
        .no_proxy();

    if let Some(ip) = pin_ip {
        let local = if ip.is_ipv6() {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        };
        builder = builder.local_address(local);
        if host.parse::<IpAddr>().is_err() {
            builder = builder.resolve(host, SocketAddr::new(ip, port));
        }
    } else if let Ok(ip) = host.parse::<IpAddr>() {
        let local = if ip.is_ipv6() {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        };
        builder = builder.local_address(local);
    }

    builder
        .build()
        .map_err(|err| NetError::Transport(fetcher::causes(&err)))
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use bench_api::CancellationToken;
    use bench_api::InetspeedDirection;
    use bench_api::InetspeedTarget;
    use bench_api::InetspeedTransferSpec;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn inetspeed_probe_measures_rtt_downloads_uploads_and_rolls_back_failed_puts() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let put_headers = Arc::new(Mutex::new(Vec::new()));
        let put_headers_srv = Arc::clone(&put_headers);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let put_rec = Arc::clone(&put_headers_srv);
                tokio::spawn(async move {
                    let mut buf = vec![0_u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let first = text.lines().next().unwrap_or_default();
                    if first.starts_with("GET /api/v1/gm/small ") {
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\n\
                                  Content-Length: 2\r\n\
                                  Connection: close\r\n\r\nok",
                            )
                            .await;
                    } else if first.starts_with("GET /api/v1/gm/large ") {
                        let body = vec![b'a'; 65_536];
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes()).await;
                        let _ = stream.write_all(&body).await;
                    } else if first.starts_with("PUT /api/v1/gm/slurp ") {
                        put_rec.lock().expect("lock").push(text.clone());
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
                                  Content-Length: 0\r\n\
                                  Connection: close\r\n\r\n",
                            )
                            .await;
                    } else if first.starts_with("PUT /api/v1/gm/fail ") {
                        while !buf[..n].windows(5).any(|w| w == b"0\r\n\r\n") {
                            let mut more = [0_u8; 8192];
                            let r = stream.read(&mut more).await.unwrap_or(0);
                            if r == 0 || more[..r].windows(5).any(|w| w == b"0\r\n\r\n") {
                                break;
                            }
                        }
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 500 Internal Server Error\r\n\
                                  Content-Length: 0\r\n\
                                  Connection: close\r\n\r\n",
                            )
                            .await;
                    }
                });
            }
        });

        let cancel = CancellationToken::new();
        let small = format!("http://{addr}/api/v1/gm/small");
        let rtts = super::probe_rtt(
            &InetspeedTarget {
                url: small.clone(),
                pin_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            },
            3,
            Duration::from_secs(2),
            &cancel,
        )
        .await
        .expect("rtts");
        assert_eq!(rtts.len(), 3);

        let dl = super::transfer(
            &InetspeedTransferSpec {
                direction: InetspeedDirection::Download,
                transfer_url: format!("http://{addr}/api/v1/gm/large"),
                latency_url: small.clone(),
                pin_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                threads: 2,
                max_bytes: 65_536,
                timeout: Duration::from_secs(2),
            },
            &cancel,
        )
        .await
        .expect("download");
        assert_eq!(dl.bytes, 131_072);
        assert_eq!(dl.fault_count, 0);

        let ul = super::transfer(
            &InetspeedTransferSpec {
                direction: InetspeedDirection::Upload,
                transfer_url: format!("http://{addr}/api/v1/gm/slurp"),
                latency_url: small.clone(),
                pin_ip: None,
                threads: 1,
                max_bytes: 65_536,
                timeout: Duration::from_secs(2),
            },
            &cancel,
        )
        .await
        .expect("upload");
        assert_eq!(ul.bytes, 65_536);
        assert_eq!(ul.fault_count, 0);
        let seen_put = put_headers.lock().expect("lock").clone();
        assert!(!seen_put.is_empty());
        assert!(
            seen_put[0]
                .to_ascii_lowercase()
                .contains("upload-draft-interop-version: 6")
        );

        let failed_ul = super::transfer(
            &InetspeedTransferSpec {
                direction: InetspeedDirection::Upload,
                transfer_url: format!("http://{addr}/api/v1/gm/fail"),
                latency_url: small,
                pin_ip: None,
                threads: 1,
                max_bytes: 65_536,
                timeout: Duration::from_secs(2),
            },
            &cancel,
        )
        .await
        .expect("failed upload returns raw");
        assert_eq!(failed_ul.bytes, 0, "500 status rolls back counted bytes");
        assert_eq!(failed_ul.fault_count, 1);
    }
}
