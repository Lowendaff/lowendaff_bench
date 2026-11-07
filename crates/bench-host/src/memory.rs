//! Memory and swap from `/proc/meminfo`, and the limits a container's cgroup imposes.

use std::collections::BTreeMap;

use crate::Root;

/// Memory as the kernel reports it, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    /// Total usable memory.
    pub total: u64,
    /// Memory available to new work without swapping.
    pub available: u64,
    /// Whether `available` is an estimate: kernels before 3.14 lack `MemAvailable`, so
    /// it is `MemFree + Buffers + Cached` there.
    pub estimated: bool,
    /// Total swap.
    pub swap_total: u64,
    /// Unused swap.
    pub swap_free: u64,
}

impl Memory {
    /// Memory in use.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// Swap in use.
    #[must_use]
    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }
}

/// Reads `/proc/meminfo`; `None` when it is missing or lacks `MemTotal`.
#[must_use]
pub fn read(root: &Root) -> Option<Memory> {
    let text = root.read("/proc/meminfo")?;
    let fields = meminfo(&text);
    let get = |key: &str| fields.get(key).copied();
    let total = get("MemTotal")?;
    let (available, estimated) = match get("MemAvailable") {
        Some(available) => (available, false),
        None => {
            let estimate = ["MemFree", "Buffers", "Cached"]
                .iter()
                .filter_map(|key| get(key))
                .sum();
            (estimate, true)
        }
    };
    Some(Memory {
        total,
        available,
        estimated,
        swap_total: get("SwapTotal").unwrap_or(0),
        swap_free: get("SwapFree").unwrap_or(0),
    })
}

/// `/proc/meminfo` fields in bytes.
fn meminfo(text: &str) -> BTreeMap<&str, u64> {
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let mut words = value.split_whitespace();
        let Some(Ok(number)) = words.next().map(str::parse::<u64>) else {
            continue;
        };
        let bytes = match words.next() {
            Some("kB") => number * 1024,
            _ => number,
        };
        fields.insert(key.trim(), bytes);
    }
    fields
}

/// Limits set on this process's cgroup, as in containers. `None` means unlimited.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CgroupLimits {
    /// Memory limit in bytes.
    pub memory: Option<u64>,
    /// Swap limit in bytes (cgroup v2).
    pub swap: Option<u64>,
    /// Swap the cgroup uses, in bytes (cgroup v2).
    pub swap_used: Option<u64>,
    /// Memory the cgroup uses, in bytes.
    pub memory_used: Option<u64>,
    /// CPU time quota, in CPUs (`1.5` is one and a half CPUs).
    pub cpus: Option<f64>,
    /// Number of CPUs the cpuset allows.
    pub cpuset: Option<usize>,
}

/// Values above this are "unlimited" in cgroup v1, which has no `max` keyword.
const CGROUP_V1_UNLIMITED: u64 = 1 << 62;

/// Reads the cgroup v2 files, else the cgroup v1 ones, as mounted inside a container.
#[must_use]
pub fn cgroup_limits(root: &Root) -> CgroupLimits {
    let v2_cpus = root
        .read_line("/sys/fs/cgroup/cpu.max")
        .and_then(|line| {
            let (quota, period) = line.split_once(' ')?;
            Some((quota.parse::<f64>().ok()?, period.parse::<f64>().ok()?))
        })
        .filter(|(_, period)| *period > 0.0)
        .map(|(quota, period)| quota / period);
    let v1_cpus = || {
        let quota: f64 = root
            .read_line("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")?
            .parse()
            .ok()?;
        let period: f64 = root
            .read_line("/sys/fs/cgroup/cpu/cpu.cfs_period_us")?
            .parse()
            .ok()?;
        (quota > 0.0 && period > 0.0).then(|| quota / period)
    };
    let memory = root
        .read_line("/sys/fs/cgroup/memory.max")
        .or_else(|| root.read_line("/sys/fs/cgroup/memory/memory.limit_in_bytes"))
        .and_then(|line| line.parse::<u64>().ok())
        .filter(|limit| *limit < CGROUP_V1_UNLIMITED);
    let number = |path: &str| {
        root.read_line(path)
            .and_then(|line| line.parse::<u64>().ok())
    };
    let swap = number("/sys/fs/cgroup/memory.swap.max");
    let swap_used = number("/sys/fs/cgroup/memory.swap.current");
    let memory_used = root
        .read_line("/sys/fs/cgroup/memory.current")
        .or_else(|| root.read_line("/sys/fs/cgroup/memory/memory.usage_in_bytes"))
        .and_then(|line| line.parse::<u64>().ok());
    let cpuset = root
        .read_line("/sys/fs/cgroup/cpuset.cpus.effective")
        .or_else(|| root.read_line("/sys/fs/cgroup/cpuset/cpuset.effective_cpus"))
        .and_then(|list| crate::cpu_list_len(&list));
    CgroupLimits {
        memory,
        memory_used,
        swap,
        swap_used,
        cpus: v2_cpus.or_else(v1_cpus),
        cpuset,
    }
}
