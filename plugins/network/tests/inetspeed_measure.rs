//! End-to-end measurement state-machine tests for `network.inetspeed`: Dual DoH candidate merging,
//! lowest-RTT endpoint selection and pinning, idle latency, and 1-thread / multi-thread download
//! and upload transfer rounds.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::DnsAnswer;
use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::InetspeedDirection;
use bench_api::InetspeedTarget;
use bench_api::InetspeedTransfer;
use bench_api::InetspeedTransferSpec;
use bench_api::IpFamily;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::SmtpBanner;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_network::inetspeed::EndpointSource;
use plugin_network::inetspeed::Output;
use plugin_network::inetspeed::Settings;
use plugin_network::inetspeed::TransferDirection;
use plugin_network::inetspeed::measure;
use plugin_network::inetspeed::measure::Endpoints;
use tokio::time::Instant;

struct MockDohHttp;

impl HttpClient for MockDohHttp {
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
        let url = request.url.clone();
        Box::pin(async move {
            let body: Vec<u8> = if url.contains("cf-doh.example") && url.contains("type=A") {
                br#"{"Answer":[{"data":"198.51.100.10"},{"data":"198.51.100.11"}]}"#.to_vec()
            } else if url.contains("ali-doh.example") && url.contains("type=A") {
                br#"["198.51.100.12","198.51.100.10"]"#.to_vec()
            } else {
                return Err(HttpError::Transport("no AAAA".to_owned()));
            };
            Ok(HttpResponse {
                status: 200,
                final_url: url,
                headers: Vec::new(),
                body,
            })
        })
    }
}

#[derive(Default)]
struct MockInetspeedNet {
    pinned_transfers: Mutex<Vec<(InetspeedDirection, u8, Option<IpAddr>)>>,
}

impl NetClient for MockInetspeedNet {
    fn dns_a<'a>(
        &'a self,
        _qname: &'a str,
        _server: Option<SocketAddr>,
        _timeout: Duration,
    ) -> NetFuture<'a, DnsAnswer> {
        Box::pin(async { Err(NetError::NoResolver) })
    }

    fn smtp_banner<'a>(
        &'a self,
        _family: IpFamily,
        _host: &'a str,
        _port: u16,
        _timeout: Duration,
    ) -> NetFuture<'a, SmtpBanner> {
        Box::pin(async { Err(NetError::TimedOut) })
    }

    fn inetspeed_probe_rtt<'a>(
        &'a self,
        target: &'a InetspeedTarget,
        count: u8,
        _timeout: Duration,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, Vec<u64>> {
        let pin = target.pin_ip;
        Box::pin(async move {
            if count == 1 {
                match pin.map(|ip| ip.to_string()).as_deref() {
                    Some("198.51.100.10") => Ok(vec![25_000]),
                    Some("198.51.100.11") => Err(NetError::TimedOut),
                    Some("198.51.100.12") => Ok(vec![8_400]),
                    _ => Ok(vec![15_000]),
                }
            } else {
                Ok(vec![8_200, 8_500, 8_800, 9_100])
            }
        })
    }

    fn inetspeed_transfer<'a>(
        &'a self,
        spec: &'a InetspeedTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, InetspeedTransfer> {
        self.pinned_transfers.lock().expect("lock").push((
            spec.direction,
            spec.threads,
            spec.pin_ip,
        ));
        let mult = u64::from(spec.threads);
        Box::pin(async move {
            Ok(InetspeedTransfer {
                bytes: 125_000_000 * mult,
                elapsed_micros: 1_000_000,
                fault_count: 0,
                loaded_rtts_micros: vec![12_000, 14_000, 16_000],
            })
        })
    }
}

#[tokio::test]
async fn inetspeed_discovers_lowest_rtt_candidate_via_dual_doh_and_runs_four_rounds() {
    let v4_sock = SocketAddr::from(([198, 51, 100, 1], 443));
    let host = HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![(v4_sock, Reach::Connected)],
                ..Family::default()
            },
            ..Network::default()
        },
        ..HostProfile::default()
    };

    let net = Arc::new(MockInetspeedNet::default());
    let (ctx, channels) = Context::<Output>::new(
        Arc::new(host),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        ".".into(),
        CancellationToken::new(),
    );
    let ctx = ctx
        .with_http(Arc::new(MockDohHttp))
        .with_net(Arc::clone(&net) as Arc<dyn NetClient>);

    let settings = Settings {
        threads: 4,
        timeout_secs: 2,
        max: "500M".to_owned(),
        max_bytes: 500_000_000,
        latency_count: 4,
        forced_endpoint: None,
        custom_endpoints: true,
    };
    let endpoints = Endpoints {
        dl_url: "https://cdn.example/api/v1/gm/large",
        ul_url: "https://cdn.example/api/v1/gm/slurp",
        latency_url: "https://cdn.example/api/v1/gm/small",
        cf_doh_url: "https://cf-doh.example/dns-query",
        ali_doh_url: "https://ali-doh.example/resolve",
    };

    let out = measure::measure(&ctx, settings, &endpoints)
        .await
        .expect("inetspeed succeeds");

    let fastest: IpAddr = "198.51.100.12".parse().expect("ip");
    assert_eq!(out.selected.ip, Some(fastest));
    assert_eq!(out.selected.source, EndpointSource::Doh);
    assert_eq!(out.candidates.len(), 3);
    assert!(out.idle_latency.is_some());
    assert_eq!(out.rounds.len(), 4);
    assert_eq!(out.rounds[0].direction, TransferDirection::Download);
    assert_eq!(out.rounds[0].threads, 1);
    assert_eq!(out.rounds[1].direction, TransferDirection::Download);
    assert_eq!(out.rounds[1].threads, 4);
    assert_eq!(out.rounds[2].direction, TransferDirection::Upload);
    assert_eq!(out.rounds[2].threads, 1);
    assert_eq!(out.rounds[3].direction, TransferDirection::Upload);
    assert_eq!(out.rounds[3].threads, 4);

    let recorded = net.pinned_transfers.lock().expect("lock").clone();
    assert!(
        recorded.iter().all(|(_, _, pin)| *pin == Some(fastest)),
        "every transfer round must stay pinned to the lowest-RTT DoH candidate"
    );
    assert!(channels.published.borrow().is_some());
}
