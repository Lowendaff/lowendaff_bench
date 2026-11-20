//! Unified CPU benchmark plugin (`cpu.bench`) and its eight selectable sub-engines:
//! `sysbench`, `7zip` (`seven_zip`), `unixbench`, `hpl`, `passmark`, `ycruncher`, `prime95`, and
//! `geekbench`.

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
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;

/// The CPUs a test can use at once, as the host profile counts them; the scheduler's count when
/// the profile has none, and one at least.
#[must_use]
pub fn usable_threads(host: &HostProfile) -> usize {
    let usable = match &host.cpu {
        Read::Done(facts) => facts.usable,
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
    let root = Root::system();
    let limits =
        (host.platform.kind() == PlatformKind::Container).then(|| memory::cgroup_limits(&root));
    let info = memory::read(&root)?;
    let ram = info.available;
    let cgroup_left = limits
        .as_ref()
        .and_then(|lim| Some(lim.memory?.saturating_sub(lim.memory_used?)));
    Some(cgroup_left.map_or(ram, |left| ram.min(left)))
}
