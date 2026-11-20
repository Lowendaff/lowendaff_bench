//! Descriptor, catalog invariants, group/service selection and `base_url` validation tests for
//! `media.unlock`.

use std::collections::BTreeSet;

use bench_api::Net;
use bench_api::Plugin;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::HostProfile;
use plugin_media::unlock;
use plugin_media::unlock::Choice;
use plugin_media::unlock::Failure;
use plugin_media::unlock::Group;
use plugin_media::unlock::Unlock;
use plugin_media::unlock::services;
use plugin_media::unlock::services::CATALOG;

#[test]
fn descriptor_applies_to_every_platform_needs_any_network_and_uses_no_external_tools() {
    let desc = Unlock::DESCRIPTOR;
    assert_eq!(desc.id, "media.unlock");
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
            "media.unlock must apply to {platform:?}"
        );
    }
}

#[test]
fn catalog_ids_are_unique_valid_and_ordered_by_group() {
    assert_eq!(CATALOG.len(), 68);
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut last_group = Group::Global;

    for spec in CATALOG {
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
        assert!(
            spec.group >= last_group,
            "{} is out of group order",
            spec.id
        );
        last_group = spec.group;
    }

    let count = |group| CATALOG.iter().filter(|spec| spec.group == group).count();
    assert_eq!(count(Group::Global), 12);
    assert_eq!(count(Group::Asia), 24);
    assert_eq!(count(Group::Europe), 14);
    assert_eq!(count(Group::Americas), 10);
    assert_eq!(count(Group::Oceania), 8);
}

#[test]
fn choose_filters_by_groups_or_lets_services_win_and_tracks_unknown_ids() {
    let all = services::choose(&[], &[]);
    assert_eq!(all.services.len(), CATALOG.len());
    assert!(all.unknown.is_empty());

    let oceania = services::choose(&[Choice::Oceania], &[]);
    assert_eq!(oceania.services.len(), 8);
    assert!(oceania.services.iter().all(|s| s.group == Group::Oceania));

    let every = services::choose(&[Choice::Oceania, Choice::All], &[]);
    assert_eq!(every.services.len(), CATALOG.len());

    let picked = services::choose(
        &[Choice::Oceania],
        &[
            "Netflix".to_owned(),
            "missing-one".to_owned(),
            "bahamut".to_owned(),
            "MISSING-ONE".to_owned(),
        ],
    );
    let ids: Vec<&str> = picked.services.iter().map(|s| s.id).collect();
    assert_eq!(ids, ["netflix", "bahamut"], "services win over groups");
    assert_eq!(picked.unknown, ["missing-one"]);
}

#[test]
fn validate_base_url_accepts_https_and_loopback_http_only() {
    assert_eq!(
        unlock::validate_base_url("https://mock.example/api/"),
        Ok("https://mock.example/api".to_owned())
    );
    assert_eq!(
        unlock::validate_base_url("http://127.0.0.1:18080/"),
        Ok("http://127.0.0.1:18080".to_owned())
    );
    assert_eq!(
        unlock::validate_base_url("http://[::1]:18080"),
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
            unlock::validate_base_url(bad),
            Err(Failure::InvalidBaseUrl(bad.to_owned())),
            "{bad} must be rejected"
        );
    }
}
