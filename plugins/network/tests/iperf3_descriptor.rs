//! The item's identity, its requirements, its configuration, and the server list it asks for.

use std::path::PathBuf;
use std::time::Duration;

use bench_api::FileSource;
use bench_api::FileSpec;
use bench_api::Net;
use bench_api::Plugin;
use plugin_network::iperf3::Iperf3;
use plugin_network::iperf3::SERVERS_URL;
use plugin_network::iperf3::result::Settings;
use serde_json::json;

fn config(value: serde_json::Value) -> Result<Iperf3, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn the_item_runs_on_every_platform_with_the_pinned_iperf3_and_either_family() {
    let descriptor = Iperf3::DESCRIPTOR;
    assert_eq!(descriptor.id, "network.iperf3");
    assert_eq!(descriptor.timeout, Duration::from_secs(45 * 60));
    assert!(descriptor.requires.platforms.is_empty(), "every platform");
    assert_eq!(descriptor.requires.net, Net::Any);
    assert_eq!(
        descriptor.requires.glibc, None,
        "iperf3 is linked statically"
    );
    assert!(!descriptor.requires.numa);
    let tools: Vec<(&str, &str)> = descriptor
        .requires
        .tools
        .iter()
        .map(|tool| (tool.name, tool.version))
        .collect();
    assert_eq!(tools, [("iperf3", "3.22")]);
}

#[test]
fn by_default_the_item_tests_yabss_way_the_repositorys_list_and_its_default_servers() {
    let item = config(json!({})).expect("an empty table");
    assert_eq!(item.settings(), Settings::YABS);
    assert_eq!(
        Settings::YABS,
        Settings {
            seconds: 10,
            streams: 8,
            rate: None,
        }
    );
    assert!(item.countries.is_empty() && item.servers.is_empty());
    assert_eq!(
        item.files(),
        [FileSpec {
            name: "servers",
            source: FileSource::Url(SERVERS_URL.to_owned()),
            family: None,
            optional: false,
        }],
        "the item needs its list, over either family"
    );
    assert_eq!(
        SERVERS_URL,
        "https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/iperf3.json"
    );
    assert_eq!(Iperf3::default().settings(), Settings::YABS);
}

#[test]
fn a_list_of_the_users_own_is_a_path_or_an_address() {
    let list = |list: &str| config(json!({ "list": list })).expect("a list").files();
    assert_eq!(
        list("/etc/my-servers.json")[0].source,
        FileSource::Path(PathBuf::from("/etc/my-servers.json"))
    );
    assert_eq!(
        list("my-servers.json")[0].source,
        FileSource::Path(PathBuf::from("my-servers.json"))
    );
    for address in [
        "https://example.com/servers.json",
        "http://example.com/servers.json",
    ] {
        assert_eq!(
            list(address)[0].source,
            FileSource::Url(address.to_owned()),
            "the program refuses an address other than HTTPS"
        );
    }
}

#[test]
fn the_settings_and_the_choice_of_servers_come_from_the_table() {
    let item = config(json!({
        "countries": ["us", "JP"],
        "servers": ["tokyo"],
        "seconds": 3,
        "streams": 2,
        "rate": 100,
    }))
    .expect("a table");
    assert_eq!(item.countries, ["us", "JP"]);
    assert_eq!(item.servers, ["tokyo"]);
    assert_eq!(
        item.settings(),
        Settings {
            seconds: 3,
            streams: 2,
            rate: Some(100),
        }
    );
}

#[test]
fn zero_seconds_streams_or_rate_and_unknown_fields_are_refused() {
    for table in [
        json!({ "seconds": 0 }),
        json!({ "streams": 0 }),
        json!({ "streams": 256 }),
        json!({ "rate": 0 }),
        json!({ "seconds": -1 }),
        json!({ "server": ["tokyo"] }),
    ] {
        assert!(config(table.clone()).is_err(), "{table}");
    }
}
