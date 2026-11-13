//! The hardware lines of dedicated servers, placed by the report next to what they belong
//! to: machine, board, firmware and BMC before the CPU; ECC and the memory modules
//! after Swap; RAID cards before the disks.

use bench_api::Record;
use bench_api::RecordError;
use bench_api::Table;
use bench_host::Root;
use bench_host::bmc;
use bench_host::bmc::Bmc;
use bench_host::bmc::InterfaceKind;
use bench_host::edac;
use bench_host::edac::Edac;
use bench_host::machine;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::machine::Machine;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::smbios;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::smbios::Smbios;
use bench_host::smbios::Unreadable;

use crate::report;

/// What a dedicated server's hardware lines are made of.
pub struct Hardware {
    /// The SMBIOS table, or why there is none.
    pub smbios: Result<Smbios, Unreadable>,
    /// The machine, its board and its firmware.
    pub machine: Machine,
    /// The BMC, or `None` when none was found.
    pub bmc: Option<Bmc>,
    /// EDAC's counts and modes, or `None` without an EDAC driver.
    pub edac: Option<Edac>,
    /// The memory the device tree describes, in bytes.
    pub board_memory: Option<u64>,
}

impl Hardware {
    /// Reads the host; `arch` is `std::env::consts::ARCH`.
    #[must_use]
    pub fn read(root: &Root, arch: &str) -> Self {
        let smbios = smbios::read(root, arch);
        let table = smbios.as_ref().ok();
        Self {
            machine: machine::read(root, table),
            bmc: bmc::read(root, table),
            edac: edac::read(root),
            board_memory: machine::board_memory(root),
            smbios,
        }
    }
}

/// 整机, 主板, BIOS or 固件, and BMC.
pub fn machine_records(records: &mut Vec<Record>, hardware: &Hardware) {
    let machine = &hardware.machine;
    let shown = |line: &Line| text(line, line.detail.clone()).unwrap_or_else(report::unavailable);
    records.push(Record::fact("machine", "整机", shown(&machine.system)));
    records.push(Record::fact("board", "主板", shown(&machine.board)));
    let (label, firmware) = match &machine.firmware {
        Some(Firmware::Bios(line)) => ("BIOS", text(line, line.detail.as_deref().map(date))),
        Some(Firmware::UBoot(version)) => ("固件", Some(format!("U-Boot {version}"))),
        Some(Firmware::RaspberryPi(built)) => (
            "固件",
            Some(format!("树莓派引导程序，{} 构建", civil_date(*built))),
        ),
        None => ("固件", None),
    };
    records.push(Record::fact(
        "firmware",
        label,
        firmware.unwrap_or_else(report::unavailable),
    ));
    records.push(Record::fact("bmc", "BMC", bmc_text(hardware.bmc.as_ref())));
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
pub fn memory_records(records: &mut Vec<Record>, hardware: &Hardware) -> Result<(), RecordError> {
    records.push(Record::fact("memory.ecc", "ECC", ecc_text(hardware)));
    let slots = hardware
        .smbios
        .as_ref()
        .map_or(&[][..], |smbios| smbios.slots.as_slice());
    if slots.is_empty() {
        let value = match (hardware.board_memory, &hardware.smbios) {
            (Some(bytes), _) => format!(
                "板载内存 {}（来自设备树）；没有可插拔内存条的信息",
                report::size_text(bytes)
            ),
            (None, Ok(_)) => "不可用：SMBIOS 里没有内存条信息".to_owned(),
            (None, Err(Unreadable::Absent)) => "不可用：读不到 SMBIOS".to_owned(),
            (None, Err(Unreadable::DevMem(error))) => {
                format!("不可用：读不到 SMBIOS（/dev/mem：{error}）")
            }
        };
        records.push(Record::fact("memory.modules", "内存条", value));
        return Ok(());
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
    let rows = slots
        .iter()
        .zip(&names)
        .map(|(slot, name)| slot_row(name, slot))
        .collect();
    records.push(Record::Table(Table::try_new(
        "memory-modules",
        "内存条",
        ["插槽", "容量", "类型", "速度", "厂商", "型号"]
            .map(str::to_owned)
            .to_vec(),
        rows,
    )?));
    let disabled: Vec<&str> = slots
        .iter()
        .zip(&names)
        .filter(|(slot, _)| slot.capacity == Capacity::Disabled)
        .map(|(_, name)| name.as_str())
        .collect();
    if !disabled.is_empty() {
        records.push(Record::fact(
            "memory.modules.note",
            "说明",
            format!(
                "{} 插着内存条但没有启用：固件报告容量为 0，没有计入内存总量",
                disabled.join("、")
            ),
        ));
    }
    Ok(())
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
pub fn raid_records(records: &mut Vec<Record>, cards: &[Card]) {
    if cards.is_empty() {
        records.push(Record::fact("raid", "RAID 卡", "无"));
    }
    let numbered: Vec<(usize, &Card)> = cards.iter().enumerate().collect();
    for (index, card) in numbered {
        records.push(Record::fact(
            &format!("raid.{}", index + 1),
            "RAID 卡",
            card_text(card),
        ));
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
