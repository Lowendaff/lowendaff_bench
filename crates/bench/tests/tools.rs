//! Every pinned tool downloads and verifies. Smoke cannot see a
//! wrong pin of a tool whose item does not apply on the CI runners, such as smartctl on
//! virtual machines. Skipped by default because it downloads; CI runs it on both
//! architectures, checking every pinned archive and SHA-256 digest independently of
//! host raw-socket file capabilities.

use bench::app;
use bench::fetcher::Fetcher;
use bench::provision;
use bench::registry;
use bench_api::CancellationToken;
use bench_api::ToolSpec;

#[tokio::test]
#[ignore = "downloads every pinned tool; CI runs it with --run-ignored only"]
async fn every_pinned_tool_downloads_and_verifies() {
    let entries = registry::all();
    let requirements = entries.iter().map(|entry| entry.requires);
    let arch = std::env::consts::ARCH;
    let tools: Vec<ToolSpec> = provision::required_tools(requirements)
        .into_iter()
        .filter(|tool| {
            if tool.all_arches {
                assert!(
                    tool.asset_for(arch).is_some(),
                    "tool {} is marked all_arches but lacks {arch}",
                    tool.name
                );
            }
            tool.asset_for(arch).is_some()
        })
        .map(|tool| ToolSpec {
            raw_sockets: false,
            ..tool
        })
        .collect();
    assert!(!tools.is_empty(), "the registry pins tools");
    let fetcher = Fetcher::try_new().expect("the HTTPS client builds");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let connections = usize::from(app::DOWNLOAD_CONNECTIONS);
    let provisioned = provision::provision_all(
        &tools,
        arch,
        dir.path(),
        connections,
        &CancellationToken::new(),
        |url, range| fetcher.open(url, range),
    )
    .await;
    assert!(provisioned.failed.is_empty(), "{:?}", provisioned.failed);
    assert_eq!(
        provisioned.ready.len(),
        tools.len(),
        "{:?}",
        provisioned.ready
    );
}
