//! End-to-end measurement state-machine tests for `network.cloudflare`: dual-stack execution,
//! `/meta` + `/cdn-cgi/trace` + `/locations` fallback, non-fatal STUN UDP blocked notes, and
//! partial result publishing.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::CloudflareStun;
use bench_api::CloudflareStunSpec;
use bench_api::CloudflareTransfer;
use bench_api::CloudflareTransferSpec;
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
use bench_api::ThroughputTick;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_network::cloudflare::FamilyIssue;
use plugin_network::cloudflare::Output;
use plugin_network::cloudflare::Settings;
use plugin_network::cloudflare::measure;
use tokio::time::Instant;

struct MockCloudflareHttp;

impl HttpClient for MockCloudflareHttp {
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

struct MockCloudflareNet;

impl NetClient for MockCloudflareNet {
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

    fn cloudflare_download<'a>(
        &'a self,
        _spec: &'a CloudflareTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareTransfer> {
        Box::pin(async {
            Ok(CloudflareTransfer {
                bytes: 125_000_000,
                duration_ms: 2_000,
                ticks: vec![
                    ThroughputTick {
                        elapsed_ms: 500,
                        bytes: 25_000_000,
                    },
                    ThroughputTick {
                        elapsed_ms: 1_000,
                        bytes: 62_500_000,
                    },
                    ThroughputTick {
                        elapsed_ms: 1_500,
                        bytes: 93_750_000,
                    },
                    ThroughputTick {
                        elapsed_ms: 2_000,
                        bytes: 125_000_000,
                    },
                ],
                loaded_sent: 4,
                loaded_rtts_micros: vec![18_000, 20_000, 19_000, 21_000],
                failed_requests: 0,
            })
        })
    }

    fn cloudflare_upload<'a>(
        &'a self,
        _spec: &'a CloudflareTransferSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareTransfer> {
        Box::pin(async {
            Ok(CloudflareTransfer {
                bytes: 62_500_000,
                duration_ms: 2_000,
                ticks: vec![
                    ThroughputTick {
                        elapsed_ms: 500,
                        bytes: 12_500_000,
                    },
                    ThroughputTick {
                        elapsed_ms: 1_000,
                        bytes: 31_250_000,
                    },
                    ThroughputTick {
                        elapsed_ms: 1_500,
                        bytes: 46_875_000,
                    },
                    ThroughputTick {
                        elapsed_ms: 2_000,
                        bytes: 62_500_000,
                    },
                ],
                loaded_sent: 4,
                loaded_rtts_micros: vec![24_000, 26_000, 25_000, 27_000],
                failed_requests: 0,
            })
        })
    }

    fn cloudflare_udp_loss<'a>(
        &'a self,
        spec: &'a CloudflareStunSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareStun> {
        let ipv6 = spec.family == IpFamily::Ipv6;
        let target = format!("stun:{}:{}", spec.host, spec.port);
        Box::pin(async move {
            if ipv6 {
                Err(NetError::Transport("UDP blocked on IPv6".to_owned()))
            } else {
                Ok(CloudflareStun {
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
        .with_http(Arc::new(MockCloudflareHttp))
        .with_net(Arc::new(MockCloudflareNet));

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
        v4.client_ip,
        Some(std::net::IpAddr::from([198, 51, 100, 42]))
    );
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
    assert_eq!(
        v6.client_ip,
        Some(std::net::IpAddr::from([0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x42]))
    );
    assert_eq!(v6.server.as_deref(), Some("NRT - Tokyo, Japan"));
    assert!(v6.udp.is_none());
    assert_eq!(
        v6.udp_note,
        Some(FamilyIssue::Transport("UDP blocked on IPv6".to_owned()))
    );
    assert_eq!(channels.published.borrow().as_ref(), Some(&out));
}

#[tokio::test]
async fn cloudflare_plugin_validates_endpoints_skips_offline_and_preserves_partial_v4_on_cancel() {
    use bench_api::Plugin;
    use bench_api::Stop;
    use plugin_network::cloudflare;
    use plugin_network::cloudflare::Cloudflare;
    use plugin_network::cloudflare::Failure;

    assert_eq!(Cloudflare::DESCRIPTOR.id, "network.cloudflare");
    assert!(cloudflare::validate_base_url("https://speed.cloudflare.com").is_ok());
    assert!(cloudflare::validate_base_url("http://127.0.0.1:8080").is_ok());
    assert!(cloudflare::validate_base_url("http://speed.example").is_err());
    assert_eq!(
        cloudflare::validate_stun_host("stun.cloudflare.com").as_deref(),
        Ok("stun.cloudflare.com")
    );
    assert_eq!(
        cloudflare::validate_stun_host("127.0.0.1").as_deref(),
        Ok("127.0.0.1")
    );
    assert_eq!(
        cloudflare::validate_stun_host("::1").as_deref(),
        Ok("[::1]")
    );
    for bad in [
        "",
        "localhost",
        "stun.localhost",
        "stun.local",
        "singlelabel",
        "10.0.0.1",
        "169.254.169.254",
        "fe80::1",
        "[127.0.0.1]",
        "[[127.0.0.1]]",
        "stun.cloudflare.com:3478",
        "stun host.example",
        "0x7f000001",
        "169.254.169.0xfe",
        "stun.cloudflare.com..",
    ] {
        assert!(
            cloudflare::validate_stun_host(bad).is_err(),
            "expected invalid stun_host: {bad}"
        );
    }

    let make_ctx = |host: Arc<HostProfile>, cancel: CancellationToken| {
        Context::<Output>::new(
            host,
            Arc::new(BTreeMap::new()),
            Arc::new(BTreeMap::new()),
            Instant::now() + Duration::from_secs(60),
            ".".into(),
            cancel,
        )
    };

    // 1. Offline host skips cleanly; invalid base_url / stun_host / stun_port fail eagerly.
    let (offline_ctx, _) = make_ctx(Arc::new(HostProfile::default()), CancellationToken::new());
    assert_eq!(
        Cloudflare::default().run(&offline_ctx).await,
        Err(Stop::Skipped(Failure::NoFamily))
    );
    let bad_port = Cloudflare {
        stun_port: 0,
        ..Cloudflare::default()
    };
    assert!(matches!(
        bad_port.run(&offline_ctx).await,
        Err(Stop::Failed(Failure::InvalidEndpoint(_)))
    ));
    let bad_stun = Cloudflare {
        stun_host: Some("bad/host".to_owned()),
        ..Cloudflare::default()
    };
    assert!(matches!(
        bad_stun.run(&offline_ctx).await,
        Err(Stop::Failed(Failure::InvalidEndpoint(_)))
    ));
    let bad_url = Cloudflare {
        base_url: Some("http://speed.example".to_owned()),
        ..Cloudflare::default()
    };
    assert!(matches!(
        bad_url.run(&offline_ctx).await,
        Err(Stop::Failed(Failure::InvalidEndpoint(_)))
    ));

    // 2. Single-family failure records note while keeping v4; mid-run cancellation after v4
    //    preserves published v4 partial output.
    struct V6FailOrCancelNet {
        cancel_on_v6: Option<CancellationToken>,
        fail_all: bool,
    }
    impl NetClient for V6FailOrCancelNet {
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

        fn cloudflare_download<'a>(
            &'a self,
            spec: &'a CloudflareTransferSpec,
            _cancel: &'a CancellationToken,
        ) -> NetFuture<'a, CloudflareTransfer> {
            let is_v6 = spec.family == IpFamily::Ipv6;
            let cancel_tok = self.cancel_on_v6.clone();
            let fail_all = self.fail_all;
            Box::pin(async move {
                if fail_all {
                    return Err(NetError::TimedOut);
                }
                if is_v6 {
                    if let Some(tok) = cancel_tok {
                        tok.cancel();
                        return Err(NetError::Cancelled);
                    }
                    return Ok(CloudflareTransfer {
                        bytes: 0,
                        duration_ms: 1_000,
                        ticks: Vec::new(),
                        loaded_sent: 0,
                        loaded_rtts_micros: Vec::new(),
                        failed_requests: 1,
                    });
                }
                MockCloudflareNet.cloudflare_download(spec, _cancel).await
            })
        }

        fn cloudflare_upload<'a>(
            &'a self,
            spec: &'a CloudflareTransferSpec,
            cancel: &'a CancellationToken,
        ) -> NetFuture<'a, CloudflareTransfer> {
            MockCloudflareNet.cloudflare_upload(spec, cancel)
        }
    }

    let v4_sock = SocketAddr::from(([198, 51, 100, 1], 443));
    let v6_sock = SocketAddr::from(([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1], 443));
    let dual = Arc::new(HostProfile {
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
    });
    let settings = Settings {
        seconds: 1,
        concurrency: 2,
        udp: false,
        udp_packets: 1,
        idle_ms: 200,
        custom_endpoints: true,
    };

    let (partial_ctx, partial_ch) = make_ctx(Arc::clone(&dual), CancellationToken::new());
    let partial_ctx = partial_ctx
        .with_http(Arc::new(MockCloudflareHttp))
        .with_net(Arc::new(V6FailOrCancelNet {
            cancel_on_v6: None,
            fail_all: false,
        }));
    let partial_out = measure::measure(
        &partial_ctx,
        settings,
        "https://speed.example",
        "stun.example",
        3478,
        "1",
    )
    .await
    .expect("v4 succeeds when v6 returns empty download");
    assert!(partial_out.ipv4.is_some());
    assert!(partial_out.ipv6.is_none());
    assert_eq!(partial_out.notes.len(), 1);
    assert_eq!(partial_ch.published.borrow().as_ref(), Some(&partial_out));

    let cancel = CancellationToken::new();
    let (cancel_ctx, cancel_ch) = make_ctx(Arc::clone(&dual), cancel.clone());
    let cancel_ctx = cancel_ctx
        .with_http(Arc::new(MockCloudflareHttp))
        .with_net(Arc::new(V6FailOrCancelNet {
            cancel_on_v6: Some(cancel),
            fail_all: false,
        }));
    assert_eq!(
        measure::measure(
            &cancel_ctx,
            settings,
            "https://speed.example",
            "stun.example",
            3478,
            "2",
        )
        .await,
        Err(Stop::Failed(Failure::Cancelled))
    );
    assert!(
        cancel_ch
            .published
            .borrow()
            .as_ref()
            .is_some_and(|pub_out| pub_out.ipv4.is_some()),
        "completed IPv4 measurement must remain published when IPv6 cancels"
    );

    let (fail_ctx, _) = make_ctx(dual, CancellationToken::new());
    let fail_ctx = fail_ctx
        .with_http(Arc::new(MockCloudflareHttp))
        .with_net(Arc::new(V6FailOrCancelNet {
            cancel_on_v6: None,
            fail_all: true,
        }));
    assert!(matches!(
        measure::measure(
            &fail_ctx,
            settings,
            "https://speed.example",
            "stun.example",
            3478,
            "3",
        )
        .await,
        Err(Stop::Failed(Failure::NoResult(Some(_))))
    ));
}
