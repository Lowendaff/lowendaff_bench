//! Measurement, 800 ms primary-to-backup failover, packet-loss, and distribution tests for
//! `network.tcping`.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::DnsAnswer;
use bench_api::IpFamily;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::Plugin;
use bench_api::Progress;
use bench_api::SmtpBanner;
use bench_api::Step;
use bench_api::Stop;
use bench_api::TcpPingOutcome;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use plugin_network::tcping::Failure;
use plugin_network::tcping::ProbeCount;
use plugin_network::tcping::TARGETS;
use plugin_network::tcping::TargetOutcome;
use plugin_network::tcping::Tcping;
use tokio::time::Instant;

struct FakeTcpNet {
    /// scripted outcomes per `SocketAddr`: popped from the front; falls back to `TimedOut`.
    scripted: Mutex<BTreeMap<SocketAddr, Vec<TcpPingOutcome>>>,
    calls: Mutex<Vec<SocketAddr>>,
}

impl FakeTcpNet {
    fn new(entries: impl IntoIterator<Item = (SocketAddr, Vec<TcpPingOutcome>)>) -> Self {
        Self {
            scripted: Mutex::new(entries.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn call_count(&self, addr: SocketAddr) -> usize {
        self.calls
            .lock()
            .expect("lock calls")
            .iter()
            .filter(|&&a| a == addr)
            .count()
    }
}

impl NetClient for FakeTcpNet {
    fn dns_a<'a>(
        &'a self,
        _qname: &'a str,
        _server: Option<SocketAddr>,
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

    fn resolve_endpoint<'a>(
        &'a self,
        family: IpFamily,
        host: &'a str,
        port: u16,
        _allow_local: bool,
        _timeout: Duration,
    ) -> NetFuture<'a, SocketAddr> {
        Box::pin(async move {
            let ip: IpAddr = host.parse().map_err(|_| NetError::NoAddress)?;
            if ip.is_ipv6() != (family == IpFamily::Ipv6) {
                return Err(NetError::NoAddress);
            }
            Ok(SocketAddr::new(ip, port))
        })
    }

    fn tcp_ping<'a>(
        &'a self,
        addr: SocketAddr,
        _allow_local: bool,
        _timeout: Duration,
    ) -> NetFuture<'a, TcpPingOutcome> {
        Box::pin(async move {
            self.calls.lock().expect("lock calls").push(addr);
            let mut map = self.scripted.lock().expect("lock scripted");
            if let Some(queue) = map.get_mut(&addr)
                && !queue.is_empty()
            {
                return Ok(queue.remove(0));
            }
            Ok(TcpPingOutcome::TimedOut)
        })
    }
}

fn ipv4_host() -> Arc<HostProfile> {
    let v4_sock = SocketAddr::from(([198, 51, 100, 1], 443));
    Arc::new(HostProfile {
        network: Network {
            ipv4: Family {
                targets: vec![(v4_sock, Reach::Connected)],
                ..Family::default()
            },
            ipv6: Family::default(),
        },
        ..HostProfile::default()
    })
}

#[tokio::test(start_paused = true)]
async fn primary_timeout_fails_over_immediately_to_backup_and_computes_stats() {
    let dir = tempfile::tempdir().expect("tempdir");
    let list_path = dir.path().join("tcping.json");
    std::fs::write(
        &list_path,
        r#"{
          "version": 1,
          "targets": [
            {
              "id": "cn-sh-ct",
              "group": "china",
              "region": "Shanghai",
              "city": "Shanghai",
              "name": "Shanghai Telecom",
              "operator": "telecom",
              "asn": 4134,
              "default": true,
              "ipv4": [
                {"host": "192.0.2.10", "port": 443},
                {"host": "192.0.2.11", "port": 65499}
              ]
            },
            {
              "id": "global-jp-tyo",
              "group": "global",
              "region": "Asia-Pacific",
              "city": "Tokyo",
              "name": "Tokyo, JP",
              "operator": "other",
              "asn": 20473,
              "default": true,
              "ipv4": [
                {"host": "198.51.100.20", "port": 443}
              ]
            }
          ]
        }"#,
    )
    .expect("write list");

    let primary_dead = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443);
    let backup_live = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 11)), 65499);
    let tokyo_lossy = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 20)), 443);

    let net = Arc::new(FakeTcpNet::new([
        // Primary times out on probe #1 -> instant failover without probing primary 4 more times.
        (primary_dead, vec![TcpPingOutcome::TimedOut]),
        // Backup answers all 5 probes (10 ms, 12 ms, 14 ms, 16 ms, 30 ms).
        (
            backup_live,
            vec![
                TcpPingOutcome::Connected {
                    micros: 10_000,
                    rst: false,
                },
                TcpPingOutcome::Connected {
                    micros: 12_000,
                    rst: false,
                },
                TcpPingOutcome::Connected {
                    micros: 14_000,
                    rst: false,
                },
                TcpPingOutcome::Connected {
                    micros: 16_000,
                    rst: false,
                },
                TcpPingOutcome::Connected {
                    micros: 30_000,
                    rst: true,
                },
            ],
        ),
        // Tokyo drops 1 of 5 probes (20% loss): 40 ms, timeout, 42 ms, 44 ms, 46 ms.
        (
            tokyo_lossy,
            vec![
                TcpPingOutcome::Connected {
                    micros: 40_000,
                    rst: false,
                },
                TcpPingOutcome::TimedOut,
                TcpPingOutcome::Connected {
                    micros: 42_000,
                    rst: false,
                },
                TcpPingOutcome::Connected {
                    micros: 44_000,
                    rst: false,
                },
                TcpPingOutcome::Connected {
                    micros: 46_000,
                    rst: false,
                },
            ],
        ),
    ]));

    let files = Arc::new(BTreeMap::from([(
        TARGETS.to_owned(),
        Ok(list_path.clone()),
    )]));
    let (ctx, mut channels) = Context::new(
        ipv4_host(),
        Arc::default(),
        files,
        Instant::now() + Duration::from_secs(60),
        dir.path().to_path_buf(),
        CancellationToken::new(),
    );
    let ctx = ctx.with_net(net.clone());

    let plugin = Tcping {
        targets: Some(list_path.display().to_string()),
        groups: Vec::new(),
        all_targets: false,
        count: ProbeCount::new(5).expect("count=5"),
    };
    let out = plugin.run(&ctx).await.expect("tcping succeeds");

    // Primary must only be probed ONCE before failing over to backup!
    assert_eq!(net.call_count(primary_dead), 1);
    assert_eq!(net.call_count(backup_live), 5);
    assert_eq!(net.call_count(tokyo_lossy), 5);

    assert_eq!(out.targets.len(), 2);
    match &out.targets[0].outcome {
        TargetOutcome::Measured(m) => {
            assert_eq!(m.endpoint, "192.0.2.11");
            assert_eq!(m.port, 65499);
            assert!(m.used_backup);
            assert_eq!(m.sent, 5);
            assert_eq!(m.received, 5);
            assert_eq!(m.loss_pct, 0.0);
            assert_eq!(m.min_ms, Some(10.0));
            assert_eq!(m.avg_ms, Some(16.4));
            assert_eq!(m.median_ms, Some(14.0));
            assert_eq!(m.p95_ms, Some(30.0));
            assert_eq!(m.max_ms, Some(30.0));
            // Consecutive diffs: |12-10|=2, |14-12|=2, |16-14|=2, |30-16|=14 -> mean = 20/4 = 5.0
            assert_eq!(m.jitter_ms, Some(5.0));
        }
        other => panic!("expected Measured on backup, got {other:?}"),
    }

    match &out.targets[1].outcome {
        TargetOutcome::Measured(m) => {
            assert!(!m.used_backup);
            assert_eq!(m.sent, 5);
            assert_eq!(m.received, 4);
            assert_eq!(m.loss_pct, 20.0);
            assert_eq!(m.min_ms, Some(40.0));
            assert_eq!(m.max_ms, Some(46.0));
        }
        other => panic!("expected Measured on tokyo, got {other:?}"),
    }

    let mut steps = Vec::new();
    while let Ok(Progress::Step(step)) = channels.progress.try_recv() {
        steps.push(step);
    }
    assert_eq!(
        steps,
        [
            Step::Tcping {
                group: "china",
                targets: 1,
                ipv6: false,
            },
            Step::Tcping {
                group: "global",
                targets: 1,
                ipv6: false,
            },
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn all_blackholed_targets_return_all_unreachable_while_publishing_partial_loss() {
    let dir = tempfile::tempdir().expect("tempdir");
    let list_path = dir.path().join("tcping.json");
    std::fs::write(
        &list_path,
        r#"{
          "version": 1,
          "targets": [
            {
              "id": "cn-bj-ct",
              "group": "china",
              "region": "Beijing",
              "city": "Beijing",
              "name": "Beijing Telecom",
              "operator": "telecom",
              "asn": 4134,
              "default": true,
              "ipv4": [{"host": "192.0.2.99", "port": 443}]
            }
          ]
        }"#,
    )
    .expect("write list");

    let net = Arc::new(FakeTcpNet::new([]));
    let files = Arc::new(BTreeMap::from([(
        TARGETS.to_owned(),
        Ok(list_path.clone()),
    )]));
    let (ctx, channels) = Context::new(
        ipv4_host(),
        Arc::default(),
        files,
        Instant::now() + Duration::from_secs(60),
        dir.path().to_path_buf(),
        CancellationToken::new(),
    );
    let ctx = ctx.with_net(net);

    let plugin = Tcping {
        targets: Some(list_path.display().to_string()),
        groups: Vec::new(),
        all_targets: false,
        count: ProbeCount::new(5).expect("count=5"),
    };
    let err = plugin.run(&ctx).await.expect_err("must fail");
    assert_eq!(err, Stop::Failed(Failure::AllUnreachable));
    let published = channels
        .published
        .borrow()
        .clone()
        .expect("partial published");
    assert_eq!(published.targets.len(), 1);
    assert!(matches!(
        &published.targets[0].outcome,
        TargetOutcome::Loss(m) if m.sent == 5 && m.received == 0 && m.loss_pct == 100.0
    ));
}
