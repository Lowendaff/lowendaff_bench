//! The records each variant prints for captured hosts (crates/bench-host/tests/fixtures).

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use bench_api::ContainerKind;
use bench_api::Env;
use bench_api::Hypervisor;
use bench_api::Platform;
use bench_api::PlatformKind;
use bench_api::Record;
use bench_host::Root;
use bench_host::storage::Space;
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

fn report(name: &str, kind: PlatformKind, platform: Platform, work_dir: (u32, u32)) -> Vec<String> {
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
    };
    lines(&report::records(&host, kind).expect("records"))
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
fn a_container_reports_its_cgroup_limits_and_own_root() {
    let lines = report(
        "lxc-limits-debian13-aarch64",
        PlatformKind::Container,
        Platform::Container(ContainerKind::Lxc),
        (0, 69),
    );
    // --memory 512m --cpus 1.5: the host's /proc/meminfo would show 8 GB here.
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
            "运行时间：2 天 20 小时 21 分钟",
            "负载：0.49 / 0.17 / 0.12",
            "TCP 拥塞控制：cubic",
            "IPv4 / IPv6：可用 / 不可用",
        ]
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
