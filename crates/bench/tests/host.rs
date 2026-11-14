//! 【系统信息】: the host profile in words. The six captured hosts (bench-host's fixtures) are
//! compared with snapshots; constructed profiles cover each form that a line about the hardware,
//! a section that did not answer, or an address family can take.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bench::report;
use bench::report::terminal::Page;
use bench_host::Root;
use bench_host::bmc::Bmc;
use bench_host::bmc::Interface;
use bench_host::bmc::InterfaceKind;
use bench_host::cpu::Cpu;
use bench_host::cpu::Model;
use bench_host::edac::Edac;
use bench_host::glibc::Libc;
use bench_host::hardware::Hardware;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::machine::Machine;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::network::Why;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;
use bench_host::profile::Sources;
use bench_host::profile::StorageFacts;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::raid::Ids;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Smbios;
use bench_host::smbios::Unreadable;
use bench_host::storage::Space;
use bench_host::system;
use bench_host::system::Counters;
use unicode_width::UnicodeWidthStr;

/// The report of a run without plugins: its header, 【系统信息】 and an empty summary.
fn render(host: &HostProfile, show_ip: bool) -> String {
    let page = Page {
        version: "0.1.0",
        host,
        show_ip,
        not_applicable: &[],
        runs: &[],
        total: Duration::from_secs(1),
    };
    report::terminal::render(&page, None)
}

/// The values of the section's lines with this label, in order.
fn field(text: &str, label: &str) -> Vec<String> {
    let fill = 16_usize.saturating_sub(label.width());
    let prefix = format!("  {label}{}  ", " ".repeat(fill));
    text.lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(str::to_owned)
        .collect()
}

/// The rows of the table under the title line `title`, its header first, with the cells
/// joined by ` | `.
fn table(text: &str, title: &str) -> Vec<String> {
    let cells = |line: &str| -> Vec<String> {
        line.trim()
            .split("  ")
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let mut lines = text
        .lines()
        .skip_while(|line| *line != format!("  {title}"))
        .skip(1);
    let header = cells(lines.next().unwrap_or_default());
    let mut rows = vec![header.join(" | ")];
    for line in lines {
        let row = cells(line);
        if row.len() != header.len() {
            break;
        }
        rows.push(row.join(" | "));
    }
    rows
}

fn line(vendor: Option<&str>, name: Option<&str>, detail: Option<&str>) -> Line {
    Line {
        vendor: vendor.map(str::to_owned),
        name: name.map(str::to_owned),
        detail: detail.map(str::to_owned),
    }
}

fn bare() -> Hardware {
    Hardware {
        smbios: Err(Unreadable::Absent),
        machine: Machine {
            system: Line::default(),
            board: Line::default(),
            firmware: None,
        },
        bmc: None,
        edac: None,
        board_memory: None,
    }
}

fn smbios(slots: Vec<Slot>, ecc: Vec<Ecc>) -> Result<Smbios, Unreadable> {
    Ok(Smbios {
        slots,
        ecc,
        bmc: None,
        system: Line::default(),
        board: Line::default(),
        bios: Line::default(),
    })
}

fn edac(corrected: u64, uncorrected: u64, modes: Vec<Ecc>) -> Option<Edac> {
    Some(Edac {
        corrected,
        uncorrected,
        modes,
    })
}

/// The report of a dedicated server with this hardware, and nothing else known.
fn dedicated(hardware: Hardware) -> String {
    render(
        &HostProfile {
            hardware: Some(Read::Done(hardware)),
            ..HostProfile::default()
        },
        false,
    )
}

#[test]
fn machine_lines_drop_missing_parts_and_repeated_vendors() {
    let text = dedicated(bare());
    for (label, value) in [
        ("整机", "不可用"),
        ("主板", "不可用"),
        ("固件", "不可用"),
        ("BMC", "未检测到"),
    ] {
        assert_eq!(field(&text, label), [value], "{label}");
    }
    let text = dedicated(Hardware {
        machine: Machine {
            system: line(Some("radxa"), Some("Radxa ROCK 3A"), None),
            board: line(Some("Supermicro"), Some("X11DPi-N"), None),
            firmware: Some(Firmware::Bios(line(None, Some("1.2"), Some("2024/01/05")))),
        },
        ..bare()
    });
    assert_eq!(field(&text, "整机"), ["Radxa ROCK 3A"]);
    assert_eq!(field(&text, "主板"), ["Supermicro X11DPi-N"]);
    // Only month/day/year becomes year-month-day.
    assert_eq!(field(&text, "BIOS"), ["1.2（2024/01/05）"]);
    let firmware = |firmware, label| {
        let text = dedicated(Hardware {
            machine: Machine {
                firmware: Some(firmware),
                ..bare().machine
            },
            ..bare()
        });
        field(&text, label)
    };
    assert_eq!(
        firmware(Firmware::Bios(line(None, None, Some("7/4/2023"))), "BIOS"),
        ["2023-07-04"]
    );
    assert_eq!(
        firmware(Firmware::UBoot("2024.01".to_owned()), "固件"),
        ["U-Boot 2024.01"]
    );
    assert_eq!(
        firmware(Firmware::RaspberryPi(1_713_139_200), "固件"),
        ["树莓派引导程序，2024-04-15 构建"]
    );
    // 2024-02-29 23:59:59 UTC and the epoch.
    assert_eq!(
        firmware(Firmware::RaspberryPi(1_709_251_199), "固件"),
        ["树莓派引导程序，2024-02-29 构建"]
    );
    assert_eq!(
        firmware(Firmware::RaspberryPi(0), "固件"),
        ["树莓派引导程序，1970-01-01 构建"]
    );
}

#[test]
fn bmc_lines_say_what_is_missing_and_why() {
    let bmc = |bmc| {
        field(
            &dedicated(Hardware {
                bmc: Some(bmc),
                ..bare()
            }),
            "BMC",
        )
    };
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: Some(12345),
            firmware: None,
            ipmi: Some("1.5".to_owned()),
        }),
        ["厂商编号 12345，IPMI 1.5"]
    );
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: Some(10876),
            firmware: Some("1.73.14".to_owned()),
            ipmi: Some("2.0".to_owned()),
        }),
        ["Supermicro，固件 1.73.14，IPMI 2.0"]
    );
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: None,
            firmware: None,
            ipmi: None,
        }),
        ["有"]
    );
    let interface = |kind, version| {
        bmc(Bmc::Interface(Interface {
            kind: Some(kind),
            version,
        }))
    };
    assert_eq!(
        interface(InterfaceKind::Ssif, Some((2, 0))),
        ["有（IPMI 2.0，SSIF 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"]
    );
    assert_eq!(
        interface(InterfaceKind::Smic, None),
        ["有（SMIC 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"]
    );
    assert_eq!(
        interface(InterfaceKind::Bt, Some((1, 5))),
        ["有（IPMI 1.5，BT 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"]
    );
    assert_eq!(
        interface(InterfaceKind::Kcs, None),
        ["有（KCS 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"]
    );
    let nothing = ["有；接口、厂商和固件不可用：内核没有加载 IPMI 驱动"];
    assert_eq!(
        bmc(Bmc::Interface(Interface {
            kind: None,
            version: None,
        })),
        nothing
    );
    assert_eq!(bmc(Bmc::Present), nothing);
}

#[test]
fn the_ecc_line_combines_smbios_and_edac() {
    let ecc = |smbios, edac| {
        field(
            &dedicated(Hardware {
                smbios,
                edac,
                ..bare()
            }),
            "ECC",
        )
    };
    assert_eq!(
        ecc(
            smbios(vec![], vec![Ecc::MultiBit, Ecc::Crc]),
            edac(3, 1, vec![])
        ),
        ["有（多比特纠错）、有（CRC 校验）；本次开机以来已纠正 3 次，不可纠正 1 次"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![Ecc::Parity]), None),
        ["无纠错（只有奇偶校验）；错误次数不可用：内核没有加载 EDAC 驱动"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![Ecc::Unknown]), None),
        ["未知；错误次数不可用：内核没有加载 EDAC 驱动"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![Ecc::None]), edac(0, 0, vec![])),
        ["无"]
    );
    // Without type 16, EDAC's mode.
    assert_eq!(
        ecc(smbios(vec![], vec![]), edac(0, 0, vec![Ecc::SingleBit])),
        ["有（单比特纠错）；本次开机以来已纠正 0 次，不可纠正 0 次"]
    );
    assert_eq!(
        ecc(Err(Unreadable::Absent), edac(2, 0, vec![])),
        ["纠错方式不可用；本次开机以来已纠正 2 次，不可纠正 0 次"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![]), None),
        ["不可用：SMBIOS 里没有内存纠错信息，也没有 EDAC 驱动"]
    );
    assert_eq!(
        ecc(Err(Unreadable::Absent), None),
        ["不可用：没有 SMBIOS，也没有 EDAC 驱动"]
    );
    assert_eq!(
        ecc(Err(Unreadable::DevMem(io::Error::other("refused"))), None),
        ["不可用：读不到 SMBIOS，也没有 EDAC 驱动"]
    );
}

fn slot(name: Option<&str>, capacity: Capacity) -> Slot {
    Slot {
        name: name.map(str::to_owned),
        capacity,
        generation: None,
        form: None,
        speed: None,
        configured_speed: None,
        manufacturer: None,
        part_number: None,
    }
}

#[test]
fn the_memory_table_shows_sizes_types_and_speeds() {
    let slots = vec![
        Slot {
            generation: Some("DDR5"),
            speed: Some(2933),
            configured_speed: Some(2666),
            manufacturer: Some("Micron".to_owned()),
            part_number: Some("MTC20F2085S1RC48BA1".to_owned()),
            ..slot(Some("DIMM 0"), Capacity::Kilobytes(16 << 20))
        },
        Slot {
            form: Some("RDIMM"),
            speed: Some(2933),
            ..slot(Some("DIMM 1"), Capacity::Kilobytes(512 << 10))
        },
        Slot {
            configured_speed: Some(3200),
            manufacturer: Some("80CE".to_owned()),
            ..slot(None, Capacity::Kilobytes(512))
        },
        slot(Some("DIMM 3"), Capacity::Unknown),
        Slot {
            generation: Some("DDR4"),
            form: Some("RDIMM"),
            speed: Some(2400),
            configured_speed: Some(2400),
            manufacturer: Some("Hynix".to_owned()),
            part_number: Some("HMA82GR7AFR8N-UH".to_owned()),
            ..slot(Some("DIMM 4"), Capacity::Disabled)
        },
        Slot {
            manufacturer: Some("Kept but not shown".to_owned()),
            ..slot(Some("DIMM 5"), Capacity::Empty)
        },
    ];
    let text = dedicated(Hardware {
        smbios: smbios(slots, vec![Ecc::SingleBit]),
        ..bare()
    });
    assert_eq!(
        table(&text, "内存条"),
        [
            "插槽 | 容量 | 类型 | 速度 | 厂商 | 型号",
            "DIMM 0 | 16 GB | DDR5 | 2666 MT/s（额定 2933） | Micron | MTC20F2085S1RC48BA1",
            "DIMM 1 | 512 MB | RDIMM | 2933 MT/s（额定） | — | —",
            "插槽 3 | 512 KB | — | 3200 MT/s | 80CE | —",
            "DIMM 3 | 未知 | — | — | — | —",
            "DIMM 4 | 未启用 | DDR4 RDIMM | 2400 MT/s | Hynix | HMA82GR7AFR8N-UH",
            "DIMM 5 | 空 | — | — | — | —",
        ]
    );
    assert_eq!(
        field(&text, "说明"),
        ["DIMM 4 插着内存条但没有启用：固件报告容量为 0，没有计入内存总量"]
    );
}

#[test]
fn without_slots_the_memory_line_says_where_else_it_looked() {
    let modules = |smbios, board_memory| {
        field(
            &dedicated(Hardware {
                smbios,
                board_memory,
                ..bare()
            }),
            "内存条",
        )
    };
    assert_eq!(
        modules(smbios(vec![], vec![]), Some(2 << 30)),
        ["板载内存 2.0 GiB（来自设备树）；没有可插拔内存条的信息"]
    );
    assert_eq!(
        modules(smbios(vec![], vec![]), None),
        ["不可用：SMBIOS 里没有内存条信息"]
    );
    assert_eq!(
        modules(Err(Unreadable::Absent), None),
        ["不可用：读不到 SMBIOS"]
    );
    assert_eq!(
        modules(Err(Unreadable::DevMem(io::Error::other("refused"))), None),
        ["不可用：读不到 SMBIOS（/dev/mem：refused）"]
    );
}

#[test]
fn raid_lines_group_disks_and_name_cards_by_their_ids() {
    let raid = |cards: Vec<Card>| {
        let host = HostProfile {
            storage: Read::Done(StorageFacts {
                raid: Some(cards),
                ..StorageFacts::default()
            }),
            ..HostProfile::default()
        };
        field(&render(&host, false), "RAID 卡")
    };
    assert_eq!(raid(vec![]), ["无"]);
    let card = |ids, driver: Option<&str>, disks: Vec<(&str, Attachment)>| Card {
        address: "0000:5e:00.0".to_owned(),
        ids,
        name: None,
        driver: driver.map(str::to_owned),
        disks: disks
            .into_iter()
            .map(|(disk, attachment)| (disk.to_owned(), attachment))
            .collect(),
    };
    let cards = vec![
        card(
            Ids {
                vendor: 0x1000,
                device: 0x005d,
                subsystem: Some((0x1000, 0x9361)),
            },
            Some("megaraid_sas"),
            vec![
                ("sda", Attachment::Array),
                ("sdb", Attachment::PassThrough),
                ("sdc", Attachment::Unknown),
                ("sdd", Attachment::PassThrough),
            ],
        ),
        card(
            Ids {
                vendor: 0x9005,
                device: 0x028f,
                subsystem: None,
            },
            None,
            vec![],
        ),
    ];
    assert_eq!(
        raid(cards),
        [
            concat!(
                "PCI 1000:005d（子系统 1000:9361，驱动 megaraid_sas）：",
                "阵列 sda；直通 sdb、sdd；未知 sdc"
            ),
            "PCI 9005:028f（未加载驱动）：没有连接硬盘",
        ]
    );
}

#[test]
fn a_section_that_did_not_answer_says_so() {
    let host = HostProfile {
        arch: "x86_64".to_owned(),
        hardware: Some(Read::TimedOut),
        cpu: Read::TimedOut,
        memory: Read::Failed,
        storage: Read::TimedOut,
        system: Read::TimedOut,
        ..HostProfile::default()
    };
    let text = render(&host, false);
    let unknown = render(&HostProfile::default(), false);
    for label in ["架构", "硬盘", "文件系统"] {
        assert_eq!(field(&unknown, label), ["不可用"], "{label}: {unknown}");
    }
    for (label, value) in [
        ("硬件", "不可用（超时）"),
        ("CPU", "不可用（超时）"),
        ("内存", "不可用"),
        ("硬盘", "不可用（超时）"),
        ("系统", "不可用（超时）"),
        ("架构", "x86_64"),
    ] {
        assert_eq!(field(&text, label), [value], "{label}: {text}");
    }
}

fn target(text: &str) -> std::net::SocketAddr {
    text.parse().expect("a target")
}

fn found(address: &str, source: &'static str) -> Family {
    Family {
        targets: vec![(target("1.1.1.1:443"), Reach::Connected)],
        address: Address::Found {
            address: address.parse().expect("an address"),
            source,
        },
    }
}

#[test]
fn each_family_says_whether_it_works_and_where_its_address_came_from() {
    let text = |family: &Family, name| report::terminal::family_text(family, name, false);
    assert_eq!(
        text(&found("203.0.113.7", "Cloudflare"), "IPv4"),
        "可用；公网地址 203.0.*.*（来自 Cloudflare）"
    );
    assert_eq!(
        text(&found("2001:db8:85a3::8a2e:370:7334", "ipify"), "IPv6"),
        "可用；公网地址 2001:db8:*（来自 ipify）"
    );
    let failed = Family {
        targets: vec![(target("1.1.1.1:443"), Reach::Connected)],
        address: Address::NotFound(vec![
            ("Cloudflare", Why::TimedOut),
            ("ipify", Why::Status(503)),
            ("icanhazip", Why::NotAnAddress),
        ]),
    };
    assert_eq!(
        text(&failed, "IPv4"),
        "可用；公网地址不可用：Cloudflare 超时，ipify 返回 HTTP 503，icanhazip 返回的不是 IPv4 地址"
    );
    let other = Family {
        address: Address::NotFound(vec![
            ("a", Why::NotPublic),
            ("b", Why::NoAddress),
            ("c", Why::Unreachable),
        ]),
        ..failed
    };
    assert_eq!(
        text(&other, "IPv6"),
        "可用；公网地址不可用：a 返回的不是公网地址，b 没有 IPv6 地址，c 无法连接"
    );
    let no_route = Family {
        targets: vec![
            (target("[2606:4700:4700::1111]:443"), Reach::NoRoute),
            (target("[2400:3200::1]:443"), Reach::NoRoute),
        ],
        address: Address::NotAsked,
    };
    assert_eq!(text(&no_route, "IPv6"), "不可用：本机没有 IPv6 路由");
    let mixed = Family {
        targets: vec![
            (target("1.1.1.1:443"), Reach::TimedOut),
            (target("223.5.5.5:443"), Reach::Refused),
            (target("8.8.8.8:443"), Reach::NoRoute),
            (target("9.9.9.9:443"), Reach::Failed),
        ],
        address: Address::NotAsked,
    };
    assert_eq!(
        text(&mixed, "IPv4"),
        "不可用：连接 1.1.1.1 超时，223.5.5.5 拒绝连接，没有到 8.8.8.8 的路由，无法连接 9.9.9.9"
    );
    assert_eq!(text(&Family::default(), "IPv4"), "不可用", "not probed");
    let not_asked = Family {
        targets: vec![(target("1.1.1.1:443"), Reach::Connected)],
        address: Address::NotAsked,
    };
    assert_eq!(text(&not_asked, "IPv4"), "可用");
}

#[test]
fn addresses_show_in_full_only_on_request() {
    let host = HostProfile {
        network: Network {
            ipv4: found("203.0.113.7", "Cloudflare"),
            ipv6: found("2001:db8::7", "icanhazip"),
        },
        ..HostProfile::default()
    };
    let masked = render(&host, false);
    assert_eq!(
        field(&masked, "IPv4"),
        ["可用；公网地址 203.0.*.*（来自 Cloudflare）"]
    );
    assert_eq!(
        field(&masked, "IPv6"),
        ["可用；公网地址 2001:db8:*（来自 icanhazip）"]
    );
    for full in ["203.0.113.7", "2001:db8::7"] {
        assert!(!masked.contains(full), "{full} in {masked}");
    }
    let shown = render(&host, true);
    assert_eq!(
        field(&shown, "IPv4"),
        ["可用；公网地址 203.0.113.7（来自 Cloudflare）"]
    );
    assert_eq!(
        field(&shown, "IPv6"),
        ["可用；公网地址 2001:db8::7（来自 icanhazip）"]
    );
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../bench-host/tests/fixtures")
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

/// The report of a captured host, on a kernel with nothing in front of `/proc`, unless the
/// kernel's own `uptime` is given, as for the Proxmox VE server, up 61 days, whose load the
/// container shows.
fn captured(name: &str, platform: Platform, work_dir: (u32, u32), uptime: Option<u64>) -> String {
    let root = Root::new(fixture(name).join("root"));
    let counters = Counters {
        uptime: uptime.map_or_else(
            || system::uptime(&root).expect("/proc/uptime"),
            Duration::from_secs,
        ),
        load: system::load(&root).expect("/proc/loadavg"),
    };
    let sources = Sources {
        root,
        arch: if name.contains("aarch64") {
            "aarch64"
        } else {
            "x86_64"
        }
        .to_owned(),
        platform,
        libc: Libc::GlibcUnknown,
        work_dir: Some(work_dir),
        space: df(name),
        kernel: move || Some(counters),
    };
    render(&HostProfile::read(sources, Duration::from_secs(30)), false)
}

#[test]
fn a_kvm_guest_reports_its_vcpus_and_virtio_disk() {
    insta::assert_snapshot!(captured(
        "kvm-debian13-x86_64",
        Platform::Vm(Hypervisor::Kvm),
        (254, 1),
        None
    ));
}

#[test]
fn a_dedicated_server_reports_sockets_media_and_its_zfs_pool() {
    insta::assert_snapshot!(captured(
        "dedicated-pve-x86_64",
        Platform::Dedicated,
        (252, 1),
        None
    ));
}

#[test]
fn an_arm_board_reports_firmware_and_memory_from_u_boot_and_its_device_tree() {
    insta::assert_snapshot!(captured(
        "rock3a-debian13-aarch64",
        Platform::Dedicated,
        (259, 2),
        None
    ));
}

#[test]
fn a_docker_container_shows_the_hosts_uptime_and_load() {
    insta::assert_snapshot!(captured(
        "docker-debian13-aarch64",
        Platform::Container(ContainerKind::Docker),
        (0, 69),
        None
    ));
}

#[test]
fn a_container_reports_its_cgroup_limits_and_own_root() {
    insta::assert_snapshot!(captured(
        "lxc-limits-debian13-aarch64",
        Platform::Container(ContainerKind::Lxc),
        (0, 69),
        None
    ));
}

#[test]
fn a_proxmox_container_reports_what_lxcfs_shows() {
    insta::assert_snapshot!(captured(
        "lxc-pve-debian13-x86_64",
        Platform::Container(ContainerKind::Lxc),
        (0, 75),
        Some(5_322_667)
    ));
}

#[test]
fn platforms_are_named_in_chinese() {
    let names = [
        (Platform::Dedicated, "未检测到虚拟化"),
        (Platform::Vm(Hypervisor::Kvm), "KVM 虚拟机"),
        (Platform::Vm(Hypervisor::Qemu), "QEMU 虚拟机（纯软件模拟）"),
        (Platform::Vm(Hypervisor::Xen), "Xen 虚拟机"),
        (Platform::Vm(Hypervisor::Vmware), "VMware 虚拟机"),
        (Platform::Vm(Hypervisor::HyperV), "Hyper-V 虚拟机"),
        (Platform::Vm(Hypervisor::VirtualBox), "VirtualBox 虚拟机"),
        (Platform::Vm(Hypervisor::Parallels), "Parallels 虚拟机"),
        (Platform::Vm(Hypervisor::AmazonEc2), "Amazon EC2 虚拟机"),
        (
            Platform::Vm(Hypervisor::GoogleCompute),
            "Google Compute Engine 虚拟机",
        ),
        (Platform::Vm(Hypervisor::Bhyve), "bhyve 虚拟机"),
        (Platform::Vm(Hypervisor::Other), "虚拟机（类型未知）"),
        (Platform::Container(ContainerKind::Lxc), "LXC 容器"),
        (Platform::Container(ContainerKind::Openvz), "OpenVZ 容器"),
        (Platform::Container(ContainerKind::Docker), "Docker 容器"),
        (Platform::Container(ContainerKind::Podman), "Podman 容器"),
        (
            Platform::Container(ContainerKind::SystemdNspawn),
            "systemd-nspawn 容器",
        ),
        (Platform::Container(ContainerKind::Wsl), "WSL"),
        (
            Platform::Container(ContainerKind::Other),
            "容器（类型未知）",
        ),
    ];
    for (platform, name) in names {
        assert_eq!(report::terminal::platform_text(platform), name);
        let host = HostProfile {
            platform,
            ..HostProfile::default()
        };
        assert!(
            render(&host, false).contains(&format!("\n运行环境：{name}\n")),
            "{name}"
        );
    }
    let kinds = [
        (PlatformKind::Dedicated, "独立服务器"),
        (PlatformKind::Vm, "虚拟机"),
        (PlatformKind::Container, "容器"),
    ];
    for (kind, name) in kinds {
        assert_eq!(report::terminal::kind_text(kind), name);
    }
}

#[test]
fn an_arm_cpu_unknown_to_the_table_shows_its_codes() {
    let model = |model| {
        let host = HostProfile {
            cpu: Read::Done(CpuFacts {
                cpu: Cpu {
                    model,
                    ..Cpu::default()
                },
                ..CpuFacts::default()
            }),
            ..HostProfile::default()
        };
        field(&render(&host, false), "CPU 型号")
    };
    assert_eq!(
        model(Some(Model::Name("ARM Neoverse-N1".to_owned()))),
        ["ARM Neoverse-N1"]
    );
    assert_eq!(
        model(Some(Model::UnknownPart {
            vendor: "Apple",
            part: 0
        })),
        ["Apple（型号代码 0x000）"]
    );
    assert_eq!(
        model(Some(Model::UnknownVendor {
            implementer: 0x99,
            part: Some(1)
        })),
        ["ARM 处理器（厂商代码 0x99，型号代码 0x001）"]
    );
    assert_eq!(
        model(Some(Model::UnknownVendor {
            implementer: 0x99,
            part: None
        })),
        ["ARM 处理器（厂商代码 0x99）"]
    );
    assert_eq!(model(None), ["不可用"]);
}
