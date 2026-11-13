//! The SMBIOS table: memory slots, error correction,
//! the BMC's interface and the machine's strings, parsed by `smbios-lib` from the first source
//! that has the table.

use std::fs::File;
use std::io;

use smbioslib::IpmiInterfaceType;
use smbioslib::MemoryArrayErrorCorrection;
use smbioslib::MemoryArrayUse;
use smbioslib::MemoryDeviceType;
use smbioslib::MemorySize;
use smbioslib::MemorySizeExtended;
use smbioslib::MemorySpeed;
use smbioslib::MemorySpeedExtended;
use smbioslib::SMBiosBaseboardInformation;
use smbioslib::SMBiosData;
use smbioslib::SMBiosInformation;
use smbioslib::SMBiosIpmiDeviceInformation;
use smbioslib::SMBiosMemoryDevice;
use smbioslib::SMBiosPhysicalMemoryArray;
use smbioslib::SMBiosString;
use smbioslib::SMBiosSystemInformation;
use smbioslib::SMBiosVersion;

use crate::Root;
use crate::bmc::Interface;
use crate::bmc::InterfaceKind;
use crate::machine::Line;

/// Larger tables count as unreadable.
const LIMIT: usize = 1 << 20;

/// What the SMBIOS table says about the hardware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smbios {
    /// The slots of the system-memory arrays (type 17), in table order.
    pub slots: Vec<Slot>,
    /// How the system-memory arrays (type 16) correct errors, each mode once, in table order.
    pub ecc: Vec<Ecc>,
    /// The BMC's interface (type 38).
    pub bmc: Option<Interface>,
    /// The machine (type 1): vendor and product.
    pub system: Line,
    /// The board (type 2): vendor, name and version.
    pub board: Line,
    /// The BIOS (type 0): vendor, version and date.
    pub bios: Line,
}

/// A memory slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The device locator, with the bank locator before it when locators repeat; `None` when
    /// the firmware names neither.
    pub name: Option<String>,
    /// What the slot holds.
    pub capacity: Capacity,
    /// The memory generation, such as `DDR4`.
    pub generation: Option<&'static str>,
    /// The module kind from the type detail: `LRDIMM`, `RDIMM` or `UDIMM`.
    pub form: Option<&'static str>,
    /// The rated speed in MT/s.
    pub speed: Option<u32>,
    /// The configured speed in MT/s.
    pub configured_speed: Option<u32>,
    /// The manufacturer, without placeholders.
    pub manufacturer: Option<String>,
    /// The part number, without placeholders.
    pub part_number: Option<String>,
}

/// What a slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capacity {
    /// No module.
    Empty,
    /// The firmware reports no module but keeps a module's manufacturer and part number: an
    /// installed module that is not in use.
    Disabled,
    /// A module of unknown size.
    Unknown,
    /// A module of this many KiB, which SMBIOS calls KB.
    Kilobytes(u64),
}

/// How memory corrects errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecc {
    /// Neither correction nor parity.
    None,
    /// Errors are detected, not corrected.
    Parity,
    /// Single-bit correction (SECDED).
    SingleBit,
    /// Multi-bit correction, such as chipkill.
    MultiBit,
    /// CRC.
    Crc,
    /// What the firmware calls other or unknown.
    Unknown,
}

/// Why there is no SMBIOS table.
#[derive(Debug)]
pub enum Unreadable {
    /// No source has one.
    Absent,
    /// `/dev/mem`, the last source on x86, could not be read.
    DevMem(io::Error),
}

/// The table from the first source that has it: the kernel's copy, the entries
/// of the dmi-sysfs module, and on x86 the firmware's memory through `/dev/mem`. `arch` is
/// `std::env::consts::ARCH`.
///
/// # Errors
///
/// [`Unreadable`] when no source has a table.
pub fn read(root: &Root, arch: &str) -> Result<Smbios, Unreadable> {
    let (table, version) = match sysfs(root).or_else(|| entries(root)) {
        Some(found) => found,
        None if arch == "x86_64" => dev_mem(root)?,
        None => return Err(Unreadable::Absent),
    };
    Ok(parse(table, version))
}

type Table = (Vec<u8>, Option<SMBiosVersion>);

fn sysfs(root: &Root) -> Option<Table> {
    let table = root
        .read_bytes("/sys/firmware/dmi/tables/DMI")
        .filter(|table| (1..=LIMIT).contains(&table.len()))?;
    let version = root
        .read_bytes("/sys/firmware/dmi/tables/smbios_entry_point")
        .and_then(|entry| entry_point(&entry))
        .map(|entry| entry.version);
    Some((table, version))
}

/// The structures of the dmi-sysfs module, joined in table order.
fn entries(root: &Root) -> Option<Table> {
    let base = "/sys/firmware/dmi/entries";
    let mut parts = Vec::new();
    for name in root.list(base) {
        let position: u64 = root
            .read_line(&format!("{base}/{name}/position"))?
            .parse()
            .ok()?;
        parts.push((position, root.read_bytes(&format!("{base}/{name}/raw"))?));
    }
    parts.sort_by_key(|(position, _)| *position);
    let table: Vec<u8> = parts.into_iter().flat_map(|(_, raw)| raw).collect();
    (1..=LIMIT).contains(&table.len()).then_some((table, None))
}

/// The table in the firmware's memory: at the address in the EFI system table, else at the
/// entry point found in 0xF0000–0xFFFFF, as dmidecode reads it on old kernels.
fn dev_mem(root: &Root) -> Result<Table, Unreadable> {
    let mut memory = match File::open(root.join("/dev/mem")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(Unreadable::Absent),
        Err(error) => return Err(Unreadable::DevMem(error)),
    };
    let entry = match efi_address(root) {
        Some(address) => {
            entry_point(&read_at(&mut memory, address, 32).map_err(Unreadable::DevMem)?)
        }
        None => scan(&mut memory).map_err(Unreadable::DevMem)?,
    };
    let entry = entry
        .filter(|entry| (1..=LIMIT).contains(&entry.length))
        .ok_or(Unreadable::Absent)?;
    let table = read_at(&mut memory, entry.address, entry.length).map_err(Unreadable::DevMem)?;
    Ok((table, Some(entry.version)))
}

/// The entry point's address from the EFI system table, the 3.x one first.
fn efi_address(root: &Root) -> Option<u64> {
    let systab = root.read("/sys/firmware/efi/systab")?;
    ["SMBIOS3=", "SMBIOS="].iter().find_map(|key| {
        let value = systab.lines().find_map(|line| line.strip_prefix(key))?;
        u64::from_str_radix(value.trim().trim_start_matches("0x"), 16).ok()
    })
}

/// The first valid entry point at a 16-byte boundary of 0xF0000–0xFFFFF, the 3.x one first.
fn scan(memory: &mut File) -> io::Result<Option<EntryPoint>> {
    let segment = read_at(memory, 0xF_0000, 0x1_0000)?;
    let find = |anchor: &[u8]| {
        (0..segment.len())
            .step_by(16)
            .filter(|&offset| segment[offset..].starts_with(anchor))
            .find_map(|offset| entry_point(&segment[offset..]))
    };
    Ok(find(b"_SM3_").or_else(|| find(b"_SM_")))
}

fn read_at(file: &mut File, address: u64, length: usize) -> io::Result<Vec<u8>> {
    use std::io::Read;
    use std::io::Seek;

    file.seek(io::SeekFrom::Start(address))?;
    let mut buffer = vec![0; length];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

/// Where a table is and which version it has, from a valid entry point.
struct EntryPoint {
    address: u64,
    /// Exact for 2.x, a maximum for 3.x: `smbios-lib` stops at the end-of-table structure
    /// (type 127), where the kernel also ends a 3.x table.
    length: usize,
    version: SMBiosVersion,
}

fn entry_point(bytes: &[u8]) -> Option<EntryPoint> {
    if bytes.starts_with(b"_SM3_") {
        let entry = bytes.get(..usize::from(*bytes.get(6)?))?;
        if entry.len() < 0x18 || !checksum(entry) {
            return None;
        }
        Some(EntryPoint {
            address: u64::from_le_bytes(entry[0x10..0x18].try_into().ok()?),
            length: usize::try_from(u32::from_le_bytes(entry[0x0C..0x10].try_into().ok()?)).ok()?,
            version: SMBiosVersion::new(entry[7], entry[8], entry[9]),
        })
    } else if bytes.starts_with(b"_SM_") {
        // Some firmware writes 0x1E for the 2.1 entry point's length of 0x1F, as dmidecode notes.
        let entry = bytes.get(..usize::from(*bytes.get(5)?))?;
        let intermediate = bytes.get(0x10..0x1F)?;
        if entry.len() < 0x1E
            || !checksum(entry)
            || !intermediate.starts_with(b"_DMI_")
            || !checksum(intermediate)
        {
            return None;
        }
        Some(EntryPoint {
            address: u64::from(u32::from_le_bytes(bytes[0x18..0x1C].try_into().ok()?)),
            length: usize::from(u16::from_le_bytes(bytes[0x16..0x18].try_into().ok()?)),
            version: SMBiosVersion::new(entry[6], entry[7], 0),
        })
    } else {
        None
    }
}

fn checksum(bytes: &[u8]) -> bool {
    bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn parse(table: Vec<u8>, version: Option<SMBiosVersion>) -> Smbios {
    use smbioslib::SMBiosStruct;

    let data = SMBiosData::from_vec_and_version(table, version);
    let mut other_arrays = Vec::new();
    let mut ecc = Vec::new();
    for array in data.defined_struct_iter::<SMBiosPhysicalMemoryArray>() {
        if array
            .usage()
            .is_some_and(|usage| usage.value != MemoryArrayUse::SystemMemory)
        {
            other_arrays.push(array.parts().header.handle().0);
            continue;
        }
        let mode = array
            .memory_error_correction()
            .map_or(Ecc::Unknown, |correction| ecc_mode(&correction.value));
        if !ecc.contains(&mode) {
            ecc.push(mode);
        }
    }
    let devices: Vec<SMBiosMemoryDevice<'_>> = data
        .defined_struct_iter::<SMBiosMemoryDevice<'_>>()
        .filter(|device| {
            !device
                .physical_memory_array_handle()
                .is_some_and(|handle| other_arrays.contains(&handle.0))
        })
        .collect();
    let locators: Vec<(Option<String>, Option<String>)> = devices
        .iter()
        .map(|device| {
            (
                string(device.device_locator()),
                string(device.bank_locator()),
            )
        })
        .collect();
    let slots = devices
        .iter()
        .zip(&locators)
        .map(|(device, (locator, bank))| {
            let repeated = locators
                .iter()
                .filter(|(other, _)| other == locator)
                .count()
                > 1;
            let name = match (locator, bank) {
                (Some(locator), Some(bank)) if repeated => Some(format!("{bank} {locator}")),
                (Some(locator), _) => Some(locator.clone()),
                (None, bank) => bank.clone(),
            };
            let manufacturer = string(device.manufacturer());
            let part_number = string(device.part_number());
            Slot {
                name,
                capacity: capacity(device, manufacturer.is_some() && part_number.is_some()),
                generation: device
                    .memory_type()
                    .and_then(|kind| generation(&kind.value)),
                form: device.type_detail().and_then(|detail| {
                    if detail.lrdimm() {
                        Some("LRDIMM")
                    } else if detail.registered() {
                        Some("RDIMM")
                    } else if detail.unbuffered() {
                        Some("UDIMM")
                    } else {
                        None
                    }
                }),
                speed: speed(device.speed(), device.extended_speed()),
                configured_speed: speed(
                    device.configured_memory_speed(),
                    device.extended_configured_memory_speed(),
                ),
                manufacturer,
                part_number,
            }
        })
        .collect();
    let bmc = data
        .defined_struct_iter::<SMBiosIpmiDeviceInformation<'_>>()
        .next()
        .map(|ipmi| Interface {
            kind: ipmi.interface_type().and_then(|kind| match kind.value {
                IpmiInterfaceType::KeyboardControllerStyle => Some(InterfaceKind::Kcs),
                IpmiInterfaceType::ServerManagementInterfaceChip => Some(InterfaceKind::Smic),
                IpmiInterfaceType::BlockTransfer => Some(InterfaceKind::Bt),
                IpmiInterfaceType::SMBusSystemInterface => Some(InterfaceKind::Ssif),
                IpmiInterfaceType::Unknown | IpmiInterfaceType::None => None,
            }),
            version: ipmi
                .ipmi_specification_revision()
                .filter(|revision| *revision != 0)
                .map(|revision| (revision >> 4, revision & 0x0F)),
        });
    let system = data
        .defined_struct_iter::<SMBiosSystemInformation<'_>>()
        .next()
        .map(|system| Line {
            vendor: string(system.manufacturer()),
            name: string(system.product_name()),
            detail: None,
        });
    let board = data
        .defined_struct_iter::<SMBiosBaseboardInformation<'_>>()
        .next()
        .map(|board| Line {
            vendor: string(board.manufacturer()),
            name: string(board.product()),
            detail: string(board.version()),
        });
    let bios = data
        .defined_struct_iter::<SMBiosInformation<'_>>()
        .next()
        .map(|bios| Line {
            vendor: string(bios.vendor()),
            name: string(bios.version()),
            detail: string(bios.release_date()),
        });
    Smbios {
        slots,
        ecc,
        bmc,
        system: system.unwrap_or_default(),
        board: board.unwrap_or_default(),
        bios: bios.unwrap_or_default(),
    }
}

fn string(text: SMBiosString) -> Option<String> {
    text.to_utf8_lossy()
        .and_then(|text| crate::firmware_string(&text))
}

/// `module` says whether the slot names a module's manufacturer and part number.
fn capacity(device: &SMBiosMemoryDevice<'_>, module: bool) -> Capacity {
    let megabytes = |megabytes: u64| Capacity::Kilobytes(megabytes << 10);
    match device.size() {
        Some(MemorySize::NotInstalled) if module => Capacity::Disabled,
        Some(MemorySize::NotInstalled) => Capacity::Empty,
        Some(MemorySize::Kilobytes(kilobytes)) => Capacity::Kilobytes(u64::from(kilobytes)),
        Some(MemorySize::Megabytes(size)) => megabytes(u64::from(size)),
        Some(MemorySize::SeeExtendedSize) => match device.extended_size() {
            Some(MemorySizeExtended::Megabytes(size)) => megabytes(u64::from(size)),
            Some(MemorySizeExtended::SeeSize) | None => Capacity::Unknown,
        },
        Some(MemorySize::Unknown) | None => Capacity::Unknown,
    }
}

fn speed(speed: Option<MemorySpeed>, extended: Option<MemorySpeedExtended>) -> Option<u32> {
    match speed? {
        MemorySpeed::MTs(speed) => Some(u32::from(speed)),
        MemorySpeed::SeeExtendedSpeed => match extended? {
            MemorySpeedExtended::MTs(speed) => Some(speed),
            MemorySpeedExtended::SeeSpeed => None,
        },
        MemorySpeed::Unknown => None,
    }
}

fn generation(kind: &MemoryDeviceType) -> Option<&'static str> {
    Some(match kind {
        MemoryDeviceType::Dram => "DRAM",
        MemoryDeviceType::Sdram => "SDRAM",
        MemoryDeviceType::Rdram => "RDRAM",
        MemoryDeviceType::Ddr => "DDR",
        MemoryDeviceType::Ddr2 => "DDR2",
        MemoryDeviceType::Ddr2Fbdimm => "DDR2 FB-DIMM",
        MemoryDeviceType::Ddr3 => "DDR3",
        MemoryDeviceType::Fbd2 => "FBD2",
        MemoryDeviceType::Ddr4 => "DDR4",
        MemoryDeviceType::Ddr5 => "DDR5",
        MemoryDeviceType::Lpddr => "LPDDR",
        MemoryDeviceType::Lpddr2 => "LPDDR2",
        MemoryDeviceType::Lpddr3 => "LPDDR3",
        MemoryDeviceType::Lpddr4 => "LPDDR4",
        MemoryDeviceType::Lpddr5 => "LPDDR5",
        MemoryDeviceType::Hbm => "HBM",
        MemoryDeviceType::Hbm2 => "HBM2",
        MemoryDeviceType::Hbm3 => "HBM3",
        MemoryDeviceType::Mrdimm => "MRDIMM",
        _ => return None,
    })
}

fn ecc_mode(correction: &MemoryArrayErrorCorrection) -> Ecc {
    match correction {
        MemoryArrayErrorCorrection::NoCorrection => Ecc::None,
        MemoryArrayErrorCorrection::Parity => Ecc::Parity,
        MemoryArrayErrorCorrection::SingleBitEcc => Ecc::SingleBit,
        MemoryArrayErrorCorrection::MultiBitEcc => Ecc::MultiBit,
        MemoryArrayErrorCorrection::Crc => Ecc::Crc,
        MemoryArrayErrorCorrection::Other
        | MemoryArrayErrorCorrection::Unknown
        | MemoryArrayErrorCorrection::None => Ecc::Unknown,
    }
}
