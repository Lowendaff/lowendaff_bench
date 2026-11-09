//! Memory for Geekbench (design cpu, §7.1): what it is estimated to need, what the host has
//! for it, and how much temporary swap it gets (CP-06).

use std::num::NonZeroUsize;
use std::path::Path;

use bench_api::PlatformKind;
use bench_host::Root;
use bench_host::cpu;
use bench_host::memory;
use bench_host::memory::CgroupLimits;
use bench_host::memory::Memory;
use bench_host::storage;

/// One GiB, in bytes.
pub const GIB: u64 = 1 << 30;

/// The most temporary swap, in GiB.
pub const MAX_SWAP_GIB: u64 = 8;

/// What the host offers Geekbench.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    /// Threads Geekbench runs, counted as for CPU 性能 (design cpu, §5.2).
    pub threads: usize,
    /// Bytes of memory and swap Geekbench can use now, or `None` when unknown.
    pub available: Option<u64>,
    /// Free bytes on the work directory's filesystem, or `None` when unknown.
    pub free_space: Option<u64>,
    /// Whether the kernel would leave swap unused: see [`swap_unused`].
    pub swap_unused: bool,
}

impl Host {
    /// Reads the host under `root` on a platform of `kind`, with `work_dir` as the work
    /// directory.
    #[must_use]
    pub fn read(root: &Root, kind: PlatformKind, work_dir: &Path) -> Self {
        let threads = cpu::usable_threads(root, kind)
            .or_else(|| {
                std::thread::available_parallelism()
                    .ok()
                    .map(NonZeroUsize::get)
            })
            .unwrap_or(1);
        let limits = (kind == PlatformKind::Container).then(|| memory::cgroup_limits(root));
        Self {
            threads,
            available: memory::read(root).map(|memory| available(&memory, limits.as_ref())),
            free_space: work_dir
                .to_str()
                .and_then(storage::statvfs)
                .map(|space| space.available),
            swap_unused: swap_unused(root),
        }
    }
}

/// Whether the kernel swaps out no process memory at all: with `vm.swappiness` 0, the multi-gen
/// LRU reclaims page cache only (Linux 6.12 `isolate_folios`), so added swap would stay unused,
/// as on the owner's VM (design cpu, §7.1; CP-18). The classic LRU still swaps to prevent OOM
/// with swappiness 0, so without the multi-gen LRU swap is worth adding.
#[must_use]
pub fn swap_unused(root: &Root) -> bool {
    let swappiness_zero = root
        .read_line("/proc/sys/vm/swappiness")
        .and_then(|value| value.parse::<u32>().ok())
        == Some(0);
    // `enabled` holds a bit mask such as `0x0007`; bit 0 is the main switch.
    let multi_gen_lru = root
        .read_line("/sys/kernel/mm/lru_gen/enabled")
        .and_then(|mask| u32::from_str_radix(mask.trim_start_matches("0x"), 16).ok())
        .is_some_and(|mask| mask & 1 != 0);
    swappiness_zero && multi_gen_lru
}

/// Bytes Geekbench is estimated to need with `threads` threads: 1 GiB, and 1 GiB per thread.
/// The runners measured 3.6 GiB with 4 threads and about 1.3 GiB with one (design cpu, §3.2).
#[must_use]
pub fn need(threads: usize) -> u64 {
    (1 + threads as u64) * GIB
}

/// Available memory and free swap; in a cgroup with a memory limit, at most the limit minus
/// the usage.
#[must_use]
pub fn available(memory: &Memory, limits: Option<&CgroupLimits>) -> u64 {
    let host = memory.available + memory.swap_free;
    let left = limits.and_then(|limits| Some(limits.memory?.saturating_sub(limits.memory_used?)));
    left.map_or(host, |left| host.min(left))
}

/// The temporary swap Geekbench gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// There is memory enough: no swap.
    Enough,
    /// Swap of this many GiB.
    Swap(u64),
    /// Memory is short, but the work directory's filesystem cannot spare 1 GiB of swap and
    /// still keep 1 GiB free.
    NoRoom,
    /// Memory is short, but the kernel would not use swap ([`swap_unused`]).
    Unused,
}

/// The swap for `need` bytes when `available` bytes are there: none when the kernel would not
/// use it, else the shortfall rounded up to whole GiB, at most [`MAX_SWAP_GIB`], and at most
/// the free space but 1 GiB, rounded down.
#[must_use]
pub fn plan(need: u64, available: u64, free_space: Option<u64>, swap_unused: bool) -> Plan {
    let shortfall = need.saturating_sub(available);
    if shortfall == 0 {
        return Plan::Enough;
    }
    if swap_unused {
        return Plan::Unused;
    }
    let wanted = shortfall.div_ceil(GIB).min(MAX_SWAP_GIB);
    let room = free_space.map_or(0, |free| free.saturating_sub(GIB) / GIB);
    match wanted.min(room) {
        0 => Plan::NoRoom,
        gib => Plan::Swap(gib),
    }
}
