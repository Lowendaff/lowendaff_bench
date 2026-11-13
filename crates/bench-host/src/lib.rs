//! Host facts for lowendaff-bench, shared by the orchestrator and the test items.
//!
//! Everything is read from kernel interfaces (`/proc`, `/sys`, and on x86 `/dev/mem` as a last
//! resort for the SMBIOS table) and standard files under `/etc` and `/usr/share`; no program is
//! run. Every reader goes through a [`Root`], so tests point it at a captured
//! fixture instead of `/`.

#![warn(missing_docs)]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

pub mod bmc;
pub mod cache;
pub mod cpu;
pub mod edac;
pub mod glibc;
pub mod machine;
pub mod memory;
pub mod numa;
pub mod os;
pub mod platform;
pub mod raid;
pub mod smbios;
pub mod storage;
pub mod system;

/// The filesystem host facts are read from: `/` on a real host, a captured copy in tests.
#[derive(Debug, Clone)]
pub struct Root {
    path: PathBuf,
}

impl Root {
    /// The root filesystem of this host.
    #[must_use]
    pub fn system() -> Self {
        Self::new("/")
    }

    /// A directory laid out like `/`, such as a captured fixture.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Where the absolute host path `path` (for example `/proc/cpuinfo`) lives.
    #[must_use]
    pub fn join(&self, path: &str) -> PathBuf {
        self.path.join(path.trim_start_matches('/'))
    }

    /// The file's contents, or `None` when it is missing or unreadable. Invalid UTF-8 is
    /// replaced, because firmware strings such as DMI fields are not always valid.
    #[must_use]
    pub fn read(&self, path: &str) -> Option<String> {
        let bytes = fs::read(self.join(path)).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// The first line of the file without surrounding whitespace; `None` when the file is
    /// missing, unreadable or blank.
    #[must_use]
    pub fn read_line(&self, path: &str) -> Option<String> {
        let text = self.read(path)?;
        let line = text.lines().next()?.trim();
        (!line.is_empty()).then(|| line.to_owned())
    }

    /// Whether the path exists.
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.join(path).exists()
    }

    /// The names in a directory, sorted; empty when it is missing.
    #[must_use]
    pub fn list(&self, path: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.join(path)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The target of a symbolic link, such as `pid:[4026531836]` for `/proc/self/ns/pid`.
    #[must_use]
    pub fn link(&self, path: &str) -> Option<String> {
        let target = fs::read_link(self.join(path)).ok()?;
        Some(target.to_string_lossy().into_owned())
    }

    /// The file's bytes, or `None` when it is missing or unreadable.
    #[must_use]
    pub fn read_bytes(&self, path: &str) -> Option<Vec<u8>> {
        fs::read(self.join(path)).ok()
    }
}

/// The strings of a device-tree property, which are NUL-terminated, without empty ones.
fn device_tree_strings(root: &Root, path: &str) -> Vec<String> {
    let Some(text) = root.read(path) else {
        return Vec::new();
    };
    text.split('\0')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A firmware string without surrounding whitespace, or `None` when the firmware left a
/// placeholder instead.
fn firmware_string(text: &str) -> Option<String> {
    const PLACEHOLDERS: [&str; 14] = [
        "to be filled by o.e.m.",
        "default string",
        "system product name",
        "system manufacturer",
        "system version",
        "base board product name",
        "not specified",
        "not applicable",
        "none",
        "n/a",
        "unknown",
        "0123456789",
        "undefined",
        "no dimm",
    ];
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    // AMI's numbered defaults, such as `Manufacturer0` and `PartNum0`.
    let numbered = ["manufacturer", "partnum"].iter().any(|prefix| {
        lower
            .strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
    });
    let zeros = text.bytes().all(|byte| byte == b'0');
    (!zeros && !numbered && !PLACEHOLDERS.contains(&lower.as_str())).then(|| text.to_owned())
}

/// The numbers in a kernel CPU or node list such as `0-15,32-47`, ascending; `None` when the
/// list is empty or malformed.
fn cpu_list(list: &str) -> Option<Vec<usize>> {
    // No kernel has this many CPUs; a larger number means a damaged list.
    const LIMIT: usize = 1 << 16;
    let mut numbers = BTreeSet::new();
    for range in list.trim().split(',') {
        let (low, high) = match range.split_once('-') {
            Some((low, high)) => (low.parse::<usize>().ok()?, high.parse::<usize>().ok()?),
            None => {
                let number = range.parse::<usize>().ok()?;
                (number, number)
            }
        };
        if low > high || high >= LIMIT {
            return None;
        }
        numbers.extend(low..=high);
    }
    Some(numbers.into_iter().collect())
}

/// Number of CPUs in a kernel CPU list such as `0-15,32-47`.
fn cpu_list_len(list: &str) -> Option<usize> {
    cpu_list(list).as_ref().map(Vec::len)
}

/// A list in this process's `/proc/self/status`, such as `Cpus_allowed_list: 0-63`.
fn status_list(root: &Root, key: &str) -> Option<Vec<usize>> {
    let status = root.read("/proc/self/status")?;
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))?;
    cpu_list(list)
}
