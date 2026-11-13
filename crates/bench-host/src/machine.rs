//! The machine, its board and its firmware: from DMI,
//! else the SMBIOS table, else the device tree of boards without SMBIOS.

use crate::Root;
use crate::smbios::Smbios;

/// A vendor, a name, and a detail that the report shows in parentheses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    /// The vendor, such as `Inspur`.
    pub vendor: Option<String>,
    /// The product, the board's name, or the firmware's version.
    pub name: Option<String>,
    /// The board's version, the firmware's date, or a board's SoC.
    pub detail: Option<String>,
}

/// The firmware that started the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Firmware {
    /// The BIOS of DMI or SMBIOS: its vendor, version, and date as the firmware writes it.
    Bios(Line),
    /// U-Boot's version, from the device tree.
    UBoot(String),
    /// When the Raspberry Pi bootloader was built, in seconds since the Unix epoch.
    RaspberryPi(u64),
}

/// 整机, 主板 and the firmware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    /// The machine: vendor and product.
    pub system: Line,
    /// The board: vendor, name and version; on device-tree boards, the board and its SoC.
    pub board: Line,
    /// The firmware, or `None` when no source names it.
    pub firmware: Option<Firmware>,
}

/// The machine from DMI, else from the SMBIOS table, else from the device tree.
#[must_use]
pub fn read(root: &Root, smbios: Option<&Smbios>) -> Machine {
    if root.exists("/sys/class/dmi/id") {
        let field = |name: &str| {
            root.read_line(&format!("/sys/class/dmi/id/{name}"))
                .and_then(|text| crate::firmware_string(&text))
        };
        return Machine {
            system: Line {
                vendor: field("sys_vendor"),
                name: field("product_name"),
                detail: None,
            },
            board: Line {
                vendor: field("board_vendor"),
                name: field("board_name"),
                detail: field("board_version"),
            },
            firmware: Some(Firmware::Bios(Line {
                vendor: field("bios_vendor"),
                name: field("bios_version"),
                detail: field("bios_date"),
            })),
        };
    }
    if let Some(smbios) = smbios {
        return Machine {
            system: smbios.system.clone(),
            board: smbios.board.clone(),
            firmware: Some(Firmware::Bios(smbios.bios.clone())),
        };
    }
    let model = crate::device_tree_strings(root, "/proc/device-tree/model");
    let compatible = crate::device_tree_strings(root, "/proc/device-tree/compatible");
    let u_boot = crate::device_tree_strings(root, "/proc/device-tree/chosen/u-boot,version");
    Machine {
        system: Line {
            name: model.into_iter().next(),
            ..Line::default()
        },
        board: Line {
            vendor: None,
            name: compatible.first().cloned(),
            detail: compatible.get(1).cloned(),
        },
        firmware: u_boot.into_iter().next().map(Firmware::UBoot).or_else(|| {
            let stamp = root.read_bytes("/proc/device-tree/chosen/bootloader/build-timestamp")?;
            Some(Firmware::RaspberryPi(big_endian(&stamp)?))
        }),
    }
}

/// The memory the device tree describes, in bytes: the sum of its `memory` nodes, or `None`
/// without any.
#[must_use]
pub fn board_memory(root: &Root) -> Option<u64> {
    let cells = |name: &str, default: u64| {
        root.read_bytes(&format!("/proc/device-tree/{name}"))
            .map_or(Some(default), |bytes| big_endian(&bytes))
    };
    let (address_cells, size_cells) = (cells("#address-cells", 2)?, cells("#size-cells", 1)?);
    // `big_endian` takes sizes of one or two cells; none would make the entries empty.
    if address_cells > 4 || size_cells == 0 {
        return None;
    }
    let (address_cells, size_cells) = (
        usize::try_from(address_cells).ok()?,
        usize::try_from(size_cells).ok()?,
    );
    let mut total = None;
    for node in root.list("/proc/device-tree") {
        if node != "memory" && !node.starts_with("memory@") {
            continue;
        }
        let kind =
            crate::device_tree_strings(root, &format!("/proc/device-tree/{node}/device_type"));
        if kind.first().is_some_and(|kind| kind != "memory") {
            continue;
        }
        let Some(reg) = root.read_bytes(&format!("/proc/device-tree/{node}/reg")) else {
            continue;
        };
        for entry in reg.chunks_exact((address_cells + size_cells) * 4) {
            let size = big_endian(&entry[address_cells * 4..])?;
            total = Some(total.unwrap_or(0_u64).saturating_add(size));
        }
    }
    total
}

/// A big-endian number of 4 or 8 bytes, as device-tree cells are.
fn big_endian(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        4 => Some(u64::from(u32::from_be_bytes(bytes.try_into().ok()?))),
        8 => Some(u64::from_be_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}
