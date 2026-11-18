//! The cache servers' lookups, from real answers in `tests/fixtures/lookups`: Google's
//! `report_mapping` in each form seen, its server's id written in the code of the cache servers'
//! host names as the documented examples are, and fast.com's API; and the answers that name no
//! server that the item can trace.

use plugin_network::trace::lookup;
use plugin_network::trace::lookup::Cache;
use plugin_network::trace::lookup::LookupError;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/lookups/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

/// The host name of the cache server whose id is `id`, through an answer of its first form.
fn host_of(id: &str) -> Result<String, LookupError> {
    lookup::youtube(format!("192.0.2.1 => {id} (192.0.2.0/24)").as_bytes())
}

#[test]
fn the_code_writes_the_documented_examples() {
    for (id, code) in [
        ("dub16s03", "q0cedn7s"),
        ("bellcanada-yyz16", "cxaaj5o5q5-tt1ed"),
        ("sin26s03", "npoldn7s"),
        ("hkg", "i3b"),
        ("pek", "2x3"),
    ] {
        assert_eq!(
            host_of(id),
            Ok(format!("r1---sn-{code}.googlevideo.com")),
            "{id}"
        );
    }
}

#[test]
fn the_code_writes_each_letter_and_digit_once() {
    let plain = "abcdefghijklmnopqrstuvwxyz0123456789";
    let host = host_of(plain).expect("every character is in the code");
    let code = host
        .strip_prefix("r1---sn-")
        .and_then(|rest| rest.strip_suffix(".googlevideo.com"))
        .expect("the host's shape");
    let mut written: Vec<char> = code.chars().collect();
    assert_eq!(written.len(), 36);
    written.sort_unstable();
    written.dedup();
    assert_eq!(written.len(), 36, "no two characters write the same");
}

#[test]
fn each_form_of_googles_answer_names_its_server() {
    for (name, code) in [
        ("report-mapping-google.txt", "q0cedn7s"),
        ("report-mapping-isp.txt", "cxaaj5o5q5-tt1ed"),
        ("report-mapping-u.txt", "f58xn2xxq-aj5l"),
    ] {
        assert_eq!(
            lookup::youtube(&fixture(name)),
            Ok(format!("r1---sn-{code}.googlevideo.com")),
            "{name}"
        );
    }
    // Google's own servers' answers go on after the id.
    assert_eq!(
        lookup::youtube(b"192.0.2.1 => dub16s03 : router: \"pr06.lax16\" next_hop_address: x"),
        host_of("dub16s03")
    );
}

#[test]
fn an_answer_without_a_server_or_with_one_outside_the_code_names_none() {
    for answer in [
        &b""[..],
        b"no mapping",
        b"192.0.2.1 =>",
        b"192.0.2.1 => \n",
        b"\xff=>a",
    ] {
        assert_eq!(
            lookup::youtube(answer),
            Err(LookupError::NoServer),
            "{answer:?}"
        );
    }
    for id in ["Dub16s03", "dub16s03.", "dub_16", "dub16s03:"] {
        assert_eq!(host_of(id), Err(LookupError::Id), "{id}");
    }
    assert!(host_of(&"a".repeat(40)).is_ok());
    assert_eq!(host_of(&"a".repeat(41)), Err(LookupError::Id));
}

#[test]
fn fast_coms_first_target_names_netflixs_server_and_its_place() {
    assert_eq!(
        lookup::netflix(&fixture("fast.json")),
        Ok(Cache {
            host: "ipv4-c001-fra002-ix.1.oca.nflxvideo.net".to_owned(),
            city: "Frankfurt".to_owned(),
            country: "DE".to_owned(),
        })
    );
}

/// fast.com's answer with one target at `url` in `city`, `country`.
fn one_target(url: &str, city: &str, country: &str) -> Vec<u8> {
    serde_json::json!({
        "client": {"ip": "192.0.2.1"},
        "targets": [{"name": url, "url": url, "location": {"city": city, "country": country}}]
    })
    .to_string()
    .into_bytes()
}

#[test]
fn a_target_at_an_address_is_traced_at_it() {
    let cache = |url| lookup::netflix(&one_target(url, "Loopback", "ZZ")).map(|cache| cache.host);
    assert_eq!(
        cache("https://127.0.0.1/speedtest"),
        Ok("127.0.0.1".to_owned())
    );
    assert_eq!(
        cache("https://[2001:db8::1]:443/x"),
        Ok("2001:db8::1".to_owned())
    );
    assert_eq!(cache("https://a.example?x=1"), Ok("a.example".to_owned()));
    assert_eq!(cache("https://a.example#f"), Ok("a.example".to_owned()));
    for url in [
        "http://a.example/",
        "https://",
        "https://-a.example/",
        "https://[2001:db8::1/",
        "https://a b/",
        "ftp://a.example/",
    ] {
        assert_eq!(cache(url), Err(LookupError::Url), "{url}");
    }
}

#[test]
fn an_answer_that_is_not_fast_coms_or_has_no_usable_target_names_none() {
    assert_eq!(lookup::netflix(b"not json"), Err(LookupError::Format));
    assert_eq!(
        lookup::netflix(br#"{"targets": [{"url": "https://a.example/"}]}"#),
        Err(LookupError::Format),
        "a target without its place"
    );
    assert_eq!(
        lookup::netflix(br#"{"client": {}, "targets": []}"#),
        Err(LookupError::NoTarget)
    );
    let url = "https://a.example/";
    assert_eq!(
        lookup::netflix(&one_target(url, "", "DE")),
        Err(LookupError::Place)
    );
    assert_eq!(
        lookup::netflix(&one_target(url, "Frankfurt", "D\u{1b}E")),
        Err(LookupError::Place)
    );
    assert_eq!(
        lookup::netflix(&one_target(url, &"F".repeat(41), "DE")),
        Err(LookupError::Place)
    );
    assert!(lookup::netflix(&one_target(url, &"F".repeat(40), "DE")).is_ok());
}
