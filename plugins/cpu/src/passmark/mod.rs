//! PassMark PerformanceTest Linux (`-r 1` CPU suite) parser and runner.

pub mod measure;
pub mod parse;
pub mod result;

use bench_host::glibc::GlibcVersion;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// Minimum glibc version required by PassMark PerformanceTest Linux 11 and `passmark-libs`.
pub const MIN_GLIBC: GlibcVersion = GlibcVersion {
    major: 2,
    minor: 28,
};
