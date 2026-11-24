//! Live `Lowendaff/bench_data` catalog validation and embedded fallback unit tests.
//!
//! The ignored test downloads every published server and target list from `Lowendaff/bench_data`
//! (at `BENCH_DATA_REF` when set by CI, otherwise `main`) and verifies that all four JSON catalogs
//! pass the Rust parsers, default slot selection, TUI region grouping, and `--taier-list`
//! formatting.

use std::env;
use std::time::Duration;

use bench::app;
use bench::fetcher::Fetcher;
use bench::provision;
use bench::provision::DownloadError;
use bench::provision::Part;
use bench::tui;
use plugin_network::iperf3;
use plugin_network::taier;
use plugin_network::trace;

const LIVE_SAMPLE: &[u8] = br#"{
    "version": 1,
    "locations": [
        {
            "id": "shanghai",
            "province": "Shanghai",
            "city": "Shanghai",
            "default": true,
            "nodes": [
                {"operator": "unicom", "ip": "198.51.100.10", "port": 65499}
            ]
        }
    ]
}"#;

const EMBEDDED_SAMPLE: &[u8] = br#"{
    "version": 1,
    "locations": [
        {
            "id": "beijing",
            "province": "Beijing",
            "city": "Beijing",
            "default": true,
            "nodes": [
                {"operator": "telecom", "ip": "198.51.100.20", "port": 65499}
            ]
        }
    ]
}"#;

#[test]
fn resolve_taier_locations_prefers_live_catalog_and_falls_back_to_embedded() {
    let live = app::resolve_taier_locations(Ok(LIVE_SAMPLE), Some(EMBEDDED_SAMPLE))
        .expect("live catalog wins");
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, "shanghai");

    let err = "connection reset by peer".to_owned();
    let fallback = app::resolve_taier_locations(Err(&err), Some(EMBEDDED_SAMPLE))
        .expect("falls back to embedded snapshot");
    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].id, "beijing");

    let corrupt: &[u8] = b"{not-json";
    let corrupt_fallback = app::resolve_taier_locations(Ok(corrupt), Some(EMBEDDED_SAMPLE))
        .expect("corrupt live response falls back to embedded snapshot");
    assert_eq!(corrupt_fallback[0].id, "beijing");

    let no_fallback = app::resolve_taier_locations(Err(&err), None)
        .expect_err("without embedded snapshot, error surfaces");
    assert_eq!(no_fallback, err);
}

async fn download_list(fetcher: &Fetcher, git_ref: &str, name: &str) -> Vec<u8> {
    use tokio::io::AsyncReadExt;

    let url =
        format!("https://raw.githubusercontent.com/Lowendaff/bench_data/{git_ref}/servers/{name}");
    let mut last_err = String::new();
    for attempt in 0_u32..4 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1_u64 << (attempt - 1))).await;
        }
        let part = match fetcher.open(&url, None).await {
            Ok(part) => part,
            Err(DownloadError::Transient(err)) => {
                last_err = err;
                continue;
            }
            Err(DownloadError::Final(err)) => panic!("failed to download {url}: {err}"),
        };
        let reader = match part {
            Part::Whole(r) | Part::Range(r) => r,
        };
        let mut buf = Vec::new();
        match reader
            .take(provision::FILE_LIMIT + 1)
            .read_to_end(&mut buf)
            .await
        {
            Ok(_) => {
                assert!(
                    (buf.len() as u64) <= provision::FILE_LIMIT,
                    "{name} exceeds {} bytes ({} bytes)",
                    provision::FILE_LIMIT,
                    buf.len()
                );
                return buf;
            }
            Err(err) => {
                last_err = err.to_string();
            }
        }
    }
    panic!("failed to download {url} after 4 attempts: {last_err}");
}

#[tokio::test]
#[ignore = "downloads live server lists from bench_data; CI runs it with --run-ignored only"]
async fn live_bench_data_lists_parse_and_render() {
    let git_ref = env::var("BENCH_DATA_REF")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_owned());
    let fetcher = Fetcher::try_new().expect("HTTPS client builds");

    let iperf3_raw = download_list(&fetcher, &git_ref, "iperf3.json").await;
    let iperf3_servers =
        iperf3::servers::parse_for_source(&iperf3_raw, false).expect("iperf3.json parses");
    assert!(!iperf3_servers.is_empty(), "iperf3.json has servers");

    let taier_raw = download_list(&fetcher, &git_ref, "taier.json").await;
    let taier_locations = taier::nodes::parse(&taier_raw).expect("taier.json parses");
    assert!(!taier_locations.is_empty(), "taier.json has locations");
    let defaults = taier::nodes::choose(&taier_locations, &[], &[]);
    assert!(
        !defaults.slots.is_empty(),
        "taier.json provides default Beijing/Shanghai/Guangzhou slots"
    );
    let regions = tui::taier_picker::build_regions(&taier_locations);
    assert!(!regions.is_empty(), "TUI region hierarchy builds");
    let mut overview = Vec::new();
    app::format_taier_list(&taier_locations, "", &mut overview).expect("format overview");
    assert!(!overview.is_empty());
    let mut guangdong = Vec::new();
    app::format_taier_list(&taier_locations, "guangdong", &mut guangdong)
        .expect("format guangdong");
    assert!(!guangdong.is_empty());

    let trace_raw = download_list(&fetcher, &git_ref, "traceroute.json").await;
    let trace_targets =
        trace::targets::parse_for_source(&trace_raw, false).expect("traceroute.json parses");
    assert!(!trace_targets.is_empty(), "traceroute.json has targets");

    let ix_raw = download_list(&fetcher, &git_ref, "netflix-ix.json").await;
    let exchanges = trace::exchanges::parse(&ix_raw).expect("netflix-ix.json parses");
    assert!(!exchanges.is_empty(), "netflix-ix.json has ports");
}
