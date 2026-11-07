//! The records of 系统基础信息, shared by the three platform variants. Labels and
//! values are Chinese (design D-15); numbers are metrics so they stay numeric (D-06).

use std::time::Duration;

use bench_api::Env;
use bench_api::Measurement;
use bench_api::Missing;
use bench_api::PlatformKind;
use bench_api::Record;
use bench_api::RecordError;
use bench_api::Table;
use bench_host::Root;
use bench_host::cpu;
use bench_host::cpu::Cpu;
use bench_host::cpu::Virtualization;
use bench_host::memory;
use bench_host::memory::CgroupLimits;
use bench_host::os;
use bench_host::storage;
use bench_host::storage::Filesystem;
use bench_host::storage::Space;
use bench_host::storage::Use;
use bench_host::system;

/// What a report reads: the host's files, the facts the orchestrator probed, where the run
/// directory is, and how to size a mounted filesystem (statvfs on a real host).
pub struct Host<F> {
    /// The host's root, or a captured fixture.
    pub root: Root,
    /// Facts probed by the orchestrator.
    pub env: Env,
    /// `major:minor` of the filesystem that holds the run directory.
    pub work_dir: Option<(u32, u32)>,
    /// Size of a filesystem by mount point.
    pub space: F,
}

/// All records for one kind of platform, in display order.
pub fn records<F>(host: &Host<F>, kind: PlatformKind) -> Result<Vec<Record>, RecordError>
where
    F: Fn(&str) -> Option<Space>,
{
    let root = &host.root;
    let limits = (kind == PlatformKind::Container).then(|| memory::cgroup_limits(root));
    let mut records = Vec::new();
    cpu_records(&mut records, &cpu::read(root), kind, limits.as_ref())?;
    memory_records(&mut records, memory::read(root), limits.as_ref())?;
    if kind == PlatformKind::Container {
        container_storage(&mut records, host)?;
    } else {
        storage_records(&mut records, host, kind == PlatformKind::Dedicated)?;
    }
    system_records(&mut records, root, &host.env);
    Ok(records)
}

fn unavailable() -> String {
    Missing::Unavailable.to_string()
}

fn cpu_records(
    records: &mut Vec<Record>,
    cpu: &Cpu,
    kind: PlatformKind,
    limits: Option<&CgroupLimits>,
) -> Result<(), RecordError> {
    records.push(Record::fact(
        "cpu.model",
        "CPU 型号",
        cpu.model.clone().unwrap_or_else(unavailable),
    ));
    records.push(Record::fact(
        "cpu.cores",
        "CPU 核心",
        cores(cpu, kind, limits),
    ));
    let mhz = match cpu.mhz {
        Some(mhz) => Measurement::try_new(mhz.round(), "MHz")?,
        None => Measurement::new_missing(Missing::Unavailable),
    };
    records.push(Record::metric("cpu.mhz", "CPU 频率", mhz));
    records.push(Record::fact("cpu.cache", "CPU 缓存", caches(&cpu.caches)));
    let aes = match cpu.aes {
        Some(true) => "支持".to_owned(),
        Some(false) => "不支持".to_owned(),
        None => unavailable(),
    };
    records.push(Record::fact("cpu.aes", "AES 指令", aes));
    if kind != PlatformKind::Container {
        let virtualization = match cpu.virtualization {
            Virtualization::IntelVtX => "支持（VT-x）",
            Virtualization::AmdV => "支持（AMD-V）",
            Virtualization::Absent => "不支持",
            Virtualization::NotApplicable => "不适用",
        };
        records.push(Record::fact(
            "cpu.virtualization",
            "硬件虚拟化",
            virtualization,
        ));
    }
    Ok(())
}

fn cores(cpu: &Cpu, kind: PlatformKind, limits: Option<&CgroupLimits>) -> String {
    let Some(logical) = cpu.logical else {
        return unavailable();
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
            let allowed = limits.and_then(|limits| limits.cpuset).unwrap_or(logical);
            match limits.and_then(|limits| limits.cpus) {
                Some(quota) if quota < allowed as f64 => {
                    format!("{allowed} 个（限额 {} 个）", number(quota))
                }
                _ => format!("{allowed} 个"),
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
        return unavailable();
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

fn memory_records(
    records: &mut Vec<Record>,
    memory: Option<memory::Memory>,
    limits: Option<&CgroupLimits>,
) -> Result<(), RecordError> {
    let Some(memory) = memory else {
        for (id, label) in [("memory.total", "内存总量"), ("memory.used", "内存已用")] {
            records.push(Record::metric(
                id,
                label,
                Measurement::new_missing(Missing::Unavailable),
            ));
        }
        return Ok(());
    };
    // A container's limits and usage come from its cgroup when /proc/meminfo shows the
    // host's (Docker); with lxcfs (LXC) /proc/meminfo already shows the container's.
    let cgroup = limits.filter(|limits| limits.memory.is_some_and(|limit| limit < memory.total));
    let (total, used) = match cgroup {
        Some(limits) => (
            limits.memory.unwrap_or(memory.total),
            limits.memory_used.unwrap_or_else(|| memory.used()),
        ),
        None => (memory.total, memory.used()),
    };
    let (swap_total, swap_used) = match cgroup.and_then(|limits| limits.swap) {
        Some(swap) => (
            swap,
            cgroup.and_then(|limits| limits.swap_used).unwrap_or(0),
        ),
        None => (memory.swap_total, memory.swap_used()),
    };
    records.push(Record::metric("memory.total", "内存总量", size(total)?));
    let used_label = if memory.estimated {
        "内存已用（估算）"
    } else {
        "内存已用"
    };
    records.push(Record::metric("memory.used", used_label, size(used)?));
    if swap_total == 0 {
        records.push(Record::fact("swap", "Swap", "无"));
    } else {
        records.push(Record::metric("swap.total", "Swap 总量", size(swap_total)?));
        records.push(Record::metric("swap.used", "Swap 已用", size(swap_used)?));
    }
    Ok(())
}

fn storage_records<F>(
    records: &mut Vec<Record>,
    host: &Host<F>,
    show_media: bool,
) -> Result<(), RecordError>
where
    F: Fn(&str) -> Option<Space>,
{
    let storage = storage::read(&host.root, &host.space);
    if storage.disks.is_empty() {
        records.push(Record::fact("disks", "硬盘", unavailable()));
    } else {
        let mut headers = vec!["设备", "接口", "容量"];
        if show_media {
            headers.push("介质");
        }
        headers.extend(["型号", "用途"]);
        let rows = storage
            .disks
            .iter()
            .map(|disk| {
                let mut row = vec![
                    disk.name.clone(),
                    disk.interface.to_owned(),
                    size_text(disk.size),
                ];
                // Inside a virtual machine the rotation flag is whatever the hypervisor
                // claims, so the medium is shown on dedicated servers only.
                if show_media {
                    row.push(
                        match disk.rotational {
                            Some(false) => "固态",
                            Some(true) => "机械",
                            None => "未知",
                        }
                        .to_owned(),
                    );
                }
                row.push(disk.model.clone().unwrap_or_else(|| "—".to_owned()));
                row.push(uses(&disk.uses));
                row
            })
            .collect();
        records.push(Record::Table(Table::try_new(
            "disks",
            "硬盘",
            headers.into_iter().map(str::to_owned).collect(),
            rows,
        )?));
    }
    if storage.filesystems.is_empty() {
        records.push(Record::fact("filesystems", "文件系统", unavailable()));
        return Ok(());
    }
    let rows = storage
        .filesystems
        .iter()
        .map(|filesystem| {
            let mut row = filesystem_row(filesystem, host.work_dir);
            row.push(if filesystem.disks.is_empty() {
                "—".to_owned()
            } else {
                filesystem.disks.join("、")
            });
            row
        })
        .collect();
    records.push(Record::Table(Table::try_new(
        "filesystems",
        "文件系统",
        ["挂载点", "类型", "容量", "已用", "可用", "所在硬盘"]
            .map(str::to_owned)
            .to_vec(),
        rows,
    )?));
    Ok(())
}

/// A container sees the host's disks, so only its own root filesystem is shown.
fn container_storage<F>(records: &mut Vec<Record>, host: &Host<F>) -> Result<(), RecordError>
where
    F: Fn(&str) -> Option<Space>,
{
    let Some(root) = storage::root_filesystem(&host.root, &host.space) else {
        records.push(Record::fact("filesystems", "文件系统", unavailable()));
        return Ok(());
    };
    // A container's ZFS dataset is sized by its quota, so its numbers are meaningful.
    let mut row = filesystem_row(
        &Filesystem {
            zfs_pool: None,
            ..root
        },
        host.work_dir,
    );
    row.truncate(5);
    records.push(Record::Table(Table::try_new(
        "filesystems",
        "文件系统",
        ["挂载点", "类型", "容量", "已用", "可用"]
            .map(str::to_owned)
            .to_vec(),
        vec![row],
    )?));
    Ok(())
}

/// Mount point, type, size, used and available. A ZFS pool shares its free space among
/// its datasets, so only the available space is shown for it (design: system-info).
fn filesystem_row(filesystem: &Filesystem, work_dir: Option<(u32, u32)>) -> Vec<String> {
    let mut mount_point = filesystem.mount_point.clone();
    if work_dir.is_some_and(|device| filesystem.devices.contains(&device)) {
        mount_point.push_str("（测试目录）");
    }
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

fn system_records(records: &mut Vec<Record>, root: &Root, env: &Env) {
    records.push(Record::fact(
        "os",
        "系统",
        os::distribution(root).unwrap_or_else(unavailable),
    ));
    records.push(Record::fact(
        "kernel",
        "内核",
        system::kernel(root).unwrap_or_else(unavailable),
    ));
    records.push(Record::fact("arch", "架构", env.arch.clone()));
    records.push(Record::fact(
        "uptime",
        "运行时间",
        system::uptime(root).map_or_else(unavailable, uptime),
    ));
    records.push(Record::fact(
        "load",
        "负载",
        system::load(root).map_or_else(unavailable, |[one, five, fifteen]| {
            format!("{one:.2} / {five:.2} / {fifteen:.2}")
        }),
    ));
    let tcp = match (
        system::congestion_control(root),
        system::default_qdisc(root),
    ) {
        (Some(algorithm), Some(qdisc)) => format!("{algorithm}（队列 {qdisc}）"),
        (Some(algorithm), None) => algorithm,
        (None, _) => unavailable(),
    };
    records.push(Record::fact("tcp", "TCP 拥塞控制", tcp));
    let reachable = |ok: bool| if ok { "可用" } else { "不可用" };
    records.push(Record::fact(
        "network",
        "IPv4 / IPv6",
        format!("{} / {}", reachable(env.ipv4), reachable(env.ipv6)),
    ));
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

/// A size as a metric, in the largest binary unit that keeps it at least 1.
fn size(bytes: u64) -> Result<Measurement, RecordError> {
    let (value, unit) = scaled(bytes);
    Measurement::try_new((value * 100.0).round() / 100.0, unit)
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
