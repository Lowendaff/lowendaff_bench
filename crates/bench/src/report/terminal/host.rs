//! 【系统信息】: the host profile in words. Dedicated servers start with the machine, add the
//! memory modules after memory and the RAID cards before the disks; a container shows only its
//! own root filesystem. The section ends with one line per address family, with the public
//! address masked unless the user asked to see it.

use std::net::IpAddr;
use std::net::SocketAddr;
use std::time::Duration;

use bench_host::bmc;
use bench_host::bmc::Bmc;
use bench_host::bmc::InterfaceKind;
use bench_host::cpu::Virtualization;
use bench_host::hardware::Hardware;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::network::Network;
use bench_host::network::Reach;
use bench_host::network::Why;
use bench_host::platform::PlatformKind;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::MemoryFacts;
use bench_host::profile::Read;
use bench_host::profile::StorageFacts;
use bench_host::profile::SystemFacts;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Unreadable;
use bench_host::storage::Filesystem;
use bench_host::storage::Use;

use super::Out;
use crate::network;

const INDENT: &str = "  ";

/// A value that could not be read.
const UNAVAILABLE: &str = "不可用";

/// The section, with full addresses when `show_ip` is set.
pub(super) fn section(out: &mut Out, host: &HostProfile, show_ip: bool) {
    out.line("\n【系统信息】");
    let kind = host.platform.kind();
    match &host.hardware {
        Some(Read::Done(hardware)) => machine(out, hardware),
        Some(missing) => out.field(INDENT, "硬件", &missing_text(missing)),
        None => {}
    }
    match &host.cpu {
        Read::Done(cpu) => self::cpu(out, cpu, kind),
        missing => out.field(INDENT, "CPU", &missing_text(missing)),
    }
    match &host.memory {
        Read::Done(memory) => self::memory(out, memory.as_ref()),
        missing => out.field(INDENT, "内存", &missing_text(missing)),
    }
    if let Some(Read::Done(hardware)) = &host.hardware {
        modules(out, hardware);
    }
    match &host.storage {
        Read::Done(storage) if kind == PlatformKind::Container => container_storage(out, storage),
        Read::Done(storage) => self::storage(out, storage, kind == PlatformKind::Dedicated),
        missing => out.field(INDENT, "硬盘", &missing_text(missing)),
    }
    match &host.system {
        Read::Done(system) => self::system(out, system, &host.arch),
        missing => {
            out.field(INDENT, "系统", &missing_text(missing));
            out.field(INDENT, "架构", self::arch(&host.arch));
        }
    }
    self::network(out, &host.network, show_ip);
}

/// The words for a section that is missing.
fn missing_text<T>(read: &Read<T>) -> String {
    match read {
        Read::TimedOut => format!("{UNAVAILABLE}（超时）"),
        Read::Done(_) | Read::Failed => UNAVAILABLE.to_owned(),
    }
}

fn cpu(out: &mut Out, facts: &CpuFacts, kind: PlatformKind) {
    let cpu = &facts.cpu;
    out.field(
        INDENT,
        "CPU 型号",
        cpu.model.as_deref().unwrap_or(UNAVAILABLE),
    );
    out.field(INDENT, "CPU 核心", &cores(facts, kind));
    let mhz = cpu.mhz.map_or_else(
        || UNAVAILABLE.to_owned(),
        |mhz| format!("{:.0} MHz", mhz.round()),
    );
    out.field(INDENT, "CPU 频率", &mhz);
    out.field(INDENT, "CPU 缓存", &caches(&cpu.caches));
    let aes = match cpu.aes {
        Some(true) => "支持",
        Some(false) => "不支持",
        None => UNAVAILABLE,
    };
    out.field(INDENT, "AES 指令", aes);
    // A container sees its host's CPU flags, which say nothing about what the container may do.
    if kind != PlatformKind::Container {
        let virtualization = match cpu.virtualization {
            Virtualization::IntelVtX => "支持（VT-x）",
            Virtualization::AmdV => "支持（AMD-V）",
            Virtualization::Absent => "不支持",
            Virtualization::NotApplicable => "不适用",
        };
        out.field(INDENT, "硬件虚拟化", virtualization);
    }
}

fn cores(facts: &CpuFacts, kind: PlatformKind) -> String {
    let cpu = &facts.cpu;
    let Some(logical) = cpu.logical else {
        return UNAVAILABLE.to_owned();
    };
    match kind {
        PlatformKind::Dedicated => match cpu.topology {
            Some(topology) if topology.sockets > 1 => format!(
                "{logical} 线程（{} 颗 CPU，共 {} 核）",
                topology.sockets, topology.cores
            ),
            Some(topology) => format!("{logical} 线程（{} 核）", topology.cores),
            None => format!("{logical} 线程"),
        },
        PlatformKind::Vm => format!("{logical} 个"),
        PlatformKind::Container => {
            let usable = facts.usable.unwrap_or(logical);
            match facts.quota {
                Some(quota) => format!("{usable} 个（限额 {} 个）", number(quota)),
                None => format!("{usable} 个"),
            }
        }
    }
}

/// `1.5`, `2`: at most two decimals, without trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn caches(caches: &[(String, u64)]) -> String {
    if caches.is_empty() {
        return UNAVAILABLE.to_owned();
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
        out.field(INDENT, "内存总量", UNAVAILABLE);
        out.field(INDENT, "内存已用", UNAVAILABLE);
        return;
    };
    out.field(INDENT, "内存总量", &size(memory.total));
    let used = if memory.estimated {
        "内存已用（估算）"
    } else {
        "内存已用"
    };
    out.field(INDENT, used, &size(memory.used));
    if memory.swap_total == 0 {
        out.field(INDENT, "Swap", "无");
    } else {
        out.field(INDENT, "Swap 总量", &size(memory.swap_total));
        out.field(INDENT, "Swap 已用", &size(memory.swap_used));
    }
}

fn storage(out: &mut Out, storage: &StorageFacts, dedicated: bool) {
    if let Some(cards) = &storage.raid {
        raid(out, cards);
    }
    if storage.disks.is_empty() {
        out.field(INDENT, "硬盘", UNAVAILABLE);
    } else {
        let mut headers = vec!["设备", "接口", "容量"];
        // Inside a virtual machine the rotation flag is whatever the hypervisor claims, so the
        // medium is shown on dedicated servers only.
        if dedicated {
            headers.push("介质");
        }
        headers.extend(["型号", "用途"]);
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
                    let medium = match disk.rotational {
                        Some(false) => "固态",
                        Some(true) => "机械",
                        None => "未知",
                    };
                    row.push(medium.to_owned());
                }
                row.push(disk.model.clone().unwrap_or_else(|| "—".to_owned()));
                row.push(uses(&disk.uses));
                row
            })
            .collect();
        table(out, "硬盘", &headers, &rows);
    }
    if storage.filesystems.is_empty() {
        out.field(INDENT, "文件系统", UNAVAILABLE);
        return;
    }
    let rows: Vec<Vec<String>> = storage
        .filesystems
        .iter()
        .map(|filesystem| {
            let mut row = filesystem_row(filesystem, storage.work_dir);
            row.push(if filesystem.disks.is_empty() {
                "—".to_owned()
            } else {
                filesystem.disks.join("、")
            });
            row
        })
        .collect();
    let headers = ["挂载点", "类型", "容量", "已用", "可用", "所在硬盘"];
    table(out, "文件系统", &headers, &rows);
}

/// A container sees its host's disks, so only its own root filesystem is shown.
fn container_storage(out: &mut Out, storage: &StorageFacts) {
    let Some(root) = storage.filesystems.first() else {
        out.field(INDENT, "文件系统", UNAVAILABLE);
        return;
    };
    let row = filesystem_row(root, storage.work_dir);
    let headers = ["挂载点", "类型", "容量", "已用", "可用"];
    table(out, "文件系统", &headers, &[row]);
}

fn table(out: &mut Out, title: &str, headers: &[&str], rows: &[Vec<String>]) {
    out.line(&format!("{INDENT}{title}"));
    let headers: Vec<String> = headers.iter().map(|header| (*header).to_owned()).collect();
    out.table(INDENT, &headers, rows);
}

/// Mount point, type, size, used and available. A ZFS pool shares its free space among its
/// datasets, so only the available space is shown for it.
fn filesystem_row(filesystem: &Filesystem, work_dir: Option<(u32, u32)>) -> Vec<String> {
    let mut mount_point = filesystem.mount_point.clone();
    if work_dir.is_some_and(|device| filesystem.devices.contains(&device)) {
        mount_point.push_str("（测试目录）");
    }
    let unavailable = || UNAVAILABLE.to_owned();
    let (total, used, available) = match filesystem.space {
        Some(space) if filesystem.zfs_pool.is_none() => (
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

fn uses(uses: &[Use]) -> String {
    if uses.is_empty() {
        return "未挂载".to_owned();
    }
    let parts: Vec<String> = uses
        .iter()
        .map(|item| match item {
            Use::Mount(point) if point == "/" => "系统盘（/）".to_owned(),
            Use::Mount(point) => point.clone(),
            Use::Swap => "Swap".to_owned(),
            Use::ZfsPool(pool) => format!("ZFS 存储池 {pool}"),
        })
        .collect();
    parts.join("，")
}

fn system(out: &mut Out, system: &SystemFacts, arch: &str) {
    out.field(
        INDENT,
        "系统",
        system.distribution.as_deref().unwrap_or(UNAVAILABLE),
    );
    out.field(
        INDENT,
        "内核",
        system.kernel.as_deref().unwrap_or(UNAVAILABLE),
    );
    out.field(INDENT, "架构", self::arch(arch));
    let mark = |host: bool| if host { "（宿主机）" } else { "" };
    let uptime_text = system.uptime.map_or_else(
        || UNAVAILABLE.to_owned(),
        |seen| format!("{}{}", uptime(seen.value), mark(seen.host)),
    );
    out.field(INDENT, "运行时间", &uptime_text);
    let load = system.load.map_or_else(
        || UNAVAILABLE.to_owned(),
        |seen| {
            let [one, five, fifteen] = seen.value;
            format!("{one:.2} / {five:.2} / {fifteen:.2}{}", mark(seen.host))
        },
    );
    out.field(INDENT, "负载", &load);
    let tcp = match (&system.congestion_control, &system.qdisc) {
        (Some(algorithm), Some(qdisc)) => format!("{algorithm}（队列 {qdisc}）"),
        (Some(algorithm), None) => algorithm.clone(),
        (None, _) => UNAVAILABLE.to_owned(),
    };
    out.field(INDENT, "TCP 拥塞控制", &tcp);
}

/// The architecture, which the program always knows; a profile built by hand may leave it out.
fn arch(arch: &str) -> &str {
    if arch.is_empty() { UNAVAILABLE } else { arch }
}

fn uptime(uptime: Duration) -> String {
    let minutes = uptime.as_secs() / 60;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    if days > 0 {
        format!("{days} 天 {hours} 小时 {minutes} 分钟")
    } else if hours > 0 {
        format!("{hours} 小时 {minutes} 分钟")
    } else {
        format!("{minutes} 分钟")
    }
}

fn network(out: &mut Out, network: &Network, show_ip: bool) {
    out.field(INDENT, "IPv4", &family_text(&network.ipv4, "IPv4", show_ip));
    out.field(INDENT, "IPv6", &family_text(&network.ipv6, "IPv6", show_ip));
}

/// Whether a family works, and its public address and source; `name` is `IPv4` or `IPv6`.
#[must_use]
pub fn family_text(family: &Family, name: &str, show_ip: bool) -> String {
    if !family.reachable() {
        if family.targets.is_empty() {
            return UNAVAILABLE.to_owned();
        }
        return format!("{UNAVAILABLE}：{}", unreachable_text(&family.targets, name));
    }
    match &family.address {
        Address::Found { address, source } => {
            format!(
                "可用；公网地址 {}（来自 {source}）",
                shown(*address, show_ip)
            )
        }
        Address::NotFound(failed) => {
            let reasons: Vec<String> = failed
                .iter()
                .map(|(source, why)| why_text(source, *why, name))
                .collect();
            format!("可用；公网地址不可用：{}", reasons.join("，"))
        }
        Address::NotAsked => "可用".to_owned(),
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
    if targets.iter().all(|(_, reach)| *reach == Reach::NoRoute) {
        return format!("本机没有 {name} 路由");
    }
    let parts: Vec<String> = targets
        .iter()
        .map(|(target, reach)| {
            let ip = target.ip();
            match reach {
                Reach::Connected => format!("已连接 {ip}"),
                Reach::TimedOut => format!("连接 {ip} 超时"),
                Reach::Refused => format!("{ip} 拒绝连接"),
                Reach::NoRoute => format!("没有到 {ip} 的路由"),
                Reach::Failed => format!("无法连接 {ip}"),
            }
        })
        .collect();
    parts.join("，")
}

/// Why a source gave no address of the family `name`.
#[must_use]
pub fn why_text(source: &str, why: Why, name: &str) -> String {
    match why {
        Why::TimedOut => format!("{source} 超时"),
        Why::Status(code) => format!("{source} 返回 HTTP {code}"),
        Why::NotAnAddress => format!("{source} 返回的不是 {name} 地址"),
        Why::NotPublic => format!("{source} 返回的不是公网地址"),
        Why::NoAddress => format!("{source} 没有 {name} 地址"),
        Why::Unreachable => format!("{source} 无法连接"),
    }
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

/// 整机, 主板, BIOS or 固件, and BMC.
fn machine(out: &mut Out, hardware: &Hardware) {
    let machine = &hardware.machine;
    let shown =
        |line: &Line| text(line, line.detail.clone()).unwrap_or_else(|| UNAVAILABLE.to_owned());
    out.field(INDENT, "整机", &shown(&machine.system));
    out.field(INDENT, "主板", &shown(&machine.board));
    let (label, firmware) = match &machine.firmware {
        Some(Firmware::Bios(line)) => ("BIOS", text(line, line.detail.as_deref().map(date))),
        Some(Firmware::UBoot(version)) => ("固件", Some(format!("U-Boot {version}"))),
        Some(Firmware::RaspberryPi(built)) => (
            "固件",
            Some(format!("树莓派引导程序，{} 构建", civil_date(*built))),
        ),
        None => ("固件", None),
    };
    out.field(INDENT, label, firmware.as_deref().unwrap_or(UNAVAILABLE));
    out.field(INDENT, "BMC", &bmc_text(hardware.bmc.as_ref()));
}

/// `vendor name（detail）`, without the vendor when the name begins with it, ignoring case, and
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
        (false, Some(detail)) => Some(format!("{main}（{detail}）")),
        (false, None) => Some(main),
    }
}

/// A date of the form month/day/year as year-month-day; other forms as they are.
fn date(text: &str) -> String {
    let digits = |part: &str, lengths: std::ops::RangeInclusive<usize>| {
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

fn bmc_text(bmc: Option<&Bmc>) -> String {
    const NO_DRIVER: &str = "内核没有加载 IPMI 驱动";
    match bmc {
        Some(Bmc::Driver {
            vendor,
            firmware,
            ipmi,
        }) => {
            let vendor = vendor.map(|number| {
                bmc::vendor(number).map_or_else(|| format!("厂商编号 {number}"), str::to_owned)
            });
            let parts: Vec<String> = [
                vendor,
                firmware.as_ref().map(|firmware| format!("固件 {firmware}")),
                ipmi.as_ref().map(|version| format!("IPMI {version}")),
            ]
            .into_iter()
            .flatten()
            .collect();
            if parts.is_empty() {
                "有".to_owned()
            } else {
                parts.join("，")
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
                    format!("{name} 接口")
                }),
            ]
            .into_iter()
            .flatten()
            .collect();
            if parts.is_empty() {
                format!("有；接口、厂商和固件不可用：{NO_DRIVER}")
            } else {
                format!("有（{}）；厂商和固件不可用：{NO_DRIVER}", parts.join("，"))
            }
        }
        Some(Bmc::Present) => format!("有；接口、厂商和固件不可用：{NO_DRIVER}"),
        None => "未检测到".to_owned(),
    }
}

/// ECC, then the memory modules: a table of the slots and a 说明 line for the ones not in use,
/// else the device tree's memory, else why there is no table.
fn modules(out: &mut Out, hardware: &Hardware) {
    out.field(INDENT, "ECC", &ecc_text(hardware));
    let slots = hardware
        .smbios
        .as_ref()
        .map_or(&[][..], |smbios| smbios.slots.as_slice());
    if slots.is_empty() {
        let value = match (hardware.board_memory, &hardware.smbios) {
            (Some(bytes), _) => format!(
                "板载内存 {}（来自设备树）；没有可插拔内存条的信息",
                size_text(bytes)
            ),
            (None, Ok(_)) => "不可用：SMBIOS 里没有内存条信息".to_owned(),
            (None, Err(Unreadable::Absent)) => "不可用：读不到 SMBIOS".to_owned(),
            (None, Err(Unreadable::DevMem(error))) => {
                format!("不可用：读不到 SMBIOS（/dev/mem：{error}）")
            }
        };
        out.field(INDENT, "内存条", &value);
        return;
    }
    let names: Vec<String> = slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.name
                .clone()
                .unwrap_or_else(|| format!("插槽 {}", index + 1))
        })
        .collect();
    let rows: Vec<Vec<String>> = slots
        .iter()
        .zip(&names)
        .map(|(slot, name)| slot_row(name, slot))
        .collect();
    let headers = ["插槽", "容量", "类型", "速度", "厂商", "型号"];
    table(out, "内存条", &headers, &rows);
    let disabled: Vec<&str> = slots
        .iter()
        .zip(&names)
        .filter(|(slot, _)| slot.capacity == Capacity::Disabled)
        .map(|(_, name)| name.as_str())
        .collect();
    if !disabled.is_empty() {
        out.field(
            INDENT,
            "说明",
            &format!(
                "{} 插着内存条但没有启用：固件报告容量为 0，没有计入内存总量",
                disabled.join("、")
            ),
        );
    }
}

fn ecc_text(hardware: &Hardware) -> String {
    let modes = match &hardware.smbios {
        Ok(smbios) if !smbios.ecc.is_empty() => smbios.ecc.clone(),
        _ => hardware
            .edac
            .as_ref()
            .map(|edac| edac.modes.clone())
            .unwrap_or_default(),
    };
    if modes == [Ecc::None] {
        return "无".to_owned();
    }
    let counts = hardware.edac.as_ref().map(|edac| {
        format!(
            "本次开机以来已纠正 {} 次，不可纠正 {} 次",
            edac.corrected, edac.uncorrected
        )
    });
    let modes: Vec<&str> = modes
        .iter()
        .map(|mode| match mode {
            Ecc::SingleBit => "有（单比特纠错）",
            Ecc::MultiBit => "有（多比特纠错）",
            Ecc::Crc => "有（CRC 校验）",
            Ecc::Parity => "无纠错（只有奇偶校验）",
            Ecc::None => "无",
            Ecc::Unknown => "未知",
        })
        .collect();
    match (modes.is_empty(), counts) {
        (false, Some(counts)) => format!("{}；{counts}", modes.join("、")),
        (false, None) => format!(
            "{}；错误次数不可用：内核没有加载 EDAC 驱动",
            modes.join("、")
        ),
        (true, Some(counts)) => format!("纠错方式不可用；{counts}"),
        (true, None) => {
            let smbios = match &hardware.smbios {
                Ok(_) => "SMBIOS 里没有内存纠错信息",
                Err(Unreadable::Absent) => "没有 SMBIOS",
                Err(Unreadable::DevMem(_)) => "读不到 SMBIOS",
            };
            format!("不可用：{smbios}，也没有 EDAC 驱动")
        }
    }
}

fn slot_row(name: &str, slot: &Slot) -> Vec<String> {
    let dash = || "—".to_owned();
    let capacity = match slot.capacity {
        Capacity::Empty => {
            return vec![
                name.to_owned(),
                "空".to_owned(),
                dash(),
                dash(),
                dash(),
                dash(),
            ];
        }
        Capacity::Disabled => "未启用".to_owned(),
        Capacity::Unknown => "未知".to_owned(),
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
    let speed = match (slot.configured_speed, slot.speed) {
        (Some(configured), Some(rated)) if configured != rated => {
            format!("{configured} MT/s（额定 {rated}）")
        }
        (Some(configured), _) => format!("{configured} MT/s"),
        (None, Some(rated)) => format!("{rated} MT/s（额定）"),
        (None, None) => dash(),
    };
    vec![
        name.to_owned(),
        capacity,
        kind,
        speed,
        slot.manufacturer.clone().unwrap_or_else(dash),
        slot.part_number.clone().unwrap_or_else(dash),
    ]
}

/// One line per RAID card, with the disks of the 硬盘 table it holds; `无` without cards.
fn raid(out: &mut Out, cards: &[Card]) {
    if cards.is_empty() {
        out.field(INDENT, "RAID 卡", "无");
    }
    for card in cards {
        out.field(INDENT, "RAID 卡", &card_text(card));
    }
}

fn card_text(card: &Card) -> String {
    let name = card.name.clone().unwrap_or_else(|| {
        let driver = card.driver.as_ref().map_or_else(
            || "未加载驱动".to_owned(),
            |driver| format!("驱动 {driver}"),
        );
        let ids = format!("PCI {:04x}:{:04x}", card.ids.vendor, card.ids.device);
        match card.ids.subsystem {
            Some((vendor, device)) => {
                format!("{ids}（子系统 {vendor:04x}:{device:04x}，{driver}）")
            }
            None => format!("{ids}（{driver}）"),
        }
    });
    if card.disks.is_empty() {
        return format!("{name}：没有连接硬盘");
    }
    let groups: Vec<String> = [
        (Attachment::Array, "阵列"),
        (Attachment::PassThrough, "直通"),
        (Attachment::Unknown, "未知"),
    ]
    .iter()
    .filter_map(|(attachment, label)| {
        let disks: Vec<&str> = card
            .disks
            .iter()
            .filter(|(_, how)| how == attachment)
            .map(|(disk, _)| disk.as_str())
            .collect();
        (!disks.is_empty()).then(|| format!("{label} {}", disks.join("、")))
    })
    .collect();
    format!("{name}：{}", groups.join("；"))
}
