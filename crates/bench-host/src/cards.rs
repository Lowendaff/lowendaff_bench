//! Network cards: the interfaces of `/sys/class/net` with a device behind them, each with the
//! names that udev's hardware database gives it, its driver, its link as negotiated, its MTU, a
//! PCI card's PCIe link, the bridge or bond it is in, and whether a default route goes out through
//! it. Loopback, bridges, bonds, VLANs, veth, tun and tap have no device, so they are no cards; a
//! container sees only such interfaces. Read from files only, keeping nothing that can identify
//! a card or a host: no address, port id, switch id or VPD, and of udev's file only its names.

use std::collections::BTreeSet;
use std::path::Path;

use crate::Root;

/// Where the interfaces are.
const NET: &str = "/sys/class/net";

/// Trailing words of a maker's name in udev's database that its short name leaves out, as in
/// "Intel Corporation" or "Realtek Semiconductor Co., Ltd.".
const COMPANY_WORDS: [&str; 21] = [
    "and subsidiaries",
    "Corporation",
    "Corp.",
    "Corp",
    "Incorporated",
    "Inc.",
    "Inc",
    "Co.",
    "Co",
    "Ltd.",
    "Ltd",
    "Limited",
    "LLC",
    "GmbH",
    "AG",
    "Technologies",
    "Technology",
    "Semiconductor",
    "Communications",
    "Systems",
    "Group",
];

/// One network card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCard {
    /// The interface, such as `eth0`.
    pub interface: String,
    /// The bus that the card is on.
    pub bus: Bus,
    /// The maker's short name and the model, from udev's hardware database, without the product
    /// of a board maker that the database adds in parentheses, as in "Intel Ethernet Controller
    /// 10-Gigabit X540-AT2"; `None` when udev names no model, as for a board's own Ethernet or
    /// inside a container.
    pub model: Option<String>,
    /// The driver of the card's device.
    pub driver: Option<String>,
    /// The link as negotiated.
    pub link: Link,
    /// The MTU in bytes.
    pub mtu: Option<u32>,
    /// A PCI card's PCIe link: as it runs, and the best that the card can do.
    pub pcie: Option<Pcie>,
    /// The virtual functions enabled on the card (SR-IOV); 0 for most cards.
    pub virtual_functions: u32,
    /// The bridge or bond that the card is in.
    pub master: Option<Master>,
    /// The address families whose default route goes out through the card, itself or the bridge
    /// or bond above it.
    pub default_route: Routes,
}

/// The bus of a card's device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bus {
    /// A PCI device, with its vendor and device ids; a virtio card's ids are its PCI device's.
    Pci {
        /// The vendor.
        vendor: u16,
        /// The device.
        device: u16,
    },
    /// A USB adapter.
    Usb,
    /// A board's own device, with the first name of its device tree node, such as
    /// `rockchip,rk3568-gmac`, when it has one.
    Platform(Option<String>),
    /// Another bus, by its name.
    Other(String),
}

/// A link as negotiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// The link is up, with its speed in Mbit/s and its duplex, each when the driver reports it;
    /// virtio's reports neither.
    Up {
        /// The speed in Mbit/s.
        speed: Option<u32>,
        /// The duplex.
        duplex: Option<Duplex>,
    },
    /// There is no carrier, or the interface is down.
    Down,
}

/// A link's duplex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duplex {
    /// Both ways at once.
    Full,
    /// One way at a time.
    Half,
}

/// A PCI card's PCIe link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pcie {
    /// The link as it runs.
    pub current: PcieLink,
    /// The best that the card can do.
    pub best: PcieLink,
}

impl Pcie {
    /// Whether the link runs slower or narrower than the card can, as in a slot of fewer lanes.
    #[must_use]
    pub fn degraded(&self) -> bool {
        self.current.generation < self.best.generation || self.current.lanes < self.best.lanes
    }
}

/// A PCIe link: its generation and its lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcieLink {
    /// The generation, from the transfer rate: 1 for 2.5 GT/s up to 6 for 64 GT/s.
    pub generation: u8,
    /// The lanes.
    pub lanes: u8,
}

/// The bridge or bond that a card is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Master {
    /// Its interface, such as `vmbr0`.
    pub name: String,
    /// What it is.
    pub kind: MasterKind,
}

/// What holds a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterKind {
    /// A bridge.
    Bridge,
    /// A bond.
    Bond,
    /// Something else, such as a team or an Open vSwitch port.
    Other,
}

/// The address families of the default routes through a card.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Routes {
    /// IPv4's default route.
    pub ipv4: bool,
    /// IPv6's default route.
    pub ipv6: bool,
}

/// The cards in interface name order. A virtual function whose physical function is a card of
/// the host counts on that card instead of having a row of its own; one passed through to a VM
/// is that VM's card.
#[must_use]
pub fn read(root: &Root) -> Vec<NetworkCard> {
    let ipv4 = default_routes(root, false);
    let ipv6 = default_routes(root, true);
    let mut cards = Vec::new();
    for interface in root.list(NET) {
        let path = format!("{NET}/{interface}");
        let Some(address) = link_name(root, &format!("{path}/device")) else {
            continue;
        };
        let function_of = link_name(root, &format!("{path}/device/physfn"));
        let card = card(root, &interface, &path, &ipv4, &ipv6);
        cards.push((card, address, function_of));
    }
    let physical: BTreeSet<String> = cards
        .iter()
        .filter(|(_, _, function_of)| function_of.is_none())
        .map(|(_, address, _)| address.clone())
        .collect();
    cards
        .into_iter()
        .filter(|(_, _, function_of)| {
            function_of
                .as_ref()
                .is_none_or(|parent| !physical.contains(parent))
        })
        .map(|(card, _, _)| card)
        .collect()
}

/// One interface with a device, at `path`.
fn card(
    root: &Root,
    interface: &str,
    path: &str,
    ipv4: &BTreeSet<String>,
    ipv6: &BTreeSet<String>,
) -> NetworkCard {
    let device = format!("{path}/device");
    let udev = root
        .read_line(&format!("{path}/ifindex"))
        .and_then(|index| root.read(&format!("/run/udev/data/n{index}")))
        .unwrap_or_default();
    let key = |name: &str| {
        udev.lines()
            .find_map(|line| line.strip_prefix(&format!("E:{name}=")))
    };
    let subsystem = link_name(root, &format!("{device}/subsystem"));
    let bus = match subsystem.as_deref() {
        // A virtio device's own ids are the virtio bus's; its card's are its PCI device's.
        Some("virtio") => pci(root, &format!("{device}/..")),
        Some("pci") => pci(root, &device),
        Some("usb") => Some(Bus::Usb),
        Some("platform") => Some(Bus::Platform(
            crate::device_tree_strings(root, &format!("{device}/of_node/compatible"))
                .into_iter()
                .next(),
        )),
        _ => None,
    }
    .unwrap_or_else(|| Bus::Other(subsystem.clone().unwrap_or_default()));
    let pcie = (subsystem.as_deref() == Some("pci"))
        .then(|| self::pcie(root, &device))
        .flatten();
    let master = link_name(root, &format!("{path}/master")).map(|name| {
        let kind = if root.exists(&format!("{NET}/{name}/bridge")) {
            MasterKind::Bridge
        } else if root.exists(&format!("{NET}/{name}/bonding")) {
            MasterKind::Bond
        } else {
            MasterKind::Other
        };
        Master { name, kind }
    });
    let chain = chain(root, interface);
    NetworkCard {
        interface: interface.to_owned(),
        bus,
        model: model(
            key("ID_VENDOR_FROM_DATABASE"),
            key("ID_MODEL_FROM_DATABASE"),
        ),
        driver: link_name(root, &format!("{device}/driver"))
            .or_else(|| key("ID_NET_DRIVER").map(str::to_owned)),
        link: link(root, path),
        mtu: number(root, &format!("{path}/mtu")),
        pcie,
        virtual_functions: number(root, &format!("{device}/sriov_numvfs")).unwrap_or(0),
        master,
        default_route: Routes {
            ipv4: chain.iter().any(|name| ipv4.contains(name)),
            ipv6: chain.iter().any(|name| ipv6.contains(name)),
        },
    }
}

/// A PCI device's ids at `path`.
fn pci(root: &Root, path: &str) -> Option<Bus> {
    let id = |name: &str| {
        let text = root.read_line(&format!("{path}/{name}"))?;
        u16::from_str_radix(text.trim_start_matches("0x"), 16).ok()
    };
    Some(Bus::Pci {
        vendor: id("vendor")?,
        device: id("device")?,
    })
}

/// A PCI device's PCIe link at `path`, from `current_link_speed` (such as "8.0 GT/s PCIe") and
/// `current_link_width`, and their best, `max_link_speed` and `max_link_width`.
fn pcie(root: &Root, path: &str) -> Option<Pcie> {
    let link = |which: &str| {
        let speed = root.read_line(&format!("{path}/{which}_link_speed"))?;
        let lanes = root.read_line(&format!("{path}/{which}_link_width"))?;
        Some(PcieLink {
            generation: generation(&speed)?,
            lanes: lanes.parse().ok().filter(|lanes| *lanes > 0)?,
        })
    };
    Some(Pcie {
        current: link("current")?,
        best: link("max")?,
    })
}

/// The PCIe generation of a transfer rate, such as "16.0 GT/s PCIe".
fn generation(speed: &str) -> Option<u8> {
    let rate = speed.split_whitespace().next()?;
    match rate {
        "2.5" => Some(1),
        "5.0" | "5" => Some(2),
        "8.0" | "8" => Some(3),
        "16.0" | "16" => Some(4),
        "32.0" | "32" => Some(5),
        "64.0" | "64" => Some(6),
        _ => None,
    }
}

/// The link of the interface at `path`. `carrier` says whether it is up, and reading it fails
/// while the interface is down. `speed` and `duplex` are valid only for drivers that report link
/// settings: reading them fails while the link is down, and a driver that knows no speed gives -1,
/// which older kernels print as the largest unsigned number.
fn link(root: &Root, path: &str) -> Link {
    if root.read_line(&format!("{path}/carrier")).as_deref() != Some("1") {
        return Link::Down;
    }
    let speed = root
        .read_line(&format!("{path}/speed"))
        .and_then(|speed| speed.parse().ok())
        .filter(|speed| (1..u32::MAX).contains(speed));
    let duplex = match root.read_line(&format!("{path}/duplex")).as_deref() {
        Some("full") => Some(Duplex::Full),
        Some("half") => Some(Duplex::Half),
        _ => None,
    };
    Link::Up { speed, duplex }
}

/// The interface, then the bridges and bonds above it, nearest first, up to a link that loops
/// back.
fn chain(root: &Root, interface: &str) -> Vec<String> {
    let mut chain = vec![interface.to_owned()];
    loop {
        let last = chain.last().map_or(interface, String::as_str);
        let Some(master) = link_name(root, &format!("{NET}/{last}/master")) else {
            return chain;
        };
        if chain.contains(&master) {
            return chain;
        }
        chain.push(master);
    }
}

/// The interfaces that the default routes of a family go out through: the routes of mask 0 in
/// `/proc/net/route`, and of prefix 0 in `/proc/net/ipv6_route`, whose destinations the kernel
/// keeps at 0 then. Without a default route, IPv6's table has one that rejects through `lo`, which
/// is no card.
fn default_routes(root: &Root, ipv6: bool) -> BTreeSet<String> {
    let path = if ipv6 {
        "/proc/net/ipv6_route"
    } else {
        "/proc/net/route"
    };
    let Some(text) = root.read(path) else {
        return BTreeSet::new();
    };
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let default = if ipv6 {
                fields.get(1) == Some(&"00")
            } else {
                fields.get(7) == Some(&"00000000")
            };
            if !default {
                return None;
            }
            // IPv4's table names the interface first, IPv6's last.
            let interface = if ipv6 { fields.last() } else { fields.first() };
            interface.map(|name| (*name).to_owned())
        })
        .collect()
}

/// A number in a file.
fn number<T: std::str::FromStr>(root: &Root, path: &str) -> Option<T> {
    root.read_line(path)?.parse().ok()
}

/// The last part of the target of the link at `path`, such as `ixgbe` for
/// `../../../../bus/pci/drivers/ixgbe`.
fn link_name(root: &Root, path: &str) -> Option<String> {
    let target = root.link(path)?;
    Some(
        Path::new(&target)
            .file_name()?
            .to_string_lossy()
            .into_owned(),
    )
}

/// A card's model from udev's hardware database: the maker's short name before the model, unless
/// the model starts with it; `None` without a model.
#[must_use]
pub fn model(vendor: Option<&str>, model: Option<&str>) -> Option<String> {
    let model = without_note(model?);
    if model.is_empty() {
        return None;
    }
    match vendor.map(short_maker) {
        Some(vendor) if !model.starts_with(&vendor) => Some(format!("{vendor} {model}")),
        _ => Some(model.to_owned()),
    }
}

/// A name without the note that the database adds in parentheses at its end: a board maker's
/// product after a model, as in "Ethernet Controller 10-Gigabit X540-AT2 (10G base-T DP EP102Ti3A
/// Adapter)", or a former name after a maker, as in "Microchip Technology, Inc. (formerly SMSC)".
/// A parenthesis of one word belongs to the name, as in "Elastic Network Adapter (ENA)", and one
/// that holds another stays whole.
fn without_note(name: &str) -> &str {
    let Some(open) = name.strip_suffix(')').and_then(|rest| rest.rfind(" (")) else {
        return name;
    };
    let inside = &name[open + 2..name.len() - 1];
    if inside.contains(' ') && !inside.contains(')') {
        &name[..open]
    } else {
        name
    }
}

/// A maker's short name: the one in brackets when the database gives one, as in "Advanced Micro
/// Devices, Inc. [AMD]"; otherwise the name without its company words, as "Intel" for "Intel
/// Corporation" or "Red Hat" for "Red Hat, Inc.". A word counts only after a space or a comma.
fn short_maker(vendor: &str) -> String {
    let vendor = without_note(vendor);
    if let (Some(open), Some(rest)) = (vendor.rfind('['), vendor.strip_suffix(']')) {
        return rest[open + 1..].to_owned();
    }
    let mut name = vendor;
    while let Some(rest) = COMPANY_WORDS.iter().find_map(|word| {
        name.strip_suffix(word)
            .filter(|rest| rest.ends_with([' ', ',']))
    }) {
        name = rest.trim_end_matches([' ', ',']);
    }
    name.to_owned()
}
