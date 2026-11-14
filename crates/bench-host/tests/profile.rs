//! The host profile of each captured host (tests/fixtures), compared with a snapshot; what a
//! container's uptime and load are taken for; and sections that do not answer in time.

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bench_host::Root;
use bench_host::glibc::Libc;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::MemoryFacts;
use bench_host::profile::Read;
use bench_host::profile::Sources;
use bench_host::profile::SystemFacts;
use bench_host::storage::Space;
use bench_host::system;
use bench_host::system::Counters;
use common::Layout;

mod common;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Filesystem sizes as `df` printed them at capture time, standing in for statvfs.
fn df(name: &str) -> impl Fn(&str) -> Option<Space> + Send + 'static {
    let text = fs::read_to_string(fixture(name).join("truth/df.txt")).expect("df.txt");
    move |mount_point: &str| {
        text.lines().skip(1).find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [target, size, used, available] = fields.as_slice() else {
                return None;
            };
            (*target == mount_point).then(|| Space {
                total: size.parse().expect("size"),
                used: used.parse().expect("used"),
                available: available.parse().expect("available"),
            })
        })
    }
}

/// A kernel with nothing in front of `/proc`: its own uptime and load are the captured ones.
fn kernel_of(name: &str) -> impl Fn() -> Option<Counters> + Send + 'static {
    let root = Root::new(fixture(name).join("root"));
    let counters = Counters {
        uptime: system::uptime(&root).expect("/proc/uptime"),
        load: system::load(&root).expect("/proc/loadavg"),
    };
    move || Some(counters)
}

fn read<K>(name: &str, platform: Platform, work_dir: (u32, u32), kernel: K) -> HostProfile
where
    K: Fn() -> Option<Counters> + Send + 'static,
{
    let arch = if name.contains("aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let sources = Sources {
        root: Root::new(fixture(name).join("root")),
        arch: arch.to_owned(),
        platform,
        libc: Libc::GlibcUnknown,
        work_dir: Some(work_dir),
        space: df(name),
        kernel,
    };
    HostProfile::read(sources, Duration::from_secs(30))
}

fn profile(name: &str, platform: Platform, work_dir: (u32, u32)) -> HostProfile {
    read(name, platform, work_dir, kernel_of(name))
}

/// The Proxmox VE container, whose server had been up 61 days, with the kernel's load averages
/// in `loads`, one per reading. Checks that the kernel is read before `/proc` and after it.
fn proxmox(loads: &[[f64; 3]]) -> HostProfile {
    let readings = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&readings);
    let loads = loads.to_vec();
    let profile = read(
        "lxc-pve-debian13-x86_64",
        Platform::Container(ContainerKind::Lxc),
        (0, 75),
        move || {
            let reading = counted.fetch_add(1, Ordering::SeqCst);
            loads.get(reading).map(|&load| Counters {
                uptime: Duration::from_secs(5_322_667),
                load,
            })
        },
    );
    assert_eq!(
        readings.load(Ordering::SeqCst),
        2,
        "the kernel is read before /proc and after it"
    );
    profile
}

fn system(profile: &HostProfile) -> &SystemFacts {
    match &profile.system {
        Read::Done(system) => system,
        other => panic!("the system section was read: {other:?}"),
    }
}

#[test]
fn a_kvm_guest() {
    insta::assert_debug_snapshot!(profile(
        "kvm-debian13-x86_64",
        Platform::Vm(Hypervisor::Kvm),
        (254, 1),
    ));
}

#[test]
fn a_dedicated_server_with_a_raid_card_and_a_zfs_pool() {
    insta::assert_debug_snapshot!(profile(
        "dedicated-pve-x86_64",
        Platform::Dedicated,
        (252, 1),
    ));
}

#[test]
fn an_arm_board_with_u_boot_and_a_device_tree() {
    insta::assert_debug_snapshot!(profile(
        "rock3a-debian13-aarch64",
        Platform::Dedicated,
        (259, 2),
    ));
}

#[test]
fn a_docker_container() {
    insta::assert_debug_snapshot!(profile(
        "docker-debian13-aarch64",
        Platform::Container(ContainerKind::Docker),
        (0, 69),
    ));
}

#[test]
fn a_container_with_cgroup_limits() {
    // --memory 512m --cpus 1.5: the host's /proc/meminfo would show 8 GB here.
    insta::assert_debug_snapshot!(profile(
        "lxc-limits-debian13-aarch64",
        Platform::Container(ContainerKind::Lxc),
        (0, 69),
    ));
}

#[test]
fn a_proxmox_container_with_lxcfs() {
    // The server's load was what the container's /proc/loadavg shows. lxcfs shows the limits
    // in /proc and the container's own uptime, but passes the host's load through unless it
    // runs with --enable-loadavg, which Proxmox VE does not.
    insta::assert_debug_snapshot!(proxmox(&[[2.40, 2.43, 2.52]; 2]));
}

#[test]
fn an_openvz_container_shows_its_own_uptime_and_load() {
    // OpenVZ's kernel answers sysinfo(2) and /proc alike with the container's own values.
    let profile = profile(
        "lxc-limits-debian13-aarch64",
        Platform::Container(ContainerKind::Openvz),
        (0, 69),
    );
    let system = system(&profile);
    assert!(!system.uptime.expect("uptime").host);
    assert!(!system.load.expect("load").host);
}

#[test]
fn a_load_change_between_the_kernels_readings_still_marks_the_hosts_load() {
    // The load averages change every five seconds; /proc was read on one side of a change.
    let (shown, changed) = ([2.40, 2.43, 2.52], [2.47, 2.45, 2.53]);
    for loads in [[shown, changed], [changed, shown]] {
        let profile = proxmox(&loads);
        let load = system(&profile).load.expect("load");
        assert_eq!(load.value, shown);
        assert!(load.host, "{loads:?}");
    }
}

#[test]
fn a_container_load_unlike_the_kernels_is_the_containers_own() {
    // lxcfs with --enable-loadavg shows the container's own averages in /proc/loadavg.
    let profile = proxmox(&[[0.15, 0.20, 0.30]; 2]);
    assert!(!system(&profile).load.expect("load").host);
}

#[test]
fn a_section_that_does_not_answer_in_time_is_left_behind() {
    // statvfs on a dead network mount can block for good.
    let sources = Sources {
        root: Root::new(fixture("kvm-debian13-x86_64").join("root")),
        arch: "x86_64".to_owned(),
        platform: Platform::Vm(Hypervisor::Kvm),
        libc: Libc::GlibcUnknown,
        work_dir: None,
        space: |_: &str| -> Option<Space> {
            std::thread::sleep(Duration::from_secs(3600));
            None
        },
        kernel: || None,
    };
    let started = Instant::now();
    let profile = HostProfile::read(sources, Duration::from_millis(500));

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the profile does not wait for the stuck section"
    );
    assert_eq!(profile.storage, Read::TimedOut);
    assert!(matches!(profile.cpu, Read::Done(_)), "{:?}", profile.cpu);
    assert!(matches!(profile.memory, Read::Done(Some(_))));
    assert!(matches!(profile.numa, Read::Done(_)));
    assert!(matches!(profile.system, Read::Done(_)));
}

#[test]
fn a_numa_layout_that_does_not_answer_in_time_is_left_behind() {
    let dir = common::host((&[("/sys/devices/system/node/node0/cpulist", "0\n")], &[]));
    // Opening a FIFO without a writer blocks, as a stuck file could.
    let fifo = dir.path().join("sys/devices/system/node/node0/meminfo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let sources = Sources {
        root: Root::new(dir.path()),
        arch: "x86_64".to_owned(),
        platform: Platform::Vm(Hypervisor::Kvm),
        libc: Libc::GlibcUnknown,
        work_dir: None,
        space: |_: &str| -> Option<Space> { None },
        kernel: || None,
    };
    let profile = HostProfile::read(sources, Duration::from_millis(500));
    assert_eq!(profile.numa, Read::TimedOut);
    assert!(matches!(profile.cpu, Read::Done(_)), "{:?}", profile.cpu);
    assert!(matches!(profile.storage, Read::Done(_)));
}

#[test]
fn a_section_whose_thread_ends_without_an_answer_failed() {
    let sources = Sources {
        root: Root::new(fixture("kvm-debian13-x86_64").join("root")),
        arch: "x86_64".to_owned(),
        platform: Platform::Vm(Hypervisor::Kvm),
        libc: Libc::GlibcUnknown,
        work_dir: None,
        space: |_: &str| -> Option<Space> { panic!("a reader that breaks") },
        kernel: || None,
    };
    let started = Instant::now();
    let profile = HostProfile::read(sources, Duration::from_secs(30));

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the profile does not wait out the budget once every thread has ended"
    );
    assert_eq!(profile.storage, Read::Failed);
    assert!(matches!(profile.system, Read::Done(_)));
}

/// The profile of a synthetic container with only these files.
fn container(layout: Layout) -> HostProfile {
    let dir = common::host(layout);
    let sources = Sources {
        root: Root::new(dir.path()),
        arch: "x86_64".to_owned(),
        platform: Platform::Container(ContainerKind::Docker),
        libc: Libc::GlibcUnknown,
        work_dir: None,
        space: |_: &str| -> Option<Space> { None },
        kernel: || None,
    };
    HostProfile::read(sources, Duration::from_secs(30))
}

const CPUINFO: &str = "processor\t: 0\n\nprocessor\t: 1\n\nprocessor\t: 2\n\nprocessor\t: 3\n";

#[test]
fn a_cpu_quota_counts_only_below_the_cpus_a_container_may_use() {
    let cpus = |profile: HostProfile| match profile.cpu {
        Read::Done(CpuFacts { usable, quota, .. }) => (usable, quota),
        other => panic!("the CPU section was read: {other:?}"),
    };
    // Four CPUs of quota, but a cpuset of two: the quota limits nothing.
    let wide = container((
        &[
            ("/proc/cpuinfo", CPUINFO),
            ("/sys/fs/cgroup/cpuset.cpus.effective", "0-1\n"),
            ("/sys/fs/cgroup/cpu.max", "400000 100000\n"),
        ],
        &[],
    ));
    assert_eq!(cpus(wide), (Some(2), None));
    let narrow = container((
        &[
            ("/proc/cpuinfo", CPUINFO),
            ("/sys/fs/cgroup/cpuset.cpus.effective", "0-1\n"),
            ("/sys/fs/cgroup/cpu.max", "150000 100000\n"),
        ],
        &[],
    ));
    assert_eq!(cpus(narrow), (Some(2), Some(1.5)));
}

const MEMINFO: &str = "MemTotal:        1048576 kB\nMemFree:          524288 kB\n\
    MemAvailable:     786432 kB\nSwapTotal:             0 kB\nSwapFree:              0 kB\n";

#[test]
fn a_memory_limit_counts_only_below_what_proc_shows() {
    let memory = |profile: HostProfile| match profile.memory {
        Read::Done(Some(memory)) => memory,
        other => panic!("the memory section was read: {other:?}"),
    };
    // With lxcfs, /proc/meminfo is the container's already, and the cgroup's own usage counts
    // its page cache too.
    let above = container((
        &[
            ("/proc/meminfo", MEMINFO),
            ("/sys/fs/cgroup/memory.max", "2147483648\n"),
            ("/sys/fs/cgroup/memory.current", "104857600\n"),
        ],
        &[],
    ));
    assert_eq!(
        memory(above),
        MemoryFacts {
            total: 1 << 30,
            used: 256 << 20,
            estimated: false,
            swap_total: 0,
            swap_used: 0,
        }
    );
    // Docker's /proc/meminfo is the host's: the limit and the cgroup's usage count.
    let below = container((
        &[
            ("/proc/meminfo", MEMINFO),
            ("/sys/fs/cgroup/memory.max", "536870912\n"),
            ("/sys/fs/cgroup/memory.current", "104857600\n"),
        ],
        &[],
    ));
    assert_eq!(
        memory(below),
        MemoryFacts {
            total: 512 << 20,
            used: 100 << 20,
            estimated: false,
            swap_total: 0,
            swap_used: 0,
        }
    );
}
