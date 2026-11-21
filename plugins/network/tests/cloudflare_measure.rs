//! End-to-end measurement state-machine tests for `network.cloudflare`: dual-stack execution,
//! `/meta` + `/cdn-cgi/trace` + `/locations` fallback, non-fatal STUN UDP blocked notes, and
//! partial result publishing.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::CfStunRaw;
use bench_api::CfStunSpec;
use bench_api::CfTransferRaw;
use bench_api::CfTransferSpec;
use bench_api::Context;
use bench_api::DnsAnswer;
use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::SmtpBanner;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_network::cloudflare::Output;
use plugin_network::cloudflare::Settings;
use plugin_network::cloudflare::measure;
use tokio::time::Instant;

struct MockCfHttp;

impl HttpClient for MockCfHttp {
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
        let url = request.url.clone();
        let ipv6 = request.family == IpFamily::Ipv6;
        Box::pin(async move {
            let (status, body): (u16, Vec<u8>) = if url.contains("/meta?") {
                if ipv6 {
                    // Simulate /meta failing on IPv6 so /cdn-cgi/trace fallback runs!
                    (503, Vec::new())
                } else {
                    (
                        200,
                        concat!(
                            r#"{"clientIp":"198.51.100.42","colo":"LAX","#,
                            r#""asn":64500,"asOrganization":"ExampleISP"}"#
                        )
                        .as_bytes()
                        .to_vec(),
                    )
                }
            } else if url.contains("/cdn-cgi/trace") {
                (200, b"ip=2001:db8::42\ncolo=NRT\nloc=JP\n".to_vec())
            } else if url.contains("/locations") {
                (
                    200,
                    concat!(
                        r#"[{"iata":"LAX","city":"Los Angeles","country":"United States"},"#,
                        r#"{"iata":"NRT","city":"Tokyo","country":"Japan"}]"#
                    )
                    .as_bytes()
                    .to_vec(),
                )
            } else if url.contains("/__down?bytes=0") {
                (200, Vec::new())
            } else {
                return Err(HttpError::Transport(format!("unexpected URL: {url}")));
            };
            Ok(HttpResponse {
                status,
                final_url: url,
                headers: Vec::new(),
                body,
            })
        })
    }
}

struct MockCfNet;

impl NetClient for MockCfNet {
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

    fn cf_download<'a>(
        &'a self,
        _spec: &'a CfTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CfTransferRaw> {
        Box::pin(async {
            Ok(CfTransferRaw {
                bytes: 125_000_000,
                duration_ms: 2_000,
                ticks: vec![
                    (500, 25_000_000),
                    (1_000, 62_500_000),
                    (1_500, 93_750_000),
                    (2_000, 125_000_000),
                ],
                loaded_sent: 4,
                loaded_rtts_micros: vec![18_000, 20_000, 19_000, 21_000],
                failed_requests: 0,
            })
        })
    }

    fn cf_upload<'a>(
        &'a self,
        _spec: &'a CfTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CfTransferRaw> {
        Box::pin(async {
            Ok(CfTransferRaw {
                bytes: 62_500_000,
                duration_ms: 2_000,
                ticks: vec![
                    (500, 12_500_000),
                    (1_000, 31_250_000),
                    (1_500, 46_875_000),
                    (2_000, 62_500_000),
                ],
                loaded_sent: 4,
                loaded_rtts_micros: vec![24_000, 26_000, 25_000, 27_000],
                failed_requests: 0,
            })
        })
    }

    fn cf_udp_loss<'a>(
        &'a self,
        spec: &'a CfStunSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CfStunRaw> {
        let ipv6 = spec.family == IpFamily::Ipv6;
        let target = format!("stun:{}:{}", spec.host, spec.port);
        Box::pin(async move {
            if ipv6 {
                Err(NetError::Transport("UDP blocked on IPv6".to_owned()))
            } else {
                Ok(CfStunRaw {
                    target,
                    sent: 10,
                    received: 10,
                    out_of_order: 0,
                    rtts_micros: vec![12_000; 10],
                })
            }
        })
    }
}

#[tokio::test]
async fn cloudflare_measures_dual_stack_merges_trace_fallback_and_keeps_udp_block_non_fatal() {
    let v4_sock = SocketAddr::from(([198, 51, 100, 1], 443));
    let v6_sock = SocketAddr::from(([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1], 443));
    let host = HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![(v4_sock, Reach::Connected)],
                ..Family::default()
            },
            ipv6: Family {
                targets: vec![(v6_sock, Reach::Connected)],
                ..Family::default()
            },
        },
        ..HostProfile::default()
    };

    let (ctx, channels) = Context::<Output>::new(
        Arc::new(host),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        ".".into(),
        CancellationToken::new(),
    );
    let ctx = ctx
        .with_http(Arc::new(MockCfHttp))
        .with_net(Arc::new(MockCfNet));

    let settings = Settings {
        seconds: 2,
        concurrency: 4,
        udp: true,
        udp_packets: 10,
        idle_ms: 200,
        custom_endpoints: true,
    };
    let out = measure::measure(
        &ctx,
        settings,
        "https://speed.example",
        "stun.example",
        3478,
        "123456",
    )
    .await
    .expect("cloudflare dual-stack succeeds");

    let v4 = out.ipv4.as_ref().expect("v4 result");
    assert_eq!(
        v4.server.as_deref(),
        Some("LAX - Los Angeles, United States")
    );
    assert_eq!(v4.asn, Some(64500));
    assert!(v4.download.bps > 400_000_000);
    assert!(v4.upload.bps > 200_000_000);
    assert!(v4.udp.is_some());
    assert!(v4.udp_note.is_none());
    assert!(v4.quality.is_some());

    let v6 = out.ipv6.as_ref().expect("v6 result");
    assert_eq!(v6.server.as_deref(), Some("NRT - Tokyo, Japan"));
    assert!(v6.udp.is_none());
    assert_eq!(v6.udp_note.as_deref(), Some("UDP blocked on IPv6"));
    assert!(channels.published.borrow().is_some());
}
