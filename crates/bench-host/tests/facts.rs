//! CPU, memory, distribution and system facts, checked against what the captured hosts'
//! own tools printed at capture time, and against synthetic hosts.

mod common;

use std::fs;
use std::time::Duration;

use bench_host::Root;
use bench_host::cpu;
use bench_host::cpu::Model;
use bench_host::cpu::Topology;
use bench_host::cpu::Virtualization;
use bench_host::memory;
use bench_host::os;
use bench_host::platform::PlatformKind;
use bench_host::system;
use bench_host::system::Counters;
use common::Layout;

/// The value after `key:` in `lscpu` or `free` output.
fn field<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map_or_else(|| panic!("{key} missing"), str::trim)
}

/// Column `index` of the line starting with `label` in `free -b` output.
fn free(text: &str, label: &str, index: usize) -> u64 {
    let line = text
        .lines()
        .find(|line| line.starts_with(label))
        .expect("free line");
    line.split_whitespace()
        .nth(index)
        .expect("free column")
        .parse()
        .expect("number")
}

#[test]
fn x86_cpus_match_lscpu() {
    for name in ["kvm-debian13-x86_64", "dedicated-pve-x86_64"] {
        let cpu = cpu::read(&common::fixture(name));
        let lscpu = common::truth(name, "lscpu");
        assert_eq!(
            cpu.model,
            Some(Model::Name(field(&lscpu, "Model name").to_owned())),
            "{name}"
        );
        assert_eq!(
            cpu.logical,
            Some(field(&lscpu, "CPU(s)").parse().expect("count")),
            "{name}"
        );
        assert_eq!(cpu.aes, Some(true), "{name}");
        assert_eq!(
            cpu.virtualization,
            Virtualization::IntelVtX,
            "{name}: lscpu says VT-x"
        );
    }

    let kvm = cpu::read(&common::fixture("kvm-debian13-x86_64"));
    // No cpufreq in this guest: the clock comes from /proc/cpuinfo.
    assert_eq!(kvm.mhz, Some(1995.311));
    assert_eq!(
        kvm.caches,
        [("L1d", 32), ("L1i", 32), ("L2", 4096), ("L3", 16384)]
            .map(|(name, kib)| (name.to_owned(), kib << 10))
    );

    let dedicated = cpu::read(&common::fixture("dedicated-pve-x86_64"));
    let lscpu = common::truth("dedicated-pve-x86_64", "lscpu");
    let sockets: usize = field(&lscpu, "Socket(s)").parse().expect("sockets");
    let per_socket: usize = field(&lscpu, "Core(s) per socket").parse().expect("cores");
    assert_eq!(
        dedicated.topology,
        Some(Topology {
            sockets,
            cores: sockets * per_socket
        })
    );
    assert_eq!(
        dedicated.mhz,
        Some(field(&lscpu, "CPU max MHz").parse().expect("MHz"))
    );
    assert_eq!(
        dedicated.caches,
        [("L1d", 32), ("L1i", 32), ("L2", 1024), ("L3", 22528)]
            .map(|(name, kib)| (name.to_owned(), kib << 10))
    );
}

#[test]
fn arm_cpus_are_named_from_their_part_codes() {
    // Apple's hypervisor hides the part; the code is shown rather than a wrong name.
    let apple = cpu::read(&common::fixture("docker-debian13-aarch64"));
    assert_eq!(
        apple.model,
        Some(Model::UnknownPart {
            vendor: "Apple",
            part: 0
        })
    );
    assert_eq!(apple.logical, Some(10));
    assert_eq!(
        apple.mhz, None,
        "no cpufreq and no clock in ARM /proc/cpuinfo"
    );
    assert_eq!(apple.caches, [], "cache directories without sizes");
    assert_eq!(apple.aes, Some(true));
    assert_eq!(apple.virtualization, Virtualization::NotApplicable);

    let name = |name: &str| Model::Name(name.to_owned());
    let cases = [
        ("0x41", "0xd0c", name("ARM Neoverse-N1")),
        ("0xc0", "0xac3", name("Ampere-1")),
        ("0x6d", "0xd49", name("Microsoft Azure-Cobalt-100")),
        ("0x48", "0xd01", name("HiSilicon Kunpeng-920")),
        (
            "0x99",
            "0x001",
            Model::UnknownVendor {
                implementer: 0x99,
                part: Some(1),
            },
        ),
    ];
    for (implementer, part, model) in cases {
        let dir = common::host((&[], &[]));
        let cpuinfo = format!(
            "processor\t: 0\nFeatures\t: fp asimd\nCPU implementer\t: {implementer}\n\
             CPU part\t: {part}\n\n"
        );
        fs::create_dir_all(dir.path().join("proc")).expect("proc");
        fs::write(dir.path().join("proc/cpuinfo"), cpuinfo).expect("cpuinfo");
        let cpu = cpu::read(&Root::new(dir.path()));
        assert_eq!(cpu.model, Some(model));
        assert_eq!(cpu.aes, Some(false), "no aes in Features");
    }
}

#[test]
fn x86_cpus_fall_back_to_cpuinfo_and_read_amd_flags() {
    const CPUINFO: &str = "processor\t: 0\nmodel name\t: AMD EPYC 7763 64-Core Processor\n\
        cpu MHz\t\t: 2445.406\ncache size\t: 512 KB\nflags\t\t: fpu svm aes\n\n\
        processor\t: 1\nmodel name\t: AMD EPYC 7763 64-Core Processor\n\
        flags\t\t: fpu svm aes\n";
    let dir = common::host((&[("/proc/cpuinfo", CPUINFO)], &[]));
    let cpu = cpu::read(&Root::new(dir.path()));
    assert_eq!(cpu.logical, Some(2), "counted from processor entries");
    assert_eq!(cpu.mhz, Some(2445.406));
    assert_eq!(cpu.caches, [(String::new(), 512 << 10)]);
    assert_eq!(cpu.virtualization, Virtualization::AmdV);
    assert_eq!(cpu.topology, None);

    let dir = common::host((
        &[
            ("/proc/cpuinfo", "processor\t: 0\nflags\t\t: fpu sse2\n"),
            ("/sys/devices/system/cpu/online", "0-15,32-47\n"),
        ],
        &[],
    ));
    let cpu = cpu::read(&Root::new(dir.path()));
    assert_eq!(cpu.logical, Some(32));
    assert_eq!(cpu.aes, Some(false));
    assert_eq!(cpu.virtualization, Virtualization::Absent);
}

#[test]
fn memory_matches_free() {
    for name in [
        "kvm-debian13-x86_64",
        "dedicated-pve-x86_64",
        "lxc-pve-debian13-x86_64",
    ] {
        let memory = memory::read(&common::fixture(name)).expect("meminfo");
        let output = common::truth(name, "free");
        assert_eq!(memory.total, free(&output, "Mem:", 1), "{name}");
        assert_eq!(memory.swap_total, free(&output, "Swap:", 1), "{name}");
        assert!(!memory.estimated, "{name} has MemAvailable");
        assert_eq!(memory.used(), memory.total - memory.available, "{name}");
    }
}

#[test]
fn old_kernels_estimate_available_memory() {
    const MEMINFO: &str = "MemTotal:  2048000 kB\nMemFree:  512000 kB\nBuffers:  12000 kB\n\
        Cached:  300000 kB\nSwapTotal:  0 kB\nSwapFree:  0 kB\n";
    let dir = common::host((&[("/proc/meminfo", MEMINFO)], &[]));
    let memory = memory::read(&Root::new(dir.path())).expect("meminfo");
    assert!(memory.estimated);
    assert_eq!(memory.available, (512_000 + 12_000 + 300_000) * 1024);
    assert_eq!(memory.swap_used(), 0);
}

#[test]
fn threads_are_the_logical_cpus_or_a_containers_cpuset() {
    let host = common::host((
        &[
            ("/sys/devices/system/cpu/online", "0-7\n"),
            ("/sys/fs/cgroup/cpuset.cpus.effective", "0-1\n"),
        ],
        &[],
    ));
    let root = Root::new(host.path());
    assert_eq!(cpu::usable_threads(&root, PlatformKind::Container), Some(2));
    assert_eq!(
        cpu::usable_threads(&root, PlatformKind::Vm),
        Some(8),
        "only containers are bound by the cpuset"
    );
    assert_eq!(cpu::usable_threads(&root, PlatformKind::Dedicated), Some(8));

    assert_eq!(
        cpu::threads(4, PlatformKind::Container, None),
        4,
        "a container without a cpuset has the logical CPUs"
    );
    assert_eq!(
        cpu::usable_threads(
            &common::fixture("dedicated-pve-x86_64"),
            PlatformKind::Dedicated
        ),
        Some(64)
    );
    // lxcfs shows the container's two CPUs online; lscpu still counts the host's 64.
    let pve = common::fixture("lxc-pve-debian13-x86_64");
    assert_eq!(cpu::read(&pve).logical, Some(2));
    assert_eq!(cpu::usable_threads(&pve, PlatformKind::Container), Some(2));
    let empty = common::host((&[], &[]));
    assert_eq!(
        cpu::usable_threads(&Root::new(empty.path()), PlatformKind::Vm),
        None
    );
}
#[test]
fn container_limits_come_from_the_cgroup() {
    let lxc = memory::cgroup_limits(&common::fixture("lxc-limits-debian13-aarch64"));
    assert_eq!(lxc.memory, Some(512 << 20), "--memory 512m");
    assert_eq!(lxc.memory_used, Some(2_674_688));
    assert_eq!(
        (lxc.swap, lxc.swap_used),
        (Some(512 << 20), Some(0)),
        "Docker's swap limit defaults to --memory"
    );
    assert_eq!(lxc.cpus, Some(1.5), "--cpus 1.5");
    assert_eq!(lxc.cpuset, Some(10));

    let docker = memory::cgroup_limits(&common::fixture("docker-debian13-aarch64"));
    assert_eq!(
        (docker.memory, docker.cpus),
        (None, None),
        "max means unlimited"
    );

    // Proxmox VE sets the limits on the container's cgroup, above the root of the namespace
    // the container sees; lxcfs shows them in /proc/meminfo instead.
    let pve = memory::cgroup_limits(&common::fixture("lxc-pve-debian13-x86_64"));
    assert_eq!((pve.memory, pve.swap, pve.cpus), (None, None, None));
    assert_eq!(pve.cpuset, Some(2), "cores: 2");

    let v1: Layout = (
        &[
            ("/sys/fs/cgroup/memory/memory.limit_in_bytes", "268435456\n"),
            ("/sys/fs/cgroup/cpu/cpu.cfs_quota_us", "50000\n"),
            ("/sys/fs/cgroup/cpu/cpu.cfs_period_us", "100000\n"),
        ],
        &[],
    );
    let dir = common::host(v1);
    let limits = memory::cgroup_limits(&Root::new(dir.path()));
    assert_eq!((limits.memory, limits.cpus), (Some(256 << 20), Some(0.5)));

    let unlimited_v1: Layout = (
        &[
            (
                "/sys/fs/cgroup/memory/memory.limit_in_bytes",
                "9223372036854771712\n",
            ),
            ("/sys/fs/cgroup/cpu/cpu.cfs_quota_us", "-1\n"),
            ("/sys/fs/cgroup/cpu/cpu.cfs_period_us", "100000\n"),
        ],
        &[],
    );
    let dir = common::host(unlimited_v1);
    let limits = memory::cgroup_limits(&Root::new(dir.path()));
    assert_eq!((limits.memory, limits.cpus), (None, None));
}

#[test]
fn distribution_names_follow_os_release() {
    for name in [
        "kvm-debian13-x86_64",
        "dedicated-pve-x86_64",
        "docker-debian13-aarch64",
        "lxc-pve-debian13-x86_64",
    ] {
        assert_eq!(
            os::distribution(&common::fixture(name)).as_deref(),
            Some("Debian GNU/Linux 13 (trixie)"),
            "{name}"
        );
    }
    let cases: [(Layout, Option<&str>); 9] = [
        (
            (
                &[(
                    "/etc/os-release",
                    "# comment\n\nNAME=\"Foo\"\nPRETTY_NAME=\"Foo \\\"Bar\\\" 1.0\"\n",
                )],
                &[],
            ),
            Some("Foo \"Bar\" 1.0"),
        ),
        (
            (
                &[(
                    "/etc/os-release",
                    "PRETTY_NAME='Rocky Linux 9.4 (Blue Onyx)'\n",
                )],
                &[],
            ),
            Some("Rocky Linux 9.4 (Blue Onyx)"),
        ),
        (
            (
                &[("/etc/os-release", "NAME=Gentoo\nVERSION_ID=2.17\n")],
                &[],
            ),
            Some("Gentoo 2.17"),
        ),
        (
            (
                &[("/usr/lib/os-release", "PRETTY_NAME=\"Arch Linux\"\n")],
                &[],
            ),
            Some("Arch Linux"),
        ),
        (
            (
                &[(
                    "/etc/lsb-release",
                    "DISTRIB_ID=Ubuntu\nDISTRIB_DESCRIPTION=\"Ubuntu 12.04 LTS\"\n",
                )],
                &[],
            ),
            Some("Ubuntu 12.04 LTS"),
        ),
        (
            (
                &[("/etc/redhat-release", "CentOS release 6.10 (Final)\n")],
                &[],
            ),
            Some("CentOS release 6.10 (Final)"),
        ),
        (
            (&[("/etc/debian_version", "7.11\n")], &[]),
            Some("Debian 7.11"),
        ),
        (
            (&[("/etc/alpine-release", "3.22.1\n")], &[]),
            Some("Alpine Linux 3.22.1"),
        ),
        ((&[], &[]), None),
    ];
    for (layout, expected) in cases {
        let dir = common::host(layout);
        assert_eq!(
            os::distribution(&Root::new(dir.path())).as_deref(),
            expected,
            "{layout:?}"
        );
    }
}

#[test]
fn kernel_uptime_load_and_tcp_settings() {
    for (name, algorithm, qdisc) in [
        ("kvm-debian13-x86_64", "bbr", Some("fq")),
        ("dedicated-pve-x86_64", "cubic", Some("fq_codel")),
        // Not visible inside a container's network namespace.
        ("docker-debian13-aarch64", "cubic", None),
        ("lxc-pve-debian13-x86_64", "cubic", None),
    ] {
        let root = common::fixture(name);
        let uname = common::truth(name, "uname");
        assert_eq!(
            system::kernel(&root).as_deref(),
            uname.split_whitespace().nth(1),
            "{name}: uname -r"
        );
        assert_eq!(
            system::congestion_control(&root).as_deref(),
            Some(algorithm),
            "{name}"
        );
        assert_eq!(system::default_qdisc(&root).as_deref(), qdisc, "{name}");
        assert!(system::uptime(&root).is_some(), "{name}");
        assert!(system::load(&root).is_some(), "{name}");
    }

    let dir = common::host((
        &[
            ("/proc/uptime", "93784.25 180000.00\n"),
            ("/proc/loadavg", "0.38 0.13 0.05 1/234 5678\n"),
        ],
        &[],
    ));
    let root = Root::new(dir.path());
    assert_eq!(
        system::uptime(&root),
        Some(Duration::from_secs_f64(93_784.25))
    );
    assert_eq!(system::load(&root), Some([0.38, 0.13, 0.05]));
}

#[test]
fn the_kernels_own_counters_tell_the_hosts_values_from_a_containers() {
    // /proc/loadavg rounds each average to hundredths: 4914 / 2048 shows as 2.40.
    let kernel = Counters {
        uptime: Duration::from_secs(5_322_697),
        load: [4914.0 / 2048.0, 4977.0 / 2048.0, 5160.0 / 2048.0],
    };
    assert!(kernel.matches_uptime(Duration::from_secs_f64(5_322_696.5)));
    assert!(
        !kernel.matches_uptime(Duration::from_secs(5_322_698)),
        "a second apart is another clock"
    );
    assert!(
        !kernel.matches_uptime(Duration::from_secs_f64(13.43)),
        "the container's own uptime, as lxcfs shows it"
    );
    assert!(kernel.matches_load([2.40, 2.43, 2.52]));
    let rounded = Counters {
        load: [10.0 / 2048.0, 0.0, 0.0],
        ..kernel
    };
    assert!(
        rounded.matches_load([0.0, 0.0, 0.0]),
        "/proc/loadavg shows 10 / 2048 as 0.00"
    );
    assert!(
        !kernel.matches_load([2.40, 2.43, 2.53]),
        "every average has to match"
    );
    assert!(
        !kernel.matches_load([0.0, 0.0, 0.0]),
        "an idle container's own load"
    );
}

#[test]
fn the_running_kernel_reports_what_its_proc_files_show() {
    // Outside containers with lxcfs, where the tests run, the files are the kernel's own.
    let root = Root::system();
    let kernel = Counters::read();
    if !cfg!(target_os = "linux") {
        assert_eq!(kernel, None, "the counters are read on Linux only");
        return;
    }
    let kernel = kernel.expect("Linux has had CLOCK_BOOTTIME since 2.6.39");
    assert!(kernel.matches_uptime(system::uptime(&root).expect("/proc/uptime")));
    // The averages change every five seconds; one of three tries falls between two changes.
    let agrees = (0..3).any(|_| {
        let shown = system::load(&root).expect("/proc/loadavg");
        Counters::read().is_some_and(|kernel| kernel.matches_load(shown))
    });
    assert!(agrees, "sysinfo(2) and /proc/loadavg disagree");
}
