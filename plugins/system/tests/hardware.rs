//! The hardware lines of dedicated servers (system information design §9.3–§9.6), from
//! constructed host data: each form a line can take.

use std::io;

use bench_api::Record;
use bench_host::bmc::Bmc;
use bench_host::bmc::Interface;
use bench_host::bmc::InterfaceKind;
use bench_host::edac::Edac;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::machine::Machine;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::raid::Ids;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Smbios;
use bench_host::smbios::Unreadable;
use plugin_system::hardware;
use plugin_system::hardware::Hardware;

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

fn table(slots: Vec<Slot>, ecc: Vec<Ecc>) -> Result<Smbios, Unreadable> {
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

fn machine_lines(hardware: &Hardware) -> Vec<String> {
    let mut records = Vec::new();
    hardware::machine_records(&mut records, hardware);
    lines(&records)
}

fn memory_lines(hardware: &Hardware) -> Vec<String> {
    let mut records = Vec::new();
    hardware::memory_records(&mut records, hardware).expect("memory records");
    lines(&records)
}

fn line(vendor: Option<&str>, name: Option<&str>, detail: Option<&str>) -> Line {
    Line {
        vendor: vendor.map(str::to_owned),
        name: name.map(str::to_owned),
        detail: detail.map(str::to_owned),
    }
}

#[test]
fn machine_lines_drop_missing_parts_and_repeated_vendors() {
    assert_eq!(
        machine_lines(&bare()),
        [
            "整机：不可用",
            "主板：不可用",
            "固件：不可用",
            "BMC：未检测到"
        ]
    );
    let hardware = Hardware {
        machine: Machine {
            system: line(Some("radxa"), Some("Radxa ROCK 3A"), None),
            board: line(Some("Supermicro"), Some("X11DPi-N"), None),
            firmware: Some(Firmware::Bios(line(None, Some("1.2"), Some("2024/01/05")))),
        },
        ..bare()
    };
    assert_eq!(
        machine_lines(&hardware)[..3],
        [
            "整机：Radxa ROCK 3A",
            "主板：Supermicro X11DPi-N",
            // Only month/day/year becomes year-month-day.
            "BIOS：1.2（2024/01/05）",
        ]
    );
    let firmware = |firmware| {
        machine_lines(&Hardware {
            machine: Machine {
                firmware: Some(firmware),
                ..bare().machine
            },
            ..bare()
        })[2]
            .clone()
    };
    assert_eq!(
        firmware(Firmware::Bios(line(None, None, Some("7/4/2023")))),
        "BIOS：2023-07-04"
    );
    assert_eq!(
        firmware(Firmware::UBoot("2024.01".to_owned())),
        "固件：U-Boot 2024.01"
    );
    assert_eq!(
        firmware(Firmware::RaspberryPi(1_713_139_200)),
        "固件：树莓派引导程序，2024-04-15 构建"
    );
    // 2024-02-29 23:59:59 UTC and the epoch.
    assert_eq!(
        firmware(Firmware::RaspberryPi(1_709_251_199)),
        "固件：树莓派引导程序，2024-02-29 构建"
    );
    assert_eq!(
        firmware(Firmware::RaspberryPi(0)),
        "固件：树莓派引导程序，1970-01-01 构建"
    );
}

#[test]
fn bmc_lines_say_what_is_missing_and_why() {
    let bmc = |bmc| {
        machine_lines(&Hardware {
            bmc: Some(bmc),
            ..bare()
        })[3]
            .clone()
    };
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: Some(12345),
            firmware: None,
            ipmi: Some("1.5".to_owned()),
        }),
        "BMC：厂商编号 12345，IPMI 1.5"
    );
    assert_eq!(
        bmc(Bmc::Driver {
            vendor: None,
            firmware: None,
            ipmi: None,
        }),
        "BMC：有"
    );
    let interface = |kind, version| {
        bmc(Bmc::Interface(Interface {
            kind: Some(kind),
            version,
        }))
    };
    assert_eq!(
        interface(InterfaceKind::Ssif, Some((2, 0))),
        "BMC：有（IPMI 2.0，SSIF 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"
    );
    assert_eq!(
        interface(InterfaceKind::Smic, None),
        "BMC：有（SMIC 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"
    );
    assert_eq!(
        interface(InterfaceKind::Bt, Some((1, 5))),
        "BMC：有（IPMI 1.5，BT 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动"
    );
    let nothing = "BMC：有；接口、厂商和固件不可用：内核没有加载 IPMI 驱动";
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
        memory_lines(&Hardware {
            smbios,
            edac,
            ..bare()
        })[0]
            .clone()
    };
    assert_eq!(
        ecc(
            table(vec![], vec![Ecc::MultiBit, Ecc::Crc]),
            edac(3, 1, vec![])
        ),
        "ECC：有（多比特纠错）、有（CRC 校验）；本次开机以来已纠正 3 次，不可纠正 1 次"
    );
    assert_eq!(
        ecc(table(vec![], vec![Ecc::Parity]), None),
        "ECC：无纠错（只有奇偶校验）；错误次数不可用：内核没有加载 EDAC 驱动"
    );
    assert_eq!(
        ecc(table(vec![], vec![Ecc::Unknown]), None),
        "ECC：未知；错误次数不可用：内核没有加载 EDAC 驱动"
    );
    assert_eq!(
        ecc(table(vec![], vec![Ecc::None]), edac(0, 0, vec![])),
        "ECC：无"
    );
    // Without type 16, EDAC's mode.
    assert_eq!(
        ecc(table(vec![], vec![]), edac(0, 0, vec![Ecc::SingleBit])),
        "ECC：有（单比特纠错）；本次开机以来已纠正 0 次，不可纠正 0 次"
    );
    assert_eq!(
        ecc(Err(Unreadable::Absent), edac(2, 0, vec![])),
        "ECC：纠错方式不可用；本次开机以来已纠正 2 次，不可纠正 0 次"
    );
    assert_eq!(
        ecc(table(vec![], vec![]), None),
        "ECC：不可用：SMBIOS 里没有内存纠错信息，也没有 EDAC 驱动"
    );
    assert_eq!(
        ecc(Err(Unreadable::Absent), None),
        "ECC：不可用：没有 SMBIOS，也没有 EDAC 驱动"
    );
    assert_eq!(
        ecc(Err(Unreadable::DevMem(io::Error::other("refused"))), None),
        "ECC：不可用：读不到 SMBIOS，也没有 EDAC 驱动"
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
    assert_eq!(
        memory_lines(&Hardware {
            smbios: table(slots, vec![Ecc::SingleBit]),
            ..bare()
        })[1..],
        [
            "内存条：",
            "插槽 | 容量 | 类型 | 速度 | 厂商 | 型号",
            "DIMM 0 | 16 GB | DDR5 | 2666 MT/s（额定 2933） | Micron | MTC20F2085S1RC48BA1",
            "DIMM 1 | 512 MB | RDIMM | 2933 MT/s（额定） | — | —",
            "插槽 3 | 512 KB | — | 3200 MT/s | 80CE | —",
            "DIMM 3 | 未知 | — | — | — | —",
            "DIMM 4 | 未启用 | DDR4 RDIMM | 2400 MT/s | Hynix | HMA82GR7AFR8N-UH",
            "DIMM 5 | 空 | — | — | — | —",
            "说明：DIMM 4 插着内存条但没有启用：固件报告容量为 0，没有计入内存总量",
        ]
    );
}

#[test]
fn without_slots_the_memory_line_says_where_else_it_looked() {
    let modules = |smbios, board_memory| {
        memory_lines(&Hardware {
            smbios,
            board_memory,
            ..bare()
        })[1..]
            .to_vec()
    };
    assert_eq!(
        modules(table(vec![], vec![]), Some(2 << 30)),
        ["内存条：板载内存 2.0 GiB（来自设备树）；没有可插拔内存条的信息"]
    );
    assert_eq!(
        modules(table(vec![], vec![]), None),
        ["内存条：不可用：SMBIOS 里没有内存条信息"]
    );
    assert_eq!(
        modules(Err(Unreadable::Absent), None),
        ["内存条：不可用：读不到 SMBIOS"]
    );
    assert_eq!(
        modules(Err(Unreadable::DevMem(io::Error::other("refused"))), None),
        ["内存条：不可用：读不到 SMBIOS（/dev/mem：refused）"]
    );
}

#[test]
fn raid_lines_group_disks_and_name_cards_by_their_ids() {
    let mut records = Vec::new();
    hardware::raid_records(&mut records, &[]);
    assert_eq!(lines(&records), ["RAID 卡：无"]);
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
    let cards = [
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
    let mut records = Vec::new();
    hardware::raid_records(&mut records, &cards);
    assert_eq!(
        lines(&records),
        [
            concat!(
                "RAID 卡：PCI 1000:005d（子系统 1000:9361，驱动 megaraid_sas）：",
                "阵列 sda；直通 sdb、sdd；未知 sdc"
            ),
            "RAID 卡：PCI 9005:028f（未加载驱动）：没有连接硬盘",
        ]
    );
    let ids: Vec<&str> = records.iter().map(Record::id).collect();
    assert_eq!(ids, ["raid.1", "raid.2"]);
}
