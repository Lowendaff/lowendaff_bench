//! What smartctl prints with `--json`: the scan's devices and its version, and one disk's verdict
//! and values.

use serde_json::Value;

use super::device::Device;
use super::result::BadBlocks;
use super::result::Fault;
use super::result::Health;
use super::result::Values;

/// What smartctl read from one device. The serial is only compared, to find one disk listed
/// twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The device as the scan listed it.
    pub device: Device,
    /// The disk's serial number, for comparing.
    pub serial: Option<String>,
    /// The verdict.
    pub health: Health,
    /// The values.
    pub values: Values,
}

impl Reading {
    /// Classifies one device's output. The first rule that applies decides: a failed read, then
    /// no SMART, then the verdict.
    #[must_use]
    pub fn new(device: Device, text: &str) -> Self {
        let Some(json) = Json::parse(text) else {
            return Self::failed(device, Fault::Unparsable);
        };
        if let Some(fault) = json.failure() {
            return Self::failed(device, fault);
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
            values: values(&json),
        }
    }

    /// A device that could not be read.
    #[must_use]
    pub fn failed(device: Device, fault: Fault) -> Self {
        Self {
            device,
            serial: None,
            health: Health::ReadFailed(fault),
            values: Values::default(),
        }
    }
}

/// The devices in the scan's output, in smartctl's order, and smartctl's version; or why the
/// scan failed. A scan that finds nothing prints no `devices` at all.
pub fn scan(text: &str) -> Result<(Option<String>, Vec<Device>), Fault> {
    let json = Json::parse(text).ok_or(Fault::Unparsable)?;
    if let Some(fault) = json.failure() {
        return Err(fault);
    }
    let devices = json.array_at("/devices").iter().filter_map(|device| {
        Some(Device {
            name: device.get("name")?.as_str()?.to_owned(),
            kind: device.get("type")?.as_str()?.to_owned(),
        })
    });
    Ok((json.version(), devices.collect()))
}

/// What one `smartctl --json` run printed.
struct Json(Value);

impl Json {
    /// The output's first JSON value, before anything that follows it, such as smartctl's
    /// standard error after its standard output; `None` when it is not JSON with an exit status,
    /// which smartctl always includes.
    fn parse(text: &str) -> Option<Self> {
        let mut values = serde_json::Deserializer::from_str(text).into_iter::<Value>();
        let value = values.next()?.ok()?;
        value.pointer("/smartctl/exit_status")?.as_u64()?;
        Some(Self(value))
    }

    /// Why the run failed, when bit 0 (command line) or bit 1 (open or identify) of the exit
    /// status is set: smartctl's first error message, else what the bit means. Other bits report
    /// the disk's state and do not make a run fail.
    fn failure(&self) -> Option<Fault> {
        let status = self.u64_at("/smartctl/exit_status")?;
        if status & 0b11 == 0 {
            return None;
        }
        // Errors that smartctl prints as plain output, such as a failed IDENTIFY, are not in
        // `messages`.
        let message = self
            .array_at("/smartctl/messages")
            .iter()
            .find_map(|message| {
                if message.get("severity").and_then(Value::as_str) != Some("error") {
                    return None;
                }
                message.get("string").and_then(Value::as_str)
            });
        Some(match message {
            Some(message) => Fault::Message(message.to_owned()),
            None if status & 0b10 != 0 => Fault::Unopened,
            None => Fault::Refused,
        })
    }

    /// smartctl's version, such as `7.5` from `"version": [7, 5]`.
    fn version(&self) -> Option<String> {
        let parts: Option<Vec<String>> = self
            .array_at("/smartctl/version")
            .iter()
            .map(|part| part.as_u64().map(|number| number.to_string()))
            .collect();
        parts
            .filter(|parts| !parts.is_empty())
            .map(|parts| parts.join("."))
    }

    fn u64_at(&self, pointer: &str) -> Option<u64> {
        self.0.pointer(pointer)?.as_u64()
    }

    fn i64_at(&self, pointer: &str) -> Option<i64> {
        self.0.pointer(pointer)?.as_i64()
    }

    fn bool_at(&self, pointer: &str) -> Option<bool> {
        self.0.pointer(pointer)?.as_bool()
    }

    fn str_at(&self, pointer: &str) -> Option<&str> {
        self.0.pointer(pointer)?.as_str()
    }

    /// The array at a JSON pointer; empty when there is none.
    fn array_at(&self, pointer: &str) -> &[Value] {
        self.0
            .pointer(pointer)
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }
}

/// The values, from the fields of the disk's protocol.
fn values(json: &Json) -> Values {
    let common = Values {
        hours: json.u64_at("/power_on_time/hours"),
        temperature: json.i64_at("/temperature/current"),
        endurance_used: json.u64_at("/endurance_used/current_percent"),
        ..Values::default()
    };
    match json.str_at("/device/protocol") {
        Some("ATA" | "ATA+SCSI") => {
            let sectors = statistic(json, 1, 0x018);
            let sector_size = json.u64_at("/logical_block_size");
            Values {
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
            Values {
                written: units.and_then(|units| units.checked_mul(512_000)),
                bad_blocks: BadBlocks::Count(json.u64_at(&format!("{log}/media_errors"))),
                ..common
            }
        }
        Some("SCSI") => {
            let processed = json.str_at("/scsi_error_counter_log/write/gigabytes_processed");
            Values {
                written: processed.and_then(gigabytes),
                bad_blocks: BadBlocks::Count(json.u64_at("/scsi_grown_defect_list")),
                ..common
            }
        }
        _ => common,
    }
}

/// A valid entry of the ATA Device Statistics log, by page and byte offset.
fn statistic(json: &Json, page: u64, offset: u64) -> Option<u64> {
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
fn attribute(json: &Json, id: u64) -> Option<u64> {
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
