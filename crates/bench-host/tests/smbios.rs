//! The SMBIOS table (system information design §9.4, §9.6): what the captured hosts' tables
//! hold, each source the table can come from, and the rules for synthetic structures.

mod common;

use std::collections::HashMap;

use bench_host::Root;
use bench_host::bmc::Interface;
use bench_host::bmc::InterfaceKind;
use bench_host::machine::Line;
use bench_host::smbios;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Smbios;
use bench_host::smbios::Unreadable;
use common::Device;
use tempfile::TempDir;

const SERVER: &str = "dedicated-pve-x86_64";
const BOARD: &str = "rock3a-debian13-aarch64";

fn server() -> Smbios {
    smbios::read(&common::fixture(SERVER), "x86_64").expect("the server's table")
}

fn table() -> Vec<u8> {
    common::fixture_bytes(SERVER, "/sys/firmware/dmi/tables/DMI")
}

fn read(dir: &TempDir, arch: &str) -> Result<Smbios, Unreadable> {
    smbios::read(&Root::new(dir.path()), arch)
}

fn line(vendor: &str, name: &str, detail: Option<&str>) -> Line {
    Line {
        vendor: Some(vendor.to_owned()),
        name: Some(name.to_owned()),
        detail: detail.map(str::to_owned),
    }
}

fn samsung(name: &str, capacity: Capacity) -> Slot {
    Slot {
        name: Some(name.to_owned()),
        capacity,
        generation: Some("DDR4"),
        form: Some("LRDIMM"),
        speed: Some(2133),
        configured_speed: Some(2133),
        manufacturer: Some("Samsung".to_owned()),
        part_number: Some("M386A4G40DM0-CPB".to_owned()),
    }
}

fn empty(name: &str) -> Slot {
    Slot {
        name: Some(name.to_owned()),
        capacity: Capacity::Empty,
        generation: None,
        form: None,
        speed: None,
        configured_speed: None,
        manufacturer: None,
        part_number: None,
    }
}

#[test]
fn the_server_has_eight_modules_of_which_two_are_not_in_use() {
    let smbios = server();
    assert_eq!(smbios.slots.len(), 24);
    assert_eq!(smbios.slots[0], samsung("CPU0_C0D0", Capacity::Disabled));
    assert_eq!(smbios.slots[1], empty("CPU0_C0D1"));
    assert_eq!(smbios.slots[2], samsung("CPU0_C1D0", Capacity::Disabled));
    assert_eq!(
        smbios.slots[6],
        samsung("CPU0_C3D0", Capacity::Kilobytes(32 << 20))
    );
    let count = |capacity| {
        smbios
            .slots
            .iter()
            .filter(|slot| slot.capacity == capacity)
            .count()
    };
    assert_eq!(
        (
            count(Capacity::Disabled),
            count(Capacity::Kilobytes(32 << 20)),
            count(Capacity::Empty)
        ),
        (2, 6, 16)
    );
    assert_eq!(smbios.ecc, [Ecc::SingleBit]);
    assert_eq!(
        smbios.bmc,
        Some(Interface {
            kind: Some(InterfaceKind::Kcs),
            version: Some((2, 0)),
        })
    );
    assert_eq!(smbios.system, line("Inspur", "SSINSPURY32-XI5-25D-1", None));
    assert_eq!(
        smbios.board,
        line("Inspur", "YZMB-00870-105", Some("SA5112M5"))
    );
    assert_eq!(
        smbios.bios,
        line("American Megatrends Inc.", "4.1.19", Some("02/07/2022"))
    );
}

#[test]
fn the_server_slots_match_dmidecode() {
    let smbios = server();
    let truth = common::truth(SERVER, "dmidecode");
    let devices: Vec<HashMap<&str, &str>> = truth
        .split("\n\n")
        .filter(|section| section.contains("\nMemory Device\n"))
        .map(|section| {
            section
                .lines()
                .filter_map(|line| line.trim().split_once(": "))
                .collect()
        })
        .collect();
    assert_eq!(devices.len(), smbios.slots.len());
    let pairs: Vec<_> = devices.iter().zip(&smbios.slots).collect();
    for (device, slot) in pairs {
        assert_eq!(slot.name.as_deref(), Some(device["Locator"]));
        match device["Size"] {
            "32 GB" => assert_eq!(slot.capacity, Capacity::Kilobytes(32 << 20)),
            "No Module Installed" => assert!(
                matches!(slot.capacity, Capacity::Empty | Capacity::Disabled),
                "{:?} has no module by dmidecode",
                slot.name
            ),
            size => panic!("unexpected size {size}"),
        }
        // dmidecode prints a slot's type for every slot, but stops before the speed, maker and
        // part number when the size is 0: it shows no more of the modules not in use.
        let generation = (device["Type"] == "DDR4").then_some("DDR4");
        assert_eq!(slot.generation, generation, "{:?}", slot.name);
        if let Capacity::Kilobytes(_) = slot.capacity {
            assert_eq!(slot.manufacturer.as_deref(), Some(device["Manufacturer"]));
            assert_eq!(slot.part_number.as_deref(), Some(device["Part Number"]));
            assert_eq!(slot.speed, Some(2133), "{}", device["Speed"]);
        }
    }
}

#[test]
fn the_board_table_has_no_memory_ecc_or_bmc() {
    let smbios = smbios::read(&common::fixture(BOARD), "aarch64").expect("the board's table");
    assert_eq!(smbios.slots, []);
    assert_eq!(smbios.ecc, []);
    assert_eq!(smbios.bmc, None);
    assert_eq!(smbios.system, line("radxa", "Radxa ROCK 3A", None));
    assert_eq!(smbios.board, line("radxa", "Radxa ROCK 3A", None));
    assert_eq!(
        smbios.bios,
        line("U-Boot", "2026.07-rc4-ga7830e87555a", Some("07/01/2026"))
    );
}

#[test]
fn dmi_sysfs_entries_join_to_the_same_table() {
    let dir = common::host((&[], &[]));
    let mut instances: HashMap<u8, usize> = HashMap::new();
    let structures = common::split(&table());
    assert_eq!(structures.len(), 100);
    let numbered: Vec<_> = structures.iter().enumerate().collect();
    for (position, structure) in numbered {
        let instance = instances.entry(structure[0]).or_default();
        let base = format!("/sys/firmware/dmi/entries/{}-{instance}", structure[0]);
        *instance += 1;
        common::add_bytes(
            &dir,
            &format!("{base}/position"),
            format!("{position}\n").as_bytes(),
        );
        common::add_bytes(&dir, &format!("{base}/raw"), structure);
    }
    assert_eq!(read(&dir, "aarch64").expect("entries"), server());
}

/// `/dev/mem` with the server's entry point and table where its firmware put them.
fn server_memory(dir: &TempDir) {
    let mut entry = common::fixture_bytes(SERVER, "/sys/firmware/dmi/tables/smbios_entry_point");
    entry.resize(32, 0);
    common::add_sparse(
        dir,
        "/dev/mem",
        &[(0x6d40_8000, &table()), (0x6d40_b000, &entry)],
    );
}

#[test]
fn dev_mem_at_the_efi_address_gives_the_same_table_on_x86_only() {
    let dir = common::host((&[], &[]));
    common::add_bytes(
        &dir,
        "/sys/firmware/efi/systab",
        &common::fixture_bytes(SERVER, "/sys/firmware/efi/systab"),
    );
    server_memory(&dir);
    assert_eq!(read(&dir, "x86_64").expect("/dev/mem"), server());
    assert!(matches!(read(&dir, "aarch64"), Err(Unreadable::Absent)));
}

#[test]
fn dev_mem_through_a_2x_entry_point() {
    let table = table();
    let length = u16::try_from(table.len()).expect("length");
    let dir = common::host((&[("/sys/firmware/efi/systab", "SMBIOS=0x1000\n")], &[]));
    let entry = common::entry_point_2(0x2000, length, 100);
    common::add_sparse(&dir, "/dev/mem", &[(0x1000, &entry), (0x2000, &table)]);
    assert_eq!(read(&dir, "x86_64").expect("2.x entry point"), server());
}

#[test]
fn the_legacy_scan_prefers_a_3x_entry_point() {
    let table = table();
    let length = u32::try_from(table.len()).expect("length");
    let other = [common::memory_array(0x10, 3, 3), common::end_of_table(0x11)].concat();
    let dir = common::host((&[], &[]));
    // The 2.x entry point comes first in the segment but points at another table.
    let entry_2 = common::entry_point_2(0x20_0000, u16::try_from(other.len()).expect("length"), 2);
    let entry_3 = common::entry_point_3(0x10_0000, length);
    common::add_sparse(
        &dir,
        "/dev/mem",
        &[
            (0xF_0010, &entry_2),
            (0xF_8000, &entry_3),
            (0x10_0000, &table),
            (0x20_0000, &other),
        ],
    );
    assert_eq!(read(&dir, "x86_64").expect("legacy scan"), server());
    // Without the 3.x entry point, the 2.x one is used.
    let dir = common::host((&[], &[]));
    common::add_sparse(
        &dir,
        "/dev/mem",
        &[(0xF_0010, &entry_2), (0x20_0000, &other)],
    );
    assert_eq!(read(&dir, "x86_64").expect("2.x").ecc, [Ecc::None]);
}

#[test]
fn a_3x_table_ends_with_its_end_of_table_structure() {
    let mut memory = table();
    let length = memory.len();
    // What follows type 127 within the maximum size is not a structure of the table.
    memory.extend(common::memory_device(
        0x900,
        Device {
            size: 1024,
            locator: "AFTER",
            ..Device::default()
        },
    ));
    let maximum = u32::try_from(memory.len()).expect("maximum");
    let dir = common::host((&[("/sys/firmware/efi/systab", "SMBIOS3=0x1000\n")], &[]));
    let entry = common::entry_point_3(0x2000, maximum);
    common::add_sparse(&dir, "/dev/mem", &[(0x1000, &entry), (0x2000, &memory)]);
    let smbios = read(&dir, "x86_64").expect("3.x table");
    assert_eq!(smbios.slots.len(), 24, "{length} bytes up to type 127");
    assert_eq!(smbios, server());
}

#[test]
fn tables_over_one_mebibyte_bad_entry_points_and_no_dev_mem_are_absent() {
    const SYSTAB: &[(&str, &str)] = &[("/sys/firmware/efi/systab", "SMBIOS3=0x1000\n")];
    let dir = common::host((SYSTAB, &[]));
    common::add_sparse(
        &dir,
        "/dev/mem",
        &[(0x1000, &common::entry_point_3(0x2000, 2 << 20))],
    );
    assert!(matches!(read(&dir, "x86_64"), Err(Unreadable::Absent)));

    let mut entry = common::entry_point_3(0x2000, 0x100);
    entry[5] = entry[5].wrapping_add(1);
    let dir = common::host((SYSTAB, &[]));
    common::add_sparse(&dir, "/dev/mem", &[(0x1000, &entry)]);
    assert!(matches!(read(&dir, "x86_64"), Err(Unreadable::Absent)));

    // A 2.x entry point whose `_DMI_` part alone has a bad checksum.
    let mut entry = common::entry_point_2(0x2000, 0x100, 2);
    entry[0x1E] = entry[0x1E].wrapping_add(1);
    entry[0x0B] = entry[0x0B].wrapping_sub(1);
    let dir = common::host((&[("/sys/firmware/efi/systab", "SMBIOS=0x1000\n")], &[]));
    common::add_sparse(&dir, "/dev/mem", &[(0x1000, &entry)]);
    assert!(matches!(read(&dir, "x86_64"), Err(Unreadable::Absent)));

    let big = vec![0x7F_u8; (1 << 20) + 1];
    let dir = common::host((SYSTAB, &[]));
    common::add_bytes(&dir, "/sys/firmware/dmi/tables/DMI", &big);
    assert!(matches!(read(&dir, "aarch64"), Err(Unreadable::Absent)));
    assert!(matches!(
        read(&common::host((&[], &[])), "x86_64"),
        Err(Unreadable::Absent)
    ));
}

#[test]
fn an_unreadable_dev_mem_says_why() {
    let dir = common::host((
        &[("/sys/firmware/efi/systab", "SMBIOS3=0x1000\n")],
        &["/dev/mem"],
    ));
    assert!(matches!(read(&dir, "x86_64"), Err(Unreadable::DevMem(_))));
}

/// The rules of design §9.4 on structures of SMBIOS 3.3 and later.
fn synthetic() -> Smbios {
    let ddr5 = Device {
        array: 0x10,
        size: 0x7FFF,
        extended_size: 64 << 10,
        locator: "DIMM 0",
        bank: "BANK 0",
        kind: 0x22,
        // Registered and LRDIMM: an LRDIMM.
        detail: 0xA080,
        speed: 0xFFFF,
        extended_speed: 8800,
        configured_speed: 0xFFFF,
        extended_configured_speed: 6400,
        manufacturer: "Samsung",
        part: " M321R8GA0BB0-CQK  ",
    };
    let table = [
        common::memory_array(0x10, 3, 5),
        common::memory_array(0x11, 4, 3),
        common::memory_array(0x12, 3, 6),
        // A second array with the same correction adds no mode.
        common::memory_array(0x13, 3, 5),
        common::memory_device(0x20, ddr5),
        common::memory_device(
            0x21,
            Device {
                size: 0x8000 | 512,
                locator: "DIMM 0",
                bank: "BANK 1",
                kind: 0x1A,
                detail: 0x4080,
                speed: 2933,
                configured_speed: 2666,
                manufacturer: "Not Specified",
                part: "PartNum0",
                ..Device::default()
            },
        ),
        // Firmware data without a module: one placeholder makes the slot empty.
        common::memory_device(
            0x22,
            Device {
                manufacturer: "Samsung",
                part: "Unknown",
                ..Device::default()
            },
        ),
        common::memory_device(
            0x23,
            Device {
                locator: "DIMM 1",
                manufacturer: "Manufacturer0",
                part: "M393A4K40CB2-CTD",
                ..Device::default()
            },
        ),
        common::memory_device(
            0x24,
            Device {
                locator: "DIMM 2",
                kind: 0x1A,
                detail: 0x2080,
                speed: 2933,
                manufacturer: "Micron",
                part: "36ASF4G72PZ-2G9E1",
                ..Device::default()
            },
        ),
        // Video memory is not system memory.
        common::memory_device(
            0x25,
            Device {
                array: 0x11,
                size: 4096,
                locator: "VRAM",
                ..Device::default()
            },
        ),
        common::memory_device(
            0x26,
            Device {
                array: 0x12,
                size: 0xFFFF,
                bank: "BANK 3",
                ..Device::default()
            },
        ),
        common::ipmi_device(0x30, 4, 0x15),
        common::end_of_table(0x40),
    ]
    .concat();
    let dir = common::host((&[], &[]));
    common::add_bytes(&dir, "/sys/firmware/dmi/tables/DMI", &table);
    read(&dir, "aarch64").expect("synthetic table")
}

#[test]
fn memory_devices_follow_the_rules_of_design_9_4() {
    let smbios = synthetic();
    let slot = |name: Option<&str>, capacity| Slot {
        name: name.map(str::to_owned),
        capacity,
        ..empty("")
    };
    assert_eq!(
        smbios.slots,
        [
            // Repeated locators take their bank; extended size and speeds.
            Slot {
                name: Some("BANK 0 DIMM 0".to_owned()),
                capacity: Capacity::Kilobytes(64 << 20),
                generation: Some("DDR5"),
                form: Some("LRDIMM"),
                speed: Some(8800),
                configured_speed: Some(6400),
                manufacturer: Some("Samsung".to_owned()),
                part_number: Some("M321R8GA0BB0-CQK".to_owned()),
            },
            // A size in KB, a configured speed below the rated one, placeholders.
            Slot {
                name: Some("BANK 1 DIMM 0".to_owned()),
                capacity: Capacity::Kilobytes(512),
                generation: Some("DDR4"),
                form: Some("UDIMM"),
                speed: Some(2933),
                configured_speed: Some(2666),
                ..slot(None, Capacity::Empty)
            },
            // An empty slot keeps the firmware's data; the report shows none of it.
            Slot {
                manufacturer: Some("Samsung".to_owned()),
                ..slot(None, Capacity::Empty)
            },
            Slot {
                part_number: Some("M393A4K40CB2-CTD".to_owned()),
                ..slot(Some("DIMM 1"), Capacity::Empty)
            },
            // Installed but not in use (design S-20).
            Slot {
                name: Some("DIMM 2".to_owned()),
                capacity: Capacity::Disabled,
                generation: Some("DDR4"),
                form: Some("RDIMM"),
                speed: Some(2933),
                configured_speed: None,
                manufacturer: Some("Micron".to_owned()),
                part_number: Some("36ASF4G72PZ-2G9E1".to_owned()),
            },
            // Without a locator, the bank names the slot.
            slot(Some("BANK 3"), Capacity::Unknown),
        ]
    );
    assert_eq!(smbios.ecc, [Ecc::SingleBit, Ecc::MultiBit]);
    assert_eq!(
        smbios.bmc,
        Some(Interface {
            kind: Some(InterfaceKind::Ssif),
            version: Some((1, 5)),
        })
    );
    assert_eq!(smbios.system, Line::default());
}

#[test]
fn cut_and_corrupt_tables_keep_what_parsed_before_the_damage() {
    let full = server();
    let mut cut = table();
    cut.truncate(3000);
    let dir = common::host((&[], &[]));
    common::add_bytes(&dir, "/sys/firmware/dmi/tables/DMI", &cut);
    let smbios = read(&dir, "aarch64").expect("cut table");
    assert!(
        (1..24).contains(&smbios.slots.len()),
        "{}",
        smbios.slots.len()
    );
    assert_eq!(smbios.slots[..], full.slots[..smbios.slots.len()]);

    let mut corrupt = table();
    corrupt[1] = 2;
    let dir = common::host((&[], &[]));
    common::add_bytes(&dir, "/sys/firmware/dmi/tables/DMI", &corrupt);
    let smbios = read(&dir, "aarch64").expect("corrupt table");
    assert_eq!(smbios.slots, []);
    assert_eq!(smbios.bios, Line::default());
}
