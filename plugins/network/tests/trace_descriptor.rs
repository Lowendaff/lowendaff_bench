//! The item's descriptor and its configuration: what it needs, the files that it asks for, each
//! cache server's answer over each family as an optional file, and the configuration's own rules.

use std::path::PathBuf;
use std::time::Duration;

use bench_api::FileSource;
use bench_api::FileSpec;
use bench_api::IpFamily;
use bench_api::Net;
use bench_api::Plugin;
use bench_tool::NEXTTRACE;
use plugin_network::trace;
use plugin_network::trace::Choice;
use plugin_network::trace::Trace;
use serde_json::json;

fn url(url: &str) -> FileSource {
    FileSource::Url(url.to_owned())
}

/// An optional file of `family`.
fn lookup(name: &'static str, source: FileSource, family: IpFamily) -> FileSpec {
    FileSpec {
        family: Some(family),
        optional: true,
        ..FileSpec::new(name, source)
    }
}

/// The files of the default: the target list, the list of exchange ports, and each cache
/// server's answer over each family.
fn default_files() -> Vec<FileSpec> {
    vec![
        FileSpec::new("targets", url(trace::TARGETS_URL)),
        FileSpec {
            optional: true,
            ..FileSpec::new("exchanges", url(trace::EXCHANGES_URL))
        },
        lookup("youtube-ipv4", url(trace::YOUTUBE_URL), IpFamily::Ipv4),
        lookup("youtube-ipv6", url(trace::YOUTUBE_URL), IpFamily::Ipv6),
        lookup("netflix-ipv4", url(trace::NETFLIX_URL), IpFamily::Ipv4),
        lookup("netflix-ipv6", url(trace::NETFLIX_URL), IpFamily::Ipv6),
    ]
}

fn configured(table: serde_json::Value) -> Trace {
    serde_json::from_value(table).expect("a table of the item")
}

#[test]
fn the_item_needs_nexttrace_and_either_family_within_an_hour() {
    let descriptor = Trace::DESCRIPTOR;
    assert_eq!(descriptor.id, "network.trace");
    assert_eq!(descriptor.requires.tools, [NEXTTRACE]);
    assert_eq!(descriptor.requires.net, Net::Any);
    assert_eq!(descriptor.timeout, Duration::from_secs(3600));
    const { assert!(NEXTTRACE.raw_sockets, "NextTrace gets CAP_NET_RAW") };
}

#[test]
fn by_default_the_item_asks_for_its_lists_and_each_cache_servers_answer_over_each_family() {
    assert_eq!(Trace::default().files(), default_files());
    assert_eq!(configured(json!({})).files(), default_files());
    assert_eq!(
        configured(json!({ "groups": ["all"] })).files(),
        default_files()
    );
}

#[test]
fn a_cache_server_that_is_not_chosen_has_no_answer_asked_for() {
    let names = |table| -> Vec<&'static str> {
        configured(table)
            .files()
            .iter()
            .map(|file| file.name)
            .collect()
    };
    assert_eq!(
        names(json!({ "groups": ["china"] })),
        ["targets", "exchanges"]
    );
    assert_eq!(
        names(json!({ "targets": ["youtube", "pek-4809"] })),
        ["targets", "exchanges", "youtube-ipv4", "youtube-ipv6"]
    );
    assert_eq!(
        names(json!({ "groups": ["china"], "targets": ["netflix"] })),
        ["targets", "exchanges", "netflix-ipv4", "netflix-ipv6"],
        "ids win over groups"
    );
}

#[test]
fn each_file_can_come_from_a_path_or_another_https_address() {
    let item = configured(json!({
        "list": "/srv/targets.json",
        "exchanges": "https://lists.example/ix.json",
        "youtube_api": "/srv/mapping.txt",
        "netflix_api": "https://api.example/netflix",
    }));
    let sources: Vec<(&str, FileSource)> = item
        .files()
        .into_iter()
        .map(|file| (file.name, file.source))
        .collect();
    let path = |path: &str| FileSource::Path(PathBuf::from(path));
    assert_eq!(
        sources,
        [
            ("targets", path("/srv/targets.json")),
            ("exchanges", url("https://lists.example/ix.json")),
            ("youtube-ipv4", path("/srv/mapping.txt")),
            ("youtube-ipv6", path("/srv/mapping.txt")),
            ("netflix-ipv4", url("https://api.example/netflix")),
            ("netflix-ipv6", url("https://api.example/netflix")),
        ]
    );
}

#[test]
fn the_configuration_knows_its_fields_and_its_groups_and_looks_hops_up_by_default() {
    assert!(Trace::default().geo);
    assert!(Trace::default().tcp);
    assert_eq!(Trace::default().large_packets, trace::DEFAULT_LARGE_PACKETS);
    assert!(configured(json!({})).geo);
    assert!(configured(json!({})).tcp);
    assert_eq!(configured(json!({})).large_packets, 1024);
    assert!(!configured(json!({ "geo": false })).geo);
    assert!(!configured(json!({ "tcp": false })).tcp);
    assert_eq!(configured(json!({ "large_packets": 64 })).large_packets, 64);
    assert_eq!(
        configured(json!({ "large_packets": 1500 })).large_packets,
        1500
    );
    assert_eq!(
        configured(json!({ "groups": ["services", "china", "international", "all"] })).groups,
        [
            Choice::Services,
            Choice::China,
            Choice::International,
            Choice::All
        ]
    );
    for wrong in [
        json!({ "group": ["china"] }),
        json!({ "groups": ["asia"] }),
        json!({ "groups": ["China"] }),
        json!({ "groups": "china" }),
        json!({ "geo": "no" }),
        json!({ "tcp": "yes" }),
        json!({ "large_packets": 63 }),
        json!({ "large_packets": 1501 }),
        json!({ "targets": "pek-4809" }),
    ] {
        assert!(
            serde_json::from_value::<Trace>(wrong.clone()).is_err(),
            "{wrong}"
        );
    }
}
