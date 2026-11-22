//! Optional build-time embedding of a verified `servers/taier.json` snapshot when
//! `BENCH_TAIER_SNAPSHOT_PATH` is set. CI and release workflows inject the latest catalog from
//! `Lowendaff/bench_data` at compile time so hosts that cannot reach `raw.githubusercontent.com`
//! directly still have an embedded fallback catalog without tracking server IP lists in Git.

use std::env;
use std::fs;
use std::path::PathBuf;

const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024;

fn main() {
    println!("cargo:rerun-if-env-changed=BENCH_TAIER_SNAPSHOT_PATH");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    let generated = out_dir.join("taier_snapshot.rs");

    let raw_path = env::var("BENCH_TAIER_SNAPSHOT_PATH").unwrap_or_default();
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        fs::write(
            &generated,
            "const EMBEDDED_SERVERS: Option<&[u8]> = None;\n",
        )
        .expect("write empty taier_snapshot.rs");
        return;
    }

    println!("cargo:rerun-if-changed={trimmed}");
    let bytes = fs::read(trimmed).unwrap_or_else(|err| {
        panic!("failed to read BENCH_TAIER_SNAPSHOT_PATH ({trimmed}): {err}")
    });
    assert!(
        !bytes.is_empty() && (bytes.len() as u64) <= MAX_SNAPSHOT_BYTES,
        "BENCH_TAIER_SNAPSHOT_PATH ({trimmed}) must be 1..={MAX_SNAPSHOT_BYTES} bytes, got {}",
        bytes.len()
    );
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|err| {
        panic!("BENCH_TAIER_SNAPSHOT_PATH ({trimmed}) is not valid JSON: {err}")
    });
    assert_eq!(
        parsed.get("version").and_then(serde_json::Value::as_u64),
        Some(1),
        "BENCH_TAIER_SNAPSHOT_PATH ({trimmed}) must have version == 1"
    );
    let has_locations = parsed
        .get("locations")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|locs| !locs.is_empty());
    assert!(
        has_locations,
        "BENCH_TAIER_SNAPSHOT_PATH ({trimmed}) must have non-empty locations"
    );

    let snapshot_copy = out_dir.join("taier-snapshot.json");
    fs::write(&snapshot_copy, &bytes).expect("copy taier-snapshot.json to OUT_DIR");
    fs::write(
        &generated,
        "const EMBEDDED_SERVERS: Option<&[u8]> = \
         Some(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/taier-snapshot.json\")));\n",
    )
    .expect("write embedded taier_snapshot.rs");
}
