//! Reading NextTrace's output, from its real output in `tests/fixtures/trace`: the hops of a trace
//! that reached its target, one that answered from two addresses, one that no router answered,
//! the loopback after the warning that NextTrace prints without root, and its errors.

use std::net::IpAddr;

use plugin_network::trace::parse;
use plugin_network::trace::result::Answer;
use plugin_network::trace::result::Hop;
use plugin_network::trace::result::Trace;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/trace/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

fn answer(address: &str, network: Option<&str>, place: Option<&str>, latency: u64) -> Answer {
    Answer {
        address: address.parse::<IpAddr>().expect("an address"),
        network: network.map(str::to_owned),
        exchange: None,
        place: place.map(str::to_owned),
        latency,
    }
}

fn hop(answers: Vec<Answer>) -> Hop {
    Hop { answers }
}

/// The hops of `reached.json`, the trace from the VM to Beijing CN2, its own network's hops
/// written with documentation addresses.
fn reached_hops() -> Vec<Hop> {
    let near = |hop_number: u8, latency| {
        hop(vec![answer(
            &format!("192.0.2.{hop_number}"),
            Some("AS64496 Example Net"),
            None,
            latency,
        )])
    };
    let telecom = Some("China Telecom");
    let beijing = Some("China Beijing");
    vec![
        hop(vec![answer("10.0.0.1", None, None, 1059)]),
        near(2, 2700),
        near(3, 3317),
        hop(Vec::new()),
        hop(Vec::new()),
        near(6, 22732),
        near(7, 20899),
        hop(Vec::new()),
        hop(vec![answer(
            "203.22.178.186",
            Some("CTGNet"),
            Some("China Hong Kong"),
            80239,
        )]),
        hop(vec![answer("59.43.181.241", telecom, beijing, 64050)]),
        hop(vec![answer("59.43.19.93", telecom, beijing, 65059)]),
        hop(Vec::new()),
        hop(vec![answer(
            "219.142.211.23",
            Some("AS4847 China Telecom"),
            beijing,
            63530,
        )]),
    ]
}

#[test]
fn a_reached_target_gives_each_hop_with_its_network_place_and_smallest_latency() {
    assert_eq!(
        parse::trace(&fixture("reached.json")),
        Some(Trace {
            reached: true,
            hops: reached_hops(),
        })
    );
}

#[test]
fn a_hop_that_answers_from_two_addresses_shows_both_in_the_order_they_first_answered() {
    let mut hops = reached_hops();
    // The second probe of hop 10 came from another address; the first and third from the same.
    hops[9].answers.push(answer(
        "203.0.113.10",
        Some("China Telecom"),
        Some("China Beijing"),
        64612,
    ));
    assert_eq!(
        parse::trace(&fixture("load-balanced.json")),
        Some(Trace {
            reached: true,
            hops,
        })
    );
}

#[test]
fn a_target_that_no_router_answered_is_not_reached_after_its_30_silent_hops() {
    let trace = parse::trace(&fixture("not-reached.json")).expect("a trace");
    assert!(!trace.reached);
    assert_eq!(trace.hops, vec![hop(Vec::new()); 30]);
}

#[test]
fn the_warning_before_the_json_is_skipped() {
    let loopback = Trace {
        reached: true,
        hops: vec![hop(vec![answer("127.0.0.1", None, None, 78)])],
    };
    assert_eq!(parse::trace(&fixture("loopback.json")), Some(loopback));
    let warned = parse::trace(&fixture("warned.txt")).expect("the trace after the warning");
    assert!(warned.reached);
    assert_eq!(warned.hops.len(), 1);
    assert_eq!(warned.hops[0].answers[0].address.to_string(), "127.0.0.1");
}

#[test]
fn an_error_is_its_last_line_without_the_time_and_what_follows_outside_ascii() {
    assert_eq!(parse::trace(&fixture("unresolved.txt")), None);
    assert_eq!(
        parse::error(&fixture("unresolved.txt")).as_deref(),
        Some("DNS lookup failed: lookup does-not-exist.invalid on 192.0.2.53:53: no such host")
    );
    assert_eq!(parse::trace(&fixture("no-capability.txt")), None);
    assert_eq!(
        parse::error(&fixture("no-capability.txt")).as_deref(),
        Some(
            "(InitICMP) ListenPacket(ip4:icmp, 127.0.0.1) failed: listen ip4:icmp 127.0.0.1: \
             socket: operation not permitted"
        )
    );
}

#[test]
fn nothing_is_quoted_of_the_warning_alone() {
    let warning: String = fixture("warned.txt")
        .lines()
        .take_while(|line| !line.starts_with('{'))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(warning.lines().count(), 3);
    assert_eq!(parse::trace(&warning), None);
    assert_eq!(
        parse::error(&warning),
        None,
        "its lines start outside ASCII"
    );
    assert_eq!(parse::error(""), None);
    assert_eq!(parse::error("\n  \n"), None);
    assert_eq!(
        parse::error("failed\n  {\"Hops\":[[{\"Address\":{\"IP\":\"192.0.2.1\"\n"),
        None,
        "no line of a JSON document, which names addresses"
    );
}

#[test]
fn only_a_line_that_starts_with_the_time_loses_it() {
    assert_eq!(
        parse::error("2026/11/18 09:49:38 failed\n").as_deref(),
        Some("failed")
    );
    assert_eq!(
        parse::error("2026/11/18 09:49:38x failed\n").as_deref(),
        Some("2026/11/18 09:49:38x failed")
    );
    assert_eq!(
        parse::error("2026-11-18 09:49:38 failed").as_deref(),
        Some("2026-11-18 09:49:38 failed")
    );
    assert_eq!(
        parse::error("2026/11/18 09:49:38 ").as_deref(),
        Some("2026/11/18 09:49:38")
    );
    assert_eq!(
        parse::error("first\nlast line  \n\n").as_deref(),
        Some("last line")
    );
}

#[test]
fn a_document_that_is_not_a_trace_gives_none() {
    for text in [
        "",
        "no json\n",
        "{\"Hops\": \n",
        "{\"version\": 1}\n",
        "{\"Hops\": [[{\"Success\": true}]], \"StopReason\": null}\n trailing",
    ] {
        let trace = parse::trace(text);
        if text.contains("Success") {
            // A probe without an address is no answer, as of a document that ends in other text.
            assert_eq!(
                trace,
                Some(Trace {
                    reached: false,
                    hops: vec![hop(Vec::new())],
                })
            );
        } else {
            assert_eq!(trace, None, "{text:?}");
        }
    }
}

#[test]
fn names_lose_control_characters_and_runs_of_spaces_and_an_as_number_must_be_one() {
    let text = concat!(
        "{\"Hops\": [[{\"Success\": true, \"Address\": {\"IP\": \"192.0.2.9\"}, \"RTT\": 1499,",
        " \"Geo\": {\"asnumber\": \"AS64496\", \"isp\": \" \", \"owner\": \"example.net \\u001b[2J",
        "  Example\", \"country_en\": \"Ex\\nample\", \"prov_en\": \" \", \"city_en\": \"\"}}]],",
        " \"StopReason\": {\"reason\": \"destination_reached\"}}\n"
    );
    assert_eq!(
        parse::trace(text),
        Some(Trace {
            reached: true,
            hops: vec![hop(vec![answer(
                "192.0.2.9",
                Some("example.net [2J Example"),
                Some("Ex ample"),
                1,
            )])],
        })
    );
}
