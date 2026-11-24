//! End-to-end measurement tests for `ip.quality` with mock `HttpClient` and `NetClient`:
//! broadcast vs native IP consensus, cross-family API fallback, DNSBL listing + refusal filtering,
//! and port-25 SMTP outcomes.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
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
use plugin_ip::quality::Failure;
use plugin_ip::quality::MailStatus;
use plugin_ip::quality::Origin;
use plugin_ip::quality::Output;
use plugin_ip::quality::Quality;
use plugin_ip::quality::RiskFactor;
use plugin_ip::quality::UsageType;
use tokio::time::Instant;

struct MockHttp;

impl HttpClient for MockHttp {
    fn send<'a>(&'a self, req: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move {
            // Simulate ProxyCheck lacking IPv6 AAAA so IPv6 query falls back to IPv4 transport.
            if req.family == IpFamily::Ipv6 && req.url.contains("/proxycheck/") {
                return Err(HttpError::NoAddress);
            }
            let is_v6_ip = req.url.contains("2001:db8::7");
            let body = if req.url.contains("/ipapi/") {
                let cc = if is_v6_ip { "SG" } else { "US" };
                format!(
                    concat!(
                        r#"{{"rir":"ARIN","is_proxy":false,"is_vpn":false,"is_tor":false,"#,
                        r#""is_datacenter":true,"is_abuser":false,"is_crawler":false,"#,
                        r#""location":{{"country_code":"{cc}"}},"#,
                        r#""company":{{"name":"Example Host","type":"hosting","#,
                        r#""abuser_score":"0.0020 (Low)"}},"#,
                        r#""asn":{{"asn":64500,"org":"Example Host","#,
                        r#""country":"US","type":"hosting"}}}}"#
                    ),
                    cc = cc
                )
            } else if req.url.contains("/ripestat/") {
                r#"{"data":{"authorities":["arin"],"records":[[{"key":"country","value":"US"}]]}}"#
                    .to_owned()
            } else if req.url.contains("/scamalytics/") {
                r#"<pre>{"score":"8","risk":"low"}</pre>"#.to_owned()
            } else if req.url.contains("/proxycheck/") {
                let cc = if is_v6_ip { "SG" } else { "US" };
                format!(
                    concat!(
                        r#"{{"status":"ok","target":{{"asn":"AS64500","#,
                        r#""isocode":"{cc}","proxy":"no","type":"Hosting","risk":10}}}}"#
                    ),
                    cc = cc
                )
            } else if req.url.contains("/greynoise/") {
                return Ok(HttpResponse {
                    status: 404,
                    final_url: req.url.clone(),
                    headers: Vec::new(),
                    body: b"{}".to_vec(),
                });
            } else if req.url.contains("/shodan/") {
                r#"{"ports":[22,443],"tags":["cloud"],"vulns":[],"hostnames":["node.example"]}"#
                    .to_owned()
            } else if req.url.contains("/ipinfo/") {
                let cc = if is_v6_ip { "SG" } else { "US" };
                format!(
                    r#"{{"country":"{cc}","org":"AS64500 Example Host","hostname":"ptr.example"}}"#
                )
            } else if req.url.contains("/dbip/") {
                let cc = if is_v6_ip { "SG" } else { "US" };
                format!(r#"{{"countryCode":"{cc}"}}"#)
            } else {
                let cc = if is_v6_ip { "SG" } else { "US" };
                format!(
                    concat!(
                        r#"{{"success":true,"country_code":"{cc}","#,
                        r#""connection":{{"asn":64500,"org":"Example Host"}}}}"#
                    ),
                    cc = cc
                )
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

struct MockNet;

impl NetClient for MockNet {
    fn dns_a<'a>(
        &'a self,
        qname: &'a str,
        _server: Option<SocketAddr>,
        _timeout: Duration,
    ) -> NetFuture<'a, DnsAnswer> {
        Box::pin(async move {
            if qname.ends_with("zen.spamhaus.org") {
                Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(
                    127, 255, 255, 254,
                )]))
            } else if qname.ends_with("bl.spamcop.net") {
                Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(127, 0, 0, 2)]))
            } else {
                Ok(DnsAnswer::NotFound)
            }
        })
    }

    fn smtp_banner<'a>(
        &'a self,
        family: IpFamily,
        host: &'a str,
        _port: u16,
        _timeout: Duration,
    ) -> NetFuture<'a, SmtpBanner> {
        Box::pin(async move {
            if family == IpFamily::Ipv6 {
                return Err(NetError::TimedOut);
            }
            if host.contains("icloud") {
                Ok(SmtpBanner { code: 554 })
            } else {
                Ok(SmtpBanner { code: 220 })
            }
        })
    }
}

fn dual_stack_host() -> HostProfile {
    HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![("1.1.1.1:443".parse().expect("v4"), Reach::Connected)],
                address: Address::Found {
                    address: "198.51.100.7".parse().expect("ip4"),
                    source: "Cloudflare",
                },
            },
            ipv6: Family {
                targets: vec![(
                    "[2606:4700:4700::1111]:443".parse().expect("v6"),
                    Reach::Connected,
                )],
                address: Address::Found {
                    address: "2001:db8::7".parse().expect("ip6"),
                    source: "Cloudflare",
                },
            },
        },
        ..HostProfile::default()
    }
}

fn make_ctx(host: HostProfile) -> (Context<Output>, bench_api::Channels<Output>) {
    let cancel = CancellationToken::new();
    let (ctx, channels) = Context::new(
        Arc::new(host),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        PathBuf::from("/run/work"),
        cancel,
    );
    (
        ctx.with_http(Arc::new(MockHttp))
            .with_net(Arc::new(MockNet)),
        channels,
    )
}

#[tokio::test]
async fn quality_evaluates_native_vs_broadcast_dnsbl_and_mail_across_both_families() {
    let (ctx, mut channels) = make_ctx(dual_stack_host());
    let plugin = Quality {
        dnsbl: true,
        mail: true,
        base_url: Some("http://127.0.0.1:18081".to_owned()),
        dns_server: None,
        smtp_server: None,
    };
    let out = plugin.run(&ctx).await.expect("quality succeeds");

    let v4 = out.ipv4.expect("ipv4 report");
    assert_eq!(v4.asn, Some(64500));
    assert_eq!(
        v4.origin,
        Origin::Native {
            country: "US".to_owned()
        }
    );
    assert_eq!(v4.usage, Some(UsageType::Hosting));
    let dc = v4
        .factors
        .iter()
        .find(|f| f.factor == RiskFactor::Datacenter)
        .expect("dc factor");
    assert_eq!(dc.flagged_by, ["ProxyCheck", "ipapi.is", "Shodan"]);

    let v4_dns = v4.dnsbl.expect("v4 dnsbl");
    assert_eq!(v4_dns.listed, ["SpamCop"]);
    assert_eq!(
        v4_dns.refused, 1,
        "Spamhaus 127.255.255.254 counted as refused"
    );
    assert_eq!(v4_dns.checked, 23);

    assert_eq!(v4.mail[0].status, MailStatus::Deliverable(220));
    assert_eq!(v4.mail[3].status, MailStatus::Rejected(554));

    let v6 = out.ipv6.expect("ipv6 report");
    assert_eq!(v6.geo_total, 5, "ProxyCheck succeeded over IPv4 fallback");
    let v6_dc = v6
        .factors
        .iter()
        .find(|f| f.factor == RiskFactor::Datacenter)
        .expect("v6 dc factor");
    assert_eq!(v6_dc.flagged_by, ["ProxyCheck", "ipapi.is", "Shodan"]);
    assert_eq!(
        v6.origin,
        Origin::Broadcast {
            registered: "US".to_owned(),
            located: "SG".to_owned(),
        }
    );
    assert!(v6.mail.iter().all(|m| m.status == MailStatus::Blocked));

    let mut steps = Vec::new();
    while let Ok(Progress::Step(step)) = channels.progress.try_recv() {
        steps.push(step);
    }
    assert_eq!(
        steps,
        [
            Step::IpQuality {
                stage: "intelligence",
                ipv6: false
            },
            Step::IpQuality {
                stage: "dnsbl",
                ipv6: false
            },
            Step::IpQuality {
                stage: "mail",
                ipv6: false
            },
            Step::IpQuality {
                stage: "intelligence",
                ipv6: true
            },
            Step::IpQuality {
                stage: "dnsbl",
                ipv6: true
            },
            Step::IpQuality {
                stage: "mail",
                ipv6: true
            },
        ]
    );
}

#[tokio::test]
async fn quality_skips_when_offline_or_without_public_address_and_rejects_bad_endpoints() {
    let (offline_ctx, _) = make_ctx(HostProfile::default());
    assert_eq!(
        Quality::default().run(&offline_ctx).await,
        Err(Stop::Skipped(Failure::NoFamily))
    );

    let no_addr_host = HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![("1.1.1.1:443".parse().expect("v4"), Reach::Connected)],
                address: Address::NotAsked,
            },
            ipv6: Family::default(),
        },
        ..HostProfile::default()
    };
    let (no_addr_ctx, _) = make_ctx(no_addr_host);
    assert_eq!(
        Quality::default().run(&no_addr_ctx).await,
        Err(Stop::Skipped(Failure::NoPublicAddress))
    );

    let (ctx, _) = make_ctx(dual_stack_host());
    let bad = Quality {
        dns_server: Some("8.8.8.8:53".to_owned()),
        ..Quality::default()
    };
    assert_eq!(
        bad.run(&ctx).await,
        Err(Stop::Failed(Failure::InvalidEndpoint(
            "8.8.8.8:53".to_owned()
        )))
    );

    let addr: std::net::IpAddr = "198.51.100.7".parse().expect("ip");
    let outcomes = [
        (
            plugin_ip::quality::providers::ripestat::SPEC,
            Ok(plugin_ip::quality::parse::ProviderData {
                org: Some("Parent Registry Block".to_owned()),
                ..Default::default()
            }),
        ),
        (
            plugin_ip::quality::providers::ipinfo::SPEC,
            Ok(plugin_ip::quality::parse::ProviderData {
                asn: Some(64500),
                org: Some("Downstream ISP".to_owned()),
                ..Default::default()
            }),
        ),
        (
            plugin_ip::quality::providers::ipapi::SPEC,
            Err(plugin_ip::quality::parse::ProviderFailure::Unexpected(
                "query failed for 198.51.100.7".to_owned(),
            )),
        ),
    ];
    let agg = plugin_ip::quality::measure::aggregate(addr, &outcomes, None, Vec::new());
    assert_eq!(agg.asn, Some(64500));
    assert_eq!(agg.org.as_deref(), Some("Downstream ISP"));
    assert_eq!(agg.notes[0].1, "query failed for 198.51.*.*");
}
