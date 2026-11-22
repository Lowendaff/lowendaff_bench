//! The server list: its rules, its versions, and the servers chosen by default, by country and by
//! id.

use plugin_network::iperf3::servers;
use plugin_network::iperf3::servers::ListError;
use plugin_network::iperf3::servers::Rule;
use plugin_network::iperf3::servers::Server;
use serde_json::Value;
use serde_json::json;

/// A server as the list writes it, which the tests change field by field.
fn written(id: &str, country: &str, default: bool) -> Value {
    json!({
        "id": id,
        "country": country,
        "location": "Tokyo",
        "provider": "Leaseweb",
        "host": "speedtest.tyo11.jp.leaseweb.net",
        "ports": "5201-5210",
        "families": ["ipv4", "ipv6"],
        "link": "10G",
        "default": default
    })
}

#[allow(clippy::needless_pass_by_value)]
fn list(servers: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({ "version": 1, "servers": servers })).expect("JSON")
}

/// The list with one server whose `field` is `value`.
fn with(field: &str, value: Value) -> Result<Vec<Server>, ListError> {
    let mut server = written("tokyo", "JP", true);
    server[field] = value;
    servers::parse(&list(vec![server]))
}

#[test]
fn a_server_is_read_with_every_field() {
    let servers = servers::parse(&list(vec![written("tokyo", "JP", true)])).expect("a list");
    assert_eq!(
        servers,
        [Server {
            id: "tokyo".to_owned(),
            country: "JP".to_owned(),
            location: "Tokyo".to_owned(),
            provider: "Leaseweb".to_owned(),
            host: "speedtest.tyo11.jp.leaseweb.net".to_owned(),
            ports: (5201, 5210),
            ipv4: true,
            ipv6: true,
            link: "10G".to_owned(),
            default: true,
        }]
    );
    assert_eq!(servers[0].port_list().len(), 10);
}

#[test]
fn fields_that_the_program_does_not_know_are_ignored_and_default_is_false_unless_given() {
    let mut server = written("tokyo", "JP", true);
    server["operator_note"] = json!("new in a later version");
    server.as_object_mut().expect("an object").remove("default");
    let bytes = serde_json::to_vec(&json!({
        "version": 1,
        "generated_by": "a later workflow",
        "servers": [server]
    }))
    .expect("JSON");
    let servers = servers::parse(&bytes).expect("a list");
    assert!(!servers[0].default);
}

#[test]
fn a_list_of_a_newer_version_is_not_read_and_one_of_version_0_never_was() {
    let bytes = |version: u64| {
        serde_json::to_vec(&json!({ "version": version, "servers": "a later shape" }))
            .expect("JSON")
    };
    assert_eq!(servers::parse(&bytes(2)), Err(ListError::Newer(2)));
    assert_eq!(servers::parse(&bytes(0)), Err(ListError::Version(0)));
}

#[test]
fn a_list_must_be_json_of_its_format_and_at_most_64_kib() {
    let Err(ListError::Format(reason)) = servers::parse(b"<html>404</html>") else {
        panic!("not JSON");
    };
    assert!(!reason.is_empty());
    let mut server = written("tokyo", "JP", true);
    server.as_object_mut().expect("an object").remove("host");
    let Err(ListError::Format(reason)) = servers::parse(&list(vec![server])) else {
        panic!("a server without a host");
    };
    assert!(reason.contains("host"), "{reason}");

    let mut large = list(vec![written("tokyo", "JP", true)]);
    large.resize(servers::LIMIT + 1, b' ');
    assert_eq!(servers::parse(&large), Err(ListError::TooLarge));
    large.truncate(servers::LIMIT);
    assert!(
        servers::parse(&large).is_ok(),
        "64 KiB, with spaces at its end"
    );
}

#[test]
fn a_list_holds_at_most_100_servers_each_with_its_own_id() {
    let many = |count: usize| {
        let servers = (0..count)
            .map(|index| written(&format!("server-{index}"), "JP", false))
            .collect();
        servers::parse(&list(servers))
    };
    assert_eq!(many(100).map(|servers| servers.len()), Ok(100));
    assert_eq!(many(101), Err(ListError::TooMany(101)));
    let twice = list(vec![
        written("tokyo", "JP", true),
        written("tokyo", "JP", false),
    ]);
    assert_eq!(
        servers::parse(&twice),
        Err(ListError::Duplicate("tokyo".to_owned()))
    );
}

#[test]
fn an_id_is_1_to_40_characters_of_a_to_z_digits_and_dashes() {
    for good in ["tokyo", "los-angeles", "nyc1", &"a".repeat(40)] {
        assert!(with("id", json!(good)).is_ok(), "{good}");
    }
    for bad in [
        "",
        "Tokyo",
        "los_angeles",
        "new york",
        "tokyo/1",
        &"a".repeat(41),
        "tōkyō",
    ] {
        assert_eq!(
            with("id", json!(bad)),
            Err(ListError::Server(1, Rule::Id)),
            "{bad}"
        );
    }
}

#[test]
fn a_country_is_two_capital_letters() {
    for bad in ["", "jp", "J", "JPN", "J1"] {
        assert_eq!(
            with("country", json!(bad)),
            Err(ListError::Server(1, Rule::Country)),
            "{bad}"
        );
    }
}

#[test]
fn a_host_is_a_dns_name_or_an_ip_address_and_never_starts_with_a_dash() {
    for good in [
        "speedtest.tyo11.jp.leaseweb.net",
        "iperf-ams-nl.eranium.net",
        "localhost",
        "127.0.0.1",
        "2001:db8::1",
    ] {
        assert!(with("host", json!(good)).is_ok(), "{good}");
    }
    let long = format!("{}.example", "a".repeat(64));
    for bad in [
        "",
        "-R",
        "--help",
        "host.-example.net",
        "host-.example.net",
        "two words",
        "example.net.",
        "a..b",
        "host;reboot",
        "[2001:db8::1]",
        &long,
    ] {
        assert_eq!(
            with("host", json!(bad)),
            Err(ListError::Server(1, Rule::Host)),
            "{bad}"
        );
    }
}

#[test]
fn ports_are_one_or_a_range_from_1024_to_65535_with_at_most_64() {
    let ports = |ports: &str| with("ports", json!(ports)).map(|servers| servers[0].ports);
    assert_eq!(ports("5201"), Ok((5201, 5201)));
    assert_eq!(ports("1024-1087"), Ok((1024, 1087)), "64 ports");
    assert_eq!(ports("65535"), Ok((65535, 65535)));
    for bad in [
        "",
        "1023",
        "1023-1030",
        "1024-1088",
        "5210-5201",
        "65536",
        "5201-",
        "-5201",
        "52o1",
        "5201 - 5210",
        "+5201",
    ] {
        assert_eq!(
            with("ports", json!(bad)),
            Err(ListError::Server(1, Rule::Ports)),
            "{bad}"
        );
    }
}

#[test]
fn families_are_ipv4_ipv6_or_both_each_once() {
    let families = |families: Value| {
        with("families", families).map(|servers| (servers[0].ipv4, servers[0].ipv6))
    };
    assert_eq!(families(json!(["ipv4"])), Ok((true, false)));
    assert_eq!(families(json!(["ipv6"])), Ok((false, true)));
    assert_eq!(families(json!(["ipv6", "ipv4"])), Ok((true, true)));
    for bad in [
        json!([]),
        json!(["ipv4", "ipv4"]),
        json!(["IPv4"]),
        json!(["ipv4", "udp"]),
    ] {
        assert_eq!(
            families(bad.clone()),
            Err(ListError::Server(1, Rule::Families)),
            "{bad}"
        );
    }
}

#[test]
fn a_place_provider_and_link_are_1_to_40_characters_that_print() {
    assert!(with("location", json!("São Paulo")).is_ok());
    assert!(with("location", json!("a".repeat(40))).is_ok());
    assert!(
        with("provider", json!("é".repeat(40))).is_ok(),
        "40 characters, though 80 bytes"
    );
    for field in ["location", "provider", "link"] {
        for bad in ["", "a\u{1b}[2Jcleared", "two\nlines", &"a".repeat(41)] {
            assert_eq!(
                with(field, json!(bad)),
                Err(ListError::Server(1, Rule::Text)),
                "{field}: {bad:?}"
            );
        }
    }
}

#[test]
fn the_position_of_a_server_that_breaks_a_rule_counts_from_1() {
    let mut second = written("osaka", "JP", false);
    second["ports"] = json!("80");
    let bytes = list(vec![written("tokyo", "JP", true), second]);
    assert_eq!(
        servers::parse(&bytes),
        Err(ListError::Server(2, Rule::Ports))
    );
}

/// Four servers in two countries, two of them the defaults.
fn four() -> Vec<Server> {
    let servers = vec![
        written("los-angeles", "US", true),
        written("new-york", "US", false),
        written("tokyo", "JP", true),
        written("frankfurt", "DE", false),
    ];
    servers::parse(&list(servers)).expect("a list")
}

fn ids(chosen: &servers::Chosen) -> Vec<&str> {
    chosen
        .servers
        .iter()
        .map(|server| server.id.as_str())
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn the_defaults_are_chosen_unless_countries_or_ids_are_given() {
    let chosen = servers::choose(&four(), &[], &[]);
    assert_eq!(ids(&chosen), ["los-angeles", "tokyo"]);
    assert!(chosen.unknown.is_empty());
}

#[test]
fn countries_choose_every_server_of_theirs_in_the_lists_order_in_any_case() {
    let chosen = servers::choose(&four(), &strings(&["jp", "US"]), &[]);
    assert_eq!(ids(&chosen), ["los-angeles", "new-york", "tokyo"]);
    assert!(chosen.unknown.is_empty());

    let chosen = servers::choose(&four(), &strings(&["DE", "BR", "xx"]), &[]);
    assert_eq!(ids(&chosen), ["frankfurt"]);
    assert_eq!(chosen.unknown, ["BR", "xx"], "as they were given");

    let chosen = servers::choose(&four(), &strings(&["BR"]), &[]);
    assert!(chosen.servers.is_empty());
    assert_eq!(chosen.unknown, ["BR"]);
}

#[test]
fn all_chooses_every_server() {
    for all in ["all", "ALL", "All"] {
        let chosen = servers::choose(&four(), &strings(&["JP", all]), &[]);
        assert_eq!(chosen.servers.len(), 4, "{all}");
        assert!(chosen.unknown.is_empty());
    }
}

#[test]
fn ids_win_over_countries() {
    let chosen = servers::choose(
        &four(),
        &strings(&["US"]),
        &strings(&["frankfurt", "sao-paulo", "tokyo"]),
    );
    assert_eq!(ids(&chosen), ["tokyo", "frankfurt"], "in the list's order");
    assert_eq!(chosen.unknown, ["sao-paulo"]);
}
