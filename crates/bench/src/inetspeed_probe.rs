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
const LOADED_PROBE_INTERVAL: Duration = Duration::from_millis(200);
const MAX_PROBE_BODY_BYTES: usize = 64 * 1024;
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
    let mut last_err: Option<NetError> = None;

    for _ in 0..total {
        if cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        let sample = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(NetError::Cancelled),
            res = single_probe(&client, url.clone(), timeout) => res,
        };
        match sample {
            Ok(micros) => samples.push(micros),
            Err(err) => last_err = Some(err),
        }
    }
    if samples.is_empty() && total == 1 {
        return Err(
            last_err.unwrap_or_else(|| NetError::Transport("endpoint RTT probe failed".to_owned()))
        );
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
            if let Ok(micros) = sample
                && let Ok(mut guard) = loaded_samples_bg.lock()
            {
                guard.push(micros);
            }
            if stop_loaded_bg.load(Ordering::Relaxed) || cancel_loaded.is_cancelled() {
                break;
            }
            tokio::select! {
                biased;
                () = cancel_loaded.cancelled() => break,
                () = tokio::time::sleep(LOADED_PROBE_INTERVAL) => {}
            }
        }
    });

    let total_bytes = Arc::new(AtomicU64::new(0));
    let workers = usize::from(spec.threads.max(1));
    let started = Instant::now();

    let mut worker_tasks: JoinSet<bool> = JoinSet::new();
    for _ in 0..workers {
        let http = client.clone();
        let url = transfer_url.clone();
        let direction = spec.direction;
        let max_bytes = spec.max_bytes.max(1);
        let timeout = spec.timeout;
        let bytes_ref = Arc::clone(&total_bytes);
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
            fault && !cancel_w.is_cancelled()
        });
    }

    let mut faults = 0_u8;
    let round_cap = spec.timeout.saturating_add(Duration::from_secs(6));
    let round_deadline = tokio::time::Instant::now() + round_cap;
    loop {
        if worker_tasks.is_empty() {
            break;
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                worker_tasks.abort_all();
                while worker_tasks.join_next().await.is_some() {}
                break;
            }
            () = tokio::time::sleep_until(round_deadline) => {
                let remaining = u8::try_from(worker_tasks.len()).unwrap_or(u8::MAX);
                faults = faults.saturating_add(remaining.max(1));
                worker_tasks.abort_all();
                while worker_tasks.join_next().await.is_some() {}
                break;
            }
            joined = worker_tasks.join_next() => match joined {
                Some(Ok(true)) => faults = faults.saturating_add(1),
                Some(Err(err)) if !err.is_cancelled() && !cancel.is_cancelled() => {
                    faults = faults.saturating_add(1);
                }
                Some(Ok(false) | Err(_)) => {}
                None => break,
            },
        }
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
        fault_count: faults,
        loaded_rtts_micros,
    })
}

async fn single_probe(client: &Client, url: Url, timeout: Duration) -> Result<u64, NetError> {
    let started = Instant::now();
    let req = client
        .get(url)
        .header(header::USER_AGENT, USER_AGENT)
        .header(header::ACCEPT, "*/*")
        .header(header::ACCEPT_ENCODING, "identity")
        .timeout(timeout)
        .send();
    let mut resp = match tokio::time::timeout(timeout, req).await {
        Ok(Ok(r)) => r,
        Ok(Err(err)) => {
            return Err(if err.is_timeout() {
                NetError::TimedOut
            } else {
                NetError::Transport(fetcher::causes(&err))
            });
        }
        Err(_) => return Err(NetError::TimedOut),
    };
    let status = resp.status().as_u16();
    if status >= 400 {
        return Err(NetError::Transport(format!(
            "endpoint RTT probe failed: HTTP {status}"
        )));
    }
    let mut drained = 0_usize;
    loop {
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            return Err(NetError::TimedOut);
        };
        match tokio::time::timeout(remaining, resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                drained = drained.saturating_add(chunk.len());
                if drained > MAX_PROBE_BODY_BYTES {
                    return Err(NetError::Transport(
                        "RTT probe response exceeded 64 KiB".to_owned(),
                    ));
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(err)) => {
                return Err(if err.is_timeout() {
                    NetError::TimedOut
                } else {
                    NetError::Transport(fetcher::causes(&err))
                });
            }
            Err(_) => return Err(NetError::TimedOut),
        }
    }
    let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    Ok(micros.max(10))
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

struct StreamStopGuard(Arc<AtomicBool>);

impl Drop for StreamStopGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
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
    let stopped = Arc::new(AtomicBool::new(false));
    let _stop_on_exit = StreamStopGuard(Arc::clone(&stopped));
    let counted_s = Arc::clone(&counted);
    let stopped_s = Arc::clone(&stopped);
    let cancel_s = cancel.clone();

    let body_stream = stream::unfold(max_bytes, move |remaining| {
        let counted_c = Arc::clone(&counted_s);
        let stopped_c = Arc::clone(&stopped_s);
        let cancel_c = cancel_s.clone();
        async move {
            if remaining == 0
                || started.elapsed() >= timeout
                || stopped_c.load(Ordering::Relaxed)
                || cancel_c.is_cancelled()
            {
                return None;
            }
            let step = remaining.min(UPLOAD_CHUNK_SIZE);
            let chunk_len = usize::try_from(step).unwrap_or(65_536);
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
        Ok(Ok(status)) if status < 400 => {
            let sent = counted.load(Ordering::Relaxed);
            shared_bytes.fetch_add(sent, Ordering::Relaxed);
            sent == 0
        }
        _ => true,
    }
}

fn parse_allowed_url(raw: &str) -> Result<Url, NetError> {
    let parsed = Url::parse(raw).map_err(|err| NetError::Transport(err.to_string()))?;
    let host_clean = parsed
        .host_str()
        .map(|h| h.trim_matches(['[', ']']))
        .unwrap_or_default();
    let host_ip = host_clean.parse::<IpAddr>().ok();
    match parsed.scheme() {
        "https" => {
            if let Some(ip) = host_ip {
                if !bench_api::is_routable_endpoint_ip(ip) {
                    return Err(NetError::Transport(
                        "non-public Apple CDN endpoint address refused".to_owned(),
                    ));
                }
            } else {
                let lower = host_clean.to_ascii_lowercase();
                if lower == "localhost" || lower.ends_with(".localhost") {
                    return Err(NetError::Transport(
                        "non-public Apple CDN endpoint host refused".to_owned(),
                    ));
                }
            }
            Ok(parsed)
        }
        "http" if host_ip.is_some_and(|ip| ip.is_loopback()) => Ok(parsed),
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
    let host_ip = host.parse::<IpAddr>().ok();
    let allow_loopback = url.scheme() == "http" && host_ip.is_some_and(|ip| ip.is_loopback());

    let mut builder = fetcher::builder()
        .map_err(NetError::Transport)?
        .user_agent(USER_AGENT)
        .redirect(Policy::none())
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT)
        .no_proxy();

    if let Some(ip) = pin_ip {
        let permitted =
            (allow_loopback && ip.is_loopback()) || bench_api::is_routable_endpoint_ip(ip);
        if !permitted {
            return Err(NetError::Transport(
                "non-public pinned endpoint IP refused".to_owned(),
            ));
        }
        let local = if ip.is_ipv6() {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        };
        builder = builder.local_address(local);
        if host_ip.is_none() {
            builder = builder.resolve(host, SocketAddr::new(ip, port));
        }
    } else if let Some(ip) = host_ip {
        let local = if ip.is_ipv6() {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        };
        builder = builder.local_address(local);
    } else {
        builder = builder.dns_resolver(Arc::new(fetcher::PublicDnsResolver::new(None)));
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
                    let mut req = Vec::with_capacity(8192);
                    let header_end = loop {
                        if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos + 4;
                        }
                        let mut chunk = [0_u8; 4096];
                        let n = stream.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        req.extend_from_slice(&chunk[..n]);
                    };
                    let text = String::from_utf8_lossy(&req[..header_end]).into_owned();
                    let first = text.lines().next().unwrap_or_default();
                    if first.starts_with("GET /api/v1/gm/small ") {
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\n\
                                  Content-Length: 2\r\n\
                                  Connection: close\r\n\r\nok",
                            )
                            .await;
                    } else if first.starts_with("GET /api/v1/gm/slow ") {
                        tokio::time::sleep(Duration::from_millis(250)).await;
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
                        while !req[header_end..].windows(5).any(|w| w == b"0\r\n\r\n") {
                            let mut more = [0_u8; 8192];
                            let r = stream.read(&mut more).await.unwrap_or(0);
                            if r == 0 {
                                break;
                            }
                            req.extend_from_slice(&more[..r]);
                        }
                        let _ = stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\n\
                                  Content-Length: 0\r\n\
                                  Connection: close\r\n\r\n",
                            )
                            .await;
                    } else if first.starts_with("PUT /api/v1/gm/fail ") {
                        while !req[header_end..].windows(5).any(|w| w == b"0\r\n\r\n") {
                            let mut more = [0_u8; 8192];
                            let r = stream.read(&mut more).await.unwrap_or(0);
                            if r == 0 {
                                break;
                            }
                            req.extend_from_slice(&more[..r]);
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

        for non_public_pin in [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
        ] {
            let blocked = super::probe_rtt(
                &InetspeedTarget {
                    url: "https://mensura.cdn-apple.com/api/v1/gm/small".to_owned(),
                    pin_ip: Some(non_public_pin),
                },
                1,
                Duration::from_millis(50),
                &cancel,
            )
            .await;
            assert!(
                blocked.is_err(),
                "HTTPS probe pinned to non-public IP {non_public_pin} must fail"
            );
        }
        for blocked_url in [
            "https://127.0.0.1/api/v1/gm/small",
            "https://169.254.169.254/api/v1/gm/small",
            "https://localhost/api/v1/gm/small",
        ] {
            let blocked = super::probe_rtt(
                &InetspeedTarget {
                    url: blocked_url.to_owned(),
                    pin_ip: None,
                },
                1,
                Duration::from_millis(50),
                &cancel,
            )
            .await;
            assert!(
                blocked.is_err(),
                "non-public HTTPS URL {blocked_url} must fail"
            );
        }

        let timed_out = super::probe_rtt(
            &InetspeedTarget {
                url: format!("http://{addr}/api/v1/gm/slow"),
                pin_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            },
            1,
            Duration::from_millis(40),
            &cancel,
        )
        .await;
        assert!(
            matches!(timed_out, Err(bench_api::NetError::TimedOut)),
            "single slow RTT probe must surface TimedOut: {timed_out:?}"
        );
    }
}
