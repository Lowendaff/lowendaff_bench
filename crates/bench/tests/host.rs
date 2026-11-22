//! [System]: the host profile in words. The six captured hosts (bench-host's fixtures) are
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
use bench_host::cards::Bus;
use bench_host::cards::Duplex;
use bench_host::cards::Link;
use bench_host::cards::Master;
use bench_host::cards::MasterKind;
use bench_host::cards::NetworkCard;
use bench_host::cards::Pcie;
use bench_host::cards::PcieLink;
use bench_host::cards::Routes;
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
use bench_host::profile::Seen;
use bench_host::profile::Sources;
use bench_host::profile::StorageFacts;
use bench_host::profile::SystemFacts;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::raid::Ids;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Smbios;
use bench_host::smbios::Unreadable;
use bench_host::storage::Disk;
use bench_host::storage::Space;
use bench_host::system;
use bench_host::system::Counters;
use unicode_width::UnicodeWidthStr;

/// The report of a run without plugins: its header, [System] and an empty summary.
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
    let fill = 18_usize.saturating_sub(label.width());
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

#[allow(clippy::unnecessary_wraps)]
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

#[allow(clippy::unnecessary_wraps)]
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
        ("Machine", "unavailable"),
        ("Board", "unavailable"),
        ("Firmware", "unavailable"),
        ("BMC", "not detected"),
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
    assert_eq!(field(&text, "Machine"), ["Radxa ROCK 3A"]);
    assert_eq!(field(&text, "Board"), ["Supermicro X11DPi-N"]);
    // Only month/day/year becomes year-month-day.
    assert_eq!(field(&text, "BIOS"), ["1.2 (2024/01/05)"]);
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
        firmware(Firmware::UBoot("2024.01".to_owned()), "Firmware"),
        ["U-Boot 2024.01"]
    );
    assert_eq!(
        firmware(Firmware::RaspberryPi(1_713_139_200), "Firmware"),
        ["Raspberry Pi bootloader, built 2024-04-15"]
    );
    // 2024-02-29 23:59:59 UTC and the epoch.
    assert_eq!(
        firmware(Firmware::RaspberryPi(1_709_251_199), "Firmware"),
        ["Raspberry Pi bootloader, built 2024-02-29"]
    );
    assert_eq!(
        firmware(Firmware::RaspberryPi(0), "Firmware"),
        ["Raspberry Pi bootloader, built 1970-01-01"]
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
        ["vendor 12345, IPMI 1.5"]
    );
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: Some(10876),
            firmware: Some("1.73.14".to_owned()),
            ipmi: Some("2.0".to_owned()),
        }),
        ["Supermicro, firmware 1.73.14, IPMI 2.0"]
    );
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: None,
            firmware: None,
            ipmi: None,
        }),
        ["present"]
    );
    let interface = |kind, version| {
        bmc(Bmc::Interface(Interface {
            kind: Some(kind),
            version,
        }))
    };
    assert_eq!(
        interface(InterfaceKind::Ssif, Some((2, 0))),
        [
            "present (IPMI 2.0, SSIF interface); vendor and firmware unavailable: the kernel has \
         not loaded the IPMI driver"
        ]
    );
    assert_eq!(
        interface(InterfaceKind::Smic, None),
        [
            "present (SMIC interface); vendor and firmware unavailable: the kernel has not loaded \
         the IPMI driver"
        ]
    );
    assert_eq!(
        interface(InterfaceKind::Bt, Some((1, 5))),
        [
            "present (IPMI 1.5, BT interface); vendor and firmware unavailable: the kernel has not \
         loaded the IPMI driver"
        ]
    );
    assert_eq!(
        interface(InterfaceKind::Kcs, None),
        [
            "present (KCS interface); vendor and firmware unavailable: the kernel has not loaded \
         the IPMI driver"
        ]
    );
    let nothing = [
        "present; interface, vendor and firmware unavailable: the kernel has not \
     loaded the IPMI driver",
    ];
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
        ["yes (multi-bit correction), yes (CRC); since boot 3 corrected, 1 uncorrected"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![Ecc::Parity]), None),
        [
            "no correction (parity only); error counts unavailable: the kernel has not loaded an \
         EDAC driver"
        ]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![Ecc::Unknown]), None),
        ["unknown; error counts unavailable: the kernel has not loaded an EDAC driver"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![Ecc::None]), edac(0, 0, vec![])),
        ["none"]
    );
    // Without type 16, EDAC's mode.
    assert_eq!(
        ecc(smbios(vec![], vec![]), edac(0, 0, vec![Ecc::SingleBit])),
        ["yes (single-bit correction); since boot 0 corrected, 0 uncorrected"]
    );
    assert_eq!(
        ecc(Err(Unreadable::Absent), edac(2, 0, vec![])),
        ["way of correction unavailable; since boot 2 corrected, 0 uncorrected"]
    );
    assert_eq!(
        ecc(smbios(vec![], vec![]), None),
        ["unavailable: SMBIOS has no word on error correction, and there is no EDAC driver"]
    );
    assert_eq!(
        ecc(Err(Unreadable::Absent), None),
        ["unavailable: there is no SMBIOS, and there is no EDAC driver"]
    );
    assert_eq!(
        ecc(Err(Unreadable::DevMem(io::Error::other("refused"))), None),
        ["unavailable: SMBIOS cannot be read, and there is no EDAC driver"]
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
        table(&text, "Memory modules"),
        [
            "Slot | Size | Type | Speed | Vendor | Model",
            "DIMM 0 | 16 GB | DDR5 | 2666 MT/s (rated 2933) | Micron | MTC20F2085S1RC48BA1",
            "DIMM 1 | 512 MB | RDIMM | 2933 MT/s (rated) | — | —",
            "Slot 3 | 512 KB | — | 3200 MT/s | 80CE | —",
            "DIMM 3 | unknown | — | — | — | —",
            "DIMM 4 | not in use | DDR4 RDIMM | 2400 MT/s | Hynix | HMA82GR7AFR8N-UH",
            "DIMM 5 | empty | — | — | — | —",
        ]
    );
    assert_eq!(
        field(&text, "Note"),
        [
            "DIMM 4 holds a module that the firmware reports as not in use, with a size of 0; the \
         total leaves it out"
        ]
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
            "Memory modules",
        )
    };
    assert_eq!(
        modules(smbios(vec![], vec![]), Some(2 << 30)),
        ["onboard memory 2.0 GiB (from the device tree); no word on removable modules"]
    );
    assert_eq!(
        modules(smbios(vec![], vec![]), None),
        ["unavailable: SMBIOS describes no modules"]
    );
    assert_eq!(
        modules(Err(Unreadable::Absent), None),
        ["unavailable: SMBIOS cannot be read"]
    );
    assert_eq!(
        modules(Err(Unreadable::DevMem(io::Error::other("refused"))), None),
        ["unavailable: SMBIOS cannot be read (/dev/mem: refused)"]
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
        field(&render(&host, false), "RAID card")
    };
    assert_eq!(raid(vec![]), ["none"]);
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
                "PCI 1000:005d (subsystem 1000:9361, driver megaraid_sas): ",
                "array sda; pass-through sdb, sdd; unknown sdc"
            ),
            "PCI 9005:028f (no driver loaded): no disks attached",
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
    for label in ["Architecture", "Disks", "Filesystems"] {
        assert_eq!(
            field(&unknown, label),
            ["unavailable"],
            "{label}: {unknown}"
        );
    }
    for (label, value) in [
        ("Hardware", "unavailable (timed out)"),
        ("CPU", "unavailable (timed out)"),
        ("Memory", "unavailable"),
        ("Disks", "unavailable (timed out)"),
        ("OS", "unavailable (timed out)"),
        ("Architecture", "x86_64"),
    ] {
        assert_eq!(field(&text, label), [value], "{label}: {text}");
    }
}

#[test]
fn the_uptime_counts_days_hours_and_minutes_in_the_singular_for_one() {
    let uptime = |seconds| {
        let host = HostProfile {
            system: Read::Done(SystemFacts {
                uptime: Some(Seen {
                    value: Duration::from_secs(seconds),
                    host: false,
                }),
                ..SystemFacts::default()
            }),
            ..HostProfile::default()
        };
        field(&render(&host, false), "Uptime")
    };
    for (seconds, shown) in [
        (59, "0 minutes"),
        (60, "1 minute"),
        (3_600, "1 hour, 0 minutes"),
        (2 * 3_600 + 120, "2 hours, 2 minutes"),
        (86_400 + 3_660, "1 day, 1 hour, 1 minute"),
        (2 * 86_400, "2 days, 0 hours, 0 minutes"),
    ] {
        assert_eq!(uptime(seconds), [shown], "{seconds} s");
    }
}

/// A network card on the PCI bus, down and in no bridge, whose fields the tests change.
fn network_card(interface: &str) -> NetworkCard {
    NetworkCard {
        interface: interface.to_owned(),
        bus: Bus::Pci {
            vendor: 0x8086,
            device: 0x1521,
        },
        model: Some("Intel I350 Gigabit Network Connection".to_owned()),
        driver: Some("igb".to_owned()),
        link: Link::Down,
        mtu: Some(1500),
        pcie: None,
        virtual_functions: 0,
        master: None,
        default_route: Routes::default(),
    }
}

/// The report of a host on this platform with these network cards, and nothing else known.
fn with_cards(platform: Platform, cards: Read<Vec<NetworkCard>>) -> String {
    render(
        &HostProfile {
            platform,
            cards,
            ..HostProfile::default()
        },
        false,
    )
}

#[test]
fn network_cards_show_their_names_links_and_uses() {
    let up = |speed, duplex| Link::Up { speed, duplex };
    let master = |name: &str, kind| {
        Some(Master {
            name: name.to_owned(),
            kind,
        })
    };
    let cards = vec![
        NetworkCard {
            // An Aquantia AQC107; ids keep their leading zeros.
            bus: Bus::Pci {
                vendor: 0x1d6a,
                device: 0x07b1,
            },
            model: None,
            driver: None,
            mtu: None,
            ..network_card("enp1s0")
        },
        NetworkCard {
            bus: Bus::Usb,
            model: None,
            link: up(Some(100), Some(Duplex::Half)),
            default_route: Routes {
                ipv4: false,
                ipv6: true,
            },
            ..network_card("enx1")
        },
        NetworkCard {
            bus: Bus::Platform(None),
            model: None,
            link: up(Some(2500), Some(Duplex::Full)),
            master: master("bond0", MasterKind::Bond),
            default_route: Routes {
                ipv4: true,
                ipv6: true,
            },
            ..network_card("eth0")
        },
        // Hyper-V's cards are on its own bus, which udev's database has no names for.
        NetworkCard {
            bus: Bus::Other("vmbus".to_owned()),
            model: None,
            link: up(Some(25_000), None),
            master: master("team0", MasterKind::Other),
            virtual_functions: 1,
            ..network_card("eth1")
        },
        NetworkCard {
            bus: Bus::Other(String::new()),
            model: None,
            link: up(None, None),
            master: master("vmbr0", MasterKind::Bridge),
            default_route: Routes {
                ipv4: true,
                ipv6: false,
            },
            virtual_functions: 8,
            ..network_card("eth2")
        },
        NetworkCard {
            link: up(Some(1000), Some(Duplex::Full)),
            ..network_card("eth3")
        },
    ];
    let text = with_cards(Platform::Dedicated, Read::Done(cards));
    assert_eq!(
        table(&text, "Network cards"),
        [
            "Interface | Model | Driver | Link | MTU | Use",
            "enp1s0 | PCI 1d6a:07b1 | — | down | — | —",
            "enx1 | USB adapter | igb | 100 Mbit/s, half duplex | 1500 | default route (IPv6)",
            concat!(
                "eth0 | built into the board | igb | 2.5 Gbit/s, full duplex | 1500 | ",
                "bond bond0, default route (IPv4, IPv6)"
            ),
            "eth1 | vmbus | igb | 25 Gbit/s | 1500 | in team0, 1 virtual function",
            concat!(
                "eth2 | — | igb | not reported | 1500 | ",
                "bridge vmbr0, default route (IPv4), 8 virtual functions"
            ),
            concat!(
                "eth3 | Intel I350 Gigabit Network Connection | igb | 1 Gbit/s, full duplex | ",
                "1500 | —"
            ),
        ]
    );
    assert_eq!(field(&text, "Note"), [] as [&str; 0], "{text}");
}

#[test]
fn a_card_whose_pcie_link_runs_below_its_best_has_a_note() {
    let link = |generation, lanes| PcieLink { generation, lanes };
    let pcie = |current, best| Some(Pcie { current, best });
    let cards = vec![
        NetworkCard {
            pcie: pcie(link(3, 4), link(3, 8)),
            ..network_card("ens1f0")
        },
        NetworkCard {
            pcie: pcie(link(2, 8), link(3, 8)),
            ..network_card("ens2f0")
        },
        NetworkCard {
            pcie: pcie(link(3, 8), link(3, 8)),
            ..network_card("ens3f0")
        },
        network_card("ens4f0"),
    ];
    let text = with_cards(Platform::Dedicated, Read::Done(cards));
    assert_eq!(
        field(&text, "Note"),
        [
            "ens1f0's card is linked at PCIe 3.0 x4, below the PCIe 3.0 x8 it supports",
            "ens2f0's card is linked at PCIe 2.0 x8, below the PCIe 3.0 x8 it supports",
        ]
    );
    assert_eq!(table(&text, "Network cards").len(), 5, "{text}");
}

#[test]
fn without_cards_the_line_says_why() {
    let docker = Platform::Container(ContainerKind::Docker);
    for (platform, cards, value) in [
        (Platform::Dedicated, Read::Done(vec![]), "none found"),
        (
            docker,
            Read::Done(vec![]),
            "none visible inside a container",
        ),
        (Platform::Vm(Hypervisor::Kvm), Read::Failed, "unavailable"),
        (docker, Read::TimedOut, "unavailable (timed out)"),
    ] {
        let text = with_cards(platform, cards);
        assert_eq!(field(&text, "Network cards"), [value], "{text}");
    }
}

#[test]
fn a_disk_with_nothing_on_it_is_not_mounted() {
    let host = HostProfile {
        storage: Read::Done(StorageFacts {
            disks: vec![Disk {
                name: "sdb".to_owned(),
                interface: "SATA",
                size: 500 << 30,
                rotational: Some(true),
                model: None,
                uses: Vec::new(),
            }],
            ..StorageFacts::default()
        }),
        ..HostProfile::default()
    };
    assert_eq!(
        table(&render(&host, false), "Disks"),
        [
            "Device | Interface | Size | Medium | Model | Use",
            "sdb | SATA | 500.0 GiB | HDD | — | not mounted",
        ]
    );
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
        "available; public address 203.0.*.* (from Cloudflare)"
    );
    assert_eq!(
        text(&found("2001:db8:85a3::8a2e:370:7334", "ipify"), "IPv6"),
        "available; public address 2001:db8:* (from ipify)"
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
        "available; public address unavailable: Cloudflare timed out, ipify returned HTTP 503, \
         icanhazip returned no IPv4 address"
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
        "available; public address unavailable: a returned no public address, b has no IPv6 \
         address, c cannot be reached"
    );
    let no_route = Family {
        targets: vec![
            (target("[2606:4700:4700::1111]:443"), Reach::NoRoute),
            (target("[2400:3200::1]:443"), Reach::NoRoute),
        ],
        address: Address::NotAsked,
    };
    assert_eq!(
        text(&no_route, "IPv6"),
        "unavailable: this host has no IPv6 route"
    );
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
        "unavailable: connecting to 1.1.1.1 timed out, 223.5.5.5 refused the connection, no \
         route to 8.8.8.8, cannot connect to 9.9.9.9"
    );
    assert_eq!(
        text(&Family::default(), "IPv4"),
        "unavailable",
        "not probed"
    );
    let not_asked = Family {
        targets: vec![(target("1.1.1.1:443"), Reach::Connected)],
        address: Address::NotAsked,
    };
    assert_eq!(text(&not_asked, "IPv4"), "available");
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
        ["available; public address 203.0.*.* (from Cloudflare)"]
    );
    assert_eq!(
        field(&masked, "IPv6"),
        ["available; public address 2001:db8:* (from icanhazip)"]
    );
    for full in ["203.0.113.7", "2001:db8::7"] {
        assert!(!masked.contains(full), "{full} in {masked}");
    }
    let shown = render(&host, true);
    assert_eq!(
        field(&shown, "IPv4"),
        ["available; public address 203.0.113.7 (from Cloudflare)"]
    );
    assert_eq!(
        field(&shown, "IPv6"),
        ["available; public address 2001:db8::7 (from icanhazip)"]
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
fn each_platform_has_its_name() {
    let names = [
        (Platform::Dedicated, "no virtualization detected"),
        (Platform::Vm(Hypervisor::Kvm), "KVM virtual machine"),
        (
            Platform::Vm(Hypervisor::Qemu),
            "QEMU virtual machine (pure software emulation)",
        ),
        (Platform::Vm(Hypervisor::Xen), "Xen virtual machine"),
        (Platform::Vm(Hypervisor::Vmware), "VMware virtual machine"),
        (Platform::Vm(Hypervisor::HyperV), "Hyper-V virtual machine"),
        (
            Platform::Vm(Hypervisor::VirtualBox),
            "VirtualBox virtual machine",
        ),
        (
            Platform::Vm(Hypervisor::Parallels),
            "Parallels virtual machine",
        ),
        (
            Platform::Vm(Hypervisor::AmazonEc2),
            "Amazon EC2 virtual machine",
        ),
        (
            Platform::Vm(Hypervisor::GoogleCompute),
            "Google Compute Engine virtual machine",
        ),
        (Platform::Vm(Hypervisor::Bhyve), "bhyve virtual machine"),
        (
            Platform::Vm(Hypervisor::Other),
            "virtual machine (type unknown)",
        ),
        (Platform::Container(ContainerKind::Lxc), "LXC container"),
        (
            Platform::Container(ContainerKind::Openvz),
            "OpenVZ container",
        ),
        (
            Platform::Container(ContainerKind::Docker),
            "Docker container",
        ),
        (
            Platform::Container(ContainerKind::Podman),
            "Podman container",
        ),
        (
            Platform::Container(ContainerKind::SystemdNspawn),
            "systemd-nspawn container",
        ),
        (Platform::Container(ContainerKind::Wsl), "WSL"),
        (
            Platform::Container(ContainerKind::Other),
            "container (type unknown)",
        ),
    ];
    for (platform, name) in names {
        assert_eq!(report::terminal::platform_text(platform), name);
        let host = HostProfile {
            platform,
            ..HostProfile::default()
        };
        assert!(
            render(&host, false).contains(&format!("\nEnvironment: {name}\n")),
            "{name}"
        );
    }
    let kinds = [
        (PlatformKind::Dedicated, "dedicated server"),
        (PlatformKind::Vm, "virtual machine"),
        (PlatformKind::Container, "container"),
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
        field(&render(&host, false), "CPU model")
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
        ["Apple (part code 0x000)"]
    );
    assert_eq!(
        model(Some(Model::UnknownVendor {
            implementer: 0x99,
            part: Some(1)
        })),
        ["ARM CPU (vendor code 0x99, part code 0x001)"]
    );
    assert_eq!(
        model(Some(Model::UnknownVendor {
            implementer: 0x99,
            part: None
        })),
        ["ARM CPU (vendor code 0x99)"]
    );
    assert_eq!(model(None), ["unavailable"]);
}
