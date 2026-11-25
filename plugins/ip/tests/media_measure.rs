//! End-to-end measurement tests for `ip.media` with a recorded mock `HttpClient`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::DnsAnswer;
use bench_api::DnsRecords;
use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::Plugin;
use bench_api::Progress;
use bench_api::SmtpBanner;
use bench_api::Step;
use bench_api::Stop;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_ip::media::Failure;
use plugin_ip::media::Media;
use plugin_ip::media::Output;
use plugin_ip::media::UnlockMode;
use plugin_ip::media::Verdict;
use tokio::time::Instant;

struct RecordedHttp {
    seen: Mutex<Vec<(IpFamily, String)>>,
}

impl RecordedHttp {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl HttpClient for RecordedHttp {
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move {
            self.seen
                .lock()
                .expect("lock")
                .push((request.family, request.url.clone()));
            if request.family == IpFamily::Ipv6 && request.url.contains("abema") {
                return Err(HttpError::NoAddress);
            }
            if request.url.contains("netflix/licensed") {
                return Ok(HttpResponse {
                    status: 404,
                    final_url: request.url.clone(),
                    headers: Vec::new(),
                    body: b"Not Available".to_vec(),
                });
            }
            if request.url.contains("netflix/original") {
                return Ok(HttpResponse {
                    status: 200,
                    final_url: "https://www.netflix.com/sg/title/80018499".to_owned(),
                    headers: Vec::new(),
                    body: b"ok".to_vec(),
                });
            }
            if request.url.contains("abema/check") {
                return Ok(HttpResponse {
                    status: 200,
                    final_url: request.url.clone(),
                    headers: Vec::new(),
                    body: br#"{"isoCountryCode":"JP"}"#.to_vec(),
                });
            }
            Ok(HttpResponse {
                status: 403,
                final_url: request.url.clone(),
                headers: vec![("cf-mitigated".to_owned(), "challenge".to_owned())],
                body: Vec::new(),
            })
        })
    }
}

fn connected_family(addr: &str) -> Family {
    Family {
        targets: vec![(addr.parse().expect("socket"), Reach::Connected)],
        address: Address::NotAsked,
    }
}

fn make_context(
    ipv4: bool,
    ipv6: bool,
    http: Arc<dyn HttpClient>,
) -> (
    Context<Output>,
    bench_api::Channels<Output>,
    CancellationToken,
) {
    let network = Network {
        ipv4: if ipv4 {
            connected_family("1.1.1.1:443")
        } else {
            Family::default()
        },
        ipv6: if ipv6 {
            connected_family("[2606:4700:4700::1111]:443")
        } else {
            Family::default()
        },
    };
    let host = HostProfile {
        network,
        ..HostProfile::default()
    };
    let cancel = CancellationToken::new();
    let (ctx, channels) = Context::new(
        Arc::new(host),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        PathBuf::from("/run/work"),
        cancel.clone(),
    );
    (ctx.with_http(http), channels, cancel)
}

#[tokio::test]
async fn unlock_probes_both_families_rewrites_base_url_and_reports_progress() {
    let http = Arc::new(RecordedHttp::new());
    let (ctx, mut channels, _) = make_context(true, true, Arc::clone(&http) as Arc<dyn HttpClient>);
    let plugin = Media {
        groups: Vec::new(),
        services: vec![
            "netflix".to_owned(),
            "abema".to_owned(),
            "bbc-iplayer".to_owned(),
            "unknown-id".to_owned(),
        ],
        base_url: Some("http://127.0.0.1:18080/".to_owned()),
    };

    let output = plugin.run(&ctx).await.expect("measurement succeeds");
    assert!(output.settings.custom_base);
    assert_eq!(channels.published.borrow().as_ref(), Some(&output));
    assert_eq!(output.unknown, ["unknown-id"]);
    assert!(output.ipv4_reachable && output.ipv6_reachable);
    assert_eq!(output.results.len(), 3);

    // Netflix: originals only (SG) on both families.
    assert_eq!(
        output.results[0].ipv4,
        Some(Verdict::OriginalsOnly {
            region: Some("SG".to_owned())
        })
    );
    assert_eq!(
        output.results[0].ipv6,
        Some(Verdict::OriginalsOnly {
            region: Some("SG".to_owned())
        })
    );
    // Abema: JP over IPv4, NoAddress over IPv6.
    assert_eq!(output.results[1].ipv4, Some(Verdict::yes_in("JP")));
    assert_eq!(output.results[1].ipv6, Some(Verdict::NoAddress));
    // BBC iPlayer: BlockedWaf from cf-mitigated challenge.
    assert_eq!(output.results[2].ipv4, Some(Verdict::BlockedWaf));
    assert_eq!(output.results[2].ipv6, Some(Verdict::BlockedWaf));

    let mut steps = Vec::new();
    while let Ok(Progress::Step(step)) = channels.progress.try_recv() {
        steps.push(step);
    }
    assert_eq!(
        steps,
        [
            Step::Media {
                group: "global",
                ipv6: false
            },
            Step::Media {
                group: "asia",
                ipv6: false
            },
            Step::Media {
                group: "europe",
                ipv6: false
            },
            Step::Media {
                group: "global",
                ipv6: true
            },
            Step::Media {
                group: "asia",
                ipv6: true
            },
            Step::Media {
                group: "europe",
                ipv6: true
            },
        ]
    );
    let seen = http.seen.lock().expect("lock").clone();
    assert_eq!(seen.len(), 8);
    assert!(
        seen.iter().all(|(_, url)| {
            url.strip_prefix("http://127.0.0.1:18080/")
                .is_some_and(|rest| !rest.starts_with('/') && !rest.contains("//"))
        }),
        "every probe honored normalized base_url without double slashes: {seen:?}"
    );
}

#[tokio::test]
async fn unlock_skips_without_network_or_matching_services_and_rejects_bad_base_url() {
    let raw_http = Arc::new(RecordedHttp::new());
    let http: Arc<dyn HttpClient> = Arc::clone(&raw_http) as Arc<dyn HttpClient>;

    let (offline_ctx, _, _) = make_context(false, false, Arc::clone(&http));
    assert_eq!(
        Media::default().run(&offline_ctx).await,
        Err(Stop::Skipped(Failure::NoFamily))
    );

    let (ctx, _, _) = make_context(true, false, Arc::clone(&http));
    let none = Media {
        groups: Vec::new(),
        services: vec!["not-a-service".to_owned()],
        base_url: None,
    };
    assert_eq!(
        none.run(&ctx).await,
        Err(Stop::Skipped(Failure::NoService(vec![
            "not-a-service".to_owned()
        ])))
    );

    let bad_base = Media {
        groups: Vec::new(),
        services: Vec::new(),
        base_url: Some("http://example.com".to_owned()),
    };
    assert_eq!(
        bad_base.run(&ctx).await,
        Err(Stop::Failed(Failure::InvalidBaseUrl(
            "http://example.com".to_owned()
        )))
    );

    let blank_only = Media {
        groups: Vec::new(),
        services: vec!["   ".to_owned()],
        base_url: None,
    };
    assert_eq!(
        blank_only.run(&ctx).await,
        Err(Stop::Skipped(Failure::NoService(Vec::new())))
    );
    assert!(raw_http.seen.lock().expect("lock").is_empty());

    let (v4_ctx, mut v4_channels, _) = make_context(true, false, Arc::clone(&http));
    let v4_only = Media {
        groups: Vec::new(),
        services: vec!["abema".to_owned()],
        base_url: Some("http://127.0.0.1:18080".to_owned()),
    };
    let v4_out = v4_only.run(&v4_ctx).await.expect("v4-only run succeeds");
    assert!(v4_out.ipv4_reachable && !v4_out.ipv6_reachable);
    assert_eq!(v4_out.results.len(), 1);
    assert_eq!(v4_out.results[0].ipv4, Some(Verdict::yes_in("JP")));
    assert_eq!(v4_out.results[0].ipv6, None);
    let mut v4_steps = Vec::new();
    while let Ok(Progress::Step(step)) = v4_channels.progress.try_recv() {
        v4_steps.push(step);
    }
    assert_eq!(
        v4_steps,
        [Step::Media {
            group: "asia",
            ipv6: false
        }]
    );

    let (pre_cancel_ctx, pre_cancel_channels, pre_cancel) =
        make_context(true, true, Arc::clone(&http));
    pre_cancel.cancel();
    assert_eq!(
        v4_only.run(&pre_cancel_ctx).await,
        Err(Stop::Failed(Failure::Cancelled))
    );
    assert!(pre_cancel_channels.published.borrow().is_none());
}

#[tokio::test]
async fn mid_run_cancellation_publishes_only_completed_continents() {
    struct CancelOnAsia {
        cancel: CancellationToken,
    }
    impl HttpClient for CancelOnAsia {
        fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
            Box::pin(async move {
                if request.url.contains("abema") {
                    self.cancel.cancel();
                    return Err(HttpError::Cancelled);
                }
                Ok(HttpResponse {
                    status: 200,
                    final_url: "https://www.netflix.com/us/title/81280792".to_owned(),
                    headers: Vec::new(),
                    body: b"ok".to_vec(),
                })
            })
        }
    }

    let cancel = CancellationToken::new();
    let http: Arc<dyn HttpClient> = Arc::new(CancelOnAsia {
        cancel: cancel.clone(),
    });
    let host = HostProfile {
        network: Network {
            ipv4: connected_family("1.1.1.1:443"),
            ipv6: Family::default(),
        },
        ..HostProfile::default()
    };
    let (ctx, channels) = Context::new(
        Arc::new(host),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        PathBuf::from("/run/work"),
        cancel,
    );
    let ctx = ctx.with_http(http);
    let plugin = Media {
        groups: Vec::new(),
        services: vec!["  netflix  ".to_owned(), " ".to_owned(), "abema".to_owned()],
        base_url: Some("http://127.0.0.1:18080".to_owned()),
    };

    assert_eq!(
        plugin.run(&ctx).await,
        Err(Stop::Failed(Failure::Cancelled))
    );
    let published = channels.published.borrow().clone().expect("partial output");
    assert_eq!(published.settings.services, ["netflix", "abema"]);
    assert_eq!(published.results.len(), 1);
    assert_eq!(published.results[0].id, "netflix");
    assert_eq!(published.results[0].ipv4, Some(Verdict::yes_in("US")));
}

#[tokio::test]
async fn dns_unlock_classifier_distinguishes_native_wildcard_cname_strip_hosts_and_convergence() {
    struct LiveUrlHttp;
    impl HttpClient for LiveUrlHttp {
        fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
            Box::pin(async move {
                let body = if request.url.contains("abema") {
                    br#"{"isoCountryCode":"JP"}"#.to_vec()
                } else if request.url.contains("dazn") {
                    br#"{"Region":{"isAllowed":true,"GeolocatedCountry":"JP"}}"#.to_vec()
                } else {
                    br#"{"media":[]}"#.to_vec()
                };
                let final_url = if request.url.contains("netflix.com") {
                    "https://www.netflix.com/jp/title/81280792".to_owned()
                } else {
                    request.url.clone()
                };
                Ok(HttpResponse {
                    status: 200,
                    final_url,
                    headers: Vec::new(),
                    body,
                })
            })
        }
    }

    struct MockDns;
    impl NetClient for MockDns {
        fn dns_a<'a>(
            &'a self,
            _qname: &'a str,
            _server: Option<std::net::SocketAddr>,
            _timeout: Duration,
        ) -> NetFuture<'a, DnsAnswer> {
            Box::pin(async { Ok(DnsAnswer::NotFound) })
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

        fn dns_lookup<'a>(
            &'a self,
            family: IpFamily,
            qname: &'a str,
            server: Option<std::net::SocketAddr>,
            _timeout: Duration,
        ) -> NetFuture<'a, DnsRecords> {
            Box::pin(async move {
                let is_public = server.is_some();
                let ip = |s: &str| -> std::net::IpAddr { s.parse().expect("ip") };
                if family == IpFamily::Ipv6 {
                    // Simulate /etc/hosts override on IPv6 for Netflix, clean native CNAME for BBC.
                    if qname.starts_with("lowendaff-nx-check.") {
                        return Ok(DnsRecords::default());
                    }
                    if !is_public && qname == "www.netflix.com" {
                        return Ok(DnsRecords {
                            addresses: vec![ip("2001:db8:99::1")],
                            has_cname: false,
                            from_hosts: true,
                        });
                    }
                    return Ok(DnsRecords {
                        addresses: vec![ip("2001:db8:10::1")],
                        has_cname: true,
                        from_hosts: false,
                    });
                }

                // IPv4 scenarios:
                // 1) www.netflix.com: GeoDNS (different /24, BOTH preserve CNAME) -> Native!
                if qname == "www.netflix.com" {
                    return Ok(DnsRecords {
                        addresses: vec![if is_public {
                            ip("198.51.100.10")
                        } else {
                            ip("203.0.113.10")
                        }],
                        has_cname: true,
                        from_hosts: false,
                    });
                }
                // 2) open.bbc.co.uk: CNAME stripped by SNI proxy on system DNS -> Dns!
                if qname.ends_with("bbc.co.uk") && !qname.starts_with("lowendaff-nx-check.") {
                    return Ok(DnsRecords {
                        addresses: vec![if is_public {
                            ip("198.51.100.20")
                        } else {
                            ip("192.0.2.50")
                        }],
                        has_cname: is_public,
                        from_hosts: false,
                    });
                }
                // 3) dazn + abema apex domains: no CNAME in public DNS, both hijacked to 192.0.2.88
                //    across Global and Asia continents -> convergence upgrades both to Dns.
                if (qname.contains("dazn") || qname.contains("abema"))
                    && !qname.starts_with("lowendaff-nx-check.")
                {
                    return Ok(DnsRecords {
                        addresses: vec![if is_public {
                            if qname.contains("dazn") {
                                ip("198.51.100.30")
                            } else {
                                ip("203.0.113.40")
                            }
                        } else {
                            ip("192.0.2.88")
                        }],
                        has_cname: false,
                        from_hosts: false,
                    });
                }
                Ok(DnsRecords::default())
            })
        }
    }

    let (ctx, _, _) = make_context(true, true, Arc::new(LiveUrlHttp));
    let ctx = ctx.with_net(Arc::new(MockDns));
    let plugin = Media {
        groups: Vec::new(),
        services: vec![
            "netflix".to_owned(),
            "dazn".to_owned(),
            "abema".to_owned(),
            "bbc-iplayer".to_owned(),
        ],
        base_url: None,
    };

    let out = plugin.run(&ctx).await.expect("media succeeds");
    assert_eq!(out.results.len(), 4);
    // Netflix: GeoDNS with CNAME preserved -> Native on IPv4; /etc/hosts -> Dns on IPv6.
    assert_eq!(out.results[0].id, "netflix");
    assert_eq!(out.results[0].ipv4_unlock, Some(UnlockMode::Native));
    assert_eq!(out.results[0].ipv6_unlock, Some(UnlockMode::Dns));
    // DAZN (Global) + Abema (Asia): divergent apex domains converging on 192.0.2.88 -> both Dns!
    assert_eq!(out.results[1].id, "dazn");
    assert_eq!(out.results[1].ipv4_unlock, Some(UnlockMode::Dns));
    assert_eq!(out.results[2].id, "abema");
    assert_eq!(out.results[2].ipv4_unlock, Some(UnlockMode::Dns));
    // BBC iPlayer: CNAME stripped on system DNS -> Dns on IPv4, Native on IPv6.
    assert_eq!(out.results[3].id, "bbc-iplayer");
    assert_eq!(out.results[3].ipv4_unlock, Some(UnlockMode::Dns));
    assert_eq!(out.results[3].ipv6_unlock, Some(UnlockMode::Native));
}
