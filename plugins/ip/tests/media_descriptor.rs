//! Descriptor, continent roster invariants, continent orchestration planning, and `base_url`
//! validation tests for `ip.media`.

use std::collections::BTreeSet;

use bench_api::Net;
use bench_api::Plugin;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::HostProfile;
use plugin_ip::media;
use plugin_ip::media::Choice;
use plugin_ip::media::Failure;
use plugin_ip::media::Group;
use plugin_ip::media::Media;
use plugin_ip::media::measure;
use plugin_ip::media::platforms;

#[test]
fn descriptor_applies_to_every_platform_needs_any_network_and_uses_no_external_tools() {
    let desc = Media::DESCRIPTOR;
    assert_eq!(desc.id, "ip.media");
    assert!(desc.requires.tools.is_empty(), "uses direct Rust probes");
    assert_eq!(desc.requires.net, Net::Any);
    assert!(!desc.timeout.is_zero());

    for platform in [
        Platform::Dedicated,
        Platform::Vm(Hypervisor::Kvm),
        Platform::Container(ContainerKind::Docker),
    ] {
        let host = HostProfile {
            platform,
            ..HostProfile::default()
        };
        assert!(
            desc.requires.applies_to(&host),
            "ip.media must apply to {platform:?}"
        );
    }
}

#[test]
fn continent_rosters_have_unique_valid_ids_and_expected_counts() {
    assert_eq!(platforms::GLOBAL.len(), 12);
    assert_eq!(platforms::ASIA.len(), 24);
    assert_eq!(platforms::EUROPE.len(), 14);
    assert_eq!(platforms::AMERICAS.len(), 10);
    assert_eq!(platforms::OCEANIA.len(), 8);

    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut total = 0;

    for group in Group::ALL {
        for spec in platforms::in_group(group) {
            total += 1;
            assert!(!spec.id.is_empty());
            assert!(
                spec.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "invalid id: {}",
                spec.id
            );
            assert!(ids.insert(spec.id), "duplicate id: {}", spec.id);
            assert!(names.insert(spec.name), "duplicate name: {}", spec.name);
        }
    }

    assert_eq!(total, 68);
}

#[test]
fn plan_orchestrates_by_continent_lets_services_win_and_tracks_unknown_ids() {
    let all = measure::plan(&[], &[]);
    assert_eq!(all.scheduled.len(), 68);
    assert!(all.unknown.is_empty());

    let mut last_group = Group::Global;
    for entry in &all.scheduled {
        assert!(
            entry.group >= last_group,
            "{} scheduled out of continent order",
            entry.platform.id
        );
        last_group = entry.group;
    }

    let oceania = measure::plan(&[Choice::Oceania], &[]);
    assert_eq!(oceania.scheduled.len(), 8);
    assert!(oceania.scheduled.iter().all(|s| s.group == Group::Oceania));

    let every = measure::plan(&[Choice::Oceania, Choice::All], &[]);
    assert_eq!(every.scheduled.len(), 68);

    let picked = measure::plan(
        &[Choice::Oceania],
        &[
            "bahamut".to_owned(),
            "missing-one".to_owned(),
            "Netflix".to_owned(),
            "MISSING-ONE".to_owned(),
        ],
    );
    let ids: Vec<&str> = picked.scheduled.iter().map(|s| s.platform.id).collect();
    assert_eq!(
        ids,
        ["netflix", "bahamut"],
        "services win over groups and stay ordered by continent"
    );
    assert_eq!(picked.unknown, ["missing-one"]);
}

#[test]
fn validate_base_url_accepts_https_and_loopback_http_only() {
    assert_eq!(
        media::validate_base_url("https://mock.example/api/"),
        Ok("https://mock.example/api".to_owned())
    );
    assert_eq!(
        media::validate_base_url("http://127.0.0.1:18080/"),
        Ok("http://127.0.0.1:18080".to_owned())
    );
    assert_eq!(
        media::validate_base_url("http://[::1]:18080"),
        Ok("http://[::1]:18080".to_owned())
    );

    for bad in [
        "",
        "https://",
        "http://example.com",
        "http://127.0.0.1.evil.example:8080",
        "http://192.168.1.1:8080",
        "ftp://127.0.0.1",
    ] {
        assert_eq!(
            media::validate_base_url(bad),
            Err(Failure::InvalidBaseUrl(bad.to_owned())),
            "{bad} must be rejected"
        );
    }
}
