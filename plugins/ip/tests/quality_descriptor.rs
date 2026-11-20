//! Descriptor, provider/DNSBL/mail roster invariants, and endpoint validation tests for
//! `ip.quality`.

use std::collections::BTreeSet;
use std::net::IpAddr;

use bench_api::Net;
use bench_api::Plugin;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::HostProfile;
use plugin_ip::quality;
use plugin_ip::quality::Failure;
use plugin_ip::quality::Quality;
use plugin_ip::quality::dnsbl;
use plugin_ip::quality::mail;
use plugin_ip::quality::providers;

#[test]
fn descriptor_applies_to_every_platform_needs_any_network_and_uses_no_external_tools() {
    let desc = Quality::DESCRIPTOR;
    assert_eq!(desc.id, "ip.quality");
    assert!(desc.requires.tools.is_empty());
    assert_eq!(desc.requires.net, Net::Any);
    assert!(!desc.timeout.is_zero());

    let def = Quality::default();
    assert!(def.dnsbl && def.mail);

    for platform in [
        Platform::Dedicated,
        Platform::Vm(Hypervisor::Kvm),
        Platform::Container(ContainerKind::Docker),
    ] {
        let host = HostProfile {
            platform,
            ..HostProfile::default()
        };
        assert!(desc.requires.applies_to(&host));
    }
}

#[test]
fn provider_dnsbl_and_mail_rosters_have_unique_ids_and_names() {
    assert_eq!(providers::ALL.len(), 9);
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for spec in providers::ALL {
        assert!(!spec.id.is_empty());
        assert!(ids.insert(spec.id), "duplicate provider id {}", spec.id);
        assert!(
            names.insert(spec.name),
            "duplicate provider name {}",
            spec.name
        );
    }

    let v4: IpAddr = "198.51.100.7".parse().expect("v4");
    let v6: IpAddr = "2001:db8::7".parse().expect("v6");
    assert_eq!(dnsbl::zones_for(v4).len(), 24);
    assert_eq!(dnsbl::zones_for(v6).len(), 9);

    let mut zones = BTreeSet::new();
    for z in dnsbl::ZONES {
        assert!(z.ipv4 || z.ipv6);
        assert!(zones.insert(z.zone), "duplicate zone {}", z.zone);
    }

    assert_eq!(mail::PROVIDERS.len(), 8);
    let mut mail_ids = BTreeSet::new();
    for m in mail::PROVIDERS {
        assert!(mail_ids.insert(m.id));
        assert!(m.mx.contains('.'));
    }
}

#[test]
fn endpoint_validators_accept_https_and_loopback_only() {
    assert_eq!(
        quality::validate_base_url("https://api.example/v1/"),
        Ok("https://api.example/v1".to_owned())
    );
    assert_eq!(
        quality::validate_base_url("http://127.0.0.1:18081/"),
        Ok("http://127.0.0.1:18081".to_owned())
    );
    assert!(quality::validate_base_url("http://example.com").is_err());

    assert_eq!(
        quality::validate_loopback_socket("127.0.0.1:18053"),
        Ok("127.0.0.1:18053".parse().expect("sock"))
    );
    assert_eq!(
        quality::validate_loopback_socket("[::1]:18025"),
        Ok("[::1]:18025".parse().expect("sock"))
    );
    for bad in ["8.8.8.8:53", "127.0.0.1:0", "not-a-socket"] {
        assert_eq!(
            quality::validate_loopback_socket(bad),
            Err(Failure::InvalidEndpoint(bad.to_owned()))
        );
    }
}
