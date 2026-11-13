//! The CPU test items: CPU 性能 (`cpu.sysbench`), sysbench's CPU test with one thread and with
//! every usable thread, as the owner's `linux_bench` runs it; and Geekbench 7 (`cpu.geekbench`).

pub mod geekbench;
pub mod sysbench;

use std::num::NonZeroUsize;

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
