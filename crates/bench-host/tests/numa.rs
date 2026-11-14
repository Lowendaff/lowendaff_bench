//! The host facts of 内存性能: the cache instances, the NUMA nodes, the
//! CPUs and nodes this process may use, the NUMA layout a test between the nodes can use, and
//! the memory a test may use, from the captured hosts and from synthetic ones.

mod common;

use bench_host::Root;
use bench_host::cache;
use bench_host::cache::Cache;
use bench_host::cache::CacheKind;
use bench_host::cpu;
use bench_host::memory;
use bench_host::numa;
use bench_host::numa::Layout;
use bench_host::numa::Node;
use bench_host::numa::Threads;
use bench_host::platform::PlatformKind;

const KIB: u64 = 1024;

/// The CPUs of one Xeon Gold 6130 of the owner's server: 16 cores from `first`, and their
/// second threads from `sibling`.
fn socket(first: usize, sibling: usize) -> Vec<usize> {
    (first..first + 16).chain(sibling..sibling + 16).collect()
}

#[test]
fn every_cache_instance_counts_once() {
    let server = cache::read(&common::fixture("dedicated-pve-x86_64"));
    let count = |level: u32, kind: CacheKind| {
        server
            .iter()
            .filter(|cache| cache.level == level && cache.kind == kind)
            .count()
    };
    assert_eq!(count(1, CacheKind::Data), 32, "one L1d per core");
    assert_eq!(count(1, CacheKind::Instruction), 32, "one L1i per core");
    assert_eq!(count(2, CacheKind::Unified), 32, "one L2 per core");
    let l3: Vec<(u64, &[usize])> = server
        .iter()
        .filter(|cache| cache.level == 3)
        .map(|cache| (cache.size, cache.cpus.as_slice()))
        .collect();
    assert_eq!(
        l3,
        [
            (22528 * KIB, socket(0, 32).as_slice()),
            (22528 * KIB, socket(16, 48).as_slice()),
        ],
        "one L3 per socket"
    );

    // The board's last level is an L2 its four cores share.
    let board = cache::read(&common::fixture("rock3a-debian13-aarch64"));
    assert_eq!(board.len(), 9, "{board:?}");
    assert_eq!(
        board.last(),
        Some(&Cache {
            level: 2,
            kind: CacheKind::Unified,
            size: 512 * KIB,
            cpus: vec![0, 1, 2, 3],
        })
    );

    // QEMU's made-up caches for the VM's single CPU.
    let vm = cache::read(&common::fixture("kvm-debian13-x86_64"));
    let sizes: Vec<(u32, CacheKind, u64)> = vm
        .iter()
        .map(|cache| (cache.level, cache.kind, cache.size))
        .collect();
    assert_eq!(
        sizes,
        [
            (1, CacheKind::Data, 32 * KIB),
            (1, CacheKind::Instruction, 32 * KIB),
            (2, CacheKind::Unified, 4096 * KIB),
            (3, CacheKind::Unified, 16384 * KIB),
        ]
    );
    assert!(vm.iter().all(|cache| cache.cpus == [0]), "{vm:?}");
}

#[test]
fn caches_without_their_sharing_or_a_known_kind_are_left_out() {
    let host = common::host((
        &[
            // An L3 both CPUs share, seen from each of them.
            ("/sys/devices/system/cpu/cpu0/cache/index3/level", "3\n"),
            (
                "/sys/devices/system/cpu/cpu0/cache/index3/type",
                "Unified\n",
            ),
            ("/sys/devices/system/cpu/cpu0/cache/index3/size", "8192K\n"),
            (
                "/sys/devices/system/cpu/cpu0/cache/index3/shared_cpu_list",
                "0-1\n",
            ),
            ("/sys/devices/system/cpu/cpu1/cache/index3/level", "3\n"),
            (
                "/sys/devices/system/cpu/cpu1/cache/index3/type",
                "Unified\n",
            ),
            ("/sys/devices/system/cpu/cpu1/cache/index3/size", "8192K\n"),
            (
                "/sys/devices/system/cpu/cpu1/cache/index3/shared_cpu_list",
                "0-1\n",
            ),
            // An L1d of each CPU.
            ("/sys/devices/system/cpu/cpu0/cache/index0/level", "1\n"),
            ("/sys/devices/system/cpu/cpu0/cache/index0/type", "Data\n"),
            ("/sys/devices/system/cpu/cpu0/cache/index0/size", "48K\n"),
            (
                "/sys/devices/system/cpu/cpu0/cache/index0/shared_cpu_list",
                "0\n",
            ),
            ("/sys/devices/system/cpu/cpu1/cache/index0/level", "1\n"),
            ("/sys/devices/system/cpu/cpu1/cache/index0/type", "Data\n"),
            ("/sys/devices/system/cpu/cpu1/cache/index0/size", "48K\n"),
            (
                "/sys/devices/system/cpu/cpu1/cache/index0/shared_cpu_list",
                "1\n",
            ),
            // No sharing given.
            ("/sys/devices/system/cpu/cpu0/cache/index2/level", "2\n"),
            (
                "/sys/devices/system/cpu/cpu0/cache/index2/type",
                "Unified\n",
            ),
            ("/sys/devices/system/cpu/cpu0/cache/index2/size", "2048K\n"),
            // A kind the kernel does not have.
            ("/sys/devices/system/cpu/cpu1/cache/index2/level", "2\n"),
            ("/sys/devices/system/cpu/cpu1/cache/index2/type", "Trace\n"),
            ("/sys/devices/system/cpu/cpu1/cache/index2/size", "2048K\n"),
            (
                "/sys/devices/system/cpu/cpu1/cache/index2/shared_cpu_list",
                "1\n",
            ),
            // A list that runs backwards.
            ("/sys/devices/system/cpu/cpu1/cache/index1/level", "1\n"),
            (
                "/sys/devices/system/cpu/cpu1/cache/index1/type",
                "Instruction\n",
            ),
            ("/sys/devices/system/cpu/cpu1/cache/index1/size", "32K\n"),
            (
                "/sys/devices/system/cpu/cpu1/cache/index1/shared_cpu_list",
                "1-0\n",
            ),
            // Not a CPU.
            ("/sys/devices/system/cpu/cpufreq/cache/index0/level", "1\n"),
            (
                "/sys/devices/system/cpu/cpufreq/cache/index0/type",
                "Data\n",
            ),
            ("/sys/devices/system/cpu/cpufreq/cache/index0/size", "16K\n"),
            (
                "/sys/devices/system/cpu/cpufreq/cache/index0/shared_cpu_list",
                "0\n",
            ),
        ],
        &[],
    ));
    let caches = cache::read(&Root::new(host.path()));
    let l1d = |cpu: usize| Cache {
        level: 1,
        kind: CacheKind::Data,
        size: 48 * KIB,
        cpus: vec![cpu],
    };
    let l3 = Cache {
        level: 3,
        kind: CacheKind::Unified,
        size: 8192 * KIB,
        cpus: vec![0, 1],
    };
    assert_eq!(caches, [l1d(0), l1d(1), l3]);
}

#[test]
fn numa_nodes_hold_their_cpus_and_memory() {
    assert_eq!(
        numa::read(&common::fixture("dedicated-pve-x86_64")),
        [
            Node {
                id: 0,
                cpus: socket(0, 32),
                total: Some(65_618_876 * KIB),
                free: Some(31_148_640 * KIB),
            },
            Node {
                id: 1,
                cpus: socket(16, 48),
                total: Some(132_059_544 * KIB),
                free: Some(54_673_808 * KIB),
            },
        ]
    );
    assert_eq!(
        numa::read(&common::fixture("kvm-debian13-x86_64")),
        [Node {
            id: 0,
            cpus: vec![0],
            total: Some(951_384 * KIB),
            free: Some(109_756 * KIB),
        }]
    );
    assert_eq!(
        numa::read(&common::fixture("rock3a-debian13-aarch64")),
        [Node {
            id: 0,
            cpus: vec![0, 1, 2, 3],
            total: Some(2_016_584 * KIB),
            free: Some(1_646_592 * KIB),
        }]
    );
    assert!(
        numa::read(&common::fixture("docker-debian13-aarch64")).is_empty(),
        "captured before capture.sh read the nodes"
    );
}

#[test]
fn a_node_of_memory_only_has_no_cpus_and_nodes_sort_by_number() {
    let host = common::host((
        &[
            ("/sys/devices/system/node/online", "0,2,10\n"),
            ("/sys/devices/system/node/has_memory", "0,10\n"),
            ("/sys/devices/system/node/node0/cpulist", "0-3\n"),
            (
                "/sys/devices/system/node/node0/meminfo",
                "Node 0 MemTotal:        1024 kB\nNode 0 MemFree:          512 kB\n",
            ),
            ("/sys/devices/system/node/node10/cpulist", "\n"),
            (
                "/sys/devices/system/node/node10/meminfo",
                "Node 10 MemTotal:       2048 kB\nNode 10 MemFree:        2000 kB\n",
            ),
            ("/sys/devices/system/node/node2/cpulist", "4-5\n"),
        ],
        &[],
    ));
    assert_eq!(
        numa::read(&Root::new(host.path())),
        [
            Node {
                id: 0,
                cpus: vec![0, 1, 2, 3],
                total: Some(1024 * KIB),
                free: Some(512 * KIB),
            },
            Node {
                id: 2,
                cpus: vec![4, 5],
                total: None,
                free: None,
            },
            Node {
                id: 10,
                cpus: Vec::new(),
                total: Some(2048 * KIB),
                free: Some(2000 * KIB),
            },
        ]
    );
}

#[test]
fn the_cpus_and_nodes_a_process_may_use_come_from_its_status() {
    for (name, cpus, nodes) in [
        ("dedicated-pve-x86_64", (0..64).collect(), vec![0, 1]),
        ("kvm-debian13-x86_64", vec![0], vec![0]),
        ("rock3a-debian13-aarch64", vec![0, 1, 2, 3], vec![0]),
    ] {
        let root = common::fixture(name);
        assert_eq!(cpu::allowed(&root), Some(cpus), "{name}");
        assert_eq!(numa::allowed(&root), Some(nodes), "{name}");
    }
    let docker = common::fixture("docker-debian13-aarch64");
    assert_eq!(cpu::allowed(&docker), None, "no status captured");
    assert_eq!(numa::allowed(&docker), None, "no status captured");

    let lists = |cpus: &str| {
        let host = common::host((&[], &[]));
        let status = format!("Name:\tsh\nCpus_allowed:\tff\nCpus_allowed_list:\t{cpus}\n");
        common::add_bytes(&host, "/proc/self/status", status.as_bytes());
        let root = Root::new(host.path());
        (cpu::allowed(&root), numa::allowed(&root))
    };
    assert_eq!(lists("0-3,8-9"), (Some(vec![0, 1, 2, 3, 8, 9]), None));
    assert_eq!(lists("3-1"), (None, None), "a range that runs backwards");
    assert_eq!(
        lists("0-65536"),
        (None, None),
        "more CPUs than any kernel has"
    );
    assert_eq!(lists("x"), (None, None));
}

#[test]
fn the_layout_holds_the_nodes_and_what_this_process_may_use() {
    let server = Layout::read(&common::fixture("dedicated-pve-x86_64"));
    assert_eq!(
        server.nodes,
        numa::read(&common::fixture("dedicated-pve-x86_64"))
    );
    assert_eq!(server.cpus, Some((0..64).collect()));
    assert_eq!(server.mems, Some(vec![0, 1]));
    assert_eq!(
        server.thread_nodes(),
        [
            Threads {
                node: 0,
                cpus: socket(0, 32),
            },
            Threads {
                node: 1,
                cpus: socket(16, 48),
            },
        ]
    );
    let memory: Vec<usize> = server.memory_nodes().iter().map(|node| node.id).collect();
    assert_eq!(memory, [0, 1]);
    assert_eq!(server.usable(), 2);

    for name in ["kvm-debian13-x86_64", "rock3a-debian13-aarch64"] {
        assert_eq!(Layout::read(&common::fixture(name)).usable(), 1, "{name}");
    }
    let docker = Layout::read(&common::fixture("docker-debian13-aarch64"));
    assert_eq!(docker, Layout::default(), "nothing captured");
    assert_eq!(docker.usable(), 0);
}

/// A node with `cpus`, and `total` bytes of memory when given.
fn node(id: usize, cpus: &[usize], total: Option<u64>) -> Node {
    Node {
        id,
        cpus: cpus.to_vec(),
        total,
        free: total,
    }
}

#[test]
fn usable_nodes_count_where_threads_and_memory_can_go_once() {
    let two = || vec![node(0, &[0, 1], Some(KIB)), node(1, &[2, 3], Some(KIB))];
    let layout = |nodes: Vec<Node>, cpus: Option<Vec<usize>>, mems: Option<Vec<usize>>| Layout {
        nodes,
        cpus,
        mems,
    };
    let all = Some(vec![0, 1, 2, 3]);
    assert_eq!(layout(two(), all.clone(), Some(vec![0, 1])).usable(), 2);
    assert_eq!(
        layout(two(), all.clone(), None).usable(),
        2,
        "every node with memory, when the allowed nodes are unknown"
    );

    // Threads on node 0 only, memory on both: a test can still reach node 1's memory.
    let one_cpuset = layout(two(), Some(vec![0, 1]), Some(vec![0, 1]));
    assert_eq!(
        one_cpuset.thread_nodes(),
        [Threads {
            node: 0,
            cpus: vec![0, 1],
        }]
    );
    assert_eq!(one_cpuset.usable(), 2);
    assert_eq!(
        layout(two(), Some(vec![1]), Some(vec![0])).usable(),
        1,
        "one CPU and memory of the same node"
    );
    assert_eq!(
        layout(two(), None, Some(vec![0, 1])).usable(),
        0,
        "threads cannot be placed when the allowed CPUs are unknown"
    );
    assert_eq!(
        layout(two(), all.clone(), Some(Vec::new())).usable(),
        0,
        "no memory to place"
    );

    // A node of memory only, such as CXL memory, holds no threads but counts for memory.
    let with_memory_node = vec![node(0, &[0, 1, 2, 3], Some(KIB)), node(1, &[], Some(KIB))];
    let cxl = layout(with_memory_node, all.clone(), None);
    assert_eq!(cxl.thread_nodes().len(), 1);
    assert_eq!(cxl.memory_nodes().len(), 2);
    assert_eq!(cxl.usable(), 2);
    // Threads on a node without memory, memory on a node without CPUs: one cell between two
    // nodes.
    let apart = vec![node(0, &[0, 1, 2, 3], Some(0)), node(1, &[], Some(KIB))];
    assert_eq!(layout(apart, all.clone(), None).usable(), 2);
    let unknown = vec![node(0, &[0, 1], None), node(1, &[2, 3], Some(KIB))];
    let unknown = layout(unknown, all, None);
    assert_eq!(
        unknown.memory_nodes().len(),
        1,
        "a node whose memory is unknown holds none"
    );
    assert_eq!(unknown.usable(), 2);
    assert_eq!(Layout::default().usable(), 0);
}

#[test]
fn a_test_may_use_the_available_memory_within_a_containers_limit() {
    let usable = |name: &str, kind: PlatformKind| memory::usable(&common::fixture(name), kind);
    assert_eq!(
        usable("dedicated-pve-x86_64", PlatformKind::Dedicated),
        Some(144_689_140 * KIB)
    );
    assert_eq!(
        usable("kvm-debian13-x86_64", PlatformKind::Vm),
        Some(577_800 * KIB)
    );
    // The cgroup's limit less its usage is below MemAvailable, which is the host's.
    assert_eq!(
        usable("lxc-limits-debian13-aarch64", PlatformKind::Container),
        Some(536_870_912 - 2_674_688)
    );
    assert_eq!(
        usable("lxc-limits-debian13-aarch64", PlatformKind::Vm),
        Some(6_692_392 * KIB),
        "outside a container the cgroup does not count"
    );
    // A Proxmox VE container's cgroup says `max`; lxcfs shows the container's MemAvailable.
    assert_eq!(
        usable("lxc-pve-debian13-x86_64", PlatformKind::Container),
        Some(1_018_497 * KIB)
    );

    let host = common::host((
        &[
            (
                "/proc/meminfo",
                "MemTotal:  4096 kB\nMemAvailable:  2048 kB\n",
            ),
            ("/sys/fs/cgroup/memory.max", "1000\n"),
            ("/sys/fs/cgroup/memory.current", "1500\n"),
        ],
        &[],
    ));
    assert_eq!(
        memory::usable(&Root::new(host.path()), PlatformKind::Container),
        Some(0),
        "a cgroup over its limit leaves nothing"
    );
    let empty = common::host((&[], &[]));
    assert_eq!(
        memory::usable(&Root::new(empty.path()), PlatformKind::Dedicated),
        None
    );
}
