//! Measurement state-machine tests for `network.taier`: busy (`dovalid: -1`) fallback across
//! same-carrier candidates, empty-token retry on `AuthRejected`, lease release on completion and
//! failure, and partial result publishing.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::TaierAdmission;
use bench_api::TaierPacketStats;
use bench_api::TaierStreamSpec;
use bench_api::TaierTransfer;
use bench_host::profile::HostProfile;
use plugin_network::taier::NodeOutcome;
use plugin_network::taier::Operator;
use plugin_network::taier::Output;
use plugin_network::taier::Settings;
use plugin_network::taier::measure;
use plugin_network::taier::nodes;
use tokio::time::Instant;

#[derive(Default)]
struct MockTaier {
    acquires: Mutex<Vec<(Ipv4Addr, String)>>,
    releases: Mutex<Vec<(Ipv4Addr, String)>>,
}

impl NetClient for MockTaier {
    fn dns_a<'a>(
        &'a self,
        _qname: &'a str,
        _server: Option<std::net::SocketAddr>,
        _timeout: Duration,
    ) -> NetFuture<'a, bench_api::DnsAnswer> {
        Box::pin(async { Err(NetError::NoResolver) })
    }

    fn smtp_banner<'a>(
        &'a self,
        _family: bench_api::IpFamily,
        _host: &'a str,
        _port: u16,
        _timeout: Duration,
    ) -> NetFuture<'a, bench_api::SmtpBanner> {
        Box::pin(async { Err(NetError::TimedOut) })
    }

    fn taier_acquire<'a>(
        &'a self,
        ip: Ipv4Addr,
        _port: u16,
        query: &'a str,
        _timeout: Duration,
    ) -> NetFuture<'a, TaierAdmission> {
        self.acquires
            .lock()
            .expect("acquires")
            .push((ip, query.to_owned()));
        Box::pin(async move {
            match ip.octets()[3] {
                1 => Ok(TaierAdmission::Busy),
                2 => {
                    if query.contains("&token=&") {
                        Ok(TaierAdmission::Admitted("lease-fallback-2".to_owned()))
                    } else {
                        Ok(TaierAdmission::AuthRejected)
                    }
                }
                3 => Ok(TaierAdmission::Busy),
                _ => Err(NetError::TimedOut),
            }
        })
    }

    fn taier_release<'a>(
        &'a self,
        ip: Ipv4Addr,
        _port: u16,
        key: &'a str,
        _timeout: Duration,
    ) -> NetFuture<'a, ()> {
        self.releases
            .lock()
            .expect("releases")
            .push((ip, key.to_owned()));
        Box::pin(async { Ok(()) })
    }

    fn taier_download<'a>(
        &'a self,
        _spec: &'a TaierStreamSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierTransfer> {
        Box::pin(async {
            Ok(TaierTransfer {
                bytes: 500_000_000,
                elapsed_micros: 4_000_000,
                bits_per_sec: 1_000_000_000,
                active_streams: 8,
            })
        })
    }

    fn taier_upload<'a>(
        &'a self,
        _spec: &'a TaierStreamSpec,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierTransfer> {
        Box::pin(async {
            Ok(TaierTransfer {
                bytes: 250_000_000,
                elapsed_micros: 4_000_000,
                bits_per_sec: 500_000_000,
                active_streams: 8,
            })
        })
    }

    fn taier_packet_probe<'a>(
        &'a self,
        _ip: Ipv4Addr,
        _port: u16,
        large: bool,
        samples: u8,
        _cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierPacketStats> {
        Box::pin(async move {
            Ok(TaierPacketStats {
                sent: samples,
                lost: u8::from(large),
                avg_micros: Some(if large { 35_000 } else { 31_000 }),
                min_micros: Some(30_000),
                max_micros: Some(42_000),
                jitter_micros: Some(2_500),
            })
        })
    }
}

#[tokio::test]
async fn busy_node_falls_back_to_next_candidate_and_empty_token_retries_and_releases() {
    let raw = br#"{
        "version": 1,
        "locations": [
            {
                "id": "beijing",
                "province": "Beijing",
                "city": "Beijing",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.1", "port": 65499},
                    {"operator": "telecom", "ip": "198.51.100.2", "port": 65499},
                    {"operator": "unicom", "ip": "198.51.100.3", "port": 65499}
                ]
            }
        ]
    }"#;
    let locations = nodes::parse(raw).unwrap();
    let chosen = nodes::choose(&locations, &[], &[]);
    let mock = Arc::new(MockTaier::default());
    let (ctx, channels) = Context::<Output>::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(60),
        ".".into(),
        CancellationToken::new(),
    );
    let ctx = ctx.with_net(Arc::clone(&mock) as Arc<dyn NetClient>);
    let settings = Settings {
        seconds: 4,
        streams: 8,
        probes: true,
        cities: Vec::new(),
        operators: chosen.operators.clone(),
        custom_list: false,
    };

    let output = measure::measure(&ctx, chosen, settings, 42, || 1_700_000_000)
        .await
        .expect("at least one slot succeeded");

    assert_eq!(output.results.len(), 2);
    // Slot 0 (Telecom): .1 returned Busy -> fell back to .2 -> retried empty token -> succeeded!
    let ct = &output.results[0];
    assert_eq!(ct.operator, Operator::Telecom);
    assert_eq!(ct.ip, Ipv4Addr::new(198, 51, 100, 2));
    assert_eq!(ct.tried_nodes, 2);
    let NodeOutcome::Measured(measured) = ct.outcome else {
        panic!("expected Measured, got {:?}", ct.outcome);
    };
    assert_eq!(measured.download_bps, 1_000_000_000);
    assert_eq!(measured.upload_bps, 500_000_000);
    assert_eq!(measured.small_probe.unwrap().lost, 0);
    assert_eq!(measured.large_probe.unwrap().lost, 1);

    // Slot 1 (Unicom): .3 returned Busy -> recorded as Busy without failing the plugin!
    assert_eq!(output.results[1].outcome, NodeOutcome::Busy);

    // Lease was cleanly released on .2.
    assert_eq!(
        *mock.releases.lock().unwrap(),
        vec![(
            Ipv4Addr::new(198, 51, 100, 2),
            "lease-fallback-2".to_owned()
        )]
    );
    assert!(channels.published.borrow().is_some());
}
