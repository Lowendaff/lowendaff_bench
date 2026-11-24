//! End-to-end measurement tests for `network.bgp` using a mock `HttpClient`: dual-stack route
//! collection, IPv6-to-IPv4 API transport fallback, RPKI and `as-overview` enrichment, and skip
//! preconditions.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Stop;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_network::bgp;
use plugin_network::bgp::Bgp;
use plugin_network::bgp::Failure;
use plugin_network::bgp::Output;
use plugin_network::bgp::RpkiStatus;
use tokio::time::Instant;

struct MockRipeHttp;

impl HttpClient for MockRipeHttp {
    fn send<'a>(&'a self, req: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move {
            // Simulate IPv6 transport lacking AAAA so `network.bgp` falls back to IPv4 transport.
            if req.family == IpFamily::Ipv6 {
                return Err(HttpError::NoAddress);
            }
            let body = if req.url.contains("/bgp-state/") {
                if req.url.contains("2001:db8::9") {
                    concat!(
                        r#"{"status":"ok","data":{"resource":"2001:db8::/48","bgp_state":["#,
                        r#"{"target_prefix":"2001:db8::/48","path":[6939,64501]}, "#,
                        r#"{"target_prefix":"2001:db8::/48","path":[1299,64501]}"#,
                        r#"]}}"#
                    )
                    .to_owned()
                } else {
                    include_str!("fixtures/bgp/bgp_state_sample.json").to_owned()
                }
            } else if req.url.contains("/rpki-validation/") {
                if req.url.contains("2001:db8::/48") {
                    r#"{"data":{"status":"unknown"}}"#.to_owned()
                } else {
                    r#"{"data":{"status":"valid"}}"#.to_owned()
                }
            } else if req.url.contains("/as-overview/") {
                if req.url.contains("AS64500") {
                    r#"{"data":{"holder":"EX-NET - Example Cloud, LLC"}}"#.to_owned()
                } else {
                    r#"{"data":{"holder":"EX6-NET - Example IPv6 Host, Inc."}}"#.to_owned()
                }
            } else {
                return Ok(HttpResponse {
                    status: 404,
                    final_url: req.url.clone(),
                    headers: Vec::new(),
                    body: b"{}".to_vec(),
                });
            };
            Ok(HttpResponse {
                status: 200,
                final_url: req.url.clone(),
                headers: Vec::new(),
                body: body.into_bytes(),
            })
        })
    }
}

fn host_with_network(network: Network) -> HostProfile {
    HostProfile {
        network,
        ..HostProfile::default()
    }
}

fn make_ctx(host: HostProfile, http: Arc<dyn HttpClient>) -> Context<Output> {
    let (ctx, _channels) = Context::new(
        Arc::new(host),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(30),
        PathBuf::from("."),
        CancellationToken::new(),
    );
    ctx.with_http(http)
}

#[tokio::test]
async fn descriptor_and_base_url_validation_follow_contract() {
    assert_eq!(Bgp::DESCRIPTOR.id, "network.bgp");
    assert_eq!(Bgp::DESCRIPTOR.requires.net, Net::Any);
    assert!(Bgp::DESCRIPTOR.requires.tools.is_empty());

    assert_eq!(
        bgp::validate_base_url("http://127.0.0.1:18083/"),
        Ok("http://127.0.0.1:18083".to_owned())
    );
    assert!(bgp::validate_base_url("http://example.com/api").is_err());
}

#[tokio::test]
async fn measures_dual_stack_bgp_with_ipv6_transport_fallback_and_holder_lookup() {
    let net = Network {
        ipv4: Family {
            targets: vec![("1.1.1.1:443".parse().unwrap(), Reach::Connected)],
            address: Address::Found {
                address: "203.0.113.7".parse().unwrap(),
                source: "Cloudflare",
            },
        },
        ipv6: Family {
            targets: vec![(
                "[2606:4700:4700::1111]:443".parse().unwrap(),
                Reach::Connected,
            )],
            address: Address::Found {
                address: "2001:db8::9".parse().unwrap(),
                source: "Cloudflare",
            },
        },
    };
    let ctx = make_ctx(host_with_network(net), Arc::new(MockRipeHttp));
    let plugin = Bgp {
        base_url: Some("http://127.0.0.1:18083".to_owned()),
        ..Bgp::default()
    };

    let out = plugin.run(&ctx).await.expect("dual-stack BGP succeeds");
    assert!(out.settings.custom_endpoints);
    assert!(out.settings.upload);

    let v4 = out.ipv4.expect("ipv4 report");
    assert!(v4.announced);
    assert_eq!(v4.prefix.as_deref(), Some("203.0.113.0/24"));
    assert_eq!(v4.origin_asn, Some(64500));
    assert_eq!(v4.rpki, RpkiStatus::Valid);
    assert_eq!(
        v4.asn_names.get(&64500).map(String::as_str),
        Some("Example Cloud")
    );
    assert_eq!(v4.asn_names.get(&3356).map(String::as_str), Some("Lumen"));

    let v6 = out.ipv6.expect("ipv6 report via fallback");
    assert!(v6.announced);
    assert_eq!(v6.prefix.as_deref(), Some("2001:db8::/48"));
    assert_eq!(v6.origin_asn, Some(64501));
    assert_eq!(v6.rpki, RpkiStatus::NotFound);
    assert_eq!(
        v6.asn_names.get(&64501).map(String::as_str),
        Some("Example IPv6 Host")
    );
}

#[tokio::test]
async fn skips_when_offline_or_without_public_ip() {
    let offline = Network::default();
    let ctx = make_ctx(host_with_network(offline), Arc::new(MockRipeHttp));
    let res = Bgp::default().run(&ctx).await;
    assert_eq!(res, Err(Stop::Skipped(Failure::NoFamily)));

    let reachable_no_ip = Network {
        ipv4: Family {
            targets: vec![("192.0.2.1:443".parse().unwrap(), Reach::Connected)],
            address: Address::NotAsked,
        },
        ipv6: Family::default(),
    };
    let no_ip_ctx = make_ctx(host_with_network(reachable_no_ip), Arc::new(MockRipeHttp));
    assert_eq!(
        Bgp::default().run(&no_ip_ctx).await,
        Err(Stop::Skipped(Failure::NoPublicAddress))
    );
}

struct ManyAsnsHttp;

impl HttpClient for ManyAsnsHttp {
    fn send<'a>(&'a self, req: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move {
            let body = if req.url.contains("/bgp-state/") {
                let mut routes = Vec::new();
                // 20 peers via top upstream AS399998 -> high-numbered origin AS399999.
                for idx in 0..20 {
                    routes.push(format!(
                        "{{\"target_prefix\":\"198.51.100.0/24\",\
                         \"source_id\":\"p{idx}\",\"path\":[3356,399998,399999]}}"
                    ));
                }
                // 24 single-peer routes through low-numbered unknown ASNs (64601..64625) that
                // would starve AS399998/AS399999 if lookups were ordered numerically by ASN.
                for low_asn in 64601..=64624 {
                    routes.push(format!(
                        "{{\"target_prefix\":\"198.51.100.0/24\",\
                         \"source_id\":\"m{low_asn}\",\"path\":[174,{low_asn},399999]}}"
                    ));
                }
                let joined = routes.join(",");
                format!(
                    "{{\"status\":\"ok\",\"data\":{{\"resource\":\"198.51.100.0/24\",\
                     \"bgp_state\":[{joined}]}}}}"
                )
            } else if req.url.contains("/rpki-validation/") {
                r#"{"data":{"status":"valid"}}"#.to_owned()
            } else if req.url.contains("/as-overview/") {
                if req.url.contains("AS399999") {
                    r#"{"data":{"holder":"HIGH-ORIGIN - High Origin Cloud, LLC"}}"#.to_owned()
                } else if req.url.contains("AS399998") {
                    r#"{"data":{"holder":"PRIMARY-UP - Primary Transit Corp."}}"#.to_owned()
                } else {
                    r#"{"data":{"holder":"MINOR-PEER - Minor Transit"}}"#.to_owned()
                }
            } else {
                r"{}".to_owned()
            };
            Ok(HttpResponse {
                status: 200,
                final_url: req.url.clone(),
                headers: Vec::new(),
                body: body.into_bytes(),
            })
        })
    }
}

#[tokio::test]
async fn prioritizes_high_numbered_origin_and_top_upstreams_under_lookup_cap() {
    let net = Network {
        ipv4: Family {
            targets: vec![("1.1.1.1:443".parse().unwrap(), Reach::Connected)],
            address: Address::Found {
                address: "198.51.100.7".parse().unwrap(),
                source: "Cloudflare",
            },
        },
        ipv6: Family::default(),
    };
    let ctx = make_ctx(host_with_network(net), Arc::new(ManyAsnsHttp));
    let plugin = Bgp {
        upload: false,
        base_url: Some("http://127.0.0.1:18083".to_owned()),
        upload_url: Some("http://127.0.0.1:18083/api/upload/".to_owned()),
    };

    let out = plugin.run(&ctx).await.expect("BGP succeeds");
    assert!(!out.settings.upload);
    assert_eq!(
        out.settings.upload_url.as_deref(),
        Some("http://127.0.0.1:18083/api/upload")
    );
    let v4 = out.ipv4.expect("ipv4 report");
    assert_eq!(
        v4.asn_names.get(&399999).map(String::as_str),
        Some("High Origin Cloud"),
        "6-digit origin ASN must be enriched first despite 24 lower-numbered unknown ASNs"
    );
    assert_eq!(
        v4.asn_names.get(&399998).map(String::as_str),
        Some("Primary Transit"),
        "dominant direct upstream must be enriched ahead of minor 1-peer transits"
    );
}

#[tokio::test]
async fn scrubs_public_ips_from_query_errors_and_avoids_premature_empty_publish() {
    struct LeakErrorHttp;
    impl HttpClient for LeakErrorHttp {
        fn send<'a>(&'a self, req: &'a HttpRequest) -> HttpFuture<'a> {
            let url = req.url.clone();
            Box::pin(async move {
                if url.contains("2001:db8") {
                    Err(HttpError::NotHttps(url))
                } else {
                    Err(HttpError::Transport(format!(
                        "error sending request for url ({url}): reset by 198.51.100.7"
                    )))
                }
            })
        }
    }

    let net = Network {
        ipv4: Family {
            targets: vec![("1.1.1.1:443".parse().unwrap(), Reach::Connected)],
            address: Address::Found {
                address: "198.51.100.7".parse().unwrap(),
                source: "Cloudflare",
            },
        },
        ipv6: Family {
            targets: vec![(
                "[2606:4700:4700::1111]:443".parse().unwrap(),
                Reach::Connected,
            )],
            address: Address::Found {
                address: "2001:db8::9".parse().unwrap(),
                source: "Cloudflare",
            },
        },
    };
    let (ctx, channels) = Context::new(
        Arc::new(host_with_network(net)),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(30),
        PathBuf::from("."),
        CancellationToken::new(),
    );
    let ctx = ctx.with_http(Arc::new(LeakErrorHttp));
    let err = bgp::measure::measure(&ctx, bgp::Settings::default(), Some("http://ripe.example/"))
        .await
        .expect_err("both families fail");
    assert!(
        channels.published.borrow().is_none(),
        "total failure must not leave empty published output"
    );
    let formatted = format!("{err:?}");
    assert!(
        !formatted.contains("198.51.100.7") && !formatted.contains("2001:db8::9"),
        "public IPs must be scrubbed from failure details: {formatted}"
    );
}
