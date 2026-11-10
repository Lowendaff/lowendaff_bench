//! The working sets, STREAM's size, the latency buffers and the NUMA matrix (design memory,
//! §5.2–§5.6), for the owner's captured hosts and for made-up ones.

use std::path::Path;

use bench_api::PlatformKind;
use bench_host::Root;
use bench_host::cache::Cache;
use bench_host::cache::CacheKind;
use bench_host::numa::Node;
use plugin_memory::plan::Cell;
use plugin_memory::plan::GIB;
use plugin_memory::plan::Host;
use plugin_memory::plan::KIB;
use plugin_memory::plan::Level;
use plugin_memory::plan::MIB;
use plugin_memory::plan::Matrix;
use plugin_memory::plan::Plan;
use plugin_memory::plan::Row;

/// A host captured by bench-host's `capture.sh`.
fn captured(name: &str, kind: PlatformKind) -> Host {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/bench-host/tests/fixtures")
        .join(name)
        .join("root");
    Host::read(&Root::new(root), kind).expect("the capture has /proc/meminfo")
}

/// A host with `threads` CPUs sharing one last-level cache of `cache` bytes, one node, and
/// `available` bytes of memory.
fn host(cache: u64, available: u64, threads: usize) -> Host {
    let cpus: Vec<usize> = (0..threads).collect();
    Host {
        available,
        threads,
        caches: vec![unified(3, cache, &cpus)],
        nodes: vec![node(0, &cpus, Some(available))],
        cpus: Some(cpus),
        mems: None,
    }
}

fn unified(level: u32, size: u64, cpus: &[usize]) -> Cache {
    Cache {
        level,
        kind: CacheKind::Unified,
        size,
        cpus: cpus.to_vec(),
    }
}

/// A node with `cpus`, and memory of which `free` bytes are free; `None` means no memory.
fn node(id: usize, cpus: &[usize], free: Option<u64>) -> Node {
    Node {
        id,
        cpus: cpus.to_vec(),
        total: free.map(|free| 2 * free),
        free,
    }
}

/// The CPUs of one Xeon Gold 6130 of the owner's server.
fn socket(first: usize, sibling: usize) -> Vec<usize> {
    (first..first + 16).chain(sibling..sibling + 16).collect()
}

fn level(name: &'static str, buffer: u64) -> Level {
    Level { name, buffer }
}

#[test]
fn the_owners_server_measures_beyond_its_two_l3_caches() {
    let plan = Plan::new(&captured("dedicated-pve-x86_64", PlatformKind::Dedicated));
    assert_eq!(plan.threads, 64);
    assert_eq!(plan.cache, 44 * MIB, "two L3 caches of 22 MiB");
    assert!(!plan.cache_unknown);
    assert_eq!(plan.budget, 144_689_140 * KIB / 2, "half of MemAvailable");
    assert_eq!(
        plan.single,
        256 * MIB,
        "the smallest power of two of at least 176 MiB"
    );
    assert_eq!(
        plan.per_thread,
        4 * MIB,
        "176 MiB over 64 threads is 2.75 MiB"
    );
    assert_eq!(plan.stream, Some(32), "the first array of at least 176 MiB");
    assert_eq!(
        plan.levels,
        [
            level("L1", 16 * KIB),
            level("L2", 512 * KIB),
            level("L3", 11 * MIB),
            level("内存", 256 * MIB),
        ]
    );
    assert!(plan.missing.is_empty());
    // Each node has one L3 of 22 MiB: 88 MiB over 32 threads, or in one buffer.
    let cell = Cell {
        per_thread: 4 * MIB,
        latency: 128 * MIB,
    };
    assert_eq!(
        plan.matrix,
        Some(Matrix {
            rows: vec![
                Row {
                    node: 0,
                    cpus: socket(0, 32),
                },
                Row {
                    node: 1,
                    cpus: socket(16, 48),
                },
            ],
            columns: vec![0, 1],
            cells: vec![vec![cell; 2]; 2],
        })
    );
    assert_eq!(plan.columns(), [1, 64]);
}

#[test]
fn the_owners_vm_runs_a_smaller_stream_than_its_caches_ask() {
    let plan = Plan::new(&captured("kvm-debian13-x86_64", PlatformKind::Vm));
    assert_eq!(plan.threads, 1);
    assert_eq!(plan.columns(), [1]);
    assert_eq!(plan.cache, 16 * MIB, "QEMU's L3");
    assert_eq!(plan.budget, 577_800 * KIB / 2);
    assert_eq!(plan.single, 64 * MIB);
    // An array of 122 MiB would be the first of at least 64 MiB, but three need 366 MiB.
    assert_eq!(plan.stream, Some(8));
    assert_eq!(
        plan.levels,
        [
            level("L1", 16 * KIB),
            level("L2", 2 * MIB),
            level("L3", 8 * MIB),
            level("内存", 64 * MIB),
        ]
    );
    assert_eq!(plan.matrix, None, "one node");
}

#[test]
fn the_owners_board_has_no_l3_and_shares_its_l2() {
    let plan = Plan::new(&captured(
        "rock3a-debian13-aarch64",
        PlatformKind::Dedicated,
    ));
    assert_eq!(plan.threads, 4);
    assert_eq!(
        plan.cache,
        512 * KIB,
        "the L2 the four cores share is the last level"
    );
    assert_eq!(plan.single, 2 * MIB);
    assert_eq!(plan.per_thread, MIB, "512 KiB per thread, raised to 1 MiB");
    assert_eq!(plan.stream, Some(2));
    assert_eq!(
        plan.levels,
        [
            level("L1", 16 * KIB),
            level("L2", 256 * KIB),
            level("内存", 2 * MIB),
        ]
    );
    assert_eq!(plan.missing, ["L3"]);
    assert_eq!(plan.matrix, None);
}

#[test]
fn unknown_caches_count_as_32_mib() {
    let mut host = host(MIB, 8 * GIB, 2);
    host.caches.clear();
    let plan = Plan::new(&host);
    assert!(plan.cache_unknown);
    assert_eq!(plan.cache, 32 * MIB);
    assert_eq!(plan.single, 128 * MIB);
    assert_eq!(plan.levels, [level("内存", 128 * MIB)]);
    assert_eq!(plan.missing, ["L1", "L2", "L3"]);

    // Instruction caches alone say nothing of data.
    host.caches = vec![Cache {
        level: 2,
        kind: CacheKind::Instruction,
        size: 64 * MIB,
        cpus: vec![0, 1],
    }];
    let plan = Plan::new(&host);
    assert!(plan.cache_unknown);
    assert_eq!(plan.missing, ["L1", "L2", "L3"], "no latency level either");

    // Caches the kernel gives no size are as good as none.
    host.caches = vec![unified(3, 0, &[0, 1])];
    assert!(Plan::new(&host).cache_unknown);
}

#[test]
fn every_instance_of_the_last_level_counts() {
    let mut host = host(8 * MIB, 64 * GIB, 8);
    host.caches = vec![
        unified(2, MIB, &[0]),
        unified(3, 8 * MIB, &[0, 1, 2, 3]),
        unified(3, 8 * MIB, &[4, 5, 6, 7]),
    ];
    let plan = Plan::new(&host);
    assert_eq!(plan.cache, 16 * MIB);
    assert_eq!(plan.single, 64 * MIB);
    assert_eq!(plan.per_thread, 8 * MIB, "64 MiB over 8 threads");
    assert_eq!(
        plan.levels,
        [
            level("L2", 512 * KIB),
            level("L3", 4 * MIB),
            level("内存", 64 * MIB),
        ],
        "half of one instance of each cache level"
    );
    assert_eq!(plan.missing, ["L1"]);
}

#[test]
fn stream_takes_the_first_size_beyond_four_times_the_caches_that_fits() {
    for (cache, available, millions) in [
        (MIB, 64 * GIB, Some(2)),
        // An array of exactly four times the caches meets the rule.
        (16_000_000, 64 * GIB, Some(8)),
        // 16 MB is below 16 MiB.
        (4 * MIB, 64 * GIB, Some(4)),
        (44 * MIB, 64 * GIB, Some(32)),
        (100 * MIB, 64 * GIB, Some(64)),
        // No array reaches 800 MiB: the largest, with a note.
        (200 * MIB, 64 * GIB, Some(64)),
        // Three arrays of 32 million take 768 MB; one byte less halves them.
        (44 * MIB, 2 * 768_000_000, Some(32)),
        (44 * MIB, 2 * 768_000_000 - 2, Some(16)),
        // The smallest takes 48 MB.
        (44 * MIB, 2 * 48_000_000, Some(2)),
        (44 * MIB, 2 * 48_000_000 - 2, None),
    ] {
        let plan = Plan::new(&host(cache, available, 4));
        assert_eq!(
            plan.stream, millions,
            "{cache} bytes of cache, {available} available"
        );
    }
}

#[test]
fn buffers_shrink_to_half_the_available_memory() {
    // A container of 64 MiB on a server: everything halves to 32 MiB.
    let plan = Plan::new(&host(44 * MIB, 64 * MIB, 4));
    assert_eq!(plan.budget, 32 * MIB);
    assert_eq!(plan.single, 32 * MIB);
    assert_eq!(plan.per_thread, 8 * MIB, "four buffers in 32 MiB");
    assert_eq!(plan.stream, None, "the smallest STREAM needs 46 MiB");
    assert_eq!(plan.levels.last(), Some(&level("内存", 32 * MIB)));

    // Many threads keep 1 MiB each while they fit.
    let plan = Plan::new(&host(44 * MIB, 64 * GIB, 256));
    assert_eq!(
        plan.per_thread, MIB,
        "176 MiB over 256 threads is below 1 MiB"
    );
    let plan = Plan::new(&host(44 * MIB, 256 * MIB, 256));
    assert_eq!(
        plan.per_thread,
        512 * KIB,
        "256 buffers of 1 MiB would not fit in 128 MiB"
    );

    // No buffer goes below a page.
    let plan = Plan::new(&host(44 * MIB, 0, 4));
    assert_eq!((plan.single, plan.per_thread), (4 * KIB, 4 * KIB));
}

#[test]
fn the_matrix_has_a_row_per_node_the_threads_may_use_and_a_column_per_node_of_memory() {
    let both = Host {
        available: 64 * GIB,
        threads: 4,
        caches: vec![unified(3, 8 * MIB, &[0, 1]), unified(3, 8 * MIB, &[2, 3])],
        nodes: vec![
            node(0, &[0, 1], Some(16 * GIB)),
            node(1, &[2, 3], Some(16 * GIB)),
        ],
        cpus: Some(vec![0, 1, 2, 3]),
        mems: Some(vec![0, 1]),
    };
    let matrix = Plan::new(&both).matrix.expect("two nodes");
    let rows: Vec<usize> = matrix.rows.iter().map(|row| row.node).collect();
    assert_eq!((rows, matrix.columns.clone()), (vec![0, 1], vec![0, 1]));
    assert_eq!(
        matrix.cells[0][1],
        Cell {
            per_thread: 16 * MIB,
            latency: 32 * MIB,
        },
        "each node's own L3: 32 MiB over 2 threads, or in one buffer"
    );

    // A container on node 0's CPUs: one row, still both columns.
    let mut pinned = both.clone();
    pinned.cpus = Some(vec![1]);
    let matrix = Plan::new(&pinned).matrix.expect("one row, two columns");
    assert_eq!(
        matrix.rows,
        [Row {
            node: 0,
            cpus: vec![1],
        }]
    );
    assert_eq!(matrix.columns, [0, 1]);
    assert_eq!(matrix.cells[0][0].per_thread, 32 * MIB, "one thread");

    // A node of memory only is a column; a node without memory is no column.
    let mut cxl = both.clone();
    cxl.nodes.push(node(2, &[], Some(64 * GIB)));
    cxl.nodes[1].total = Some(0);
    cxl.mems = Some(vec![0, 1, 2]);
    let matrix = Plan::new(&cxl).matrix.expect("two rows, two columns");
    let rows: Vec<usize> = matrix.rows.iter().map(|row| row.node).collect();
    assert_eq!((rows, matrix.columns), (vec![0, 1], vec![0, 2]));

    // A node the process may not allocate on is no column.
    let mut bound = both.clone();
    bound.mems = Some(vec![1]);
    assert_eq!(
        Plan::new(&bound).matrix.map(|matrix| matrix.columns),
        Some(vec![1])
    );

    // One cell, or unknown CPUs: no matrix.
    let mut single = both.clone();
    single.cpus = Some(vec![0]);
    single.mems = Some(vec![0]);
    assert_eq!(Plan::new(&single).matrix, None);
    let mut unknown = both;
    unknown.cpus = None;
    assert_eq!(Plan::new(&unknown).matrix, None);
}

#[test]
fn a_node_whose_cpus_share_no_known_cache_counts_all_of_them() {
    let host = Host {
        available: 64 * GIB,
        threads: 4,
        caches: vec![unified(3, 8 * MIB, &[0, 1])],
        nodes: vec![
            node(0, &[0, 1], Some(16 * GIB)),
            node(1, &[2, 3], Some(16 * GIB)),
        ],
        cpus: Some(vec![0, 1, 2, 3]),
        mems: None,
    };
    let matrix = Plan::new(&host).matrix.expect("two nodes");
    assert_eq!(
        matrix.cells[1][0],
        Cell {
            per_thread: 16 * MIB,
            latency: 32 * MIB,
        },
        "node 1's CPUs share no L3 the kernel describes, so C counts"
    );
}

#[test]
fn a_cell_fits_in_half_of_its_nodes_free_memory() {
    let host = Host {
        available: 64 * GIB,
        threads: 4,
        // One L3 shared across both nodes, as with sub-NUMA clustering.
        caches: vec![unified(3, 32 * MIB, &[0, 1, 2, 3])],
        nodes: vec![
            node(0, &[0, 1], Some(16 * GIB)),
            node(1, &[2, 3], Some(40 * MIB)),
        ],
        cpus: Some(vec![0, 1, 2, 3]),
        mems: None,
    };
    let matrix = Plan::new(&host).matrix.expect("two nodes");
    assert_eq!(
        matrix.cells[0],
        [
            Cell {
                per_thread: 64 * MIB,
                latency: 128 * MIB,
            },
            Cell {
                per_thread: 8 * MIB,
                latency: 16 * MIB,
            },
        ],
        "the shared L3 serves both nodes; node 1 has 20 MiB to spare"
    );
}
