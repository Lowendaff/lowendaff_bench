//! The item's run up to its first test: the server list, which it reads and checks, and the
//! servers it chooses, each of which may end the item before iperf3 runs.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Plugin;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use plugin_network::iperf3::Failure;
use plugin_network::iperf3::Iperf3;
use plugin_network::iperf3::Output;
use plugin_network::iperf3::servers::ListError;
use plugin_network::iperf3::servers::Rule;
use serde_json::json;
use tokio::time::Instant;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/server-lists")
        .join(name)
}

/// Runs the item, configured by `table`, with the list at `list` on a host of which nothing is
/// known, so that it reaches no family; iperf3 is not there, and must not be needed.
async fn run(table: serde_json::Value, list: &Path) -> Result<Output, Stop<Failure>> {
    let item: Iperf3 = serde_json::from_value(table).expect("a table");
    let (ctx, _channels) = Context::<Output>::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::from([(
            "iperf3".to_owned(),
            PathBuf::from("/nonexistent/iperf3"),
        )])),
        Arc::new(BTreeMap::from([(
            "servers".to_owned(),
            Ok(list.to_path_buf()),
        )])),
        Instant::now() + Duration::from_secs(60),
        "/run/work".into(),
        CancellationToken::new(),
    );
    item.run(&ctx).await
}

#[tokio::test]
async fn a_list_that_breaks_a_rule_fails_the_item_and_a_newer_one_skips_it() {
    assert_eq!(
        run(json!({}), &fixture("bad-host-option.json")).await,
        Err(Stop::Failed(Failure::List(ListError::Server(
            1,
            Rule::Host
        ))))
    );
    assert_eq!(
        run(json!({}), &fixture("bad-version-2.json")).await,
        Err(Stop::Skipped(Failure::List(ListError::Newer(2))))
    );
}

#[tokio::test]
async fn a_list_that_cannot_be_read_fails_the_item_with_the_systems_reason() {
    let Err(Stop::Failed(Failure::Unreadable(reason))) =
        run(json!({}), Path::new("/nonexistent/servers.json")).await
    else {
        panic!("unreadable");
    };
    assert!(!reason.is_empty());
}

#[tokio::test]
async fn a_list_that_never_ends_is_read_up_to_a_byte_more_than_a_list_may_hold() {
    // A device that never ends stands for a list larger than any: read to its end, it would
    // never be done.
    assert_eq!(
        run(json!({}), Path::new("/dev/zero")).await,
        Err(Stop::Failed(Failure::List(ListError::TooLarge)))
    );
}

#[tokio::test]
async fn no_server_chosen_skips_the_item_with_what_was_asked_for() {
    let list = fixture("good-one-server.json");
    assert_eq!(
        run(json!({ "countries": ["BR"] }), &list).await,
        Err(Stop::Skipped(Failure::NoServer(vec!["BR".to_owned()])))
    );
    assert_eq!(
        run(json!({}), &fixture("good-no-servers.json")).await,
        Err(Stop::Skipped(Failure::NoServer(Vec::new())))
    );
    assert_eq!(
        run(json!({}), &list).await,
        Err(Stop::Skipped(Failure::NoFamily)),
        "a host that reaches no family"
    );
}

#[tokio::test]
async fn remote_server_list_rejects_loopback_localhost_and_private_ips_unless_local_override() {
    let dir = tempfile::tempdir().expect("tempdir");
    for bad_host in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "localhost",
        "2130706433",
        "0x7f000001",
        "127.1",
        "0177.0.0.1",
        "169.254.43518",
        "metadata",
        "metadata.google.internal",
        "host.local",
        "home.arpa",
    ] {
        let path = dir.path().join(format!("list-{bad_host}.json"));
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version": 1,
                "servers": [{
                    "id": "test",
                    "country": "US",
                    "location": "Test",
                    "provider": "Test",
                    "host": bad_host,
                    "ports": "5201",
                    "families": ["ipv4"],
                    "link": "1G",
                    "default": true
                }]
            }))
            .expect("json"),
        )
        .expect("write");

        assert_eq!(
            run(json!({}), &path).await,
            Err(Stop::Failed(Failure::List(ListError::Server(
                1,
                Rule::Host
            )))),
            "default remote list mode must reject {bad_host}"
        );
    }

    let lo_path = dir.path().join("list-127.0.0.1.json");
    assert_eq!(
        run(json!({ "list": lo_path.to_str().unwrap() }), &lo_path).await,
        Err(Stop::Skipped(Failure::NoFamily)),
        "local file list override permits 127.0.0.1 and advances past list validation"
    );
}
