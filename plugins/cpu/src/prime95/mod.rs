//! Prime95 (`mprime`) timed Small-FFT torture stability test (`x86_64`).

pub mod measure;
pub mod parse;
pub mod result;

use bench_host::glibc::GlibcVersion;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// Minimum glibc version required by upstream `mprime` Linux64.
pub const MIN_GLIBC: GlibcVersion = GlibcVersion {
    major: 2,
    minor: 18,
};
