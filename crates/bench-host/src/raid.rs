//! RAID cards (system information design §9.5): the PCI devices of class 0x0104, named by the
//! host's `pci.ids`, and how the disks of the 硬盘 table attach to them.

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;

use crate::Root;

/// Where distributions keep the PCI ID database, in the order they are tried.
const PCI_IDS: [&str; 3] = [
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
];

/// A RAID card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// The PCI address, such as `0000:5e:00.0`.
    pub address: String,
    /// The PCI IDs: vendor, device, and the subsystem's vendor and device.
    pub ids: Ids,
    /// The name `pci.ids` gives the card, or `None` without the file or an entry.
    pub name: Option<String>,
    /// The kernel driver, or `None` when none is bound.
    pub driver: Option<String>,
    /// The card's disks among the ones asked about, in their order, with how they attach.
    pub disks: Vec<(String, Attachment)>,
}

/// A PCI device's IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    /// The vendor.
    pub vendor: u16,
    /// The device.
    pub device: u16,
    /// The subsystem's vendor and device, when the device has them.
    pub subsystem: Option<(u16, u16)>,
}

/// How a disk attaches to its card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attachment {
    /// A volume of the card's RAID.
    Array,
    /// A disk the card passes through.
    PassThrough,
    /// A disk on a driver whose channels are not known.
    Unknown,
}

/// The RAID cards in PCI address order, with which of `disks` attach to each.
#[must_use]
pub fn read(root: &Root, disks: &[String]) -> Vec<Card> {
    let base = "/sys/bus/pci/devices";
    let mut cards = Vec::new();
    for address in root.list(base) {
        let path = format!("{base}/{address}");
        if !root
            .read_line(&format!("{path}/class"))
            .is_some_and(|class| class.starts_with("0x0104"))
        {
            continue;
        }
        let id = |name: &str| {
            let text = root.read_line(&format!("{path}/{name}"))?;
            u16::from_str_radix(text.trim_start_matches("0x"), 16).ok()
        };
        let (Some(vendor), Some(device)) = (id("vendor"), id("device")) else {
            continue;
        };
        let ids = Ids {
            vendor,
            device,
            subsystem: id("subsystem_vendor").zip(id("subsystem_device")),
        };
        let driver = root.link(&format!("{path}/driver")).and_then(|target| {
            Path::new(&target)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        let disks = disks
            .iter()
            .filter_map(|disk| {
                let target = root.link(&format!("/sys/block/{disk}"))?;
                let segments: Vec<&str> = target.split('/').collect();
                segments
                    .contains(&address.as_str())
                    .then(|| (disk.clone(), attachment(driver.as_deref(), &segments)))
            })
            .collect();
        cards.push(Card {
            name: name(root, ids),
            address,
            ids,
            driver,
            disks,
        });
    }
    cards
}

/// How a disk attaches, from its card's driver and the SCSI channel in its sysfs path, which
/// ends `…/host0/target0:0:18/0:0:18:0/block/sda`. The channels follow each driver's source
/// (design §9.5).
fn attachment(driver: Option<&str>, segments: &[&str]) -> Attachment {
    let channel = segments
        .iter()
        .position(|segment| *segment == "block")
        .and_then(|block| segments.get(block.checked_sub(1)?))
        .and_then(|address| {
            let numbers: Vec<u32> = address
                .split(':')
                .map(str::parse::<u32>)
                .collect::<Result<_, _>>()
                .ok()?;
            (numbers.len() == 4).then(|| numbers[1])
        });
    match (driver, channel) {
        // RST and VROC are software RAID: the disks are visible one by one.
        (Some("ahci" | "vmd"), _) => Attachment::PassThrough,
        (Some("megaraid_sas"), Some(channel)) => {
            if channel >= 2 {
                Attachment::Array
            } else {
                Attachment::PassThrough
            }
        }
        (Some("hpsa"), Some(1 | 2)) | (Some("smartpqi"), Some(1 | 3)) => Attachment::Array,
        (Some("hpsa"), Some(0 | 3)) | (Some("smartpqi"), Some(0 | 2)) => Attachment::PassThrough,
        (Some("aacraid"), Some(0)) | (Some("mpt3sas"), Some(1)) => Attachment::Array,
        (Some("aacraid"), Some(_)) | (Some("mpt3sas"), Some(0 | 2)) => Attachment::PassThrough,
        _ => Attachment::Unknown,
    }
}

/// The card's name in the first `pci.ids` there is: the subsystem's name, else the vendor's
/// and the device's names. The file is read only up to the entry.
fn name(root: &Root, ids: Ids) -> Option<String> {
    let path = PCI_IDS.iter().find(|path| root.exists(path))?;
    let file = File::open(root.join(path)).ok()?;
    let vendor_key = format!("{:04x}  ", ids.vendor);
    let device_key = format!("\t{:04x}  ", ids.device);
    let subsystem_key = ids
        .subsystem
        .map(|(vendor, device)| format!("\t\t{vendor:04x} {device:04x}  "));
    let (mut vendor, mut device) = (None, None);
    let lines = BufReader::new(file).lines();
    for line in lines {
        let Ok(line) = line else {
            break;
        };
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with("\t\t") {
            if device.is_some()
                && let Some(name) = subsystem_key
                    .as_ref()
                    .and_then(|key| line.strip_prefix(key))
            {
                return Some(name.to_owned());
            }
        } else if line.starts_with('\t') {
            if device.is_some() {
                break;
            }
            if vendor.is_some() {
                device = line.strip_prefix(&device_key).map(str::to_owned);
            }
        } else {
            if vendor.is_some() {
                break;
            }
            vendor = line.strip_prefix(&vendor_key).map(str::to_owned);
        }
    }
    Some(format!("{} {}", vendor?, device?))
}
