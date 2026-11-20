//! Shared-fixture validation and selection tests for `network.taier`'s node list reader.

use std::path::Path;

use plugin_network::taier::Operator;
use plugin_network::taier::OperatorChoice;
use plugin_network::taier::nodes;
use plugin_network::taier::nodes::ListError;

#[test]
fn fixtures_agree_with_the_taier_list_script() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/taier-lists");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .map(|entry| entry.expect("entry").path())
        .collect();
    entries.sort();
    assert!(entries.len() >= 20, "found {} fixtures", entries.len());

    for path in entries {
        let name = path.file_name().unwrap().to_str().unwrap();
        let bytes = std::fs::read(&path).expect("read fixture");
        let parsed = nodes::parse(&bytes);
        if name.starts_with("good-") {
            assert!(parsed.is_ok(), "{name} should pass: {parsed:?}");
        } else if name == "bad-version-2.json" {
            assert_eq!(parsed, Err(ListError::Newer(2)));
        } else {
            assert!(parsed.is_err(), "{name} should fail: {parsed:?}");
        }
    }
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
