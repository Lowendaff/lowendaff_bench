//! The hardware of dedicated servers (system information design §9): the machine and its
//! firmware, EDAC, the BMC and RAID cards, on the captured hosts and on synthetic ones.

mod common;

use bench_host::Root;
use bench_host::bmc;
use bench_host::bmc::Bmc;
use bench_host::bmc::Interface;
use bench_host::bmc::InterfaceKind;
use bench_host::edac;
use bench_host::edac::Edac;
use bench_host::machine;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::machine::Machine;
use bench_host::raid;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::raid::Ids;
use bench_host::smbios;
use bench_host::smbios::Ecc;
use bench_host::smbios::Smbios;
use tempfile::TempDir;

const SERVER: &str = "dedicated-pve-x86_64";
const BOARD: &str = "rock3a-debian13-aarch64";

fn server_smbios() -> Smbios {
    smbios::read(&common::fixture(SERVER), "x86_64").expect("the server's table")
}

fn line(vendor: Option<&str>, name: &str, detail: Option<&str>) -> Line {
    Line {
        vendor: vendor.map(str::to_owned),
        name: Some(name.to_owned()),
        detail: detail.map(str::to_owned),
    }
}

#[test]
fn the_machines_come_from_dmi() {
    let server = machine::read(&common::fixture(SERVER), Some(&server_smbios()));
    assert_eq!(
        server,
        Machine {
            system: line(Some("Inspur"), "SSINSPURY32-XI5-25D-1", None),
            board: line(Some("Inspur"), "YZMB-00870-105", Some("SA5112M5")),
            firmware: Some(Firmware::Bios(line(
                Some("American Megatrends Inc."),
                "4.1.19",
                Some("02/07/2022")
            ))),
        }
    );
    let board = machine::read(&common::fixture(BOARD), None);
    assert_eq!(
        board,
        Machine {
            system: line(Some("radxa"), "Radxa ROCK 3A", None),
            board: line(Some("radxa"), "Radxa ROCK 3A", None),
            firmware: Some(Firmware::Bios(line(
                Some("U-Boot"),
                "2026.07-rc4-ga7830e87555a",
                Some("07/01/2026")
            ))),
        }
    );
}

#[test]
fn without_dmi_the_smbios_table_names_the_machine() {
    let dir = common::host((&[("/proc/device-tree/model", "Board\0")], &[]));
    let smbios = server_smbios();
    let machine = machine::read(&Root::new(dir.path()), Some(&smbios));
    assert_eq!(machine.system, smbios.system);
    assert_eq!(machine.board, smbios.board);
    assert_eq!(machine.firmware, Some(Firmware::Bios(smbios.bios)));
}

#[test]
fn placeholders_count_as_missing() {
    let dir = common::host((
        &[
            ("/sys/class/dmi/id/sys_vendor", "To Be Filled By O.E.M.\n"),
            ("/sys/class/dmi/id/product_name", "  X11DPi-N  \n"),
            ("/sys/class/dmi/id/board_vendor", "Default string\n"),
            ("/sys/class/dmi/id/board_name", "System Product Name\n"),
            ("/sys/class/dmi/id/board_version", "0123456789\n"),
            ("/sys/class/dmi/id/bios_vendor", "000000\n"),
            ("/sys/class/dmi/id/bios_version", "N/A\n"),
        ],
        &[],
    ));
    assert_eq!(
        machine::read(&Root::new(dir.path()), None),
        Machine {
            system: Line {
                name: Some("X11DPi-N".to_owned()),
                ..Line::default()
            },
            board: Line::default(),
            firmware: Some(Firmware::Bios(Line::default())),
        }
    );
}

#[test]
fn device_tree_boards_without_smbios() {
    let dir = common::host((
        &[
            (
                "/proc/device-tree/model",
                "Raspberry Pi 4 Model B Rev 1.4\0",
            ),
            (
                "/proc/device-tree/compatible",
                "raspberrypi,4-model-b\0brcm,bcm2711\0",
            ),
        ],
        &[],
    ));
    // 2024-04-15 00:00:00 UTC, big-endian, as the bootloader writes it.
    common::add_bytes(
        &dir,
        "/proc/device-tree/chosen/bootloader/build-timestamp",
        &1_713_139_200_u32.to_be_bytes(),
    );
    assert_eq!(
        machine::read(&Root::new(dir.path()), None),
        Machine {
            system: line(None, "Raspberry Pi 4 Model B Rev 1.4", None),
            board: line(None, "raspberrypi,4-model-b", Some("brcm,bcm2711")),
            firmware: Some(Firmware::RaspberryPi(1_713_139_200)),
        }
    );
    // U-Boot's version comes first; without either, the firmware is unknown.
    common::add(
        &dir,
        (
            &[("/proc/device-tree/chosen/u-boot,version", "2024.01\0")],
            &[],
        ),
    );
    assert_eq!(
        machine::read(&Root::new(dir.path()), None).firmware,
        Some(Firmware::UBoot("2024.01".to_owned()))
    );
    let bare = common::host((&[("/proc/device-tree/model", "Board\0")], &[]));
    assert_eq!(machine::read(&Root::new(bare.path()), None).firmware, None);
}

#[test]
fn board_memory_sums_the_memory_nodes() {
    assert_eq!(
        machine::board_memory(&common::fixture(BOARD)),
        Some(0x7FE0_0000)
    );
    assert_eq!(machine::board_memory(&common::fixture(SERVER)), None);
    // One address cell more than size cells, as on a Raspberry Pi 4.
    let dir = common::host((&[], &[]));
    common::add_bytes(
        &dir,
        "/proc/device-tree/#address-cells",
        &2_u32.to_be_bytes(),
    );
    common::add_bytes(&dir, "/proc/device-tree/#size-cells", &1_u32.to_be_bytes());
    let reg = [
        0_u32,
        0,
        0x3B40_0000, // 0x0, 948 MiB
        0,
        0x4000_0000,
        0xBC00_0000, // 0x4000_0000, 3008 MiB
    ];
    let bytes: Vec<u8> = reg.iter().flat_map(|cell| cell.to_be_bytes()).collect();
    common::add_bytes(&dir, "/proc/device-tree/memory@0/reg", &bytes);
    common::add_bytes(&dir, "/proc/device-tree/memory@0/device_type", b"memory\0");
    // Neither a memory controller's registers nor reserved memory count.
    common::add_bytes(
        &dir,
        "/proc/device-tree/memory-controller@fe000000/reg",
        &bytes,
    );
    common::add_bytes(&dir, "/proc/device-tree/memory@f0000000/reg", &bytes);
    common::add_bytes(
        &dir,
        "/proc/device-tree/memory@f0000000/device_type",
        b"reserved\0",
    );
    assert_eq!(
        machine::board_memory(&Root::new(dir.path())),
        Some(0x3B40_0000 + 0xBC00_0000)
    );
    common::add_bytes(&dir, "/proc/device-tree/#size-cells", &3_u32.to_be_bytes());
    assert_eq!(machine::board_memory(&Root::new(dir.path())), None);
    common::add_bytes(&dir, "/proc/device-tree/#size-cells", &1_u32.to_be_bytes());
    common::add_bytes(
        &dir,
        "/proc/device-tree/#address-cells",
        &5_u32.to_be_bytes(),
    );
    assert_eq!(machine::board_memory(&Root::new(dir.path())), None);
    // Without the cell sizes, the defaults of the device tree specification: 2 and 1.
    let dir = common::host((&[], &[]));
    common::add_bytes(&dir, "/proc/device-tree/memory/reg", &bytes[..12]);
    assert_eq!(
        machine::board_memory(&Root::new(dir.path())),
        Some(0x3B40_0000)
    );
}

#[test]
fn edac_sums_the_controllers_and_names_their_modes() {
    assert_eq!(
        edac::read(&common::fixture(SERVER)),
        Some(Edac {
            corrected: 0,
            uncorrected: 0,
            modes: vec![Ecc::SingleBit],
        })
    );
    // The board has EDAC's directory but no driver.
    assert_eq!(edac::read(&common::fixture(BOARD)), None);
    let dir = common::host((
        &[
            ("/sys/devices/system/edac/mc/mc0/ce_count", "3\n"),
            ("/sys/devices/system/edac/mc/mc0/ue_count", "0\n"),
            (
                "/sys/devices/system/edac/mc/mc0/dimm0/dimm_edac_mode",
                "SECDED\n",
            ),
            ("/sys/devices/system/edac/mc/mc1/ce_count", "2\n"),
            ("/sys/devices/system/edac/mc/mc1/ue_count", "1\n"),
            (
                "/sys/devices/system/edac/mc/mc1/rank0/dimm_edac_mode",
                "S4ECD4ED\n",
            ),
            ("/sys/devices/system/edac/mc/mc2/ce_count", "unreadable\n"),
            (
                "/sys/devices/system/edac/mc/mc2/csrow0/edac_mode",
                "Unknown\n",
            ),
            (
                // Only `csrow1` names PARITY; `mc3/dimm0` repeats a mode.
                "/sys/devices/system/edac/mc/mc2/csrow1/edac_mode",
                "PARITY\n",
            ),
            (
                "/sys/devices/system/edac/mc/mc3/dimm0/dimm_edac_mode",
                "SECDED\n",
            ),
            (
                "/sys/devices/system/edac/mc/mc3/dimm1/dimm_edac_mode",
                "None\n",
            ),
        ],
        &[],
    ));
    assert_eq!(
        edac::read(&Root::new(dir.path())),
        Some(Edac {
            corrected: 5,
            uncorrected: 1,
            modes: vec![Ecc::SingleBit, Ecc::MultiBit, Ecc::Parity, Ecc::None],
        })
    );
}

#[test]
fn the_bmc_comes_from_the_first_source_that_has_it() {
    let smbios = server_smbios();
    assert_eq!(
        bmc::read(&common::fixture(SERVER), Some(&smbios)),
        Some(Bmc::Driver {
            vendor: Some(37945),
            firmware: Some("4.18".to_owned()),
            ipmi: Some("2.0".to_owned()),
        })
    );
    // Without the driver: SMBIOS type 38, then ACPI's SPMI table, then ACPI's device.
    let mut spmi = common::fixture_bytes(SERVER, "/sys/firmware/acpi/tables/SPMI");
    spmi[36] = 4;
    let dir = common::host((&[("/sys/bus/acpi/devices/IPI0001:00/status", "15\n")], &[]));
    common::add_bytes(&dir, "/sys/firmware/acpi/tables/SPMI", &spmi);
    let root = Root::new(dir.path());
    let kcs = Interface {
        kind: Some(InterfaceKind::Kcs),
        version: Some((2, 0)),
    };
    assert_eq!(bmc::read(&root, Some(&smbios)), Some(Bmc::Interface(kcs)));
    let ssif = Interface {
        kind: Some(InterfaceKind::Ssif),
        ..kcs
    };
    assert_eq!(bmc::read(&root, None), Some(Bmc::Interface(ssif)));
    let dir = common::host((&[("/sys/bus/acpi/devices/IPI0001:00/status", "15\n")], &[]));
    assert_eq!(bmc::read(&Root::new(dir.path()), None), Some(Bmc::Present));
    let dir = common::host((&[], &["/sys/bus/acpi/devices/IPI0001:00"]));
    assert_eq!(bmc::read(&Root::new(dir.path()), None), Some(Bmc::Present));
    // `_STA` without its present bit.
    let dir = common::host((&[("/sys/bus/acpi/devices/IPI0001:00/status", "0\n")], &[]));
    assert_eq!(bmc::read(&Root::new(dir.path()), None), None);
    assert_eq!(bmc::read(&common::fixture(BOARD), None), None);
}

#[test]
fn bmc_vendors_by_their_iana_numbers() {
    assert_eq!(bmc::vendor(37945), Some("Inspur"));
    assert_eq!(bmc::vendor(674), Some("Dell"));
    assert_eq!(bmc::vendor(11), Some("HPE"));
    assert_eq!(bmc::vendor(10876), Some("Supermicro"));
    // Wiwynn is 40092; 40981 belongs to Facebook.
    assert_eq!(bmc::vendor(40092), Some("Wiwynn"));
    assert_eq!(bmc::vendor(40981), None);
}

fn disks(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn the_server_passes_its_disks_through_a_megaraid() {
    let cards = raid::read(
        &common::fixture(SERVER),
        &disks(&["sda", "sdb", "sdc", "sdd"]),
    );
    assert_eq!(
        cards,
        [Card {
            address: "0000:5e:00.0".to_owned(),
            ids: Ids {
                vendor: 0x1000,
                device: 0x005d,
                subsystem: Some((0x1000, 0x9361)),
            },
            name: Some("MegaRAID SAS 9361-8i".to_owned()),
            driver: Some("megaraid_sas".to_owned()),
            disks: ["sda", "sdb", "sdc", "sdd"]
                .map(|disk| (disk.to_owned(), Attachment::PassThrough))
                .to_vec(),
        }]
    );
    assert_eq!(
        raid::read(&common::fixture(BOARD), &disks(&["nvme0n1"])),
        []
    );
}

/// A RAID-class PCI device: vendor, device and subsystem IDs, and its driver.
fn card(dir: &TempDir, address: &str, ids: [&str; 4], driver: Option<&str>) {
    let base = format!("/sys/bus/pci/devices/{address}");
    let files = [
        ("class", "0x010400"),
        ("vendor", ids[0]),
        ("device", ids[1]),
        ("subsystem_vendor", ids[2]),
        ("subsystem_device", ids[3]),
    ];
    for (name, value) in files {
        common::add_bytes(
            dir,
            &format!("{base}/{name}"),
            format!("{value}\n").as_bytes(),
        );
    }
    if let Some(driver) = driver {
        common::link(
            dir,
            &format!("{base}/driver"),
            &format!("../../../bus/pci/drivers/{driver}"),
        );
    }
}

/// A disk behind a PCI device: its SCSI address, or a path without one for NVMe.
fn disk(dir: &TempDir, name: &str, address: &str, scsi: Option<&str>) {
    let tail = match scsi {
        Some(scsi) => format!("host9/target9:0:0/{scsi}/block/{name}"),
        None => format!("10000:01:00.0/nvme/nvme0/{name}"),
    };
    common::link(
        dir,
        &format!("/sys/block/{name}"),
        &format!("../devices/pci0000:00/{address}/{tail}"),
    );
}

#[test]
fn each_driver_attaches_disks_by_its_channels() {
    let dir = common::host((&[], &[]));
    common::add_bytes(
        &dir,
        "/usr/share/misc/pci.ids",
        &common::fixture_bytes(SERVER, "/usr/share/misc/pci.ids"),
    );
    let ids = |vendor, device| [vendor, device, "0x0000", "0x0000"];
    card(&dir, "0000:00:0e.0", ids("0x8086", "0x467f"), Some("vmd"));
    card(&dir, "0000:00:17.0", ids("0x8086", "0x2826"), Some("ahci"));
    card(
        &dir,
        "0000:3b:00.0",
        ["0x1000", "0x005d", "0x1028", "0x1f47"],
        Some("megaraid_sas"),
    );
    card(&dir, "0000:5c:00.0", ids("0x103c", "0x323a"), Some("hpsa"));
    card(
        &dir,
        "0000:5d:00.0",
        ids("0x9005", "0x028f"),
        Some("smartpqi"),
    );
    card(
        &dir,
        "0000:5e:00.0",
        ids("0x9005", "0x028c"),
        Some("aacraid"),
    );
    card(
        &dir,
        "0000:5f:00.0",
        ids("0x1000", "0x0097"),
        Some("mpt3sas"),
    );
    card(
        &dir,
        "0000:af:00.0",
        ids("0x17d3", "0x1880"),
        Some("arcmsr"),
    );
    card(
        &dir,
        "0000:d8:00.0",
        ["0x1000", "0x005d", "0x1000", "0xdead"],
        None,
    );
    common::add(
        &dir,
        (
            &[("/sys/bus/pci/devices/0000:00:11.5/class", "0x010601\n")],
            &[],
        ),
    );
    let attached = [
        ("nvme0n1", "0000:00:0e.0", None),
        ("sda", "0000:00:17.0", Some("0:0:0:0")),
        ("sdb", "0000:3b:00.0", Some("0:2:0:0")),
        ("sdc", "0000:3b:00.0", Some("0:0:4:0")),
        ("sdq", "0000:3b:00.0", Some("0:1:5:0")),
        ("sdd", "0000:5c:00.0", Some("2:1:0:0")),
        ("sde", "0000:5c:00.0", Some("2:0:1:0")),
        ("sdf", "0000:5c:00.0", Some("2:3:0:0")),
        ("sdr", "0000:5c:00.0", Some("2:2:0:0")),
        ("sdg", "0000:5d:00.0", Some("3:1:0:0")),
        ("sdh", "0000:5d:00.0", Some("3:2:0:0")),
        ("sds", "0000:5d:00.0", Some("3:3:0:0")),
        ("sdt", "0000:5d:00.0", Some("3:0:0:0")),
        ("sdi", "0000:5e:00.0", Some("4:0:0:0")),
        ("sdj", "0000:5e:00.0", Some("4:1:0:0")),
        ("sdk", "0000:5f:00.0", Some("5:1:0:0")),
        ("sdl", "0000:5f:00.0", Some("5:0:3:0")),
        ("sdm", "0000:5f:00.0", Some("5:2:0:0")),
        ("sdn", "0000:af:00.0", Some("6:0:0:0")),
        ("sdo", "0000:d8:00.0", Some("7:2:0:0")),
        ("sdp", "0000:00:11.5", Some("8:0:0:0")),
    ];
    for (name, address, scsi) in attached {
        disk(&dir, name, address, scsi);
    }
    let names: Vec<&str> = attached.iter().map(|(name, ..)| *name).collect();
    let cards = raid::read(&Root::new(dir.path()), &disks(&names));
    // Address, name, driver, and the disks with how they attach.
    type Summary<'a> = (
        &'a str,
        Option<&'a str>,
        Option<&'a str>,
        Vec<(&'a str, Attachment)>,
    );
    let summary: Vec<Summary<'_>> = cards
        .iter()
        .map(|card| {
            (
                card.address.as_str(),
                card.name.as_deref(),
                card.driver.as_deref(),
                card.disks
                    .iter()
                    .map(|(disk, attachment)| (disk.as_str(), *attachment))
                    .collect(),
            )
        })
        .collect();
    use Attachment::Array;
    use Attachment::PassThrough;
    use Attachment::Unknown;
    assert_eq!(
        summary,
        [
            (
                "0000:00:0e.0",
                None,
                Some("vmd"),
                vec![("nvme0n1", PassThrough)]
            ),
            (
                "0000:00:17.0",
                None,
                Some("ahci"),
                vec![("sda", PassThrough)]
            ),
            (
                "0000:3b:00.0",
                Some("PERC H730P Mini"),
                Some("megaraid_sas"),
                vec![("sdb", Array), ("sdc", PassThrough), ("sdq", PassThrough)]
            ),
            (
                "0000:5c:00.0",
                None,
                Some("hpsa"),
                vec![
                    ("sdd", Array),
                    ("sde", PassThrough),
                    ("sdf", PassThrough),
                    ("sdr", Array)
                ]
            ),
            (
                "0000:5d:00.0",
                None,
                Some("smartpqi"),
                vec![
                    ("sdg", Array),
                    ("sdh", PassThrough),
                    ("sds", Array),
                    ("sdt", PassThrough)
                ]
            ),
            (
                "0000:5e:00.0",
                None,
                Some("aacraid"),
                vec![("sdi", Array), ("sdj", PassThrough)]
            ),
            (
                "0000:5f:00.0",
                None,
                Some("mpt3sas"),
                vec![("sdk", Array), ("sdl", PassThrough), ("sdm", PassThrough)]
            ),
            ("0000:af:00.0", None, Some("arcmsr"), vec![("sdn", Unknown)]),
            (
                "0000:d8:00.0",
                Some("Broadcom / LSI MegaRAID SAS-3 3108 [Invader]"),
                None,
                vec![("sdo", Unknown)]
            ),
        ]
    );
}

#[test]
fn pci_ids_is_looked_up_in_order_and_may_be_missing() {
    let dir = common::host((&[], &[]));
    card(
        &dir,
        "0000:5e:00.0",
        ["0x1000", "0x005d", "0x1000", "0x9361"],
        Some("megaraid_sas"),
    );
    let name = |dir: &TempDir| raid::read(&Root::new(dir.path()), &[])[0].name.clone();
    assert_eq!(name(&dir), None);
    let excerpt = common::fixture_bytes(SERVER, "/usr/share/misc/pci.ids");
    common::add_bytes(&dir, "/usr/share/pci.ids", &excerpt);
    assert_eq!(name(&dir).as_deref(), Some("MegaRAID SAS 9361-8i"));
    let text = String::from_utf8(excerpt).expect("pci.ids text");
    let renamed = |label: &str| text.replace("MegaRAID SAS 9361-8i", label);
    common::add_bytes(
        &dir,
        "/usr/share/misc/pci.ids",
        renamed("second file").as_bytes(),
    );
    assert_eq!(name(&dir).as_deref(), Some("second file"));
    common::add_bytes(
        &dir,
        "/usr/share/hwdata/pci.ids",
        renamed("first file").as_bytes(),
    );
    assert_eq!(name(&dir).as_deref(), Some("first file"));
    // The search ends with the device's subsystems and with the vendor's devices.
    let ids = concat!(
        "1000  Broadcom / LSI\n",
        "\t005d  MegaRAID SAS-3 3108 [Invader]\n",
        "\t\t1000 9361  MegaRAID SAS 9361-8i\n",
        "\t005f  MegaRAID SAS-3 3008 [Fury]\n",
        "\t\t1000 dead  another device's subsystem\n",
        "1001  Kolter Electronic\n",
        "\t005d  another vendor's device\n",
    );
    let other = common::host((&[], &[]));
    card(
        &other,
        "0000:5e:00.0",
        ["0x1000", "0x005d", "0x1000", "0xdead"],
        None,
    );
    common::add_bytes(&other, "/usr/share/hwdata/pci.ids", ids.as_bytes());
    assert_eq!(
        name(&other).as_deref(),
        Some("Broadcom / LSI MegaRAID SAS-3 3108 [Invader]")
    );
}
