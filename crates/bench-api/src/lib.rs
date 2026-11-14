//! Plugin contract for lowendaff-bench, which every test-item crate implements.

#![warn(missing_docs)]

use bench_host::glibc::GlibcVersion;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;

pub use tokio_util::sync::CancellationToken;

pub mod typed;

/// Architectures every pinned tool must cover.
pub const RELEASE_ARCHES: [&str; 2] = ["x86_64", "aarch64"];

/// Declarative preconditions. Root is not listed: the whole program requires root.
#[derive(Debug, Clone, Copy)]
pub struct Requirements {
    /// Pinned tools, downloaded and verified by the orchestrator before the first test.
    pub tools: &'static [ToolSpec],
    /// Network access the test needs.
    pub net: Net,
    /// Kinds of platform the test applies to; empty means every platform. On any other
    /// platform the plugin does not run.
    pub platforms: &'static [PlatformKind],
    /// The oldest glibc that the test's tools run on, or `None` when they do not need glibc.
    /// The orchestrator checks it before downloading the tools.
    pub glibc: Option<GlibcVersion>,
    /// Whether the test needs at least two NUMA nodes that it can use, as a test between them
    /// does. On any other host the plugin does not run.
    pub numa: bool,
}

impl Requirements {
    /// No tools, no network, every platform, any C library, any NUMA layout.
    pub const NONE: Self = Self {
        tools: &[],
        net: Net::None,
        platforms: &[],
        glibc: None,
        numa: false,
    };

    /// Whether the plugin runs on `host`: on one of its platforms, and with two NUMA nodes or
    /// more when it needs them. A layout that could not be read has none.
    #[must_use]
    pub fn applies_to(&self, host: &HostProfile) -> bool {
        let platform = self.platforms.is_empty() || self.platforms.contains(&host.platform.kind());
        let numa = !self.numa || matches!(&host.numa, Read::Done(layout) if layout.usable() >= 2);
        platform && numa
    }
}

/// Network access a plugin needs; an unreachable one skips the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    /// Works offline.
    None,
    /// Either IPv4 or IPv6 must be reachable.
    Any,
    /// IPv4 must be reachable.
    Ipv4,
    /// IPv6 must be reachable.
    Ipv6,
}

/// A pinned external tool. Plugins that share a tool must declare identical specs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Tool name, e.g. `fio`: the executable's file name, or the directory of an archive.
    pub name: &'static str,
    /// Upstream version the builds were made from.
    pub version: &'static str,
    /// One static build per architecture in [`RELEASE_ARCHES`].
    pub assets: &'static [ToolAsset],
    /// What each asset is.
    pub form: ToolForm,
}

impl ToolSpec {
    /// The build for `arch` (a value of `std::env::consts::ARCH`), if there is one.
    #[must_use]
    pub fn asset_for(&self, arch: &str) -> Option<&ToolAsset> {
        self.assets.iter().find(|asset| asset.arch == arch)
    }
}

/// What the download of a tool is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolForm {
    /// The download is the static executable itself.
    Executable,
    /// A gzip-compressed tar archive with one top-level directory. It is unpacked into a
    /// directory named after the tool, without that top-level directory.
    TarGz {
        /// File name of the executable in the archive's top-level directory.
        executable: &'static str,
    },
}

/// One downloadable build of a [`ToolSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolAsset {
    /// Value of `std::env::consts::ARCH`.
    pub arch: &'static str,
    /// HTTPS download URL.
    pub url: &'static str,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: &'static str,
    /// Exact size in bytes; a download that grows past it is aborted.
    pub size: u64,
}
