//! The estimate, the available memory and the temporary swap.

use bench_host::Root;
use bench_host::memory::CgroupLimits;
use bench_host::memory::Memory;
use plugin_geekbench::memory;
use plugin_geekbench::memory::GIB;
use plugin_geekbench::memory::Plan;

const MIB: u64 = 1 << 20;

fn host(available: u64, swap_free: u64) -> Memory {
    Memory {
        total: 64 * GIB,
        available,
        estimated: false,
        swap_total: swap_free,
        swap_free,
    }
}

#[test]
fn geekbench_needs_one_gib_and_one_gib_per_thread() {
    assert_eq!(memory::need(1), 2 * GIB);
    assert_eq!(memory::need(4), 5 * GIB, "the runners used 3.6 GiB");
    assert_eq!(memory::need(64), 65 * GIB, "the owner's dedicated server");
}

#[test]
fn available_memory_counts_free_swap_and_a_containers_limit() {
    assert_eq!(
        memory::available(&host(GIB, 512 * MIB), None),
        GIB + 512 * MIB
    );
    let limited = CgroupLimits {
        memory: Some(2 * GIB),
        memory_used: Some(GIB + 512 * MIB),
        ..CgroupLimits::default()
    };
    assert_eq!(
        memory::available(&host(8 * GIB, 0), Some(&limited)),
        512 * MIB,
        "the limit minus the usage"
    );
    let unlimited = CgroupLimits::default();
    assert_eq!(
        memory::available(&host(8 * GIB, 0), Some(&unlimited)),
        8 * GIB
    );
}

#[test]
fn enough_memory_needs_no_swap() {
    assert_eq!(
        memory::plan(5 * GIB, 6 * GIB, Some(100 * GIB), false),
        Plan::Enough
    );
    assert_eq!(memory::plan(5 * GIB, 5 * GIB, None, false), Plan::Enough);
}

#[test]
fn the_shortfall_is_rounded_up_to_whole_gib_and_capped_at_eight() {
    // The ARM board: 4 threads and 1.69 GiB available.
    assert_eq!(
        memory::plan(5 * GIB, 1730 * MIB, Some(100 * GIB), false),
        Plan::Swap(4)
    );
    assert_eq!(
        memory::plan(2 * GIB, 2 * GIB - 1, Some(100 * GIB), false),
        Plan::Swap(1)
    );
    assert_eq!(
        memory::plan(65 * GIB, 4 * GIB, Some(100 * GIB), false),
        Plan::Swap(8)
    );
}

#[test]
fn the_disk_keeps_one_gib_free() {
    assert_eq!(
        memory::plan(5 * GIB, GIB, Some(3 * GIB + 512 * MIB), false),
        Plan::Swap(2),
        "3.5 GiB free: 2 whole GiB fit beside 1 GiB kept free"
    );
    assert_eq!(
        memory::plan(5 * GIB, GIB, Some(2 * GIB - 1), false),
        Plan::NoRoom
    );
    assert_eq!(
        memory::plan(5 * GIB, GIB, None, false),
        Plan::NoRoom,
        "unknown free space"
    );
}

#[test]
fn no_swap_is_added_where_the_kernel_would_not_use_it() {
    assert_eq!(
        memory::plan(2 * GIB, 600 * MIB, Some(7 * GIB), true),
        Plan::Unused,
        "the owner's VM: swappiness 0 under the multi-gen LRU"
    );
    assert_eq!(
        memory::plan(2 * GIB, 3 * GIB, Some(7 * GIB), true),
        Plan::Enough,
        "enough memory needs no note"
    );
}

/// A host with these `vm.swappiness` and multi-gen LRU switches; `None` leaves a file out.
fn kernel(swappiness: Option<&str>, lru_gen: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (path, value) in [
        ("proc/sys/vm/swappiness", swappiness),
        ("sys/kernel/mm/lru_gen/enabled", lru_gen),
    ] {
        if let Some(value) = value {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
            std::fs::write(path, format!("{value}\n")).expect("the file");
        }
    }
    dir
}

#[test]
fn swap_goes_unused_with_swappiness_zero_under_the_multi_gen_lru_only() {
    let unused = |swappiness, lru_gen| {
        let dir = kernel(swappiness, lru_gen);
        memory::swap_unused(&Root::new(dir.path()))
    };
    assert!(unused(Some("0"), Some("0x0007")), "the owner's VM");
    assert!(unused(Some("0"), Some("0x0001")), "the main switch alone");
    assert!(
        !unused(Some("60"), Some("0x0007")),
        "the server and the ARM board"
    );
    assert!(
        !unused(Some("0"), Some("0x0006")),
        "the multi-gen LRU is off without its main switch"
    );
    assert!(
        !unused(Some("0"), None),
        "the classic LRU swaps to prevent OOM even with swappiness 0"
    );
    assert!(!unused(None, Some("0x0007")), "an unknown swappiness");
}
