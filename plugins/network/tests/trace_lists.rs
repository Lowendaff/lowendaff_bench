//! The fixtures of the item's two lists, in `tests/fixtures/target-lists` and
//! `tests/fixtures/exchange-lists`, which the program's readers and the lists' checking scripts
//! both read: a list named `good-*` follows every rule and one named `bad-*` breaks one, so that
//! the readers agree on each of them; and the rule that each of them breaks.

use std::net::IpAddr;
use std::path::Path;

use plugin_network::trace::exchanges;
use plugin_network::trace::lists;
use plugin_network::trace::lists::ListError;
use plugin_network::trace::lists::Rule;
use plugin_network::trace::targets;
use plugin_network::trace::targets::Group;

fn read(directory: &str, name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory)
        .join(format!("{name}.json"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Each fixture of `directory`, by its name without `.json`.
fn names(directory: &str) -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the fixtures' directory reads")
        .map(|entry| {
            let path = entry.expect("a directory entry").path();
            let name = path.file_stem().expect("a file name").to_string_lossy();
            name.into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Checks that each fixture of `directory` gets the verdict that its name gives, from `parse`.
fn verdicts<T: std::fmt::Debug, E: std::fmt::Debug>(
    directory: &str,
    parse: impl Fn(&[u8]) -> Result<T, E>,
    least: usize,
) {
    let names = names(directory);
    for name in &names {
        let verdict = parse(&read(directory, name));
        if name.starts_with("good-") {
            assert!(verdict.is_ok(), "{name}: {verdict:?}");
        } else if name.starts_with("bad-") {
            assert!(verdict.is_err(), "{name}: {verdict:?}");
        } else {
            panic!("{name} names no verdict");
        }
    }
    assert!(names.len() >= least, "only {} fixtures", names.len());
}

#[test]
fn each_target_list_gets_the_verdict_that_its_name_gives() {
    verdicts("target-lists", targets::parse, 52);
}

#[test]
fn each_exchange_list_gets_the_verdict_that_its_name_gives() {
    verdicts("exchange-lists", exchanges::parse, 40);
}

#[test]
fn a_target_list_breaks_the_rule_that_its_name_says() {
    let entry = |rule| ListError::Entry(1, rule);
    let cases = [
        ("bad-id-capital", entry(Rule::Id)),
        ("bad-id-41-characters", entry(Rule::Id)),
        ("bad-id-empty", entry(Rule::Id)),
        ("bad-id-final-newline", entry(Rule::Id)),
        ("bad-id-youtube", entry(Rule::Id)),
        ("bad-id-netflix", entry(Rule::Id)),
        ("bad-id-twice", ListError::Duplicate("pek-4809".to_owned())),
        ("bad-group-services", entry(Rule::Group)),
        ("bad-group-word", entry(Rule::Group)),
        ("bad-name-empty", entry(Rule::Name)),
        ("bad-name-41-characters", entry(Rule::Name)),
        ("bad-name-c1-control", entry(Rule::Name)),
        ("bad-name-newline", entry(Rule::Name)),
        ("bad-name-escape", entry(Rule::Name)),
        ("bad-asn-zero", entry(Rule::Asn)),
        ("bad-asn-too-large", entry(Rule::Asn)),
        ("bad-ipv4-254-characters", entry(Rule::Ipv4)),
        ("bad-ipv4-address-final-newline", entry(Rule::Ipv4)),
        ("bad-ipv4-final-newline", entry(Rule::Ipv4)),
        ("bad-ipv4-ipv6-address", entry(Rule::Ipv4)),
        ("bad-ipv4-label-64-characters", entry(Rule::Ipv4)),
        ("bad-ipv4-option", entry(Rule::Ipv4)),
        ("bad-ipv4-space", entry(Rule::Ipv4)),
        ("bad-ipv4-trailing-dot", entry(Rule::Ipv4)),
        ("bad-ipv6-address-final-newline", entry(Rule::Ipv6)),
        ("bad-ipv6-ipv4-address", entry(Rule::Ipv6)),
        ("bad-ipv6-group-of-five", entry(Rule::Ipv6)),
        ("bad-ipv6-ipv4-part-before-cut", entry(Rule::Ipv6)),
        ("bad-ipv6-label-dash", entry(Rule::Ipv6)),
        ("bad-ipv6-empty", entry(Rule::Ipv6)),
        ("bad-version-0", ListError::Version(0)),
        ("bad-version-2", ListError::Newer(2)),
        ("bad-too-many-targets", ListError::TooMany(201)),
    ];
    for (name, error) in &cases {
        assert_eq!(
            targets::parse(&read("target-lists", name)),
            Err(error.clone()),
            "{name}"
        );
    }
    // What is not JSON of the format, the parser names.
    let format_cases = [
        "bad-not-json",
        "bad-not-an-object",
        "bad-version-string",
        "bad-targets-missing",
        "bad-id-missing",
        "bad-group-missing",
        "bad-asn-string",
        "bad-asn-negative",
        "bad-asn-fraction",
        "bad-asn-missing",
        "bad-ipv4-missing",
        "bad-default-word",
    ];
    for name in format_cases {
        assert!(
            matches!(
                targets::parse(&read("target-lists", name)),
                Err(ListError::Format(_))
            ),
            "{name}"
        );
    }
    let all_bad: Vec<String> = names("target-lists")
        .into_iter()
        .filter(|name| name.starts_with("bad-"))
        .collect();
    for bad in &all_bad {
        let covered =
            cases.iter().any(|(n, _)| *n == bad.as_str()) || format_cases.contains(&bad.as_str());
        assert!(covered, "untested bad target-list fixture: {bad}");
    }
}

#[test]
fn an_exchange_list_breaks_the_rule_that_its_name_says() {
    let entry = |rule| ListError::Entry(1, rule);
    let cases = [
        ("bad-address-word", entry(Rule::Address)),
        ("bad-address-network", entry(Rule::Address)),
        ("bad-address-leading-zero", entry(Rule::Address)),
        ("bad-address-capitals", entry(Rule::Address)),
        ("bad-address-leading-zeros", entry(Rule::Address)),
        ("bad-address-not-cut-short", entry(Rule::Address)),
        ("bad-address-one-group-cut", entry(Rule::Address)),
        ("bad-address-second-run-cut", entry(Rule::Address)),
        ("bad-address-ipv4-part", entry(Rule::Address)),
        ("bad-address-mapped-in-hex", entry(Rule::Address)),
        ("bad-address-cut-for-nothing", entry(Rule::Address)),
        ("bad-address-last-group-cut", entry(Rule::Address)),
        ("bad-address-three-groups", entry(Rule::Address)),
        ("bad-address-nine-groups", entry(Rule::Address)),
        ("bad-address-five-octets", entry(Rule::Address)),
        ("bad-address-group-of-five", entry(Rule::Address)),
        ("bad-address-ipv4-final-newline", entry(Rule::Address)),
        ("bad-address-ipv6-final-newline", entry(Rule::Address)),
        ("bad-address-octet-256", entry(Rule::Address)),
        ("bad-address-three-octets", entry(Rule::Address)),
        (
            "bad-address-twice",
            ListError::Duplicate("206.223.116.133".to_owned()),
        ),
        ("bad-exchange-empty", entry(Rule::Exchange)),
        ("bad-exchange-61-characters", entry(Rule::Exchange)),
        ("bad-exchange-c1-control", entry(Rule::Exchange)),
        ("bad-exchange-newline", entry(Rule::Exchange)),
        ("bad-version-0", ListError::Version(0)),
        ("bad-version-2", ListError::Newer(2)),
        ("bad-too-many-ports", ListError::TooMany(1001)),
    ];
    for (name, error) in &cases {
        assert_eq!(
            exchanges::parse(&read("exchange-lists", name)),
            Err(error.clone()),
            "{name}"
        );
    }
    let format_cases = [
        "bad-not-json",
        "bad-not-an-object",
        "bad-ports-missing",
        "bad-address-missing",
        "bad-exchange-missing",
        "bad-exchange-number",
    ];
    for name in format_cases {
        assert!(
            matches!(
                exchanges::parse(&read("exchange-lists", name)),
                Err(ListError::Format(_))
            ),
            "{name}"
        );
    }
    let all_bad: Vec<String> = names("exchange-lists")
        .into_iter()
        .filter(|name| name.starts_with("bad-"))
        .collect();
    for bad in &all_bad {
        let covered =
            cases.iter().any(|(n, _)| *n == bad.as_str()) || format_cases.contains(&bad.as_str());
        assert!(covered, "untested bad exchange-list fixture: {bad}");
    }
}

#[test]
fn a_target_list_reads_as_written() {
    let read = targets::parse(&read("target-lists", "good-every-shape")).expect("a good list");
    let shapes: Vec<_> = read
        .iter()
        .map(|target| {
            (
                target.id.as_str(),
                target.group,
                target.asn,
                target.ipv4.as_str(),
                target.ipv6.as_deref(),
                target.default,
            )
        })
        .collect();
    assert_eq!(
        shapes,
        [
            (
                "pek-4809",
                Group::China,
                4809,
                "ipv4.pek-4809.endpoint.nxtrace.org",
                Some("ipv6.pek-4809.endpoint.nxtrace.org"),
                true
            ),
            (
                "gcp-sin",
                Group::International,
                396_982,
                "35.187.238.97",
                None,
                false
            ),
            (
                "v6-literal",
                Group::International,
                4_294_967_295,
                "192.0.2.1",
                Some("2001:db8::1"),
                false
            ),
            ("a", Group::China, 1, "a.example", None, true),
        ]
    );
    assert_eq!(read[3].name, "N".repeat(40));
}

#[test]
fn an_exchange_list_names_each_port_by_its_address() {
    let read = exchanges::parse(&read("exchange-lists", "good-every-shape")).expect("good");
    let address = |text: &str| text.parse::<IpAddr>().expect("an address");
    assert_eq!(read.len(), 7);
    assert_eq!(
        read.get(&address("206.223.116.133")).map(String::as_str),
        Some("Equinix San Jose")
    );
    assert_eq!(
        read.get(&address("2001:504:0:1::2906:1"))
            .map(String::as_str),
        Some("Equinix Ashburn")
    );
}

#[test]
fn a_list_larger_than_64_kib_is_refused_before_it_is_read() {
    let mut large = br#"{"version": 1, "targets": []}"#.to_vec();
    large.resize(lists::LIMIT + 1, b' ');
    assert_eq!(targets::parse(&large), Err(ListError::TooLarge));
    assert_eq!(exchanges::parse(&large), Err(ListError::TooLarge));
    large.truncate(lists::LIMIT);
    assert!(targets::parse(&large).is_ok(), "64 KiB is allowed");
}

#[test]
fn a_dns_name_never_starts_with_a_dash_nor_has_an_empty_or_long_label() {
    for good in ["example.net", "a", "1.2.3.4", "2001:db8::1", "a-b.c"] {
        assert!(lists::host(good), "{good}");
    }
    let long = format!("{}.net", "a".repeat(64));
    let longest = ["a".repeat(63).as_str(); 4].join(".");
    for bad in [
        "", "-n", "a-.net", "a..net", "a.net.", "a b", "a_b.net", &long, &longest,
    ] {
        assert!(!lists::host(bad), "{bad}");
    }
    assert!(lists::host(&longest[..253]), "253 characters at most");
    assert!(
        !lists::host(&longest[..254]),
        "254 characters, one too many"
    );
}

#[test]
fn remote_safe_host_and_target_source_guard_loopback_and_private_v4_and_v6() {
    for forbidden in [
        "localhost",
        "LOCALHOST",
        "sub.localhost",
        "ipv6.LocalHost",
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "::1",
        "fe80::1",
        "fc00::1",
    ] {
        assert!(
            !lists::is_remote_safe_host(forbidden),
            "{forbidden} must be rejected"
        );
    }
    for allowed in [
        "notlocalhost",
        "my-localhost.example",
        "192.0.2.1",
        "2001:db8::1",
    ] {
        assert!(
            lists::is_remote_safe_host(allowed),
            "{allowed} must be allowed"
        );
    }

    for bad_v6 in ["::1", "fe80::1", "fc00::1", "ipv6.localhost"] {
        let raw = format!(
            "{{\"version\":1,\"targets\":[{{\"id\":\"t\",\"group\":\"international\",\
             \"name\":\"T\",\"asn\":64496,\"ipv4\":\"192.0.2.1\",\"ipv6\":\"{bad_v6}\"}}]}}"
        );
        assert_eq!(
            targets::parse_for_source(raw.as_bytes(), false),
            Err(ListError::Entry(1, Rule::Ipv6)),
            "remote list must reject IPv6 {bad_v6}"
        );
        assert!(
            targets::parse_for_source(raw.as_bytes(), true).is_ok(),
            "local override must allow {bad_v6}"
        );
    }
}
