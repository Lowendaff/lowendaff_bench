//! The disks that smartctl's scan finds.

use crate::health::output::Output;

/// smartctl's scan, which opens every device to detect its type.
pub const SCAN_ARGS: [&str; 2] = ["--scan-open", "--json"];

/// One device from the scan, read back with `-d <kind> <name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Device path, such as `/dev/sda` or `/dev/bus/0`.
    pub name: String,
    /// smartctl's device type, such as `sat` or `sat+megaraid,18`.
    pub kind: String,
}

impl Device {
    /// The arguments that read all of this device's SMART data and logs, as JSON. Every
    /// command they send is read-only.
    #[must_use]
    pub fn read_args(&self) -> Vec<String> {
        let args = ["--json", "-x", "-d", self.kind.as_str(), self.name.as_str()];
        args.map(str::to_owned).to_vec()
    }
}

/// The devices in the scan's output, in smartctl's order, or why the scan failed. A scan
/// that finds nothing prints no `devices` at all.
pub fn scan(text: &str) -> Result<Vec<Device>, String> {
    let output = Output::parse(text).ok_or("smartctl 的输出无法解析")?;
    if let Some(reason) = output.failure() {
        return Err(reason);
    }
    let devices = output.array_at("/devices").iter().filter_map(|device| {
        Some(Device {
            name: device.get("name")?.as_str()?.to_owned(),
            kind: device.get("type")?.as_str()?.to_owned(),
        })
    });
    Ok(devices.collect())
}
