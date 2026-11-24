//! The item's run: the target list, which it reads and checks; what it chooses, which may end the
//! item before NextTrace runs; and the plan of a host that reaches IPv4, whose traces fail here,
//! since NextTrace is not there.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Plugin;
use bench_api::Stop;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::profile::HostProfile;
use bench_tool::job::Fault;
use plugin_network::trace::Failure;
use plugin_network::trace::Output;
use plugin_network::trace::Trace;
use plugin_network::trace::lists::ListError;
use plugin_network::trace::lists::Rule;
use plugin_network::trace::lookup::LookupError;
use plugin_network::trace::result::ExchangeList;
use plugin_network::trace::result::Place;
use plugin_network::trace::result::Route;
use plugin_network::trace::result::Target;
use plugin_network::trace::result::TraceError;
use plugin_network::trace::result::Traced;
use plugin_network::trace::result::Unavailable;
use plugin_network::trace::targets::Group;
use serde_json::json;
use tokio::time::Instant;

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

/// A family that reached `target`.
fn reached(target: &str) -> Family {
    Family {
        targets: vec![(target.parse().expect("an address"), Reach::Connected)],
        address: Address::NotAsked,
    }
}

/// A host that reaches IPv4 alone.
fn ipv4_host() -> HostProfile {
    HostProfile {
        network: Network {
            ipv4: reached("192.0.2.1:443"),
            ipv6: Family::default(),
        },
        ..HostProfile::default()
    }
}

/// A host that reaches both families.
fn dual_host() -> HostProfile {
    HostProfile {
        network: Network {
            ipv4: reached("192.0.2.1:443"),
            ipv6: reached("[2001:db8::1]:443"),
        },
        ..HostProfile::default()
    }
}

/// Runs the item, configured by `table`, on `host` with `files`, NextTrace being absent; the
/// result, and what it published last.
async fn run_with(
    table: serde_json::Value,
    host: HostProfile,
    files: BTreeMap<String, Result<PathBuf, String>>,
) -> (Result<Output, Stop<Failure>>, Option<Output>) {
    let item: Trace = serde_json::from_value(table).expect("a table");
    let (ctx, channels) = Context::<Output>::new(
        Arc::new(host),
        Arc::new(BTreeMap::from([(
            "nexttrace".to_owned(),
            PathBuf::from("/nonexistent/nexttrace"),
        )])),
        Arc::new(files),
        Instant::now() + Duration::from_secs(60),
        "/run/work".into(),
        CancellationToken::new(),
    );
    let result = item.run(&ctx).await;
    let published = channels.published.borrow().clone();
    (result, published)
}

/// Runs the item, configured by `table`, with the target list at `list` alone, on a host of which
/// nothing is known, so that it reaches no family.
async fn run(table: serde_json::Value, list: &Path) -> Result<Output, Stop<Failure>> {
    let files = BTreeMap::from([("targets".to_owned(), Ok(list.to_path_buf()))]);
    run_with(table, HostProfile::default(), files).await.0
}

#[tokio::test]
async fn a_list_that_breaks_a_rule_fails_the_item_and_a_newer_one_skips_it() {
    assert_eq!(
        run(json!({}), &fixture("target-lists/bad-id-youtube.json")).await,
        Err(Stop::Failed(Failure::List(ListError::Entry(1, Rule::Id))))
    );
    assert_eq!(
        run(json!({}), &fixture("target-lists/bad-version-2.json")).await,
        Err(Stop::Skipped(Failure::List(ListError::Newer(2))))
    );
}

#[tokio::test]
async fn a_list_that_cannot_be_read_fails_the_item_with_the_systems_reason() {
    let Err(Stop::Failed(Failure::Unreadable(reason))) =
        run(json!({}), Path::new("/nonexistent/targets.json")).await
    else {
        panic!("unreadable");
    };
    assert!(!reason.is_empty());
}

#[tokio::test]
async fn a_list_that_never_ends_is_read_up_to_a_byte_more_than_a_list_may_hold() {
    // A device that never ends stands for a list larger than any.
    assert_eq!(
        run(json!({}), Path::new("/dev/zero")).await,
        Err(Stop::Failed(Failure::List(ListError::TooLarge)))
    );
}

#[tokio::test]
async fn nothing_chosen_skips_the_item_with_what_was_asked_for_and_no_family_skips_it_too() {
    let list = fixture("target-lists/good-one-target.json");
    assert_eq!(
        run(json!({ "targets": ["xyz"] }), &list).await,
        Err(Stop::Skipped(Failure::NoTarget(vec!["xyz".to_owned()])))
    );
    assert_eq!(
        run(
            json!({ "groups": ["china"] }),
            &fixture("target-lists/good-no-targets.json")
        )
        .await,
        Err(Stop::Skipped(Failure::NoTarget(Vec::new())))
    );
    assert_eq!(
        run(json!({}), &list).await,
        Err(Stop::Skipped(Failure::NoFamily)),
        "a host that reaches no family"
    );
}

#[tokio::test]
async fn remote_target_list_rejects_loopback_localhost_and_private_ips_unless_local_override() {
    let dir = tempfile::tempdir().expect("tempdir");
    for bad_v4 in ["127.0.0.1", "10.0.0.1", "169.254.169.254", "localhost"] {
        let path = dir.path().join(format!("targets-{bad_v4}.json"));
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version": 1,
                "targets": [{
                    "id": "test",
                    "group": "international",
                    "name": "Test",
                    "asn": 64496,
                    "ipv4": bad_v4,
                    "default": true
                }]
            }))
            .expect("json"),
        )
        .expect("write");

        assert_eq!(
            run(json!({}), &path).await,
            Err(Stop::Failed(Failure::List(ListError::Entry(1, Rule::Ipv4)))),
            "default remote target list must reject {bad_v4}"
        );
    }

    let lo_path = dir.path().join("targets-127.0.0.1.json");
    assert_eq!(
        run(json!({ "list": lo_path.to_str().unwrap() }), &lo_path).await,
        Err(Stop::Skipped(Failure::NoFamily)),
        "local target list file override permits 127.0.0.1"
    );
}

#[tokio::test]
async fn the_plan_traces_each_cache_server_by_its_answer_then_the_lists_targets() {
    let files = BTreeMap::from([
        (
            "targets".to_owned(),
            Ok(fixture("target-lists/good-every-shape.json")),
        ),
        (
            "exchanges".to_owned(),
            Ok(fixture("exchange-lists/good-every-shape.json")),
        ),
        (
            "youtube-ipv4".to_owned(),
            Ok(fixture("lookups/report-mapping-isp.txt")),
        ),
        (
            "netflix-ipv4".to_owned(),
            Err("download failed: HTTP 404".to_owned()),
        ),
    ]);
    // Every target, the defaults and the others.
    let (result, published) = run_with(json!({ "groups": ["all"] }), ipv4_host(), files).await;
    assert_eq!(result, Err(Stop::Failed(Failure::NoRoute)));
    let published = published.expect("the item published what it traced");
    assert_eq!(published.exchanges, ExchangeList::Read(7));
    assert!(published.ipv6.is_empty(), "the host has no IPv6");
    let plan: Vec<(Target, Option<&str>)> = published
        .ipv4
        .iter()
        .map(|traced| (traced.target.clone(), traced.host.as_deref()))
        .collect();
    let listed = |id: &str, group, name: &str| Target::Listed {
        id: id.to_owned(),
        group,
        name: name.to_owned(),
    };
    assert_eq!(
        plan,
        [
            (
                Target::YouTube,
                Some("r1---sn-cxaaj5o5q5-tt1ed.googlevideo.com")
            ),
            (Target::Netflix(None), None),
            (
                listed("pek-4809", Group::China, "Beijing Telecom CN2"),
                Some("ipv4.pek-4809.endpoint.nxtrace.org")
            ),
            (
                listed("gcp-sin", Group::International, "GCP - Singapore, SG"),
                Some("35.187.238.97")
            ),
            (
                listed("v6-literal", Group::International, "Example - Six"),
                Some("192.0.2.1")
            ),
            (
                listed("a", Group::China, &"N".repeat(40)),
                Some("a.example")
            ),
        ]
    );
    assert_eq!(
        published.ipv4[1].route,
        Route::Unavailable(Unavailable::Unfetched(
            "download failed: HTTP 404".to_owned()
        ))
    );
    let with_hosts = published.ipv4.iter().filter(|traced| traced.host.is_some());
    for traced in with_hosts {
        assert!(
            matches!(
                &traced.route,
                Route::Failed(TraceError::Fault(Fault::Spawn(_)))
            ),
            "{traced:?}"
        );
    }
}

#[tokio::test]
async fn over_ipv6_the_plan_takes_the_ipv6_answers_and_the_targets_ipv6_names() {
    let files = BTreeMap::from([
        (
            "targets".to_owned(),
            Ok(fixture("target-lists/good-every-shape.json")),
        ),
        (
            "youtube-ipv4".to_owned(),
            Ok(fixture("lookups/report-mapping-isp.txt")),
        ),
        (
            "youtube-ipv6".to_owned(),
            Ok(fixture("lookups/report-mapping-u.txt")),
        ),
        ("netflix-ipv4".to_owned(), Ok(fixture("lookups/fast.json"))),
        (
            "netflix-ipv6".to_owned(),
            Err("not fetched: the host has no IPv6 connection".to_owned()),
        ),
    ]);
    let (_, published) = run_with(json!({ "groups": ["all"] }), dual_host(), files).await;
    let published = published.expect("a result");
    let plan = |traced: &[Traced]| -> Vec<(String, Option<String>)> {
        traced
            .iter()
            .map(|traced| {
                let name = match &traced.target {
                    Target::YouTube => "youtube".to_owned(),
                    Target::Netflix(_) => "netflix".to_owned(),
                    Target::Listed { id, .. } => id.clone(),
                };
                (name, traced.host.clone())
            })
            .collect()
    };
    let some = |host: &str| Some(host.to_owned());
    assert_eq!(
        plan(&published.ipv4),
        [
            (
                "youtube".to_owned(),
                some("r1---sn-cxaaj5o5q5-tt1ed.googlevideo.com")
            ),
            (
                "netflix".to_owned(),
                some("ipv4-c001-fra002-ix.1.oca.nflxvideo.net")
            ),
            (
                "pek-4809".to_owned(),
                some("ipv4.pek-4809.endpoint.nxtrace.org")
            ),
            ("gcp-sin".to_owned(), some("35.187.238.97")),
            ("v6-literal".to_owned(), some("192.0.2.1")),
            ("a".to_owned(), some("a.example")),
        ]
    );
    assert_eq!(
        plan(&published.ipv6),
        [
            (
                "youtube".to_owned(),
                some("r1---sn-f58xn2xxq-aj5l.googlevideo.com")
            ),
            ("netflix".to_owned(), None),
            (
                "pek-4809".to_owned(),
                some("ipv6.pek-4809.endpoint.nxtrace.org")
            ),
            ("v6-literal".to_owned(), some("2001:db8::1")),
        ],
        "over IPv6, the answers fetched over it, and the targets with an IPv6 name"
    );
    assert_eq!(
        published.ipv6[1].route,
        Route::Unavailable(Unavailable::Unfetched(
            "not fetched: the host has no IPv6 connection".to_owned()
        ))
    );
}

#[tokio::test]
async fn an_exchange_list_that_cannot_be_used_leaves_the_hops_without_exchanges() {
    let with = |exchanges: PathBuf| {
        BTreeMap::from([
            (
                "targets".to_owned(),
                Ok(fixture("target-lists/good-one-target.json")),
            ),
            ("exchanges".to_owned(), Ok(exchanges)),
        ])
    };
    let table = json!({ "groups": ["china"] });
    let (_, published) = run_with(
        table.clone(),
        ipv4_host(),
        with(fixture("exchange-lists/bad-address-capitals.json")),
    )
    .await;
    assert_eq!(
        published.expect("a result").exchanges,
        ExchangeList::Unusable(ListError::Entry(1, Rule::Address))
    );
    let (_, published) = run_with(
        table,
        ipv4_host(),
        with(PathBuf::from("/nonexistent/ix.json")),
    )
    .await;
    assert!(
        matches!(
            published.expect("a result").exchanges,
            ExchangeList::Unreadable(_)
        ),
        "an exchange list that cannot be read"
    );
}

#[tokio::test]
async fn a_cache_servers_answer_that_names_none_or_is_missing_says_why() {
    let files = BTreeMap::from([
        (
            "targets".to_owned(),
            Ok(fixture("target-lists/good-no-targets.json")),
        ),
        ("youtube-ipv4".to_owned(), Ok(fixture("lookups/fast.json"))),
        ("netflix-ipv4".to_owned(), Ok(fixture("lookups/fast.json"))),
    ]);
    let (result, published) = run_with(json!({}), ipv4_host(), files).await;
    assert_eq!(result, Err(Stop::Failed(Failure::NoRoute)));
    let published = published.expect("a result");
    assert_eq!(
        published.exchanges,
        ExchangeList::Missing(String::new()),
        "the exchange list was not asked for here"
    );
    assert_eq!(
        published.ipv4[0].route,
        Route::Unavailable(Unavailable::Answer(LookupError::NoServer)),
        "fast.com's answer names no YouTube server"
    );
    assert_eq!(
        published.ipv4[1].target,
        Target::Netflix(Some(Place {
            city: "Frankfurt".to_owned(),
            country: "DE".to_owned(),
        }))
    );
    assert_eq!(
        published.ipv4[1].host.as_deref(),
        Some("ipv4-c001-fra002-ix.1.oca.nflxvideo.net")
    );
}

#[tokio::test]
async fn remote_netflix_lookup_rejects_loopback_localhost_and_private_ips_unless_local_override() {
    let dir = tempfile::tempdir().expect("tempdir");
    for bad_host in ["127.0.0.1", "10.0.0.1", "169.254.169.254", "localhost"] {
        let fast_path = dir.path().join(format!("fast-{bad_host}.json"));
        let url = format!("https://{bad_host}/speedtest?v=1");
        std::fs::write(
            &fast_path,
            serde_json::to_vec(&json!({
                "targets": [{
                    "name": url,
                    "url": url,
                    "location": {"city": "Test", "country": "US"}
                }]
            }))
            .expect("json"),
        )
        .expect("write");

        for cfg in [
            json!({"groups": ["services"]}),
            json!({
                "groups": ["services"],
                "netflix_api": "https://api.fast.com/netflix/speedtest/v2"
            }),
        ] {
            let files = BTreeMap::from([
                (
                    "targets".to_owned(),
                    Ok(fixture("target-lists/good-no-targets.json")),
                ),
                (
                    "youtube-ipv4".to_owned(),
                    Ok(fixture("lookups/report-mapping-isp.txt")),
                ),
                ("netflix-ipv4".to_owned(), Ok(fast_path.clone())),
            ]);
            let (_, published) = run_with(cfg, ipv4_host(), files).await;
            let published = published.expect("published");
            assert_eq!(published.ipv4[1].target, Target::Netflix(None));
            assert_eq!(published.ipv4[1].host, None);
            assert_eq!(
                published.ipv4[1].route,
                Route::Unavailable(Unavailable::Answer(LookupError::Url)),
                "remote netflix_api must reject {bad_host}"
            );
        }
    }

    let lo_fast = dir.path().join("fast-127.0.0.1.json");
    let files = BTreeMap::from([
        (
            "targets".to_owned(),
            Ok(fixture("target-lists/good-no-targets.json")),
        ),
        (
            "youtube-ipv4".to_owned(),
            Ok(fixture("lookups/report-mapping-isp.txt")),
        ),
        ("netflix-ipv4".to_owned(), Ok(lo_fast.clone())),
    ]);
    let (_, published) = run_with(
        json!({
            "groups": ["services"],
            "netflix_api": lo_fast.to_str().unwrap()
        }),
        ipv4_host(),
        files,
    )
    .await;
    let published = published.expect("published");
    assert_eq!(published.ipv4[1].host.as_deref(), Some("127.0.0.1"));
}
