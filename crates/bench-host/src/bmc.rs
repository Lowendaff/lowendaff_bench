//! The BMC: from the IPMI driver, else the interface that
//! SMBIOS type 38 or ACPI's SPMI table describes, else ACPI's IPMI device alone. No IPMI
//! command is sent.

use crate::Root;
use crate::smbios::Smbios;

/// What is known about the BMC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bmc {
    /// The IPMI driver's view of the BMC.
    Driver {
        /// The manufacturer's IANA enterprise number.
        vendor: Option<u32>,
        /// The firmware revision, such as `4.18`.
        firmware: Option<String>,
        /// The IPMI version, such as `2.0`.
        ipmi: Option<String>,
    },
    /// The interface the firmware describes, without the driver.
    Interface(Interface),
    /// Only ACPI's IPMI device, `IPI0001`.
    Present,
}

/// How the host talks to the BMC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interface {
    /// The kind of interface.
    pub kind: Option<InterfaceKind>,
    /// The IPMI version: major and minor.
    pub version: Option<(u8, u8)>,
}

/// The IPMI system interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceKind {
    /// Keyboard Controller Style.
    Kcs,
    /// Server Management Interface Chip.
    Smic,
    /// Block Transfer.
    Bt,
    /// SMBus System Interface.
    Ssif,
}

/// The BMC from the first source that has one, or `None`.
#[must_use]
pub fn read(root: &Root, smbios: Option<&Smbios>) -> Option<Bmc> {
    driver(root)
        .or_else(|| smbios.and_then(|smbios| smbios.bmc).map(Bmc::Interface))
        .or_else(|| spmi(root).map(Bmc::Interface))
        .or_else(|| acpi_device(root).then_some(Bmc::Present))
}

/// The vendor an IANA enterprise number names, for the server and BMC makers listed here.
/// Checked against IANA's registry, last updated 2026-11-06, on 2026-11-09.
#[must_use]
pub fn vendor(number: u32) -> Option<&'static str> {
    Some(match number {
        2 | 20301 => "IBM",
        9 | 5771 => "Cisco",
        11 | 47196 => "HPE",
        211 | 10368 => "Fujitsu",
        343 => "Intel",
        674 => "Dell",
        2011 => "Huawei",
        2623 => "ASUS",
        6569 | 20569 => "Inventec",
        6653 => "Tyan",
        7244 => "Quanta",
        10876 => "Supermicro",
        15370 => "Gigabyte",
        19046 => "Lenovo",
        20974 => "AMI",
        37945 | 48512 | 60775 => "Inspur",
        40092 => "Wiwynn",
        49622 => "ASRock Rack",
        52538 => "Ampere",
        58132 => "xFusion",
        _ => return None,
    })
}

/// The platform device the IPMI driver registers for the BMC. `/sys/class/ipmi/ipmi0/device/bmc`
/// links to it, but exists only while `ipmi_devintf` is loaded.
fn driver(root: &Root) -> Option<Bmc> {
    let base = "/sys/bus/platform/devices";
    let device = root
        .list(base)
        .into_iter()
        .find(|name| name.starts_with("ipmi_bmc."))?;
    let field = |name: &str| root.read_line(&format!("{base}/{device}/{name}"));
    Some(Bmc::Driver {
        vendor: field("manufacturer_id")
            .and_then(|id| u32::from_str_radix(id.trim_start_matches("0x"), 16).ok()),
        firmware: field("firmware_revision"),
        ipmi: field("ipmi_version"),
    })
}

/// ACPI's Service Processor Management Interface table: `SPMI`, or `SPMI1` and on when there
/// are several.
fn spmi(root: &Root) -> Option<Interface> {
    let base = "/sys/firmware/acpi/tables";
    root.list(base)
        .into_iter()
        .filter(|name| name.starts_with("SPMI"))
        .find_map(|name| {
            let table = root.read_bytes(&format!("{base}/{name}"))?;
            if !table.starts_with(b"SPMI") {
                return None;
            }
            let kind = match *table.get(36)? {
                1 => Some(InterfaceKind::Kcs),
                2 => Some(InterfaceKind::Smic),
                3 => Some(InterfaceKind::Bt),
                4 => Some(InterfaceKind::Ssif),
                _ => None,
            };
            // The specification revision in BCD: 0x0200 is IPMI 2.0, 0x0150 IPMI 1.5.
            let [minor, major] = [*table.get(38)?, *table.get(39)?];
            Some(Interface {
                kind,
                version: (major != 0).then_some((major, minor >> 4)),
            })
        })
}

/// Whether ACPI lists a present IPMI device. Its `status` is ACPI's `_STA`, whose bit 0 means
/// present; without `_STA` a device is present.
fn acpi_device(root: &Root) -> bool {
    let base = "/sys/bus/acpi/devices";
    root.list(base)
        .iter()
        .filter(|name| name.starts_with("IPI0001:"))
        .any(|name| {
            root.read_line(&format!("{base}/{name}/status"))
                .and_then(|status| status.parse::<u32>().ok())
                .is_none_or(|status| status & 1 == 1)
        })
}
