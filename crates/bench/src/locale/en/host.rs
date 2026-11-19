//! English: the system section's words.

use std::net::IpAddr;
use std::time::Duration;

use bench_host::bmc;
use bench_host::bmc::Bmc;
use bench_host::bmc::InterfaceKind;
use bench_host::cards::Bus;
use bench_host::cards::Duplex;
use bench_host::cards::Link;
use bench_host::cards::MasterKind;
use bench_host::cards::NetworkCard;
use bench_host::cards::Pcie;
use bench_host::cards::PcieLink;
use bench_host::cpu::Model;
use bench_host::cpu::Virtualization;
use bench_host::network::Reach;
use bench_host::network::Why;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::smbios::Ecc;
use bench_host::storage::Use;

use super::English;
use super::count;
use crate::locale::Cores;
use crate::locale::HostHeader;
use crate::locale::HostLabel;
use crate::locale::HostWords;
use crate::locale::NoEcc;
use crate::locale::NoModules;

/// Why the BMC's vendor and firmware are not known.
const NO_DRIVER: &str = "the kernel has not loaded the IPMI driver";

impl HostWords for English {
    fn host_label(&self, label: HostLabel) -> &'static str {
        match label {
            HostLabel::Hardware => "Hardware",
            HostLabel::Machine => "Machine",
            HostLabel::Board => "Board",
            HostLabel::Bios => "BIOS",
            HostLabel::Firmware => "Firmware",
            HostLabel::Bmc => "BMC",
            HostLabel::Cpu => "CPU",
            HostLabel::CpuModel => "CPU model",
            HostLabel::CpuCores => "CPU cores",
            HostLabel::CpuClock => "CPU clock",
            HostLabel::CpuCaches => "CPU caches",
            HostLabel::Aes => "AES",
            HostLabel::Virtualization => "Virtualization",
            HostLabel::Memory => "Memory",
            HostLabel::MemoryTotal => "Memory total",
            HostLabel::MemoryUsed => "Memory used",
            HostLabel::MemoryUsedEstimated => "Memory used (est.)",
            HostLabel::Swap => "Swap",
            HostLabel::SwapTotal => "Swap total",
            HostLabel::SwapUsed => "Swap used",
            HostLabel::Ecc => "ECC",
            HostLabel::Modules => "Memory modules",
            HostLabel::Raid => "RAID card",
            HostLabel::Disks => "Disks",
            HostLabel::Filesystems => "Filesystems",
            HostLabel::Cards => "Network cards",
            HostLabel::Os => "OS",
            HostLabel::Kernel => "Kernel",
            HostLabel::Arch => "Architecture",
            HostLabel::Uptime => "Uptime",
            HostLabel::Load => "Load",
            HostLabel::Tcp => "TCP congestion",
        }
    }

    fn host_header(&self, header: HostHeader) -> &'static str {
        match header {
            HostHeader::Device => "Device",
            HostHeader::Interface => "Interface",
            HostHeader::Size => "Size",
            HostHeader::Medium => "Medium",
            HostHeader::Model => "Model",
            HostHeader::Use => "Use",
            HostHeader::MountPoint => "Mount point",
            HostHeader::Type => "Type",
            HostHeader::Used => "Used",
            HostHeader::Available => "Available",
            HostHeader::OnDisks => "Disks",
            HostHeader::Slot => "Slot",
            HostHeader::Speed => "Speed",
            HostHeader::Vendor => "Vendor",
            HostHeader::Driver => "Driver",
            HostHeader::Link => "Link",
            HostHeader::Mtu => "MTU",
        }
    }

    fn platform(&self, platform: Platform) -> String {
        match platform {
            Platform::Dedicated => "no virtualization detected".to_owned(),
            Platform::Vm(Hypervisor::Qemu) => {
                "QEMU virtual machine (pure software emulation)".to_owned()
            }
            Platform::Vm(hypervisor) => match hypervisor.name() {
                Some(name) => format!("{name} virtual machine"),
                None => "virtual machine (type unknown)".to_owned(),
            },
            Platform::Container(ContainerKind::Wsl) => "WSL".to_owned(),
            Platform::Container(container) => match container.name() {
                Some(name) => format!("{name} container"),
                None => "container (type unknown)".to_owned(),
            },
        }
    }

    fn platform_kind(&self, kind: PlatformKind) -> &'static str {
        match kind {
            PlatformKind::Dedicated => "dedicated server",
            PlatformKind::Vm => "virtual machine",
            PlatformKind::Container => "container",
        }
    }

    fn read_timed_out(&self) -> String {
        "unavailable (timed out)".to_owned()
    }

    fn none(&self) -> &'static str {
        "none"
    }

    fn aes(&self, supported: bool) -> &'static str {
        if supported {
            "supported"
        } else {
            "not supported"
        }
    }

    fn virtualization(&self, virtualization: Virtualization) -> &'static str {
        match virtualization {
            Virtualization::IntelVtX => "supported (VT-x)",
            Virtualization::AmdV => "supported (AMD-V)",
            Virtualization::Absent => "not supported",
            Virtualization::NotApplicable => "not applicable",
        }
    }

    fn cpu_model(&self, model: &Model) -> String {
        match model {
            Model::Name(name) => name.clone(),
            Model::UnknownPart { vendor, part } => format!("{vendor} (part code {part:#05x})"),
            Model::UnknownVendor {
                implementer,
                part: Some(part),
            } => format!("ARM CPU (vendor code {implementer:#04x}, part code {part:#05x})"),
            Model::UnknownVendor {
                implementer,
                part: None,
            } => format!("ARM CPU (vendor code {implementer:#04x})"),
        }
    }

    fn cores(&self, cores: Cores) -> String {
        match cores {
            Cores::Dedicated {
                threads,
                topology: Some((sockets, cores)),
            } if sockets > 1 => format!(
                "{} ({} CPUs, {} in all)",
                count(threads, "thread", "threads"),
                sockets,
                count(cores, "core", "cores")
            ),
            Cores::Dedicated {
                threads,
                topology: Some((_, cores)),
            } => format!(
                "{} ({})",
                count(threads, "thread", "threads"),
                count(cores, "core", "cores")
            ),
            Cores::Dedicated {
                threads,
                topology: None,
            } => count(threads, "thread", "threads"),
            Cores::Vm(vcpus) => count(vcpus, "vCPU", "vCPUs"),
            Cores::Container {
                usable,
                quota: Some(quota),
            } => format!(
                "{} (quota {})",
                count(usable, "CPU", "CPUs"),
                decimals(quota)
            ),
            Cores::Container {
                usable,
                quota: None,
            } => count(usable, "CPU", "CPUs"),
        }
    }

    fn with_detail(&self, name: &str, detail: &str) -> String {
        format!("{name} ({detail})")
    }

    fn u_boot(&self, version: &str) -> String {
        format!("U-Boot {version}")
    }

    fn raspberry_pi(&self, built: &str) -> String {
        format!("Raspberry Pi bootloader, built {built}")
    }

    fn bmc(&self, bmc: Option<&Bmc>) -> String {
        match bmc {
            Some(Bmc::Driver {
                vendor,
                firmware,
                ipmi,
            }) => {
                let vendor = vendor.map(|number| {
                    bmc::vendor(number).map_or_else(|| format!("vendor {number}"), str::to_owned)
                });
                let parts: Vec<String> = [
                    vendor,
                    firmware
                        .as_ref()
                        .map(|firmware| format!("firmware {firmware}")),
                    ipmi.as_ref().map(|version| format!("IPMI {version}")),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    "present".to_owned()
                } else {
                    parts.join(", ")
                }
            }
            Some(Bmc::Interface(interface)) => {
                let parts: Vec<String> = [
                    interface
                        .version
                        .map(|(major, minor)| format!("IPMI {major}.{minor}")),
                    interface.kind.map(|kind| {
                        let name = match kind {
                            InterfaceKind::Kcs => "KCS",
                            InterfaceKind::Smic => "SMIC",
                            InterfaceKind::Bt => "BT",
                            InterfaceKind::Ssif => "SSIF",
                        };
                        format!("{name} interface")
                    }),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    format!("present; interface, vendor and firmware unavailable: {NO_DRIVER}")
                } else {
                    format!(
                        "present ({}); vendor and firmware unavailable: {NO_DRIVER}",
                        parts.join(", ")
                    )
                }
            }
            Some(Bmc::Present) => {
                format!("present; interface, vendor and firmware unavailable: {NO_DRIVER}")
            }
            None => "not detected".to_owned(),
        }
    }

    fn ecc(&self, modes: &[Ecc], counts: Option<(u64, u64)>) -> String {
        let counts = counts.map(|(corrected, uncorrected)| {
            format!("since boot {corrected} corrected, {uncorrected} uncorrected")
        });
        let modes: Vec<&str> = modes
            .iter()
            .map(|mode| match mode {
                Ecc::SingleBit => "yes (single-bit correction)",
                Ecc::MultiBit => "yes (multi-bit correction)",
                Ecc::Crc => "yes (CRC)",
                Ecc::Parity => "no correction (parity only)",
                Ecc::None => "none",
                Ecc::Unknown => "unknown",
            })
            .collect();
        match (modes.is_empty(), counts) {
            (false, Some(counts)) => format!("{}; {counts}", modes.join(", ")),
            (false, None) => format!(
                "{}; error counts unavailable: the kernel has not loaded an EDAC driver",
                modes.join(", ")
            ),
            (true, Some(counts)) => format!("way of correction unavailable; {counts}"),
            (true, None) => "unavailable".to_owned(),
        }
    }

    fn no_ecc(&self, why: NoEcc) -> String {
        let smbios = match why {
            NoEcc::NotInSmbios => "SMBIOS has no word on error correction",
            NoEcc::NoSmbios => "there is no SMBIOS",
            NoEcc::SmbiosUnreadable => "SMBIOS cannot be read",
        };
        format!("unavailable: {smbios}, and there is no EDAC driver")
    }

    fn no_modules(&self, why: NoModules<'_>) -> String {
        match why {
            NoModules::Board(size) => format!(
                "onboard memory {size} (from the device tree); no word on removable modules"
            ),
            NoModules::NotInSmbios => "unavailable: SMBIOS describes no modules".to_owned(),
            NoModules::NoSmbios => "unavailable: SMBIOS cannot be read".to_owned(),
            NoModules::DevMem(error) => {
                format!("unavailable: SMBIOS cannot be read (/dev/mem: {error})")
            }
        }
    }

    fn slot(&self, number: usize) -> String {
        format!("Slot {number}")
    }

    fn empty_slot(&self) -> &'static str {
        "empty"
    }

    fn disabled_module(&self) -> &'static str {
        "not in use"
    }

    fn module_speed(&self, configured: Option<u32>, rated: Option<u32>) -> Option<String> {
        match (configured, rated) {
            (Some(configured), Some(rated)) if configured != rated => {
                Some(format!("{configured} MT/s (rated {rated})"))
            }
            (Some(configured), _) => Some(format!("{configured} MT/s")),
            (None, Some(rated)) => Some(format!("{rated} MT/s (rated)")),
            (None, None) => None,
        }
    }

    fn disabled_modules(&self, slots: &[String]) -> String {
        let (verb, them) = if slots.len() == 1 {
            ("holds a module", "it")
        } else {
            ("hold modules", "them")
        };
        format!(
            "{} {verb} that the firmware reports as not in use, with a size of 0; the total \
             leaves {them} out",
            slots.join(", ")
        )
    }

    fn raid_card(&self, card: &Card) -> String {
        let name = card.name.clone().unwrap_or_else(|| {
            let driver = card.driver.as_ref().map_or_else(
                || "no driver loaded".to_owned(),
                |driver| format!("driver {driver}"),
            );
            let ids = format!("PCI {:04x}:{:04x}", card.ids.vendor, card.ids.device);
            match card.ids.subsystem {
                Some((vendor, device)) => {
                    format!("{ids} (subsystem {vendor:04x}:{device:04x}, {driver})")
                }
                None => format!("{ids} ({driver})"),
            }
        });
        if card.disks.is_empty() {
            return format!("{name}: no disks attached");
        }
        let groups: Vec<String> = [
            (Attachment::Array, "array"),
            (Attachment::PassThrough, "pass-through"),
            (Attachment::Unknown, "unknown"),
        ]
        .iter()
        .filter_map(|(attachment, label)| {
            let disks: Vec<&str> = card
                .disks
                .iter()
                .filter(|(_, how)| how == attachment)
                .map(|(disk, _)| disk.as_str())
                .collect();
            (!disks.is_empty()).then(|| format!("{label} {}", disks.join(", ")))
        })
        .collect();
        format!("{name}: {}", groups.join("; "))
    }

    fn medium(&self, rotational: Option<bool>) -> &'static str {
        match rotational {
            Some(false) => "SSD",
            Some(true) => "HDD",
            None => "unknown",
        }
    }

    fn uses(&self, uses: &[Use]) -> String {
        if uses.is_empty() {
            return "not mounted".to_owned();
        }
        let parts: Vec<String> = uses
            .iter()
            .map(|item| match item {
                Use::Mount(point) if point == "/" => "system disk (/)".to_owned(),
                Use::Mount(point) => point.clone(),
                Use::Swap => "swap".to_owned(),
                Use::ZfsPool(pool) => format!("ZFS pool {pool}"),
            })
            .collect();
        parts.join(", ")
    }

    fn card_link(&self, link: Link) -> String {
        let Link::Up { speed, duplex } = link else {
            return "down".to_owned();
        };
        let Some(speed) = speed else {
            return "not reported".to_owned();
        };
        let speed = if speed >= 1000 {
            format!("{} Gbit/s", f64::from(speed) / 1000.0)
        } else {
            format!("{speed} Mbit/s")
        };
        match duplex {
            Some(Duplex::Full) => format!("{speed}, full duplex"),
            Some(Duplex::Half) => format!("{speed}, half duplex"),
            None => speed,
        }
    }

    fn card_uses(&self, card: &NetworkCard) -> String {
        let mut parts = Vec::new();
        if let Some(master) = &card.master {
            parts.push(match master.kind {
                MasterKind::Bridge => format!("bridge {}", master.name),
                MasterKind::Bond => format!("bond {}", master.name),
                MasterKind::Other => format!("in {}", master.name),
            });
        }
        match (card.default_route.ipv4, card.default_route.ipv6) {
            (true, true) => parts.push("default route (IPv4, IPv6)".to_owned()),
            (true, false) => parts.push("default route (IPv4)".to_owned()),
            (false, true) => parts.push("default route (IPv6)".to_owned()),
            (false, false) => {}
        }
        if card.virtual_functions > 0 {
            let functions = usize::try_from(card.virtual_functions).unwrap_or(usize::MAX);
            parts.push(count(functions, "virtual function", "virtual functions"));
        }
        if parts.is_empty() {
            "—".to_owned()
        } else {
            parts.join(", ")
        }
    }

    fn unnamed_card(&self, bus: &Bus) -> String {
        match bus {
            Bus::Pci { vendor, device } => format!("PCI {vendor:04x}:{device:04x}"),
            Bus::Usb => "USB adapter".to_owned(),
            Bus::Platform(Some(name)) => name.clone(),
            Bus::Platform(None) => "built into the board".to_owned(),
            Bus::Other(name) if name.is_empty() => "—".to_owned(),
            Bus::Other(name) => name.clone(),
        }
    }

    fn no_cards(&self, container: bool) -> &'static str {
        if container {
            "none visible inside a container"
        } else {
            "none found"
        }
    }

    fn slow_pcie(&self, interface: &str, pcie: Pcie) -> String {
        let link = |link: PcieLink| format!("PCIe {}.0 x{}", link.generation, link.lanes);
        format!(
            "{interface}'s card is linked at {}, below the {} it supports",
            link(pcie.current),
            link(pcie.best)
        )
    }

    fn test_directory(&self) -> &'static str {
        " (test directory)"
    }

    fn of_the_host(&self) -> &'static str {
        " (host)"
    }

    fn uptime(&self, uptime: Duration) -> String {
        let minutes = usize::try_from(uptime.as_secs() / 60).unwrap_or(usize::MAX);
        let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
        let minutes = count(minutes, "minute", "minutes");
        if days > 0 {
            let days = count(days, "day", "days");
            format!("{days}, {}, {minutes}", count(hours, "hour", "hours"))
        } else if hours > 0 {
            format!("{}, {minutes}", count(hours, "hour", "hours"))
        } else {
            minutes
        }
    }

    fn tcp(&self, algorithm: &str, qdisc: Option<&str>) -> String {
        match qdisc {
            Some(qdisc) => format!("{algorithm} (queue {qdisc})"),
            None => algorithm.to_owned(),
        }
    }

    fn reachable(&self) -> String {
        "available".to_owned()
    }

    fn public_address(&self, address: &str, source: &str) -> String {
        format!("available; public address {address} (from {source})")
    }

    fn no_public_address(&self, reasons: &[String]) -> String {
        format!(
            "available; public address unavailable: {}",
            reasons.join(", ")
        )
    }

    fn no_route(&self, family: &str) -> String {
        format!("this host has no {family} route")
    }

    fn reach(&self, ip: IpAddr, reach: Reach) -> String {
        match reach {
            Reach::Connected => format!("connected to {ip}"),
            Reach::TimedOut => format!("connecting to {ip} timed out"),
            Reach::Refused => format!("{ip} refused the connection"),
            Reach::NoRoute => format!("no route to {ip}"),
            Reach::Failed => format!("cannot connect to {ip}"),
        }
    }

    fn why(&self, source: &str, why: Why, family: &str) -> String {
        match why {
            Why::TimedOut => format!("{source} timed out"),
            Why::Status(code) => format!("{source} returned HTTP {code}"),
            Why::NotAnAddress => format!("{source} returned no {family} address"),
            Why::NotPublic => format!("{source} returned no public address"),
            Why::NoAddress => format!("{source} has no {family} address"),
            Why::Unreachable => format!("{source} cannot be reached"),
        }
    }
}

/// `1.5`, `2`: at most two decimals, without trailing zeros.
fn decimals(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
