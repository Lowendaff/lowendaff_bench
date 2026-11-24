//! Optional build-time embedding of a verified `servers/taier.json` snapshot when
//! `BENCH_TAIER_SNAPSHOT_PATH` is set. CI and release workflows inject the latest catalog from
//! `Lowendaff/bench_data` at compile time so hosts that cannot reach `raw.githubusercontent.com`
//! directly still have an embedded fallback catalog without tracking server IP lists in Git.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use serde_json::Value;

const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024;
const MOST_LOCATIONS: usize = 400;
const MOST_NODES: usize = 32;

fn main() {
    println!("cargo:rerun-if-env-changed=BENCH_TAIER_SNAPSHOT_PATH");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    let generated = out_dir.join("taier_snapshot.rs");

    let raw_path = match env::var("BENCH_TAIER_SNAPSHOT_PATH") {
        Ok(val) => val,
        Err(env::VarError::NotPresent) => String::new(),
        Err(env::VarError::NotUnicode(_)) => {
            panic!("BENCH_TAIER_SNAPSHOT_PATH must be valid UTF-8")
        }
    };
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        fs::write(
            &generated,
            "const EMBEDDED_SERVERS: Option<&[u8]> = None;\n",
        )
        .expect("write empty taier_snapshot.rs");
        return;
    }

    assert!(
        !trimmed.contains(['\n', '\r']),
        "BENCH_TAIER_SNAPSHOT_PATH must not contain newline characters"
    );
    println!("cargo:rerun-if-changed={trimmed}");

    let file = fs::File::open(trimmed).unwrap_or_else(|err| {
        panic!("failed to open BENCH_TAIER_SNAPSHOT_PATH ({trimmed}): {err}")
    });
    let mut bytes = Vec::new();
    file.take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)
        .unwrap_or_else(|err| {
            panic!("failed to read BENCH_TAIER_SNAPSHOT_PATH ({trimmed}): {err}")
        });
    validate_snapshot(trimmed, &bytes);

    let snapshot_copy = out_dir.join("taier-snapshot.json");
    fs::write(&snapshot_copy, &bytes).expect("copy taier-snapshot.json to OUT_DIR");
    fs::write(
        &generated,
        "const EMBEDDED_SERVERS: Option<&[u8]> = \
         Some(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/taier-snapshot.json\")));\n",
    )
    .expect("write embedded taier_snapshot.rs");
}

fn validate_snapshot(path_label: &str, bytes: &[u8]) {
    assert!(
        !bytes.is_empty() && (bytes.len() as u64) <= MAX_SNAPSHOT_BYTES,
        "BENCH_TAIER_SNAPSHOT_PATH ({path_label}) must be 1..={MAX_SNAPSHOT_BYTES} bytes, got {}",
        bytes.len()
    );
    let parsed: Value = serde_json::from_slice(bytes).unwrap_or_else(|err| {
        panic!("BENCH_TAIER_SNAPSHOT_PATH ({path_label}) is not valid JSON: {err}")
    });
    assert_no_duplicate_keys(path_label, bytes);
    assert_eq!(
        parsed.get("version").and_then(Value::as_u64),
        Some(1),
        "BENCH_TAIER_SNAPSHOT_PATH ({path_label}) must have version == 1"
    );
    let Some(locations) = parsed.get("locations").and_then(Value::as_array) else {
        panic!("BENCH_TAIER_SNAPSHOT_PATH ({path_label}) must have a locations array");
    };
    assert!(
        (1..=MOST_LOCATIONS).contains(&locations.len()),
        "BENCH_TAIER_SNAPSHOT_PATH ({path_label}) must have 1..={MOST_LOCATIONS} locations, got {}",
        locations.len()
    );

    let mut seen_ids = BTreeSet::new();
    for (idx, loc) in locations.iter().enumerate() {
        let pos = idx + 1;
        let Some(id) = loc.get("id").and_then(Value::as_str) else {
            panic!("location {pos} in {path_label} is missing string id");
        };
        assert!(
            valid_id(id),
            "location {pos} in {path_label} has invalid id `{id}`"
        );
        assert!(
            seen_ids.insert(id),
            "duplicate location id `{id}` in {path_label}"
        );
        let Some(province) = loc.get("province").and_then(Value::as_str) else {
            panic!("location {pos} (`{id}`) in {path_label} is missing string province");
        };
        assert!(
            ascii_printable(province, 40),
            "location {pos} (`{id}`) in {path_label} has invalid province"
        );
        let Some(city) = loc.get("city").and_then(Value::as_str) else {
            panic!("location {pos} (`{id}`) in {path_label} is missing string city");
        };
        assert!(
            ascii_printable(city, 40),
            "location {pos} (`{id}`) in {path_label} has invalid city"
        );
        assert!(
            loc.get("default").is_none_or(Value::is_boolean),
            "location {pos} (`{id}`) in {path_label} has non-boolean default"
        );
        let Some(nodes) = loc.get("nodes").and_then(Value::as_array) else {
            panic!("location {pos} (`{id}`) in {path_label} is missing nodes array");
        };
        assert!(
            (1..=MOST_NODES).contains(&nodes.len()),
            "location {pos} (`{id}`) in {path_label} must have 1..={MOST_NODES} nodes"
        );

        let mut endpoints = BTreeSet::new();
        for node in nodes {
            let Some(op) = node.get("operator").and_then(Value::as_str) else {
                panic!("location {pos} (`{id}`) in {path_label} has node without operator");
            };
            assert!(
                matches!(
                    op,
                    "telecom" | "unicom" | "mobile" | "cernet" | "cbn" | "other"
                ),
                "location {pos} (`{id}`) in {path_label} has unknown operator `{op}`"
            );
            let Some(ip_raw) = node.get("ip").and_then(Value::as_str) else {
                panic!("location {pos} (`{id}`) in {path_label} has node without string ip");
            };
            let Some(ip) = parse_routable_ipv4(ip_raw) else {
                panic!("location {pos} (`{id}`) in {path_label} has invalid or non-public ip");
            };
            let Some(raw_port) = node.get("port").and_then(Value::as_u64) else {
                panic!("location {pos} (`{id}`) in {path_label} has node without integer port");
            };
            assert!(
                (1024..=65535).contains(&raw_port),
                "location {pos} (`{id}`) in {path_label} has out-of-range port {raw_port}"
            );
            let port = u16::try_from(raw_port).expect("port within 1024..=65535");
            assert!(
                endpoints.insert((ip, port)),
                "location {pos} (`{id}`) in {path_label} has duplicate endpoint {ip}:{port}"
            );
        }
    }
}

fn valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id != "all"
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn ascii_printable(text: &str, max_len: usize) -> bool {
    (1..=max_len).contains(&text.len()) && text.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

fn assert_no_duplicate_keys(path_label: &str, bytes: &[u8]) {
    let mut stack: Vec<(bool, BTreeSet<String>, bool)> = Vec::new();
    let mut idx = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' => {
                stack.push((true, BTreeSet::new(), false));
                idx += 1;
            }
            b'[' => {
                stack.push((false, BTreeSet::new(), false));
                idx += 1;
            }
            b'}' | b']' => {
                stack.pop();
                idx += 1;
            }
            b':' => {
                if let Some((true, _, after_colon)) = stack.last_mut() {
                    *after_colon = true;
                }
                idx += 1;
            }
            b',' => {
                if let Some((true, _, after_colon)) = stack.last_mut() {
                    *after_colon = false;
                }
                idx += 1;
            }
            b'"' => {
                let mut end = idx + 1;
                while end < bytes.len() {
                    if bytes[end] == b'\\' {
                        end += 2;
                        continue;
                    }
                    if bytes[end] == b'"' {
                        break;
                    }
                    end += 1;
                }
                if let Some((true, seen, false)) = stack.last_mut() {
                    let key: String =
                        serde_json::from_slice(&bytes[idx..=end]).expect("valid JSON object key");
                    assert!(
                        seen.insert(key.clone()),
                        "duplicate JSON key `{key}` in {path_label}"
                    );
                }
                idx = end + 1;
            }
            _ => idx += 1,
        }
    }
}

fn parse_routable_ipv4(raw: &str) -> Option<Ipv4Addr> {
    let ip: Ipv4Addr = raw.parse().ok()?;
    if ip.to_string() != raw {
        return None;
    }
    let [a, b, c, _] = ip.octets();
    let routable = !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || a == 0
        || a >= 224
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19)));
    routable.then_some(ip)
}
