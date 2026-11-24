//! Memory for Geekbench: what it is estimated to need, what the host has for it, how much
//! temporary swap it gets, and the kernel's count of processes killed for lack of memory.

use std::path::Path;

use bench_host::Root;
use bench_host::memory;
use bench_host::memory::CgroupLimits;
use bench_host::memory::Memory;
use bench_host::platform::PlatformKind;
use bench_host::storage;

/// One GiB, in bytes.
pub const GIB: u64 = 1 << 30;

/// The most temporary swap, in GiB.
pub const MAX_SWAP_GIB: u64 = 8;

/// What the host offers Geekbench.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    /// Threads Geekbench runs, counted as for CPU performance.
    pub threads: usize,
    /// Bytes of memory and swap Geekbench can use now, or `None` when unknown.
    pub available: Option<u64>,
    /// Free bytes on the work directory's filesystem, or `None` when unknown.
    pub free_space: Option<u64>,
    /// Whether the kernel would leave swap unused: see [`swap_unused`].
    pub swap_unused: bool,
}

impl Host {
    /// Reads the host under `root` on a platform of `kind`, for Geekbench with `threads`
    /// threads and `work_dir` as the work directory. Memory and free space change during a run,
    /// so they are read now rather than taken from the host profile.
    #[must_use]
    pub fn read(root: &Root, threads: usize, kind: PlatformKind, work_dir: &Path) -> Self {
        let limits = (kind == PlatformKind::Container).then(|| memory::cgroup_limits(root));
        Self {
            threads,
            available: memory::read(root).map(|memory| available(&memory, limits.as_ref())),
            free_space: storage::statvfs_path(work_dir).map(|space| space.available),
            swap_unused: swap_unused(root),
        }
    }
}

/// Whether the kernel swaps out no process memory at all: with `vm.swappiness` 0, the multi-gen
/// LRU reclaims page cache only (Linux 6.12 `isolate_folios`), so added swap would stay unused,
/// as on the owner's VM. The classic LRU still swaps to prevent OOM
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
/// The runners measured 3.6 GiB with 4 threads and about 1.3 GiB with one.
#[must_use]
pub fn need(threads: usize) -> u64 {
    u64::try_from(threads)
        .unwrap_or(u64::MAX)
        .saturating_add(1)
        .saturating_mul(GIB)
}

/// Available memory and free swap, capping RAM and swap independently against cgroup limits when
/// present.
#[must_use]
pub fn available(memory: &Memory, limits: Option<&CgroupLimits>) -> u64 {
    let ram = match limits.and_then(|lim| lim.memory) {
        Some(max) => {
            let used = limits.and_then(|lim| lim.memory_used).unwrap_or(0);
            memory.available.min(max.saturating_sub(used))
        }
        None => memory.available,
    };
    let swap = match limits.and_then(|lim| lim.swap) {
        Some(max) => {
            let used = limits.and_then(|lim| lim.swap_used).unwrap_or(0);
            memory.swap_free.min(max.saturating_sub(used))
        }
        None => memory.swap_free,
    };
    ram.saturating_add(swap)
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

/// The kernel's count of processes killed for lack of memory, from `/proc/vmstat`; `None` before
/// Linux 4.13, which does not count them.
#[must_use]
pub fn oom_kills(root: &Root) -> Option<u64> {
    root.read("/proc/vmstat")?
        .lines()
        .find_map(|line| line.strip_prefix("oom_kill "))?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use bench_host::memory::Memory;

    #[test]
    fn need_and_available_saturate_on_extreme_inputs() {
        assert_eq!(super::need(usize::MAX), u64::MAX);
        let mem = Memory {
            total: u64::MAX,
            available: u64::MAX,
            estimated: false,
            swap_total: u64::MAX,
            swap_free: u64::MAX,
        };
        assert_eq!(super::available(&mem, None), u64::MAX);
    }
}
