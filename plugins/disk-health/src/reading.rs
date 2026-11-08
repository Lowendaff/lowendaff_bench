//! One disk's health and values from `smartctl --json -x` (design disk-health, §5.4, §5.6).

use std::fmt;
use std::time::Duration;

use serde_json::Value;

use crate::device::Device;
use crate::output::Output;

/// How long one smartctl run may take before it is stopped.
pub const READ_LIMIT: Duration = Duration::from_secs(60);

/// What smartctl reported for one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The device as the scan listed it.
    pub device: Device,
    /// Only compared, to find one disk listed twice; never shown.
    pub serial: Option<String>,
    /// The 健康 column.
    pub health: Health,
    /// The other columns.
    pub values: Values,
}

/// smartctl's verdict, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// SMART status passed.
    Passed,
    /// SMART status failed.
    Failed,
    /// No verdict; `smart_off` when the disk's SMART is switched off.
    Unknown {
        /// The disk supports SMART but has it switched off.
        smart_off: bool,
    },
    /// The disk has no SMART.
    Unsupported,
    /// smartctl could not read the disk, for the reason shown in the 说明 line.
    ReadFailed(String),
}

impl fmt::Display for Health {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Passed => "通过",
            Self::Failed => "未通过",
            Self::Unknown { .. } => "未知",
            Self::Unsupported => "不支持",
            Self::ReadFailed(_) => "读取失败",
        })
    }
}

impl Health {
    /// The 说明 line's text after the device's name, for the rows that need one.
    #[must_use]
    pub fn note(&self) -> Option<String> {
        match self {
            Self::ReadFailed(reason) => Some(format!("读取失败：{reason}")),
            Self::Unsupported => Some("不支持 SMART".to_owned()),
            Self::Unknown { smart_off: true } => {
                Some("的 SMART 功能已关闭，本程序不会替它打开".to_owned())
            }
            Self::Passed | Self::Failed | Self::Unknown { smart_off: false } => None,
        }
    }
}

/// The columns after 健康; `None` shows as `—`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    /// 通电时间, in hours.
    pub hours: Option<u64>,
    /// 温度, in °C.
    pub temperature: Option<i64>,
    /// 已用寿命, in percent; NVMe reports over 100 past the rated endurance.
    pub endurance_used: Option<u64>,
    /// 累计写入, in bytes.
    pub written: Option<u64>,
    /// 坏块.
    pub bad_blocks: BadBlocks,
    /// 接口错误: SATA's interface CRC errors.
    pub interface_errors: Option<u64>,
}

/// The 坏块 column, which counts different things per protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadBlocks {
    /// SATA: reallocated and pending sectors.
    Sectors {
        /// Sectors already replaced by spares.
        reallocated: Option<u64>,
        /// Sectors waiting to be replaced.
        pending: Option<u64>,
    },
    /// NVMe media errors, or the entries of a SAS disk's grown defect list.
    Count(Option<u64>),
}

impl Default for BadBlocks {
    fn default() -> Self {
        Self::Count(None)
    }
}

impl fmt::Display for BadBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sectors {
                reallocated: None,
                pending: None,
            }
            | Self::Count(None) => f.write_str("—"),
            Self::Sectors {
                reallocated,
                pending,
            } => write!(f, "{} / {}", Shown(*reallocated), Shown(*pending)),
            Self::Count(Some(count)) => write!(f, "{count}"),
        }
    }
}

/// A count, or `—` when it is missing.
struct Shown(Option<u64>);

impl fmt::Display for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(count) => write!(f, "{count}"),
            None => f.write_str("—"),
        }
    }
}

impl Reading {
    /// Classifies one device's output; `None` means the read was stopped at [`READ_LIMIT`].
    /// The first rule that applies decides: 读取失败, then 不支持, then the verdict.
    #[must_use]
    pub fn new(device: Device, output: Option<&str>) -> Self {
        let Some(text) = output else {
            let limit = READ_LIMIT.as_secs();
            return Self::failed(device, format!("超过 {limit} 秒仍未读完，已停止"));
        };
        let Some(json) = Output::parse(text) else {
            return Self::failed(device, "smartctl 的输出无法解析".to_owned());
        };
        if let Some(reason) = json.failure() {
            return Self::failed(device, reason);
        }
        let serial = json.str_at("/serial_number").map(str::to_owned);
        if json.bool_at("/smart_support/available") == Some(false) {
            return Self {
                device,
                serial,
                health: Health::Unsupported,
                values: Values::default(),
            };
        }
        let health = match json.bool_at("/smart_status/passed") {
            Some(true) => Health::Passed,
            Some(false) => Health::Failed,
            None => Health::Unknown {
                smart_off: json.bool_at("/smart_support/enabled") == Some(false),
            },
        };
        Self {
            device,
            serial,
            health,
            values: Values::read(&json),
        }
    }

    fn failed(device: Device, reason: String) -> Self {
        Self {
            device,
            serial: None,
            health: Health::ReadFailed(reason),
            values: Values::default(),
        }
    }
}

impl Values {
    /// The values, from the fields of the disk's protocol (design §5.4).
    fn read(json: &Output) -> Self {
        let common = Self {
            hours: json.u64_at("/power_on_time/hours"),
            temperature: json.i64_at("/temperature/current"),
            endurance_used: json.u64_at("/endurance_used/current_percent"),
            ..Self::default()
        };
        match json.str_at("/device/protocol") {
            Some("ATA" | "ATA+SCSI") => {
                let sectors = statistic(json, 1, 0x018);
                let sector_size = json.u64_at("/logical_block_size");
                Self {
                    written: sectors
                        .zip(sector_size)
                        .and_then(|(n, size)| n.checked_mul(size)),
                    bad_blocks: BadBlocks::Sectors {
                        reallocated: statistic(json, 3, 0x020).or_else(|| attribute(json, 5)),
                        pending: statistic(json, 3, 0x038).or_else(|| attribute(json, 197)),
                    },
                    interface_errors: statistic(json, 6, 0x018).or_else(|| attribute(json, 199)),
                    ..common
                }
            }
            Some("NVMe") => {
                let log = "/nvme_smart_health_information_log";
                // Units of 1000 sectors of 512 bytes (NVMe base specification).
                let units = json.u64_at(&format!("{log}/data_units_written"));
                Self {
                    written: units.and_then(|units| units.checked_mul(512_000)),
                    bad_blocks: BadBlocks::Count(json.u64_at(&format!("{log}/media_errors"))),
                    ..common
                }
            }
            Some("SCSI") => {
                let processed = json.str_at("/scsi_error_counter_log/write/gigabytes_processed");
                Self {
                    written: processed.and_then(gigabytes),
                    bad_blocks: BadBlocks::Count(json.u64_at("/scsi_grown_defect_list")),
                    ..common
                }
            }
            _ => common,
        }
    }
}

/// A valid entry of the ATA Device Statistics log, by page and byte offset.
fn statistic(json: &Output, page: u64, offset: u64) -> Option<u64> {
    let number = |entry: &Value, key: &str| entry.get(key).and_then(Value::as_u64);
    let pages = json.array_at("/ata_device_statistics/pages");
    let page = pages
        .iter()
        .find(|entry| number(entry, "number") == Some(page))?;
    let table = page.get("table")?.as_array()?;
    let entry = table
        .iter()
        .find(|entry| number(entry, "offset") == Some(offset))?;
    // smartctl gives no value for an entry the disk marks invalid.
    number(entry, "value")
}

/// An ATA attribute's count: the number that smartctl shows as its raw value.
fn attribute(json: &Output, id: u64) -> Option<u64> {
    let table = json.array_at("/ata_smart_attributes/table");
    let entry = table
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_u64) == Some(id))?;
    leading_number(entry.pointer("/raw/string")?.as_str()?)
}

/// The number that starts an attribute's raw string, such as 0 in `0 (2000)`. `raw.value`
/// cannot be used: for formats such as `raw16(raw16)` it also holds the parenthesized parts.
/// A hexadecimal string such as `0x00000000005a` has no count.
#[must_use]
pub fn leading_number(raw: &str) -> Option<u64> {
    let end = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    let (digits, rest) = raw.split_at(end);
    if digits.is_empty() || rest.starts_with('x') {
        return None;
    }
    digits.parse().ok()
}

/// Bytes from SAS's "gigabytes processed", a decimal string such as `12345.678` (10^9 bytes).
#[must_use]
pub fn gigabytes(text: &str) -> Option<u64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 9 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let fraction: u64 = format!("{fraction:0<9}").parse().ok()?;
    whole.checked_mul(1_000_000_000)?.checked_add(fraction)
}
