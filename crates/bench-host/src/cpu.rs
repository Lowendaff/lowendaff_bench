//! CPU facts: model, topology, frequency, caches and feature flags.

use std::collections::BTreeSet;
use std::mem;

use bench_api::PlatformKind;

use crate::Root;
use crate::memory;

/// What the CPU reports about itself. Every field is `None` (or empty) when the host does
/// not expose it, which is common in virtual machines and containers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cpu {
    /// Model name. On ARM it is decoded from the implementer and part codes, the way
    /// `lscpu` does, because `/proc/cpuinfo` has no model name there.
    pub model: Option<String>,
    /// Logical CPUs online.
    pub logical: Option<usize>,
    /// Physical packages (sockets) and cores, from the topology files.
    pub topology: Option<Topology>,
    /// Highest frequency in MHz: the cpufreq maximum, else the current x86 clock.
    pub mhz: Option<f64>,
    /// Caches of CPU 0 in sysfs order, such as `("L1d", 32768)`. The name is empty when
    /// only `/proc/cpuinfo`'s unnamed "cache size" is known.
    pub caches: Vec<(String, u64)>,
    /// Whether AES instructions are available.
    pub aes: Option<bool>,
    /// Hardware support for running virtual machines.
    pub virtualization: Virtualization,
}

/// Sockets and physical cores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Topology {
    /// Physical packages.
    pub sockets: usize,
    /// Physical cores across all packages.
    pub cores: usize,
}

/// Hardware virtualization extensions, as advertised in the x86 CPU flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Virtualization {
    /// Intel VT-x (`vmx`).
    IntelVtX,
    /// AMD-V (`svm`).
    AmdV,
    /// Neither flag is present.
    Absent,
    /// Not advertised in CPU flags on this architecture.
    #[default]
    NotApplicable,
}

/// Threads a test can run at once: the logical CPUs, or in a container
/// the CPUs of its cpuset when it has one. 系统基础信息 shows the same number.
#[must_use]
pub fn threads(logical: usize, kind: PlatformKind, cpuset: Option<usize>) -> usize {
    match kind {
        PlatformKind::Container => cpuset.unwrap_or(logical),
        PlatformKind::Dedicated | PlatformKind::Vm => logical,
    }
}

/// [`threads`] for the host under `root`; `None` when its logical CPUs cannot be counted.
#[must_use]
pub fn usable_threads(root: &Root, kind: PlatformKind) -> Option<usize> {
    let logical = read(root).logical?;
    let cpuset = if kind == PlatformKind::Container {
        memory::cgroup_limits(root).cpuset
    } else {
        None
    };
    Some(threads(logical, kind, cpuset))
}

/// The CPUs this process may run on, from `Cpus_allowed_list` in
/// `/proc/self/status`: its affinity, within a container's cpuset, in the kernel's numbering,
/// which the NUMA nodes use too. `None` when it is missing.
#[must_use]
pub fn allowed(root: &Root) -> Option<Vec<usize>> {
    crate::status_list(root, "Cpus_allowed_list")
}

/// Reads the CPU facts under `root`.
#[must_use]
pub fn read(root: &Root) -> Cpu {
    let cpuinfo = root.read("/proc/cpuinfo").unwrap_or_default();
    let blocks = cpuinfo_blocks(&cpuinfo);
    let first = blocks.first().map(Vec::as_slice).unwrap_or_default();
    let flags = field(first, "flags");
    let features = field(first, "Features");
    let words = flags.or(features).unwrap_or_default();
    let has = |word: &str| words.split_whitespace().any(|flag| flag == word);
    Cpu {
        model: field(first, "model name")
            .map(str::to_owned)
            .or_else(|| arm_model(first)),
        logical: root
            .read_line("/sys/devices/system/cpu/online")
            .and_then(|list| crate::cpu_list_len(&list))
            .or_else(|| {
                let processors = blocks
                    .iter()
                    .filter(|block| field(block, "processor").is_some())
                    .count();
                (processors > 0).then_some(processors)
            }),
        topology: topology(root),
        mhz: root
            .read_line("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
            .and_then(|khz| khz.parse::<f64>().ok())
            .map(|khz| khz / 1000.0)
            .or_else(|| field(first, "cpu MHz").and_then(|mhz| mhz.parse().ok())),
        caches: caches(root, first),
        aes: (!words.is_empty()).then(|| has("aes")),
        virtualization: if flags.is_none() {
            Virtualization::NotApplicable
        } else if has("vmx") {
            Virtualization::IntelVtX
        } else if has("svm") {
            Virtualization::AmdV
        } else {
            Virtualization::Absent
        },
    }
}

/// `/proc/cpuinfo` as one list of `key: value` pairs per block.
fn cpuinfo_blocks(text: &str) -> Vec<Vec<(String, String)>> {
    let mut blocks = Vec::new();
    let mut block = Vec::new();
    for line in text.lines() {
        match line.split_once(':') {
            Some((key, value)) => block.push((key.trim().to_owned(), value.trim().to_owned())),
            None if line.trim().is_empty() && !block.is_empty() => {
                blocks.push(mem::take(&mut block));
            }
            None => {}
        }
    }
    if !block.is_empty() {
        blocks.push(block);
    }
    blocks
}

fn field<'a>(block: &'a [(String, String)], key: &str) -> Option<&'a str> {
    block
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty())
}

fn topology(root: &Root) -> Option<Topology> {
    let mut sockets = BTreeSet::new();
    let mut cores = BTreeSet::new();
    for cpu in root.list("/sys/devices/system/cpu") {
        if !cpu
            .strip_prefix("cpu")
            .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        let read =
            |name: &str| root.read_line(&format!("/sys/devices/system/cpu/{cpu}/topology/{name}"));
        // Offline CPUs have no topology files.
        let (Some(socket), Some(core)) = (read("physical_package_id"), read("core_id")) else {
            continue;
        };
        let die = read("die_id").unwrap_or_default();
        cores.insert((socket.clone(), die, core));
        sockets.insert(socket);
    }
    (!sockets.is_empty()).then_some(Topology {
        sockets: sockets.len(),
        cores: cores.len(),
    })
}

fn caches(root: &Root, first: &[(String, String)]) -> Vec<(String, u64)> {
    let dir = "/sys/devices/system/cpu/cpu0/cache";
    let mut indexes: Vec<(u32, String)> = root
        .list(dir)
        .into_iter()
        .filter_map(|name| Some((name.strip_prefix("index")?.parse().ok()?, name)))
        .collect();
    indexes.sort();
    let mut caches = Vec::new();
    for (_, index) in indexes {
        let read = |name: &str| root.read_line(&format!("{dir}/{index}/{name}"));
        let (Some(level), Some(size)) = (read("level"), read("size").and_then(|size| bytes(&size)))
        else {
            continue;
        };
        let kind = match read("type").as_deref() {
            Some("Data") => "d",
            Some("Instruction") => "i",
            _ => "",
        };
        caches.push((format!("L{level}{kind}"), size));
    }
    if caches.is_empty()
        && let Some(size) = field(first, "cache size").and_then(bytes)
    {
        caches.push((String::new(), size));
    }
    caches
}

/// Sizes such as `32K`, `22528 KB` or `1M`.
pub(crate) fn bytes(text: &str) -> Option<u64> {
    let text = text.trim();
    let digits = text.trim_end_matches(|c: char| !c.is_ascii_digit());
    let number: u64 = digits.trim().parse().ok()?;
    let unit = text[digits.len()..].trim().to_ascii_uppercase();
    let factor = match unit.as_str() {
        "" | "B" => 1,
        "K" | "KB" | "KIB" => 1 << 10,
        "M" | "MB" | "MIB" => 1 << 20,
        "G" | "GB" | "GIB" => 1 << 30,
        _ => return None,
    };
    Some(number * factor)
}

/// The vendor and core name for ARM CPUs, from util-linux's table of implementer and
/// part codes (ARMv8 and later parts only).
fn arm_model(block: &[(String, String)]) -> Option<String> {
    let code = |key: &str| {
        let value = field(block, key)?;
        u32::from_str_radix(value.strip_prefix("0x")?, 16).ok()
    };
    let implementer = code("CPU implementer")?;
    let part = code("CPU part");
    let Some((vendor, parts)) = ARM_IMPLEMENTERS
        .iter()
        .find(|(id, _, _)| *id == implementer)
        .map(|(_, vendor, parts)| (*vendor, *parts))
    else {
        return Some(match part {
            Some(part) => {
                format!("ARM 处理器（厂商代码 {implementer:#04x}，型号代码 {part:#05x}）")
            }
            None => format!("ARM 处理器（厂商代码 {implementer:#04x}）"),
        });
    };
    let name = part.and_then(|part| {
        parts
            .iter()
            .find(|(id, _)| *id == part)
            .map(|(_, name)| *name)
    });
    Some(match (name, part) {
        (Some(name), _) if name.starts_with(vendor) => name.to_owned(),
        (Some(name), _) => format!("{vendor} {name}"),
        (None, Some(part)) => format!("{vendor}（型号代码 {part:#05x}）"),
        (None, None) => vendor.to_owned(),
    })
}

type Parts = &'static [(u32, &'static str)];

const ARM_IMPLEMENTERS: [(u32, &str, Parts); 14] = [
    (0x41, "ARM", ARM_PARTS),
    (
        0x42,
        "Broadcom",
        &[(0x100, "Brahma-B53"), (0x516, "ThunderX2")],
    ),
    (
        0x43,
        "Cavium",
        &[
            (0x0a1, "ThunderX-88XX"),
            (0x0a2, "ThunderX-81XX"),
            (0x0a3, "ThunderX-83XX"),
            (0x0af, "ThunderX2-99xx"),
            (0x0b8, "ThunderX3-T110"),
        ],
    ),
    (0x46, "FUJITSU", &[(0x001, "A64FX"), (0x003, "MONAKA")]),
    (
        0x48,
        "HiSilicon",
        &[
            (0xd01, "Kunpeng-920"),
            (0xd02, "Kunpeng-920"),
            (0xd03, "Kunpeng-920"),
            (0xd06, "Kunpeng-950"),
            (0xd22, "Kunpeng-920"),
            (0xd40, "Cortex-A76"),
            (0xd41, "Cortex-A77"),
        ],
    ),
    (
        0x4e,
        "NVIDIA",
        &[(0x003, "Denver-2"), (0x004, "Carmel"), (0x010, "Olympus")],
    ),
    (0x50, "APM", &[(0x000, "X-Gene")]),
    (
        0x51,
        "Qualcomm",
        &[
            (0x001, "Oryon"),
            (0x002, "Oryon-2"),
            (0x800, "Falkor-V1/Kryo"),
            (0x801, "Kryo-V2"),
            (0xc00, "Falkor"),
            (0xc01, "Saphira"),
        ],
    ),
    (0x53, "Samsung", &[]),
    (0x56, "Marvell", &[]),
    (0x61, "Apple", APPLE_PARTS),
    (0x6d, "Microsoft", &[(0xd49, "Azure-Cobalt-100")]),
    (
        0x70,
        "Phytium",
        &[
            (0x662, "FTC662"),
            (0x663, "FTC663"),
            (0x664, "FTC664"),
            (0x862, "FTC862"),
        ],
    ),
    (0xc0, "Ampere", &[(0xac3, "Ampere-1"), (0xac4, "Ampere-1a")]),
];

const ARM_PARTS: Parts = &[
    (0xd02, "Cortex-A34"),
    (0xd03, "Cortex-A53"),
    (0xd04, "Cortex-A35"),
    (0xd05, "Cortex-A55"),
    (0xd06, "Cortex-A65"),
    (0xd07, "Cortex-A57"),
    (0xd08, "Cortex-A72"),
    (0xd09, "Cortex-A73"),
    (0xd0a, "Cortex-A75"),
    (0xd0b, "Cortex-A76"),
    (0xd0c, "Neoverse-N1"),
    (0xd0d, "Cortex-A77"),
    (0xd0e, "Cortex-A76AE"),
    (0xd40, "Neoverse-V1"),
    (0xd41, "Cortex-A78"),
    (0xd42, "Cortex-A78AE"),
    (0xd43, "Cortex-A65AE"),
    (0xd44, "Cortex-X1"),
    (0xd46, "Cortex-A510"),
    (0xd47, "Cortex-A710"),
    (0xd48, "Cortex-X2"),
    (0xd49, "Neoverse-N2"),
    (0xd4a, "Neoverse-E1"),
    (0xd4b, "Cortex-A78C"),
    (0xd4c, "Cortex-X1C"),
    (0xd4d, "Cortex-A715"),
    (0xd4e, "Cortex-X3"),
    (0xd4f, "Neoverse-V2"),
    (0xd80, "Cortex-A520"),
    (0xd81, "Cortex-A720"),
    (0xd82, "Cortex-X4"),
    (0xd83, "Neoverse-V3AE"),
    (0xd84, "Neoverse-V3"),
    (0xd85, "Cortex-X925"),
    (0xd87, "Cortex-A725"),
    (0xd88, "Cortex-A520AE"),
    (0xd89, "Cortex-A720AE"),
    (0xd8a, "C1-Nano"),
    (0xd8b, "C1-Pro"),
    (0xd8c, "C1-Ultra"),
    (0xd8e, "Neoverse-N3"),
    (0xd8f, "Cortex-A320"),
    (0xd90, "C1-Premium"),
];

/// Apple's 64-bit cores only: under Apple's hypervisor the part reads `0x000`, which the
/// full table maps to a 32-bit core.
const APPLE_PARTS: Parts = &[
    (0x020, "Icestorm-A14"),
    (0x021, "Firestorm-A14"),
    (0x022, "Icestorm-M1"),
    (0x023, "Firestorm-M1"),
    (0x024, "Icestorm-M1-Pro"),
    (0x025, "Firestorm-M1-Pro"),
    (0x028, "Icestorm-M1-Max"),
    (0x029, "Firestorm-M1-Max"),
    (0x030, "Blizzard-A15"),
    (0x031, "Avalanche-A15"),
    (0x032, "Blizzard-M2"),
    (0x033, "Avalanche-M2"),
    (0x034, "Blizzard-M2-Pro"),
    (0x035, "Avalanche-M2-Pro"),
    (0x036, "Sawtooth-A16"),
    (0x037, "Everest-A16"),
    (0x038, "Blizzard-M2-Max"),
    (0x039, "Avalanche-M2-Max"),
];
