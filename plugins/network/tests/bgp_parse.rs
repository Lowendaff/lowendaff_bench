//! Unit tests for `network.bgp`'s pure RIPEstat JSON parsers and ASN utilities.

use plugin_network::bgp::RpkiStatus;
use plugin_network::bgp::asns;
use plugin_network::bgp::parse;

#[test]
fn bgp_state_picks_most_specific_prefix_and_collapses_prepends() {
    let raw = include_bytes!("fixtures/bgp/bgp_state_sample.json");
    let parsed = parse::parse_bgp_state(raw).expect("sample bgp-state parses");

    assert!(parsed.announced);
    assert_eq!(parsed.prefix.as_deref(), Some("203.0.113.0/24"));
    assert_eq!(parsed.prefix_len, Some(24));
    assert_eq!(parsed.origin_asn, Some(64500));
    assert_eq!(parsed.total_peers, 5);
    assert_eq!(parsed.distinct_paths, 5);

    // Direct upstreams before AS64500: AS4809 (3 peers), AS2914 (1 peer), AS9929 (1 peer).
    assert_eq!(parsed.upstreams[0].asn, 4809);
    assert_eq!(parsed.upstreams[0].peers, 3);

    // Prepended path [1299, 9929, 9929, 64500] collapses to [1299, 9929, 64500] in `asns` while
    // keeping `asns_raw` intact for dashed span rendering.
    let prepended = parsed
        .paths
        .iter()
        .find(|p| p.asns == [1299, 9929, 64500])
        .expect("prepended path present");
    assert_eq!(prepended.asns_raw, vec![1299, 9929, 9929, 64500]);
    assert_eq!(prepended.peers, 1);

    // AS_SET `[64500]` at the end of `[2914, [64500]]` flattens cleanly.
    assert!(parsed.paths.iter().any(|p| p.asns == [2914, 64500]));
    assert_eq!(parsed.paths.iter().filter(|p| p.is_best).count(), 1);

    let zero_in_set = parse::parse_bgp_state(
        concat!(
            r#"{"status":"ok","data":{"bgp_state":[{"#,
            r#""target_prefix":"203.0.113.0/24","path":[2914,[0,64500]]}]}}"#
        )
        .as_bytes(),
    )
    .expect("AS_SET with leading 0 parses");
    assert_eq!(zero_in_set.origin_asn, Some(64500));
    assert_eq!(zero_in_set.paths[0].asns, [2914, 64500]);
}

#[test]
fn unannounced_prefix_returns_clean_empty_state() {
    let body = br#"{"status":"ok","data":{"resource":"198.51.100.9","bgp_state":[]}}"#;
    let parsed = parse::parse_bgp_state(body).expect("empty bgp_state parses");
    assert!(!parsed.announced);
    assert_eq!(parsed.total_peers, 0);
    assert!(parsed.paths.is_empty());
}

#[test]
fn rpki_and_holder_parsers_cover_expected_variants() {
    assert_eq!(
        parse::parse_rpki(br#"{"data":{"status":"valid"}}"#),
        Some(RpkiStatus::Valid)
    );
    assert_eq!(
        parse::parse_rpki(br#"{"data":{"status":"invalid_asn"}}"#),
        Some(RpkiStatus::Invalid)
    );
    assert_eq!(
        parse::parse_rpki(br#"{"data":{"status":"unknown"}}"#),
        Some(RpkiStatus::NotFound)
    );

    assert_eq!(
        parse::parse_as_overview(br#"{"data":{"holder":"CLOUDFLARENET - Cloudflare, Inc."}}"#)
            .as_deref(),
        Some("Cloudflare")
    );
    assert_eq!(asns::clean_holder("HETZNER-AS, DE"), "HETZNER-AS");
    assert_eq!(
        asns::trunc_to_tier1(&[65001, 3356, 174, 64500]),
        vec![174, 64500]
    );
    assert_eq!(
        asns::trunc_to_tier1(&[65001, 3356, 1299, 6939]),
        vec![1299, 6939]
    );
}

#[test]
fn retains_rare_multi_layer_upstreams_when_primary_tier1_has_over_128_collector_peers() {
    let mut routes = Vec::new();
    // 150 distinct collector-side peers behind AS6939 (exceeding MAX_PATHS = 128 on their own).
    for peer in 64700..64850 {
        routes.push(format!(
            "{{\"target_prefix\":\"2001:db8:f57d::/40\",\"path\":[{peer},6939,64500]}}"
        ));
    }
    routes.push("{\"target_prefix\":\"2001:db8:f57d::/40\",\"path\":[6461,1299,64500]}".to_owned());
    routes.push(
        "{\"target_prefix\":\"2001:db8:f57d::/40\",\
         \"path\":[3257,64520,64520,64500,64500,64500,64500]}"
            .to_owned(),
    );
    routes.push(
        "{\"target_prefix\":\"2001:db8:f57d::/40\",\
         \"path\":[65200,174,64521,64520,64500,64500,64500,64500]}"
            .to_owned(),
    );
    let joined = routes.join(",");
    let body = format!(
        "{{\"status\":\"ok\",\"data\":{{\"resource\":\"2001:db8:f57d::/40\",\
         \"bgp_state\":[{joined}]}}}}"
    );

    let parsed = parse::parse_bgp_state(body.as_bytes()).expect("parses 153 routes");
    assert_eq!(parsed.paths.len(), parse::MAX_PATHS);
    assert!(
        parsed.paths.iter().any(|p| p.asns == [3257, 64520, 64500]),
        "retains 2-hop backup upstream path despite >128 AS6939 collector variants"
    );
    assert!(
        parsed
            .paths
            .iter()
            .any(|p| p.asns == [65200, 174, 64521, 64520, 64500]),
        "retains 3-hop multi-layer backup chain despite >128 AS6939 collector variants"
    );
}
