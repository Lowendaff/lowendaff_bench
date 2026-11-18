//! Every pinned tool downloads and verifies. Smoke cannot see a
//! wrong pin of a tool whose item does not apply on the CI runners, such as smartctl on
//! virtual machines. Skipped by default because it downloads; CI runs it on both
//! architectures, on Linux, where NextTrace's download is checked before its file capability.

use bench::app;
use bench::fetcher::Fetcher;
use bench::provision;
use bench::registry;
use bench_api::CancellationToken;

#[tokio::test]
#[ignore = "downloads every pinned tool; CI runs it with --run-ignored only"]
async fn every_pinned_tool_downloads_and_verifies() {
    let entries = registry::all();
    let requirements = entries.iter().map(|entry| entry.requires);
    let tools = provision::required_tools(requirements);
    assert!(!tools.is_empty(), "the registry pins tools");
    let fetcher = Fetcher::try_new().expect("the HTTPS client builds");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let arch = std::env::consts::ARCH;
    // In ranges, as the program downloads them: both servers serve ranges.
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
    // A tool that sends raw packets downloads and is checked as the others are, and then gets its
    // file capability, which only root may give: run as another user, as in CI, that last step
    // alone fails, after the download was checked.
    let root = rustix::process::geteuid().is_root();
    let refused = |name: &str, reason: &str| {
        !root
            && tools
                .iter()
                .any(|tool| tool.name == name && tool.raw_sockets)
            && reason.starts_with("cannot give it raw sockets (the file capability CAP_NET_RAW): ")
    };
    let (capabilities, failed): (Vec<_>, Vec<_>) = provisioned
        .failed
        .iter()
        .partition(|(name, reason)| refused(name, reason));
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(
        provisioned.ready.len() + capabilities.len(),
        tools.len(),
        "{:?}",
        provisioned.ready
    );
}
