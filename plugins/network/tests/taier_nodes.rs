//! Shared-fixture validation and selection tests for `network.taier`'s node list reader.

use std::path::Path;

use plugin_network::taier::Operator;
use plugin_network::taier::OperatorChoice;
use plugin_network::taier::Selector;
use plugin_network::taier::nodes;
use plugin_network::taier::nodes::ListError;

#[test]
fn fixtures_agree_with_the_taier_list_script() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/taier-lists");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('.'))
        })
        .collect();
    entries.sort();
    assert_eq!(entries.len(), 21, "found {} fixtures", entries.len());

    for path in entries {
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(
            Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json")),
            "unexpected non-JSON fixture: {name}"
        );
        let bytes = std::fs::read(&path).expect("read fixture");
        let parsed = nodes::parse(&bytes);
        match name {
            n if n.starts_with("good-") => {
                assert!(parsed.is_ok(), "{name} should pass: {parsed:?}");
            }
            "bad-version-0.json" => assert_eq!(parsed, Err(ListError::Version(0))),
            "bad-version-2.json" => assert_eq!(parsed, Err(ListError::Newer(2))),
            "bad-empty-locations.json" => assert_eq!(parsed, Err(ListError::Empty)),
            "bad-duplicate-id.json" => {
                assert_eq!(parsed, Err(ListError::Duplicate("beijing".to_owned())));
            }
            "bad-id-all.json" | "bad-id-uppercase.json" => {
                assert_eq!(
                    parsed,
                    Err(ListError::Location(1, nodes::Rule::Id)),
                    "{name}"
                );
            }
            "bad-empty-province.json" => {
                assert_eq!(parsed, Err(ListError::Location(1, nodes::Rule::Province)));
            }
            "bad-control-city.json" | "bad-non-ascii-city.json" => {
                assert_eq!(
                    parsed,
                    Err(ListError::Location(1, nodes::Rule::City)),
                    "{name}"
                );
            }
            "bad-empty-nodes.json" => {
                assert_eq!(parsed, Err(ListError::Location(1, nodes::Rule::Nodes)));
            }
            "bad-operator.json" => {
                assert_eq!(parsed, Err(ListError::Location(1, nodes::Rule::Operator)));
            }
            "bad-ip-domain.json" | "bad-ip-ipv6.json" => {
                assert_eq!(
                    parsed,
                    Err(ListError::Location(1, nodes::Rule::Ip)),
                    "{name}"
                );
            }
            "bad-port-high.json" | "bad-port-low.json" => {
                assert_eq!(
                    parsed,
                    Err(ListError::Location(1, nodes::Rule::Port)),
                    "{name}"
                );
            }
            "bad-duplicate-endpoint.json" => {
                assert_eq!(
                    parsed,
                    Err(ListError::Location(1, nodes::Rule::DuplicateEndpoint))
                );
            }
            "bad-not-json.json" | "bad-not-object.json" => {
                assert!(
                    matches!(parsed, Err(ListError::Format(_))),
                    "{name}: {parsed:?}"
                );
            }
            other => panic!("unhandled taier fixture: {other}"),
        }
    }

    // Size, location-count, node-count, and 1-based entry position bounds.
    let mut oversized = br#"{"version":1,"locations":[]}"#.to_vec();
    oversized.resize(nodes::LIMIT + 1, b' ');
    assert_eq!(nodes::parse(&oversized), Err(ListError::TooLarge));

    let many_locs: Vec<String> = (0..=nodes::MOST_LOCATIONS)
        .map(|idx| {
            format!(
                "{{\"id\":\"loc-{idx}\",\"province\":\"P\",\"city\":\"C\",\
                 \"nodes\":[{{\"operator\":\"telecom\",\"ip\":\"198.51.100.1\",\"port\":65499}}]}}"
            )
        })
        .collect();
    let too_many_json = format!("{{\"version\":1,\"locations\":[{}]}}", many_locs.join(","));
    assert_eq!(
        nodes::parse(too_many_json.as_bytes()),
        Err(ListError::TooMany(nodes::MOST_LOCATIONS + 1))
    );

    let many_nodes: Vec<String> = (0..=nodes::MOST_NODES)
        .map(|idx| {
            let port = 10_000 + idx;
            format!("{{\"operator\":\"telecom\",\"ip\":\"198.51.100.1\",\"port\":{port}}}")
        })
        .collect();
    let too_many_nodes = format!(
        "{{\"version\":1,\"locations\":[{{\"id\":\"loc\",\"province\":\"P\",\"city\":\"C\",\
         \"nodes\":[{}]}}]}}",
        many_nodes.join(",")
    );
    assert_eq!(
        nodes::parse(too_many_nodes.as_bytes()),
        Err(ListError::Location(1, nodes::Rule::Nodes))
    );

    let second_bad = concat!(
        r#"{"version":1,"locations":["#,
        r#"{"id":"ok","province":"Beijing","city":"Beijing","#,
        r#""nodes":[{"operator":"telecom","ip":"198.51.100.1","port":65499}]},"#,
        r#"{"id":"bad","province":"Shanghai","city":"Shanghai","#,
        r#""nodes":[{"operator":"telecom","ip":"198.51.100.2","port":80}]}"#,
        r#"]}"#,
    )
    .as_bytes();
    assert_eq!(
        nodes::parse(second_bad),
        Err(ListError::Location(2, nodes::Rule::Port))
    );
}

#[test]
fn choose_orders_defaults_matches_cities_provinces_and_operators() {
    let raw = br#"{
        "version": 1,
        "locations": [
            {
                "id": "guangdong-guangzhou",
                "province": "Guangdong",
                "city": "Guangzhou",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.31", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.32", "port": 9443},
                    {"operator": "mobile", "ip": "198.51.100.33", "port": 9443}
                ]
            },
            {
                "id": "jiangsu-nanjing",
                "province": "Jiangsu",
                "city": "Nanjing",
                "nodes": [
                    {"operator": "unicom", "ip": "203.0.113.10", "port": 65499},
                    {"operator": "cernet", "ip": "203.0.113.11", "port": 65499}
                ]
            },
            {
                "id": "beijing",
                "province": "Beijing",
                "city": "Beijing",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.1", "port": 65499},
                    {"operator": "unicom", "ip": "198.51.100.2", "port": 65499}
                ]
            },
            {
                "id": "shanghai",
                "province": "Shanghai",
                "city": "Shanghai",
                "default": true,
                "nodes": [
                    {"operator": "mobile", "ip": "198.51.100.11", "port": 65499}
                ]
            }
        ]
    }"#;
    let locations = nodes::parse(raw).unwrap();

    // 1. Defaults run Beijing -> Shanghai -> Guangzhou across Telecom/Unicom/Mobile.
    let def = nodes::choose(&locations, &[], &[]);
    assert!(def.unknown.is_empty());
    let pairs: Vec<(&str, Operator)> = def
        .slots
        .iter()
        .map(|slot| (slot.city.as_str(), slot.operator))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("Beijing", Operator::Telecom),
            ("Beijing", Operator::Unicom),
            ("Shanghai", Operator::Mobile),
            ("Guangzhou", Operator::Telecom),
            ("Guangzhou", Operator::Mobile),
        ]
    );
    // Guangzhou Mobile preserves both candidate nodes in fallback order.
    assert_eq!(def.slots[4].candidates.len(), 2);

    // 2. Province/city filtering + unknown tracking + operator selection.
    let custom = nodes::choose(
        &locations,
        &["jiangsu".to_owned(), "atlantis".to_owned()],
        &[OperatorChoice::All],
    );
    assert_eq!(custom.unknown, vec!["atlantis".to_owned()]);
    assert_eq!(custom.slots.len(), 2);
    assert_eq!(custom.slots[0].operator, Operator::Unicom);
    assert_eq!(custom.slots[1].operator, Operator::Cernet);
}

#[test]
fn unified_selectors_resolve_municipalities_province_city_homonyms_and_deduplicate() {
    let raw = br#"{
        "version": 1,
        "locations": [
            {
                "id": "beijing",
                "province": "Beijing",
                "city": "Beijing",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.1", "port": 65499},
                    {"operator": "unicom", "ip": "198.51.100.2", "port": 65499},
                    {"operator": "cernet", "ip": "198.51.100.3", "port": 65499}
                ]
            },
            {
                "id": "guangdong-guangzhou",
                "province": "Guangdong",
                "city": "Guangzhou",
                "default": true,
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.11", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.12", "port": 9443}
                ]
            },
            {
                "id": "guangdong-shenzhen",
                "province": "Guangdong",
                "city": "Shenzhen",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.13", "port": 65499},
                    {"operator": "mobile", "ip": "198.51.100.14", "port": 9443}
                ]
            },
            {
                "id": "jilin",
                "province": "Jilin",
                "city": "Jilin",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.21", "port": 65499}
                ]
            },
            {
                "id": "jilin-changchun",
                "province": "Jilin",
                "city": "Changchun",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.22", "port": 65499}
                ]
            },
            {
                "id": "hainan-haikou",
                "province": "Hainan",
                "city": "Haikou",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.31", "port": 65499}
                ]
            },
            {
                "id": "qinghai-hainan",
                "province": "Qinghai",
                "city": "Hainan",
                "nodes": [
                    {"operator": "telecom", "ip": "198.51.100.32", "port": 65499}
                ]
            },
            {
                "id": "inner-mongolia-hohhot",
                "province": "Inner Mongolia",
                "city": "Hohhot",
                "nodes": [
                    {"operator": "unicom", "ip": "198.51.100.41", "port": 65499}
                ]
            }
        ]
    }"#;
    let locations = nodes::parse(raw).expect("valid fixture");

    // 1. Municipality + province/city + homonym disambiguation + order preservation + dedup.
    let selectors = [
        Selector::try_new("guangdong/shenzhen:cm").expect("shenzhen"),
        Selector::try_new("beijing:cu+edu").expect("beijing"),
        Selector::try_new("jilin/jilin:ct").expect("jilin city only"),
        Selector::try_new("hainan:ct").expect("hainan province only"),
        Selector::try_new("qinghai/hainan:ct").expect("qinghai hainan prefecture"),
        Selector::try_new("inner-mongolia/hohhot").expect("inherits fallback operators"),
        Selector::try_new("shenzhen:mobile").expect("duplicate slot ignored"),
    ];
    let chosen = nodes::select(&locations, &selectors, &[], &[OperatorChoice::Unicom]);
    assert!(chosen.unknown.is_empty());
    let pairs: Vec<(&str, Operator)> = chosen
        .slots
        .iter()
        .map(|slot| (slot.id.as_str(), slot.operator))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("guangdong-shenzhen", Operator::Mobile),
            ("beijing", Operator::Unicom),
            ("beijing", Operator::Cernet),
            ("jilin", Operator::Telecom),
            ("hainan-haikou", Operator::Telecom),
            ("qinghai-hainan", Operator::Telecom),
            ("inner-mongolia-hohhot", Operator::Unicom),
        ]
    );

    // 2. Bare `jilin` selects the entire Jilin province (both Jilin city and Changchun).
    let whole_jilin = nodes::select(
        &locations,
        &[Selector::try_new("jilin:ct").expect("jilin province")],
        &[],
        &[],
    );
    let jilin_ids: Vec<&str> = whole_jilin.slots.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(jilin_ids, vec!["jilin", "jilin-changchun"]);

    // 3. Invalid selector syntax is rejected eagerly.
    for bad in [
        "",
        "   ",
        ":ct",
        "beijing:",
        "beijing:ct+",
        "beijing:starlink",
        "beijing:ct:cu",
        "guangdong/",
        "/shenzhen",
        "guangdong/shenzhen/extra:cm",
    ] {
        assert!(
            Selector::try_new(bad).is_err(),
            "selector `{bad}` must be rejected"
        );
    }
}

#[test]
fn remote_taier_list_rejects_internal_and_loopback_ips_while_local_override_allows_loopback() {
    let make_list = |ip: &str| {
        format!(
            "{{\"version\":1,\"locations\":[{{\"id\":\"test\",\"province\":\"Beijing\",\
             \"city\":\"Beijing\",\"default\":true,\"nodes\":[{{\"operator\":\"telecom\",\
             \"ip\":\"{ip}\",\"port\":65499}}]}}]}}"
        )
        .into_bytes()
    };

    for forbidden in [
        "0.0.0.0",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.100.100.200",
        "224.0.0.1",
        "255.255.255.255",
    ] {
        let bytes = make_list(forbidden);
        assert_eq!(
            nodes::parse(&bytes),
            Err(ListError::Location(1, nodes::Rule::Ip)),
            "remote list must reject {forbidden}"
        );
        assert_eq!(
            nodes::parse_for_source(&bytes, true),
            Err(ListError::Location(1, nodes::Rule::Ip)),
            "local list must also reject {forbidden}"
        );
    }

    let loopback = make_list("127.0.0.1");
    assert_eq!(
        nodes::parse(&loopback),
        Err(ListError::Location(1, nodes::Rule::Ip)),
        "remote list must reject 127.0.0.1"
    );
    assert!(
        nodes::parse_for_source(&loopback, true).is_ok(),
        "local file override may use 127.0.0.1 for smoke tests"
    );
}
