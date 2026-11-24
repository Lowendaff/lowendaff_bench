//! The system section: the host profile in the catalog's words. Dedicated servers start with the
//! machine, add the memory modules after memory and the RAID cards before the disks; a container
//! shows only its own root filesystem. The network cards come after the system's lines, and the
//! section ends with one line per address family, with the public address masked unless the user
//! asked to see it.

use std::net::IpAddr;
use std::net::SocketAddr;

use bench_host::cards::NetworkCard;
use bench_host::cards::Pcie;
use bench_host::cpu::Model;
use bench_host::hardware::Hardware;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::network::Why;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::MemoryFacts;
use bench_host::profile::Read;
use bench_host::profile::StorageFacts;
use bench_host::profile::SystemFacts;
use bench_host::raid::Card;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Unreadable;
use bench_host::storage::Filesystem;

use super::Out;
use crate::locale;
use crate::locale::Cores;
use crate::locale::HostHeader;
use crate::locale::HostLabel;
use crate::locale::NoEcc;
use crate::locale::NoModules;
use crate::network;

const INDENT: &str = "  ";

/// A label of the section, in the catalog's words.
fn label(label: HostLabel) -> &'static str {
    locale::words().host_label(label)
}

/// A value that could not be read.
fn unavailable() -> String {
    locale::words().unavailable()
}

/// The section, with full addresses when `show_ip` is set.
pub(super) fn section(out: &mut Out, host: &HostProfile, show_ip: bool) {
    let words = locale::words();
    out.section(&words.system_title());
    let kind = host.platform.kind();
    match &host.hardware {
        Some(Read::Done(hardware)) => machine(out, hardware),
        Some(missing) => out.field(INDENT, label(HostLabel::Hardware), &missing_text(missing)),
        None => {}
    }
    match &host.cpu {
        Read::Done(cpu) => self::cpu(out, cpu, kind),
        missing => out.field(INDENT, label(HostLabel::Cpu), &missing_text(missing)),
    }
    match &host.memory {
        Read::Done(memory) => self::memory(out, memory.as_ref()),
        missing => out.field(INDENT, label(HostLabel::Memory), &missing_text(missing)),
    }
    if let Some(Read::Done(hardware)) = &host.hardware {
        modules(out, hardware);
    }
    match &host.storage {
        Read::Done(storage) if kind == PlatformKind::Container => container_storage(out, storage),
        Read::Done(storage) => self::storage(out, storage, kind == PlatformKind::Dedicated),
        missing => {
            let storage_label = if kind == PlatformKind::Container {
                HostLabel::Filesystems
            } else {
                HostLabel::Disks
            };
            out.field(INDENT, label(storage_label), &missing_text(missing));
        }
    }
    match &host.system {
        Read::Done(system) => self::system(out, system, &host.arch),
        missing => {
            out.field(INDENT, label(HostLabel::Os), &missing_text(missing));
            out.field(INDENT, label(HostLabel::Arch), &self::arch(&host.arch));
        }
    }
    match &host.cards {
        Read::Done(cards) => self::cards(out, cards, kind == PlatformKind::Container),
        missing => out.field(INDENT, label(HostLabel::Cards), &missing_text(missing)),
    }
    self::network(out, &host.network, show_ip);
}

/// Where the program runs, as the report's environment line and the log name it.
#[must_use]
pub fn platform_text(platform: Platform) -> String {
    locale::words().platform(platform)
}

/// A kind of platform, as `--list` names the ones a plugin applies to.
#[must_use]
pub fn kind_text(kind: PlatformKind) -> &'static str {
    locale::words().platform_kind(kind)
}

/// The words for a section that is missing.
fn missing_text<T>(read: &Read<T>) -> String {
    match read {
        Read::TimedOut => locale::words().read_timed_out(),
        Read::Done(_) | Read::Failed => unavailable(),
    }
}

fn cpu(out: &mut Out, facts: &CpuFacts, kind: PlatformKind) {
    let words = locale::words();
    let cpu = &facts.cpu;
    out.field(
        INDENT,
        label(HostLabel::CpuModel),
        &model(cpu.model.as_ref()),
    );
    out.field(INDENT, label(HostLabel::CpuCores), &cores(facts, kind));
    let mhz = cpu
        .mhz
        .map_or_else(unavailable, |mhz| format!("{:.0} MHz", mhz.round()));
    out.field(INDENT, label(HostLabel::CpuClock), &mhz);
    out.field(INDENT, label(HostLabel::CpuCaches), &caches(&cpu.caches));
    let aes = cpu
        .aes
        .map_or_else(unavailable, |aes| words.aes(aes).to_owned());
    out.field(INDENT, label(HostLabel::Aes), &aes);
    // A container sees its host's CPU flags, which say nothing about what the container may do.
    if kind != PlatformKind::Container {
        let virtualization = words.virtualization(cpu.virtualization);
        out.field(INDENT, label(HostLabel::Virtualization), virtualization);
    }
}

/// An ARM CPU that the table of codes does not know shows its codes rather than a wrong name.
fn model(model: Option<&Model>) -> String {
    model.map_or_else(unavailable, |model| locale::words().cpu_model(model))
}

fn cores(facts: &CpuFacts, kind: PlatformKind) -> String {
    let cpu = &facts.cpu;
    let Some(logical) = cpu.logical else {
        return unavailable();
    };
    let cores = match kind {
        PlatformKind::Dedicated => Cores::Dedicated {
            threads: logical,
            topology: cpu
                .topology
                .map(|topology| (topology.sockets, topology.cores)),
        },
        PlatformKind::Vm => Cores::Vm(logical),
        PlatformKind::Container => Cores::Container {
            usable: facts.usable.unwrap_or(logical),
            quota: facts.quota,
        },
    };
    locale::words().cores(cores)
}

fn caches(caches: &[(String, u64)]) -> String {
    if caches.is_empty() {
        return unavailable();
    }
    let parts: Vec<String> = caches
        .iter()
        .map(|(name, size)| {
            let size = if *size < 1 << 20 {
                format!("{} KiB", size >> 10)
            } else if size % (1 << 20) == 0 {
                format!("{} MiB", size >> 20)
            } else {
                let mib = *size as f64 / f64::from(1 << 20);
                format!("{mib:.1} MiB")
            };
            if name.is_empty() {
                size
            } else {
                format!("{name} {size}")
            }
        })
        .collect();
    parts.join(" / ")
}

fn memory(out: &mut Out, memory: Option<&MemoryFacts>) {
    let Some(memory) = memory else {
        out.field(INDENT, label(HostLabel::MemoryTotal), &unavailable());
        out.field(INDENT, label(HostLabel::MemoryUsed), &unavailable());
        return;
    };
    out.field(INDENT, label(HostLabel::MemoryTotal), &size(memory.total));
    let used = if memory.estimated {
        HostLabel::MemoryUsedEstimated
    } else {
        HostLabel::MemoryUsed
    };
    out.field(INDENT, label(used), &size(memory.used));
    if memory.swap_total == 0 {
        out.field(INDENT, label(HostLabel::Swap), locale::words().none());
    } else {
        out.field(
            INDENT,
            label(HostLabel::SwapTotal),
            &size(memory.swap_total),
        );
        out.field(INDENT, label(HostLabel::SwapUsed), &size(memory.swap_used));
    }
}

fn storage(out: &mut Out, storage: &StorageFacts, dedicated: bool) {
    let words = locale::words();
    if let Some(cards) = &storage.raid {
        raid(out, cards);
    }
    if storage.disks.is_empty() {
        out.field(INDENT, label(HostLabel::Disks), &unavailable());
    } else {
        let mut headers = vec![HostHeader::Device, HostHeader::Interface, HostHeader::Size];
        // Inside a virtual machine the rotation flag is whatever the hypervisor claims, so the
        // medium is shown on dedicated servers only.
        if dedicated {
            headers.push(HostHeader::Medium);
        }
        headers.extend([HostHeader::Model, HostHeader::Use]);
        let rows: Vec<Vec<String>> = storage
            .disks
            .iter()
            .map(|disk| {
                let mut row = vec![
                    disk.name.clone(),
                    disk.interface.to_owned(),
                    size_text(disk.size),
                ];
                if dedicated {
                    row.push(words.medium(disk.rotational).to_owned());
                }
                row.push(disk.model.clone().unwrap_or_else(|| "—".to_owned()));
                row.push(words.uses(&disk.uses));
                row
            })
            .collect();
        table(out, HostLabel::Disks, &headers, &rows);
    }
    if storage.filesystems.is_empty() {
        out.field(INDENT, label(HostLabel::Filesystems), &unavailable());
        return;
    }
    let rows: Vec<Vec<String>> = storage
        .filesystems
        .iter()
        .map(|filesystem| {
            let mut row = filesystem_row(filesystem, storage.work_dir, false);
            row.push(if filesystem.disks.is_empty() {
                "—".to_owned()
            } else {
                words.and(&filesystem.disks)
            });
            row
        })
        .collect();
    let headers = [
        HostHeader::MountPoint,
        HostHeader::Type,
        HostHeader::Size,
        HostHeader::Used,
        HostHeader::Available,
        HostHeader::OnDisks,
    ];
    table(out, HostLabel::Filesystems, &headers, &rows);
}

/// A container sees its host's disks, so only its own root filesystem is shown.
fn container_storage(out: &mut Out, storage: &StorageFacts) {
    let Some(root) = storage.filesystems.first() else {
        out.field(INDENT, label(HostLabel::Filesystems), &unavailable());
        return;
    };
    let row = filesystem_row(root, storage.work_dir, true);
    let headers = [
        HostHeader::MountPoint,
        HostHeader::Type,
        HostHeader::Size,
        HostHeader::Used,
        HostHeader::Available,
    ];
    table(out, HostLabel::Filesystems, &headers, &[row]);
}

/// One row per network card, then a note on each whose PCIe link runs slower or narrower than
/// the card can, as in a slot of fewer lanes; without cards, as inside a container, a line.
fn cards(out: &mut Out, cards: &[NetworkCard], container: bool) {
    let words = locale::words();
    if cards.is_empty() {
        out.field(INDENT, label(HostLabel::Cards), words.no_cards(container));
        return;
    }
    let rows: Vec<Vec<String>> = cards
        .iter()
        .map(|card| {
            vec![
                card.interface.clone(),
                card.model
                    .clone()
                    .unwrap_or_else(|| words.unnamed_card(&card.bus)),
                card.driver.clone().unwrap_or_else(|| "—".to_owned()),
                words.card_link(card.link),
                card.mtu
                    .map_or_else(|| "—".to_owned(), |mtu| mtu.to_string()),
                words.card_uses(card),
            ]
        })
        .collect();
    let headers = [
        HostHeader::Interface,
        HostHeader::Model,
        HostHeader::Driver,
        HostHeader::Link,
        HostHeader::Mtu,
        HostHeader::Use,
    ];
    table(out, HostLabel::Cards, &headers, &rows);
    for card in cards {
        if let Some(pcie) = card.pcie.filter(Pcie::degraded) {
            out.field(
                INDENT,
                &words.note(),
                &words.slow_pcie(&card.interface, pcie),
            );
        }
    }
}

fn table(out: &mut Out, title: HostLabel, headers: &[HostHeader], rows: &[Vec<String>]) {
    let words = locale::words();
    out.heading(INDENT, label(title));
    let headers: Vec<String> = headers
        .iter()
        .map(|header| words.host_header(*header).to_owned())
        .collect();
    out.table(INDENT, &headers, rows);
}

/// Mount point, type, size, used and available. A ZFS pool shares its free space among its
/// datasets, so only the available space is shown for it on bare-metal/VM hosts (whereas inside a
/// container `statvfs("/")` reflects the container dataset quota).
fn filesystem_row(
    filesystem: &Filesystem,
    work_dir: Option<(u32, u32)>,
    container: bool,
) -> Vec<String> {
    let mut mount_point = filesystem.mount_point.clone();
    if work_dir.is_some_and(|device| filesystem.devices.contains(&device)) {
        mount_point.push_str(locale::words().test_directory());
    }
    let (total, used, available) = match filesystem.space {
        Some(space) if filesystem.zfs_pool.is_none() || container => (
            size_text(space.total),
            size_text(space.used),
            size_text(space.available),
        ),
        Some(space) => (unavailable(), unavailable(), size_text(space.available)),
        None => (unavailable(), unavailable(), unavailable()),
    };
    vec![
        mount_point,
        filesystem.fs_type.clone(),
        total,
        used,
        available,
    ]
}

fn system(out: &mut Out, system: &SystemFacts, arch: &str) {
    let words = locale::words();
    let known = |value: Option<&str>| value.map_or_else(unavailable, str::to_owned);
    out.field(
        INDENT,
        label(HostLabel::Os),
        &known(system.distribution.as_deref()),
    );
    out.field(
        INDENT,
        label(HostLabel::Kernel),
        &known(system.kernel.as_deref()),
    );
    out.field(INDENT, label(HostLabel::Arch), &self::arch(arch));
    let mark = |host: bool| if host { words.of_the_host() } else { "" };
    let uptime_text = system.uptime.map_or_else(unavailable, |seen| {
        format!("{}{}", words.uptime(seen.value), mark(seen.host))
    });
    out.field(INDENT, label(HostLabel::Uptime), &uptime_text);
    let load = system.load.map_or_else(unavailable, |seen| {
        let [one, five, fifteen] = seen.value;
        format!("{one:.2} / {five:.2} / {fifteen:.2}{}", mark(seen.host))
    });
    out.field(INDENT, label(HostLabel::Load), &load);
    let tcp = match &system.congestion_control {
        Some(algorithm) => words.tcp(algorithm, system.qdisc.as_deref()),
        None => unavailable(),
    };
    out.field(INDENT, label(HostLabel::Tcp), &tcp);
}

/// The architecture, which the program always knows; a profile built by hand may leave it out.
fn arch(arch: &str) -> String {
    if arch.is_empty() {
        unavailable()
    } else {
        arch.to_owned()
    }
}

fn network(out: &mut Out, network: &Network, show_ip: bool) {
    out.field(INDENT, "IPv4", &family_text(&network.ipv4, "IPv4", show_ip));
    out.field(INDENT, "IPv6", &family_text(&network.ipv6, "IPv6", show_ip));
}

/// Whether a family works, and its public address and source; `name` is `IPv4` or `IPv6`.
#[must_use]
pub fn family_text(family: &Family, name: &str, show_ip: bool) -> String {
    let words = locale::words();
    if !family.reachable() {
        if family.targets.is_empty() {
            return unavailable();
        }
        return words.unavailable_because(&unreachable_text(&family.targets, name));
    }
    match &family.address {
        Address::Found { address, source } => {
            words.public_address(&shown(*address, show_ip), source)
        }
        Address::NotFound(failed) => {
            if failed.is_empty() {
                return words.reachable();
            }
            let reasons: Vec<String> = failed
                .iter()
                .map(|(source, why)| why_text(source, *why, name))
                .collect();
            words.no_public_address(&reasons)
        }
        Address::NotAsked => words.reachable(),
    }
}

/// An address as the user asked to see it: masked, or in full with `--show-ip`.
fn shown(address: IpAddr, show_ip: bool) -> String {
    if show_ip {
        address.to_string()
    } else {
        network::mask(address)
    }
}

fn unreachable_text(targets: &[(SocketAddr, Reach)], name: &str) -> String {
    let words = locale::words();
    if targets.iter().all(|(_, reach)| *reach == Reach::NoRoute) {
        return words.no_route(name);
    }
    let parts: Vec<String> = targets
        .iter()
        .map(|(target, reach)| words.reach(target.ip(), *reach))
        .collect();
    words.and(&parts)
}

/// Why a source gave no address of the family `name`.
#[must_use]
pub fn why_text(source: &str, why: Why, name: &str) -> String {
    locale::words().why(source, why, name)
}

/// A size in the largest binary unit that keeps it at least 1, with two decimals at most, as
/// the system item showed it: `929.08 MiB`, `8 GiB`.
fn size(bytes: u64) -> String {
    let (value, unit) = scaled(bytes);
    let value = (value * 100.0).round() / 100.0;
    if value.fract() == 0.0 {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.2} {unit}")
    }
}

/// A size as table text, such as `931.5 GiB` or `512 MiB`.
fn size_text(bytes: u64) -> String {
    match scaled(bytes) {
        (value, unit @ ("KiB" | "MiB")) => format!("{value:.0} {unit}"),
        (value, "TiB") => format!("{value:.2} TiB"),
        (value, unit) => format!("{value:.1} {unit}"),
    }
}

fn scaled(bytes: u64) -> (f64, &'static str) {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    for (unit, scale) in [
        ("TiB", KIB.powi(4)),
        ("GiB", KIB.powi(3)),
        ("MiB", KIB.powi(2)),
    ] {
        if bytes >= scale {
            return (bytes / scale, unit);
        }
    }
    (bytes / KIB, "KiB")
}

/// The machine, the board, the BIOS or other firmware, and the BMC.
fn machine(out: &mut Out, hardware: &Hardware) {
    let words = locale::words();
    let machine = &hardware.machine;
    let shown = |line: &Line| text(line, line.detail.clone()).unwrap_or_else(unavailable);
    out.field(INDENT, label(HostLabel::Machine), &shown(&machine.system));
    out.field(INDENT, label(HostLabel::Board), &shown(&machine.board));
    let (name, firmware) = match &machine.firmware {
        Some(Firmware::Bios(line)) => (
            HostLabel::Bios,
            text(line, line.detail.as_deref().map(date)),
        ),
        Some(Firmware::UBoot(version)) => (HostLabel::Firmware, Some(words.u_boot(version))),
        Some(Firmware::RaspberryPi(built)) => (
            HostLabel::Firmware,
            Some(words.raspberry_pi(&civil_date(*built))),
        ),
        None => (HostLabel::Firmware, None),
    };
    out.field(INDENT, label(name), &firmware.unwrap_or_else(unavailable));
    out.field(
        INDENT,
        label(HostLabel::Bmc),
        &words.bmc(hardware.bmc.as_ref()),
    );
}

/// `vendor name (detail)`, without the vendor when the name begins with it, ignoring case, and
/// without the parts that are missing; `None` when all are.
fn text(line: &Line, detail: Option<String>) -> Option<String> {
    let name = line.name.as_deref();
    let vendor = line.vendor.as_deref().filter(|vendor| {
        !name.is_some_and(|name| name.to_lowercase().starts_with(&vendor.to_lowercase()))
    });
    let main = [vendor, name]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    match (main.is_empty(), detail) {
        (true, detail) => detail,
        (false, Some(detail)) => Some(locale::words().with_detail(&main, &detail)),
        (false, None) => Some(main),
    }
}

/// A date of the form month/day/year as year-month-day; other forms as they are.
fn date(text: &str) -> String {
    use std::ops::RangeInclusive;

    let digits = |part: &str, lengths: RangeInclusive<usize>| {
        lengths.contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    match text.split('/').collect::<Vec<_>>().as_slice() {
        [month, day, year] if digits(month, 1..=2) && digits(day, 1..=2) && digits(year, 4..=4) => {
            format!("{year}-{month:0>2}-{day:0>2}")
        }
        _ => text.to_owned(),
    }
}

/// The UTC date of a Unix time, as `2024-04-15` (Howard Hinnant's `civil_from_days`).
fn civil_date(seconds: u64) -> String {
    let days = seconds / 86_400 + 719_468;
    let era = days / 146_097;
    let day_of_era = days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!("{year}-{month:02}-{day:02}")
}

/// ECC, then the memory modules: a table of the slots and a note on the ones not in use, else
/// the device tree's memory, else why there is no table.
fn modules(out: &mut Out, hardware: &Hardware) {
    let words = locale::words();
    out.field(INDENT, label(HostLabel::Ecc), &ecc_text(hardware));
    let slots = hardware
        .smbios
        .as_ref()
        .map_or(&[][..], |smbios| smbios.slots.as_slice());
    if slots.is_empty() {
        let board = hardware.board_memory.map(size_text);
        let dev_mem = match &hardware.smbios {
            Err(Unreadable::DevMem(error)) => error.to_string(),
            _ => String::new(),
        };
        let why = match (&board, &hardware.smbios) {
            (Some(size), _) => NoModules::Board(size),
            (None, Ok(_)) => NoModules::NotInSmbios,
            (None, Err(Unreadable::Absent)) => NoModules::NoSmbios,
            (None, Err(Unreadable::DevMem(_))) => NoModules::DevMem(&dev_mem),
        };
        out.field(INDENT, label(HostLabel::Modules), &words.no_modules(why));
        return;
    }
    let names: Vec<String> = slots
        .iter()
        .enumerate()
        .map(|(index, slot)| slot.name.clone().unwrap_or_else(|| words.slot(index + 1)))
        .collect();
    let rows: Vec<Vec<String>> = slots
        .iter()
        .zip(&names)
        .map(|(slot, name)| slot_row(name, slot))
        .collect();
    let headers = [
        HostHeader::Slot,
        HostHeader::Size,
        HostHeader::Type,
        HostHeader::Speed,
        HostHeader::Vendor,
        HostHeader::Model,
    ];
    table(out, HostLabel::Modules, &headers, &rows);
    let disabled: Vec<String> = slots
        .iter()
        .zip(&names)
        .filter(|(slot, _)| slot.capacity == Capacity::Disabled)
        .map(|(_, name)| name.clone())
        .collect();
    if !disabled.is_empty() {
        out.field(INDENT, &words.note(), &words.disabled_modules(&disabled));
    }
}

fn ecc_text(hardware: &Hardware) -> String {
    let words = locale::words();
    let modes = match &hardware.smbios {
        Ok(smbios) if !smbios.ecc.is_empty() => smbios.ecc.clone(),
        _ => hardware
            .edac
            .as_ref()
            .map(|edac| edac.modes.clone())
            .unwrap_or_default(),
    };
    if modes == [Ecc::None] {
        return words.none().to_owned();
    }
    let counts = hardware
        .edac
        .as_ref()
        .map(|edac| (edac.corrected, edac.uncorrected));
    if modes.is_empty() && counts.is_none() {
        let why = match &hardware.smbios {
            Ok(_) => NoEcc::NotInSmbios,
            Err(Unreadable::Absent) => NoEcc::NoSmbios,
            Err(Unreadable::DevMem(_)) => NoEcc::SmbiosUnreadable,
        };
        return words.no_ecc(why);
    }
    words.ecc(&modes, counts)
}

fn slot_row(name: &str, slot: &Slot) -> Vec<String> {
    let words = locale::words();
    let dash = || "—".to_owned();
    let capacity = match slot.capacity {
        Capacity::Empty => {
            return vec![
                name.to_owned(),
                words.empty_slot().to_owned(),
                dash(),
                dash(),
                dash(),
                dash(),
            ];
        }
        Capacity::Disabled => words.disabled_module().to_owned(),
        Capacity::Unknown => words.unknown(),
        Capacity::Kilobytes(kilobytes) if kilobytes % (1 << 20) == 0 => {
            format!("{} GB", kilobytes >> 20)
        }
        Capacity::Kilobytes(kilobytes) if kilobytes % (1 << 10) == 0 => {
            format!("{} MB", kilobytes >> 10)
        }
        Capacity::Kilobytes(kilobytes) => format!("{kilobytes} KB"),
    };
    let kind = match (slot.generation, slot.form) {
        (Some(generation), Some(form)) => format!("{generation} {form}"),
        (Some(only), None) | (None, Some(only)) => only.to_owned(),
        (None, None) => dash(),
    };
    let speed = words
        .module_speed(slot.configured_speed, slot.speed)
        .unwrap_or_else(dash);
    vec![
        name.to_owned(),
        capacity,
        kind,
        speed,
        slot.manufacturer.clone().unwrap_or_else(dash),
        slot.part_number.clone().unwrap_or_else(dash),
    ]
}

/// One line per RAID card, with the disks of the disk table that it holds; none without cards.
fn raid(out: &mut Out, cards: &[Card]) {
    let words = locale::words();
    if cards.is_empty() {
        out.field(INDENT, label(HostLabel::Raid), words.none());
    }
    for card in cards {
        out.field(INDENT, label(HostLabel::Raid), &words.raid_card(card));
    }
}
