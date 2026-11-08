//! smartctl's JSON output (design disk-health, §3.3).

use serde_json::Value;

/// What one `smartctl --json` run printed.
#[derive(Debug)]
pub struct Output(Value);

impl Output {
    /// The parsed output; `None` when it is not JSON with an exit status, which smartctl
    /// always includes.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        value.pointer("/smartctl/exit_status")?.as_u64()?;
        Some(Self(value))
    }

    /// Why the run failed, when bit 0 (command line) or bit 1 (open or identify) of the exit
    /// status is set: smartctl's first error message, else what the bit means. Other bits
    /// report the disk's state and do not make a run fail.
    #[must_use]
    pub fn failure(&self) -> Option<String> {
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
        let reason = match message {
            Some(message) => message,
            None if status & 0b10 != 0 => "打不开设备，或读不到设备识别信息",
            None => "smartctl 不接受命令参数",
        };
        Some(reason.to_owned())
    }

    /// The unsigned number at a JSON pointer such as `/power_on_time/hours`.
    #[must_use]
    pub fn u64_at(&self, pointer: &str) -> Option<u64> {
        self.0.pointer(pointer)?.as_u64()
    }

    /// The signed number at a JSON pointer.
    #[must_use]
    pub fn i64_at(&self, pointer: &str) -> Option<i64> {
        self.0.pointer(pointer)?.as_i64()
    }

    /// The boolean at a JSON pointer.
    #[must_use]
    pub fn bool_at(&self, pointer: &str) -> Option<bool> {
        self.0.pointer(pointer)?.as_bool()
    }

    /// The string at a JSON pointer.
    #[must_use]
    pub fn str_at(&self, pointer: &str) -> Option<&str> {
        self.0.pointer(pointer)?.as_str()
    }

    /// The array at a JSON pointer; empty when there is none.
    #[must_use]
    pub fn array_at(&self, pointer: &str) -> &[Value] {
        self.0
            .pointer(pointer)
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }
}
