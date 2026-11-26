//! Shared fixture and selector tests for `network.tcping` (`tests/fixtures/tcping-lists/`).

use std::path::Path;

use plugin_network::tcping::Group;
use plugin_network::tcping::GroupChoice;
use plugin_network::tcping::targets;
use plugin_network::tcping::targets::ListError;
use plugin_network::tcping::targets::Rule;

fn read_fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tcping-lists")
        .join(format!("{name}.json"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

#[test]
fn every_shared_tcping_fixture_matches_expected_verdict() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tcping-lists");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("tcping-lists fixture dir reads")
        .map(|entry| {
            entry
                .expect("dir entry")
                .path()
                .file_stem()
                .expect("file stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names.len(), 36, "expected all 36 shared fixtures");

    for name in &names {
        let raw = read_fixture(name);
        let remote_verdict = targets::parse_for_source(&raw, false);
        if name.starts_with("good-") {
            assert!(remote_verdict.is_ok(), "{name}: {remote_verdict:?}");
        } else if name.starts_with("bad-") {
            assert!(remote_verdict.is_err(), "{name}: {remote_verdict:?}");
        } else {
            panic!("unexpected fixture prefix: {name}");
        }
    }
}

#[test]
fn broken_fixtures_report_specific_rules() {
    let entry = |rule| ListError::Entry(1, rule);
    let cases = [
        ("bad-id-empty", entry(Rule::Id)),
        ("bad-id-uppercase", entry(Rule::Id)),
        ("bad-id-41-chars", entry(Rule::Id)),
        ("bad-id-all", entry(Rule::Id)),
        ("bad-group-unknown", entry(Rule::Group)),
        ("bad-region-empty", entry(Rule::Region)),
        ("bad-region-non-ascii", entry(Rule::Region)),
        ("bad-city-control", entry(Rule::City)),
        ("bad-name-41-chars", entry(Rule::Name)),
        ("bad-operator-unknown", entry(Rule::Operator)),
        ("bad-asn-zero", entry(Rule::Asn)),
        ("bad-asn-too-large", entry(Rule::Asn)),
        ("bad-asn-fraction", entry(Rule::Asn)),
        ("bad-default-string", entry(Rule::Default)),
        ("bad-no-endpoints", entry(Rule::Endpoints)),
        ("bad-ipv4-empty", entry(Rule::Ipv4)),
        ("bad-ipv4-three", entry(Rule::Ipv4)),
        ("bad-ipv4-duplicate", entry(Rule::Ipv4)),
        ("bad-ipv4-has-ipv6", entry(Rule::Ipv4)),
        ("bad-ipv6-has-ipv4", entry(Rule::Ipv6)),
        ("bad-host-option", entry(Rule::Ipv4)),
        ("bad-port-zero", entry(Rule::Ipv4)),
        ("bad-port-high", entry(Rule::Ipv4)),
        ("bad-version-0", ListError::Version(0)),
        ("bad-version-2", ListError::Version(2)),
        ("bad-empty-targets", ListError::Count(0)),
        ("bad-too-many-targets", ListError::Count(301)),
        (
            "bad-duplicate-id",
            ListError::Duplicate("cn-bj-ct".to_owned()),
        ),
    ];
    for (name, expected) in cases {
        let got = targets::parse_for_source(&read_fixture(name), false)
            .expect_err("broken fixture must fail");
        assert_eq!(got, expected, "fixture {name}");
    }
}

#[test]
fn choose_filters_by_group_and_default_flag_in_canonical_order() {
    let parsed =
        targets::parse_for_source(&read_fixture("good-every-shape"), false).expect("good fixture");
    assert_eq!(parsed.len(), 3);

    // Default run picks default=true targets only (cn-bj-ct; omitted default -> false).
    let defaults = targets::choose(&parsed, &[], false);
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].id, "cn-bj-ct");

    // --tcping-all picks all 3 targets in (china, china_premium, global) order.
    let all = targets::choose(&parsed, &[], true);
    let all_ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(all_ids, ["cn-bj-ct", "cn-sh-cuii-9929", "global-jp-tyo"]);

    // --tcping-groups china-premium --tcping-all picks only ChinaPremium.
    let prem = targets::choose(&parsed, &[GroupChoice::ChinaPremium], true);
    assert_eq!(prem.len(), 1);
    assert_eq!(prem[0].group, Group::ChinaPremium);
    assert_eq!(prem[0].ipv4.len(), 2);
    assert_eq!(prem[0].ipv6.len(), 2);
}
