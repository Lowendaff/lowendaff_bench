//! End-to-end measurement tests for `ip.media` with a recorded mock `HttpClient`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::Plugin;
use bench_api::Progress;
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
        base_url: Some("http://127.0.0.1:18080".to_owned()),
    };

    let output = plugin.run(&ctx).await.expect("measurement succeeds");
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
    // Abema: JP over IPv4, NoAddress over IPv6.
    assert_eq!(output.results[1].ipv4, Some(Verdict::yes_in("JP")));
    assert_eq!(output.results[1].ipv6, Some(Verdict::NoAddress));
    // BBC iPlayer: BlockedWaf from cf-mitigated challenge.
    assert_eq!(output.results[2].ipv4, Some(Verdict::BlockedWaf));

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
    assert!(
        http.seen
            .lock()
            .expect("lock")
            .iter()
            .all(|(_, url)| url.starts_with("http://127.0.0.1:18080/")),
        "every probe honored base_url"
    );
}

#[tokio::test]
async fn unlock_skips_without_network_or_matching_services_and_rejects_bad_base_url() {
    let http: Arc<dyn HttpClient> = Arc::new(RecordedHttp::new());

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
}
