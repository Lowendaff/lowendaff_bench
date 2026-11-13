//! CPU caches: every instance once, with the CPUs that share it.

use std::collections::BTreeSet;

use crate::Root;
use crate::cpu;

/// What a cache holds, from its `type` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheKind {
    /// Data only.
    Data,
    /// Instructions only.
    Instruction,
    /// Data and instructions.
    Unified,
}

/// One cache instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cache {
    /// Its level: 1 for L1 and so on.
    pub level: u32,
    /// What it holds.
    pub kind: CacheKind,
    /// Its size in bytes.
    pub size: u64,
    /// The CPUs that share it, ascending.
    pub cpus: Vec<usize>,
}

/// Every cache instance under `root`, once, ordered by level, kind, size and CPUs. Empty when
/// the kernel does not describe the caches. A cache whose sharing is not given is left out, as
/// it cannot be told apart from the other instances.
#[must_use]
pub fn read(root: &Root) -> Vec<Cache> {
    const CPUS: &str = "/sys/devices/system/cpu";
    let mut caches = BTreeSet::new();
    for name in root.list(CPUS) {
        let numbered = name
            .strip_prefix("cpu")
            .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
        if !numbered {
            continue;
        }
        let dir = format!("{CPUS}/{name}/cache");
        for index in root.list(&dir) {
            if !index.starts_with("index") {
                continue;
            }
            let read = |file: &str| root.read_line(&format!("{dir}/{index}/{file}"));
            let kind = match read("type").as_deref() {
                Some("Data") => CacheKind::Data,
                Some("Instruction") => CacheKind::Instruction,
                Some("Unified") => CacheKind::Unified,
                _ => continue,
            };
            let (Some(level), Some(size), Some(cpus)) = (
                read("level").and_then(|level| level.parse().ok()),
                read("size").and_then(|size| cpu::bytes(&size)),
                read("shared_cpu_list").and_then(|list| crate::cpu_list(&list)),
            ) else {
                continue;
            };
            caches.insert(Cache {
                level,
                kind,
                size,
                cpus,
            });
        }
    }
    caches.into_iter().collect()
}
