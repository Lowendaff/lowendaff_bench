//! Every pinned tool downloads and verifies. Smoke cannot see a
//! wrong pin of a tool whose item does not apply on the CI runners, such as smartctl on
//! virtual machines. Skipped by default because it downloads; CI runs it on both
//! architectures.

use bench::fetcher::Fetcher;
use bench::provision;
use bench::registry::REGISTRY;

#[tokio::test]
#[ignore = "downloads every pinned tool; CI runs it with --run-ignored only"]
async fn every_pinned_tool_downloads_and_verifies() {
    let requirements = REGISTRY
        .iter()
        .map(|registration| registration.descriptor.requires);
    let tools = provision::required_tools(requirements);
    assert!(!tools.is_empty(), "the registry pins tools");
    let fetcher = Fetcher::try_new().expect("the HTTPS client builds");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let arch = std::env::consts::ARCH;
    let provisioned =
        provision::provision_all(&tools, arch, dir.path(), |url| fetcher.open(url)).await;
    assert!(provisioned.failed.is_empty(), "{:?}", provisioned.failed);
    assert_eq!(
        provisioned.ready.len(),
        tools.len(),
        "{:?}",
        provisioned.ready
    );
}
