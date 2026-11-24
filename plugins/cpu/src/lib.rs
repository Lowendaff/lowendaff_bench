//! Unified CPU benchmark plugin (`cpu.bench`) and its eight selectable sub-engines:
//! `sysbench`, `7zip` (`seven_zip`), `unixbench`, `hpl`, `passmark`, `ycruncher`, `prime95`, and
//! `geekbench`.

#![warn(missing_docs)]

pub mod bench;
pub mod geekbench;
pub mod hpl;
pub mod passmark;
pub mod prime95;
pub mod seven_zip;
pub mod sysbench;
pub mod unixbench;
pub mod ycruncher;

use std::num::NonZeroUsize;

use bench_host::Root;
use bench_host::memory;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;

/// The CPUs a test can use at once, as the host profile counts them; the scheduler's count when
/// the profile has none, and one at least.
#[must_use]
pub fn usable_threads(host: &HostProfile) -> usize {
    let usable = match &host.cpu {
        Read::Done(facts) => facts.usable.filter(|&n| n > 0),
        Read::TimedOut | Read::Failed => None,
    };
    usable
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(NonZeroUsize::get)
        })
        .unwrap_or(1)
}

/// Available memory in bytes right now (honoring container cgroup limits), or `None` when
/// `/proc/meminfo` cannot be read.
#[must_use]
pub fn available_memory_bytes(host: &HostProfile) -> Option<u64> {
    memory::usable(&Root::system(), host.platform.kind())
}
