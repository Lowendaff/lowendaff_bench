//! The records each variant prints for captured hosts (crates/bench-host/tests/fixtures).

use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bench_api::ContainerKind;
use bench_api::Env;
use bench_api::Hypervisor;
use bench_api::Platform;
use bench_api::PlatformKind;
use bench_api::Record;
use bench_host::Root;
use bench_host::storage::Space;
use bench_host::system;
use bench_host::system::Counters;
use plugin_system::report;
use plugin_system::report::Host;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/bench-host/tests/fixtures")
        .join(name)
}

/// Filesystem sizes as `df` printed them at capture time, standing in for statvfs.
fn df(name: &str) -> impl Fn(&str) -> Option<Space> {
    let text = fs::read_to_string(fixture_dir(name).join("truth/df.txt")).expect("df.txt");
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

/// The records as the terminal shows them, one string per line.
fn lines(records: &[Record]) -> Vec<String> {
    let mut lines = Vec::new();
    for record in records {
        match record {
            Record::Fact { label, value, .. } => lines.push(format!("{label}：{value}")),
            Record::Metric { label, result, .. } => lines.push(format!("{label}：{result}")),
            Record::Table(table) => {
                lines.push(format!("{}：", table.title()));
                lines.push(table.headers().join(" | "));
                for row in table.rows() {
                    lines.push(row.join(" | "));
                }
            }
        }
    }
    lines
}

/// The report on a kernel with nothing in front of `/proc`: its own uptime and load are the
/// ones the captured files show.
fn report(name: &str, kind: PlatformKind, platform: Platform, work_dir: (u32, u32)) -> Vec<String> {
    let root = Root::new(fixture_dir(name).join("root"));
    let kernel = Counters {
        uptime: system::uptime(&root).expect("/proc/uptime"),
        load: system::load(&root).expect("/proc/loadavg"),
    };
    report_with(name, kind, platform, work_dir, move || Some(kernel))
}

fn report_with(
    name: &str,
    kind: PlatformKind,
    platform: Platform,
    work_dir: (u32, u32),
    kernel: impl Fn() -> Option<Counters>,
) -> Vec<String> {
    let host = Host {
        root: Root::new(fixture_dir(name).join("root")),
        env: Env {
            arch: if name.contains("aarch64") {
                "aarch64"
            } else {
                "x86_64"
            }
            .to_owned(),
            ipv4: true,
            ipv6: false,
            platform,
            ..Env::default()
        },
        work_dir: Some(work_dir),
        space: df(name),
        kernel,
    };
    lines(&report::records(&host, kind).expect("records"))
}

/// The report of the Proxmox VE container, whose server had been up 61 days, with the kernel's
/// load averages in `loads`, one per reading.
fn proxmox(loads: &[[f64; 3]]) -> Vec<String> {
    let readings = Cell::new(0);
    let lines = report_with(
        "lxc-pve-debian13-x86_64",
        PlatformKind::Container,
        Platform::Container(ContainerKind::Lxc),
        (0, 75),
        || {
            let reading = readings.get();
            readings.set(reading + 1);
            loads.get(reading).map(|&load| Counters {
                uptime: Duration::from_secs(5_322_667),
                load,
            })
        },
    );
    assert_eq!(
        readings.get(),
        2,
        "the kernel is read before /proc and after it"
    );
    lines
}

#[test]
fn a_kvm_guest_reports_its_vcpus_and_virtio_disk() {
    let lines = report(
        "kvm-debian13-x86_64",
        PlatformKind::Vm,
        Platform::Vm(Hypervisor::Kvm),
        (254, 1),
    );
    // Sizes match `free`, `lsblk` and `df` from capture time; no 介质 column in a VM.
    assert_eq!(
        lines,
        [
            "CPU 型号：Intel Xeon Processor (Skylake, IBRS)",
            "CPU 核心：1 个",
            "CPU 频率：1995 MHz",
            "CPU 缓存：L1d 32 KiB / L1i 32 KiB / L2 4 MiB / L3 16 MiB",
            "AES 指令：支持",
            "硬件虚拟化：支持（VT-x）",
            "内存总量：929.08 MiB",
            "内存已用：364.82 MiB",
            "Swap：无",
            "硬盘：",
            "设备 | 接口 | 容量 | 型号 | 用途",
            "vda | virtio | 20.0 GiB | — | 系统盘（/）",
            "文件系统：",
            "挂载点 | 类型 | 容量 | 已用 | 可用 | 所在硬盘",
            "/（测试目录） | ext4 | 19.6 GiB | 11.0 GiB | 7.7 GiB | vda",
            "系统：Debian GNU/Linux 13 (trixie)",
            "内核：6.12.73+deb13-amd64",
            "架构：x86_64",
            "运行时间：2 天 9 小时 53 分钟",
            "负载：0.14 / 0.08 / 0.08",
            "TCP 拥塞控制：bbr（队列 fq）",
            "IPv4 / IPv6：可用 / 不可用",
        ]
    );
}

#[test]
fn a_dedicated_server_reports_sockets_media_and_its_zfs_pool() {
    let lines = report(
        "dedicated-pve-x86_64",
        PlatformKind::Dedicated,
        Platform::Dedicated,
        (252, 1),
    );
    assert_eq!(
        lines,
        [
            "整机：Inspur SSINSPURY32-XI5-25D-1",
            "主板：Inspur YZMB-00870-105（SA5112M5）",
            "BIOS：American Megatrends Inc. 4.1.19（2022-02-07）",
            "BMC：Inspur，固件 4.18，IPMI 2.0",
            "CPU 型号：Intel(R) Xeon(R) Gold 6130 CPU @ 2.10GHz",
            "CPU 核心：64 线程（2 颗 CPU，共 32 核）",
            "CPU 频率：3700 MHz",
            "CPU 缓存：L1d 32 KiB / L1i 32 KiB / L2 1 MiB / L3 22 MiB",
            "AES 指令：支持",
            "硬件虚拟化：支持（VT-x）",
            "内存总量：188.52 GiB",
            "内存已用：50.53 GiB",
            "Swap 总量：8 GiB",
            "Swap 已用：4.04 GiB",
            // Two modules are installed but not in use.
            "ECC：有（单比特纠错）；本次开机以来已纠正 0 次，不可纠正 0 次",
            "内存条：",
            "插槽 | 容量 | 类型 | 速度 | 厂商 | 型号",
            "CPU0_C0D0 | 未启用 | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU0_C0D1 | 空 | — | — | — | —",
            "CPU0_C1D0 | 未启用 | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU0_C1D1 | 空 | — | — | — | —",
            "CPU0_C2D0 | 空 | — | — | — | —",
            "CPU0_C2D1 | 空 | — | — | — | —",
            "CPU0_C3D0 | 32 GB | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU0_C3D1 | 空 | — | — | — | —",
            "CPU0_C4D0 | 32 GB | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU0_C4D1 | 空 | — | — | — | —",
            "CPU0_C5D0 | 空 | — | — | — | —",
            "CPU0_C5D1 | 空 | — | — | — | —",
            "CPU1_C0D0 | 32 GB | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU1_C0D1 | 空 | — | — | — | —",
            "CPU1_C1D0 | 32 GB | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU1_C1D1 | 空 | — | — | — | —",
            "CPU1_C2D0 | 空 | — | — | — | —",
            "CPU1_C2D1 | 空 | — | — | — | —",
            "CPU1_C3D0 | 32 GB | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU1_C3D1 | 空 | — | — | — | —",
            "CPU1_C4D0 | 32 GB | DDR4 LRDIMM | 2133 MT/s | Samsung | M386A4G40DM0-CPB",
            "CPU1_C4D1 | 空 | — | — | — | —",
            "CPU1_C5D0 | 空 | — | — | — | —",
            "CPU1_C5D1 | 空 | — | — | — | —",
            "说明：CPU0_C0D0、CPU0_C1D0 插着内存条但没有启用：固件报告容量为 0，没有计入内存总量",
            "RAID 卡：MegaRAID SAS 9361-8i：直通 sda、sdb、sdc、sdd",
            "硬盘：",
            "设备 | 接口 | 容量 | 介质 | 型号 | 用途",
            "sda | SATA | 232.9 GiB | 固态 | Samsung SSD 860 EVO 250GB | 系统盘（/），Swap",
            "sdb | SATA | 931.5 GiB | 固态 | Samsung SSD 870 EVO 1TB | ZFS 存储池 Volume1",
            "sdc | SATA | 931.5 GiB | 固态 | Samsung SSD 860 EVO 1TB | ZFS 存储池 Volume1",
            "sdd | SATA | 931.5 GiB | 固态 | Samsung SSD 870 EVO 1TB | ZFS 存储池 Volume1",
            "文件系统：",
            "挂载点 | 类型 | 容量 | 已用 | 可用 | 所在硬盘",
            "/（测试目录） | ext4 | 219.8 GiB | 54.7 GiB | 162.9 GiB | sda",
            // A pool's datasets share its space, so only the available space is shown.
            "/Volume1 | zfs | 不可用 | 不可用 | 1.38 TiB | sdb、sdc、sdd",
            "系统：Debian GNU/Linux 13 (trixie)",
            "内核：7.0.14-15-pve",
            "架构：x86_64",
            "运行时间：58 天 20 小时 27 分钟",
            "负载：2.69 / 2.58 / 2.70",
            "TCP 拥塞控制：cubic（队列 fq_codel）",
            "IPv4 / IPv6：可用 / 不可用",
        ]
    );
}

#[test]
fn an_arm_board_reports_firmware_and_memory_from_u_boot_and_its_device_tree() {
    let lines = report(
        "rock3a-debian13-aarch64",
        PlatformKind::Dedicated,
        Platform::Dedicated,
        (259, 2),
    );
    // U-Boot's SMBIOS has no memory structures, so the device tree gives the memory.
    assert_eq!(
        lines,
        [
            "整机：Radxa ROCK 3A",
            "主板：Radxa ROCK 3A",
            "BIOS：U-Boot 2026.07-rc4-ga7830e87555a（2026-07-01）",
            "BMC：未检测到",
            "CPU 型号：ARM Cortex-A55",
            "CPU 核心：4 线程（4 核）",
            "CPU 频率：1992 MHz",
            "CPU 缓存：L1d 32 KiB / L1i 32 KiB / L2 512 KiB",
            "AES 指令：支持",
            "硬件虚拟化：不适用",
            "内存总量：1.92 GiB",
            "内存已用：233.42 MiB",
            "Swap：无",
            "ECC：不可用：SMBIOS 里没有内存纠错信息，也没有 EDAC 驱动",
            "内存条：板载内存 2.0 GiB（来自设备树）；没有可插拔内存条的信息",
            "RAID 卡：无",
            "硬盘：",
            "设备 | 接口 | 容量 | 介质 | 型号 | 用途",
            "nvme0n1 | NVMe | 119.2 GiB | 固态 | KBG40ZNS128G BG4A KIOXIA | 系统盘（/）",
            "文件系统：",
            "挂载点 | 类型 | 容量 | 已用 | 可用 | 所在硬盘",
            "/（测试目录） | ext4 | 117.0 GiB | 896 MiB | 116.1 GiB | nvme0n1",
            "系统：Debian GNU/Linux 13 (trixie)",
            "内核：6.12.95+deb13-arm64",
            "架构：aarch64",
            "运行时间：8 分钟",
            "负载：0.00 / 0.10 / 0.10",
            "TCP 拥塞控制：cubic（队列 fq_codel）",
            "IPv4 / IPv6：可用 / 不可用",
        ]
    );
}

#[test]
fn a_container_reports_its_cgroup_limits_and_own_root() {
    let lines = report(
        "lxc-limits-debian13-aarch64",
        PlatformKind::Container,
        Platform::Container(ContainerKind::Lxc),
        (0, 69),
    );
    // --memory 512m --cpus 1.5: the host's /proc/meminfo would show 8 GB here. Docker shares the
    // kernel's uptime and load, which are the host's.
    assert_eq!(
        lines,
        [
            "CPU 型号：Apple（型号代码 0x000）",
            "CPU 核心：10 个（限额 1.5 个）",
            "CPU 频率：不可用",
            "CPU 缓存：不可用",
            "AES 指令：支持",
            "内存总量：512 MiB",
            "内存已用：2.55 MiB",
            "Swap 总量：512 MiB",
            "Swap 已用：0 KiB",
            "文件系统：",
            "挂载点 | 类型 | 容量 | 已用 | 可用",
            "/（测试目录） | overlay | 910.7 GiB | 14.6 GiB | 849.8 GiB",
            "系统：Debian GNU/Linux 13 (trixie)",
            "内核：7.0.14-linuxkit",
            "架构：aarch64",
            "运行时间：2 天 20 小时 21 分钟（宿主机）",
            "负载：0.49 / 0.17 / 0.12（宿主机）",
            "TCP 拥塞控制：cubic",
            "IPv4 / IPv6：可用 / 不可用",
        ]
    );
}

#[test]
fn a_proxmox_container_reports_what_lxcfs_shows() {
    // The server's load was what the container's /proc/loadavg shows.
    let lines = proxmox(&[[2.40, 2.43, 2.52]; 2]);
    // cores 2, memory 1024 and swap 512 (MiB). The cgroup files the container sees say max,
    // because the limits sit above its namespace; lxcfs shows them in /proc instead. Its uptime
    // is the container's, but the load is the host's: lxcfs serves /proc/loadavg too, yet passes
    // the host's values through unless it runs with --enable-loadavg, which Proxmox VE does not.
    assert_eq!(
        lines,
        [
            "CPU 型号：Intel(R) Xeon(R) Gold 6130 CPU @ 2.10GHz",
            "CPU 核心：2 个",
            "CPU 频率：3700 MHz",
            "CPU 缓存：L1d 32 KiB / L1i 32 KiB / L2 1 MiB / L3 22 MiB",
            "AES 指令：支持",
            "内存总量：1 GiB",
            "内存已用：29.37 MiB",
            "Swap 总量：512 MiB",
            "Swap 已用：0 KiB",
            "文件系统：",
            "挂载点 | 类型 | 容量 | 已用 | 可用",
            "/（测试目录） | zfs | 8.0 GiB | 351 MiB | 7.7 GiB",
            "系统：Debian GNU/Linux 13 (trixie)",
            "内核：7.0.14-15-pve",
            "架构：x86_64",
            "运行时间：0 分钟",
            "负载：2.40 / 2.43 / 2.52（宿主机）",
            "TCP 拥塞控制：cubic",
            "IPv4 / IPv6：可用 / 不可用",
        ]
    );
}

#[test]
fn an_openvz_container_shows_its_own_uptime_and_load() {
    // OpenVZ's kernel answers sysinfo(2) and /proc alike with the container's own values.
    let lines = report(
        "lxc-limits-debian13-aarch64",
        PlatformKind::Container,
        Platform::Container(ContainerKind::Openvz),
        (0, 69),
    );
    for line in ["运行时间：2 天 20 小时 21 分钟", "负载：0.49 / 0.17 / 0.12"] {
        assert!(lines.contains(&line.to_owned()), "{line} in {lines:?}");
    }
}

#[test]
fn a_load_change_between_the_kernels_readings_still_marks_the_hosts_load() {
    // The load averages change every five seconds; /proc was read on one side of a change.
    let (shown, changed) = ([2.40, 2.43, 2.52], [2.47, 2.45, 2.53]);
    for loads in [[shown, changed], [changed, shown]] {
        let lines = proxmox(&loads);
        assert!(
            lines.contains(&"负载：2.40 / 2.43 / 2.52（宿主机）".to_owned()),
            "{loads:?}: {lines:?}"
        );
    }
}

#[test]
fn a_container_load_unlike_the_kernels_is_the_containers_own() {
    // lxcfs with --enable-loadavg shows the container's own averages in /proc/loadavg.
    let lines = proxmox(&[[0.15, 0.20, 0.30]; 2]);
    assert!(
        lines.contains(&"负载：2.40 / 2.43 / 2.52".to_owned()),
        "{lines:?}"
    );
}

#[test]
fn the_three_variants_share_one_title_and_split_the_platforms() {
    use bench_api::Plugin;

    let variants = [
        (
            plugin_system::Dedicated::DESCRIPTOR,
            "system.dedicated",
            PlatformKind::Dedicated,
        ),
        (plugin_system::Vm::DESCRIPTOR, "system.vm", PlatformKind::Vm),
        (
            plugin_system::Container::DESCRIPTOR,
            "system.container",
            PlatformKind::Container,
        ),
    ];
    for (descriptor, id, kind) in variants {
        assert_eq!(descriptor.id, id);
        assert_eq!(descriptor.title, "系统基础信息", "{id}");
        assert_eq!(descriptor.requires.platforms, [kind], "{id}");
        assert!(
            descriptor.requires.tools.is_empty(),
            "{id} needs no external tools"
        );
    }
}
