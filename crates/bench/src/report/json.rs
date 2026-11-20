//! Structured JSON report renderer (`--format json`), preserving the `--show-ip` privacy boundary
//! across the system profile, route-tracing hops and IP reputation reports.

use std::net::IpAddr;

use bench_host::bmc;
use bench_host::bmc::Bmc;
use bench_host::bmc::InterfaceKind;
use bench_host::cards::Bus;
use bench_host::cards::Duplex;
use bench_host::cards::Link;
use bench_host::cards::MasterKind;
use bench_host::cards::NetworkCard;
use bench_host::cards::Pcie;
use bench_host::cpu::Model;
use bench_host::cpu::Virtualization;
use bench_host::glibc::Libc;
use bench_host::hardware::Hardware;
use bench_host::machine::Firmware;
use bench_host::machine::Line;
use bench_host::network::Address;
use bench_host::network::Family;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::MemoryFacts;
use bench_host::profile::Read;
use bench_host::profile::StorageFacts;
use bench_host::profile::SystemFacts;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::smbios::Capacity;
use bench_host::smbios::Ecc;
use bench_host::smbios::Slot;
use bench_host::storage::Disk;
use bench_host::storage::Filesystem;
use bench_host::storage::Use;
use serde::Serialize;
use serde_json::Value;

use super::bgp_graph;
use super::terminal;
use super::terminal::Page;
use crate::locale;
use crate::network;
use crate::orchestrator::Fault;
use crate::orchestrator::Run;
use crate::orchestrator::Status;
use crate::results::Measured;

/// Renders `page` as pretty-printed JSON with a trailing newline.
#[must_use]
pub fn render(page: &Page) -> String {
    let own = terminal::own_addresses(page.host);
    let totals = terminal::totals(page);
    let document = Document {
        version: page.version,
        environment: terminal::platform_text(page.host.platform),
        show_ip: page.show_ip,
        system: build_system(page.host, page.show_ip),
        runs: page
            .runs
            .iter()
            .map(|run| build_run(run, &own, page.show_ip))
            .collect(),
        not_applicable: page.not_applicable,
        summary: SummaryJson {
            items: totals.items,
            completed: totals.completed,
            skipped: totals.skipped,
            failed: totals.failed,
            timed_out: totals.timed_out,
            interrupted: totals.interrupted,
            total_seconds: totals.seconds,
        },
    };
    let mut out = serde_json::to_string_pretty(&document).expect("report serializes to JSON");
    out.push('\n');
    out
}

#[derive(Serialize)]
struct Document<'a> {
    version: &'a str,
    environment: String,
    show_ip: bool,
    system: SystemSection,
    runs: Vec<RunJson<'a>>,
    not_applicable: &'a [&'a str],
    summary: SummaryJson,
}

#[derive(Serialize)]
struct SummaryJson {
    items: usize,
    completed: usize,
    skipped: usize,
    failed: usize,
    timed_out: usize,
    interrupted: usize,
    total_seconds: f64,
}

#[derive(Serialize)]
struct RunJson<'a> {
    id: &'a str,
    group: &'a str,
    title: &'a str,
    status: &'static str,
    status_text: String,
    reason: Option<String>,
    started_seconds: Option<f64>,
    elapsed_seconds: Option<f64>,
    output: Option<Value>,
}

fn build_run<'a>(run: &'a Run<Measured>, own: &[IpAddr], show_ip: bool) -> RunJson<'a> {
    let (status, reason) = status_fields(run);
    RunJson {
        id: run.id,
        group: run.group,
        title: run.title.unwrap_or(run.id),
        status,
        status_text: terminal::run_status(run),
        reason,
        started_seconds: run.started.map(|d| d.as_secs_f64()),
        elapsed_seconds: run.elapsed.map(|d| d.as_secs_f64()),
        output: run
            .result
            .as_ref()
            .and_then(|measured| serialize_measured(measured, own, show_ip)),
    }
}

fn status_fields(run: &Run<Measured>) -> (&'static str, Option<String>) {
    match &run.status {
        Status::Completed => ("completed", None),
        Status::Skipped(Some(unmet)) => ("skipped", Some(terminal::unmet_text(unmet))),
        Status::Skipped(None) => ("skipped", terminal::stop_reason(run)),
        Status::Failed(Some(fault)) => ("failed", Some(fault_reason(fault))),
        Status::Failed(None) => ("failed", terminal::stop_reason(run)),
        Status::TimedOut => ("timed_out", None),
        Status::Cancelled => ("interrupted", None),
    }
}

fn fault_reason(fault: &Fault) -> String {
    terminal::fault_text(fault)
}

fn serialize_measured(measured: &Measured, own: &[IpAddr], show_ip: bool) -> Option<Value> {
    match measured {
        Measured::CpuBench(r) => r.output.as_ref().map(to_value),
        Measured::MemorySysbench(r) => r.output.as_ref().map(to_value),
        Measured::MemoryStream(r) => r.output.as_ref().map(to_value),
        Measured::MemoryLatency(r) => r.output.as_ref().map(to_value),
        Measured::MemoryNuma(r) => r.output.as_ref().map(to_value),
        Measured::DiskHealth(r) => r.output.as_ref().map(to_value),
        Measured::DiskFio(r) => r.output.as_ref().map(to_value),
        Measured::NetworkIperf3(r) => r.output.as_ref().map(to_value),
        Measured::NetworkTaier(r) => r.output.as_ref().map(to_value),
        Measured::NetworkTrace(r) => r.output.as_ref().map(|out| {
            let mut val = to_value(out);
            if !show_ip && !own.is_empty() {
                mask_trace_value(&mut val, own);
            }
            val
        }),
        Measured::NetworkBgp(r) => r.output.as_ref().map(|out| {
            let mut val = to_value(out);
            enrich_and_mask_bgp(&mut val, out, show_ip);
            val
        }),
        Measured::IpQuality(r) => r.output.as_ref().map(|out| {
            let mut val = to_value(out);
            if !show_ip {
                if let Some(rep) = &out.ipv4 {
                    mask_quality_family(&mut val, "ipv4", rep.address);
                }
                if let Some(rep) = &out.ipv6 {
                    mask_quality_family(&mut val, "ipv6", rep.address);
                }
            }
            val
        }),
        Measured::IpMedia(r) => r.output.as_ref().map(to_value),
    }
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("plugin output serializes to JSON")
}

fn mask_quality_family(root: &mut Value, key: &str, address: IpAddr) {
    if let Some(family) = root.get_mut(key).and_then(Value::as_object_mut) {
        family.insert("address".to_owned(), Value::String(network::mask(address)));
        family.insert("hostname".to_owned(), Value::Null);
    }
}

fn enrich_and_mask_bgp(root: &mut Value, out: &plugin_network::bgp::Output, show_ip: bool) {
    for (key, maybe_rep) in [("ipv4", out.ipv4.as_deref()), ("ipv6", out.ipv6.as_deref())] {
        let Some(rep) = maybe_rep else {
            continue;
        };
        let Some(family) = root.get_mut(key).and_then(Value::as_object_mut) else {
            continue;
        };
        if !show_ip {
            family.insert(
                "address".to_owned(),
                Value::String(network::mask(rep.address)),
            );
            if let (Some(base), Some(plen)) = (rep.prefix_base, rep.prefix_len) {
                family.insert(
                    "prefix".to_owned(),
                    Value::String(format!("{}/{plen}", network::mask(base))),
                );
            }
        }
        let svg = bgp_graph::layout(rep, None)
            .map(|graph| Value::String(bgp_graph::render_svg(&graph)))
            .unwrap_or(Value::Null);
        family.insert("svg".to_owned(), svg);
    }
}

fn mask_trace_value(root: &mut Value, own: &[IpAddr]) {
    for key in ["ipv4", "ipv6"] {
        let Some(targets) = root.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        for target in targets {
            let Some(hops) = target
                .get_mut("route")
                .and_then(|route| route.get_mut("Traced"))
                .and_then(|traced| traced.get_mut("hops"))
                .and_then(Value::as_array_mut)
            else {
                continue;
            };
            for hop in hops {
                let Some(answers) = hop.get_mut("answers").and_then(Value::as_array_mut) else {
                    continue;
                };
                for answer in answers {
                    let Some(field) = answer.get_mut("address") else {
                        continue;
                    };
                    if let Some(ip) = field.as_str().and_then(|text| text.parse::<IpAddr>().ok())
                        && own.contains(&ip)
                    {
                        *field = Value::String(network::mask(ip));
                    }
                }
            }
        }
    }
}

#[derive(Serialize)]
struct SystemSection {
    platform: PlatformJson,
    arch: Option<String>,
    libc: LibcJson,
    hardware: Option<ReadJson<HardwareJson>>,
    cpu: ReadJson<CpuJson>,
    memory: ReadJson<Option<MemoryJson>>,
    storage: ReadJson<StorageJson>,
    os: ReadJson<OsJson>,
    cards: ReadJson<Vec<CardJson>>,
    network: NetworkJson,
}

#[derive(Serialize)]
#[serde(tag = "state", content = "data", rename_all = "snake_case")]
enum ReadJson<T> {
    Done(T),
    TimedOut,
    Failed,
}

impl<T> ReadJson<T> {
    fn map<U>(source: &Read<U>, f: impl FnOnce(&U) -> T) -> Self {
        match source {
            Read::Done(value) => Self::Done(f(value)),
            Read::TimedOut => Self::TimedOut,
            Read::Failed => Self::Failed,
        }
    }
}

#[derive(Serialize)]
struct PlatformJson {
    kind: &'static str,
    hypervisor: Option<&'static str>,
    container: Option<&'static str>,
}

#[derive(Serialize)]
struct LibcJson {
    kind: &'static str,
    version: Option<String>,
}

#[derive(Serialize)]
struct HardwareJson {
    machine: LineJson,
    board: LineJson,
    firmware: Option<FirmwareJson>,
    bmc: Option<BmcJson>,
    ecc_modes: Vec<&'static str>,
    edac_errors: Option<EdacErrorsJson>,
    board_memory_bytes: Option<u64>,
    slots: Vec<SlotJson>,
}

#[derive(Serialize)]
struct LineJson {
    vendor: Option<String>,
    name: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize)]
struct FirmwareJson {
    kind: &'static str,
    vendor: Option<String>,
    version: Option<String>,
    date: Option<String>,
    timestamp: Option<u64>,
}

#[derive(Serialize)]
struct BmcJson {
    source: &'static str,
    vendor: Option<String>,
    firmware: Option<String>,
    ipmi: Option<String>,
    interface: Option<&'static str>,
}

#[derive(Serialize)]
struct EdacErrorsJson {
    corrected: u64,
    uncorrected: u64,
}

#[derive(Serialize)]
struct SlotJson {
    name: Option<String>,
    status: &'static str,
    size_bytes: Option<u64>,
    generation: Option<&'static str>,
    form: Option<&'static str>,
    speed_mts: Option<u32>,
    configured_speed_mts: Option<u32>,
    manufacturer: Option<String>,
    part_number: Option<String>,
}

#[derive(Serialize)]
struct CpuJson {
    model: Option<String>,
    logical: Option<usize>,
    usable: Option<usize>,
    quota: Option<f64>,
    sockets: Option<usize>,
    cores: Option<usize>,
    mhz: Option<f64>,
    caches: Vec<CacheJson>,
    aes: Option<bool>,
    virtualization: Option<&'static str>,
}

#[derive(Serialize)]
struct CacheJson {
    name: String,
    size_bytes: u64,
}

#[derive(Serialize)]
struct MemoryJson {
    total_bytes: u64,
    used_bytes: u64,
    used_estimated: bool,
    swap_total_bytes: u64,
    swap_used_bytes: u64,
}

#[derive(Serialize)]
struct StorageJson {
    raid: Option<Vec<RaidCardJson>>,
    disks: Vec<DiskJson>,
    filesystems: Vec<FilesystemJson>,
}

#[derive(Serialize)]
struct RaidCardJson {
    address: String,
    name: Option<String>,
    driver: Option<String>,
    disks: Vec<RaidDiskJson>,
}

#[derive(Serialize)]
struct RaidDiskJson {
    name: String,
    attachment: &'static str,
}

#[derive(Serialize)]
struct DiskJson {
    name: String,
    interface: &'static str,
    size_bytes: u64,
    rotational: Option<bool>,
    model: Option<String>,
    uses: Vec<String>,
}

#[derive(Serialize)]
struct FilesystemJson {
    mount_point: String,
    fs_type: String,
    zfs_pool: Option<String>,
    disks: Vec<String>,
    test_directory: bool,
    total_bytes: Option<u64>,
    used_bytes: Option<u64>,
    available_bytes: Option<u64>,
}

#[derive(Serialize)]
struct OsJson {
    distribution: Option<String>,
    kernel: Option<String>,
    uptime_seconds: Option<u64>,
    uptime_from_host: Option<bool>,
    load: Option<[f64; 3]>,
    load_from_host: Option<bool>,
    congestion_control: Option<String>,
    qdisc: Option<String>,
}

#[derive(Serialize)]
struct CardJson {
    interface: String,
    bus: &'static str,
    model: Option<String>,
    driver: Option<String>,
    link_up: bool,
    speed_mbps: Option<u32>,
    duplex: Option<&'static str>,
    mtu: Option<u32>,
    pcie_degraded: bool,
    virtual_functions: u32,
    master: Option<String>,
    master_kind: Option<&'static str>,
    default_ipv4: bool,
    default_ipv6: bool,
}

#[derive(Serialize)]
struct NetworkJson {
    ipv4: FamilyJson,
    ipv6: FamilyJson,
}

#[derive(Serialize)]
struct FamilyJson {
    reachable: bool,
    public_address: Option<String>,
    source: Option<&'static str>,
    text: String,
}

fn build_system(host: &HostProfile, show_ip: bool) -> SystemSection {
    let kind = host.platform.kind();
    SystemSection {
        platform: build_platform(host.platform),
        arch: (!host.arch.is_empty()).then(|| host.arch.clone()),
        libc: build_libc(host.libc),
        hardware: host
            .hardware
            .as_ref()
            .map(|hw| ReadJson::map(hw, build_hardware)),
        cpu: ReadJson::map(&host.cpu, |cpu| build_cpu(cpu, kind)),
        memory: ReadJson::map(&host.memory, |mem| mem.map(build_memory)),
        storage: ReadJson::map(&host.storage, |st| build_storage(st, kind)),
        os: ReadJson::map(&host.system, build_os),
        cards: ReadJson::map(&host.cards, |cards| cards.iter().map(build_card).collect()),
        network: NetworkJson {
            ipv4: build_family(&host.network.ipv4, "IPv4", show_ip),
            ipv6: build_family(&host.network.ipv6, "IPv6", show_ip),
        },
    }
}

fn build_platform(platform: Platform) -> PlatformJson {
    match platform {
        Platform::Dedicated => PlatformJson {
            kind: "dedicated",
            hypervisor: None,
            container: None,
        },
        Platform::Vm(hv) => PlatformJson {
            kind: "vm",
            hypervisor: Some(hypervisor_code(hv)),
            container: None,
        },
        Platform::Container(ct) => PlatformJson {
            kind: "container",
            hypervisor: None,
            container: Some(container_code(ct)),
        },
    }
}

const fn hypervisor_code(hv: Hypervisor) -> &'static str {
    match hv {
        Hypervisor::Kvm => "kvm",
        Hypervisor::Qemu => "qemu",
        Hypervisor::Xen => "xen",
        Hypervisor::Vmware => "vmware",
        Hypervisor::HyperV => "hyperv",
        Hypervisor::VirtualBox => "virtualbox",
        Hypervisor::Parallels => "parallels",
        Hypervisor::AmazonEc2 => "amazon_ec2",
        Hypervisor::GoogleCompute => "google_compute",
        Hypervisor::Bhyve => "bhyve",
        Hypervisor::Other => "other",
    }
}

const fn container_code(ct: ContainerKind) -> &'static str {
    match ct {
        ContainerKind::Lxc => "lxc",
        ContainerKind::Openvz => "openvz",
        ContainerKind::Docker => "docker",
        ContainerKind::Podman => "podman",
        ContainerKind::SystemdNspawn => "systemd_nspawn",
        ContainerKind::Wsl => "wsl",
        ContainerKind::Other => "other",
    }
}

fn build_libc(libc: Libc) -> LibcJson {
    match libc {
        Libc::Glibc(version) => LibcJson {
            kind: "glibc",
            version: Some(version.to_string()),
        },
        Libc::GlibcUnknown => LibcJson {
            kind: "glibc",
            version: None,
        },
        Libc::NoGlibc => LibcJson {
            kind: "non_glibc",
            version: None,
        },
    }
}

fn build_hardware(hardware: &Hardware) -> HardwareJson {
    let line = |item: &Line| LineJson {
        vendor: item.vendor.clone(),
        name: item.name.clone(),
        detail: item.detail.clone(),
    };
    let firmware = hardware.machine.firmware.as_ref().map(|fw| match fw {
        Firmware::Bios(bios) => FirmwareJson {
            kind: "bios",
            vendor: bios.vendor.clone(),
            version: bios.name.clone(),
            date: bios.detail.clone(),
            timestamp: None,
        },
        Firmware::UBoot(version) => FirmwareJson {
            kind: "u_boot",
            vendor: None,
            version: Some(version.clone()),
            date: None,
            timestamp: None,
        },
        Firmware::RaspberryPi(built) => FirmwareJson {
            kind: "raspberry_pi",
            vendor: None,
            version: None,
            date: None,
            timestamp: Some(*built),
        },
    });
    let bmc = hardware.bmc.as_ref().map(|entry| match entry {
        Bmc::Driver {
            vendor,
            firmware,
            ipmi,
        } => BmcJson {
            source: "driver",
            vendor: vendor
                .map(|id| bmc::vendor(id).map_or_else(|| format!("IANA {id}"), str::to_owned)),
            firmware: firmware.clone(),
            ipmi: ipmi.clone(),
            interface: None,
        },
        Bmc::Interface(iface) => BmcJson {
            source: "firmware",
            vendor: None,
            firmware: None,
            ipmi: iface.version.map(|(maj, min)| format!("{maj}.{min}")),
            interface: iface.kind.map(interface_code),
        },
        Bmc::Present => BmcJson {
            source: "acpi",
            vendor: None,
            firmware: None,
            ipmi: None,
            interface: None,
        },
    });
    let ecc_modes = match &hardware.smbios {
        Ok(smbios) if !smbios.ecc.is_empty() => smbios.ecc.iter().copied().map(ecc_code).collect(),
        _ => hardware
            .edac
            .as_ref()
            .map(|edac| edac.modes.iter().copied().map(ecc_code).collect())
            .unwrap_or_default(),
    };
    let edac_errors = hardware.edac.as_ref().map(|edac| EdacErrorsJson {
        corrected: edac.corrected,
        uncorrected: edac.uncorrected,
    });
    let slots = hardware
        .smbios
        .as_ref()
        .map(|smbios| smbios.slots.iter().map(build_slot).collect())
        .unwrap_or_default();
    HardwareJson {
        machine: line(&hardware.machine.system),
        board: line(&hardware.machine.board),
        firmware,
        bmc,
        ecc_modes,
        edac_errors,
        board_memory_bytes: hardware.board_memory,
        slots,
    }
}

const fn interface_code(kind: InterfaceKind) -> &'static str {
    match kind {
        InterfaceKind::Kcs => "kcs",
        InterfaceKind::Smic => "smic",
        InterfaceKind::Bt => "bt",
        InterfaceKind::Ssif => "ssif",
    }
}

const fn ecc_code(ecc: Ecc) -> &'static str {
    match ecc {
        Ecc::None => "none",
        Ecc::Parity => "parity",
        Ecc::SingleBit => "single_bit",
        Ecc::MultiBit => "multi_bit",
        Ecc::Crc => "crc",
        Ecc::Unknown => "unknown",
    }
}

fn build_slot(slot: &Slot) -> SlotJson {
    let (status, size_bytes) = match slot.capacity {
        Capacity::Empty => ("empty", None),
        Capacity::Disabled => ("disabled", None),
        Capacity::Unknown => ("unknown", None),
        Capacity::Kilobytes(kib) => ("installed", Some(kib.saturating_mul(1024))),
    };
    SlotJson {
        name: slot.name.clone(),
        status,
        size_bytes,
        generation: slot.generation,
        form: slot.form,
        speed_mts: slot.speed,
        configured_speed_mts: slot.configured_speed,
        manufacturer: slot.manufacturer.clone(),
        part_number: slot.part_number.clone(),
    }
}

fn build_cpu(facts: &CpuFacts, kind: PlatformKind) -> CpuJson {
    let words = locale::words();
    let cpu = &facts.cpu;
    let model = cpu.model.as_ref().map(|m| match m {
        Model::Name(name) => name.clone(),
        other => words.cpu_model(other),
    });
    let virtualization = (kind != PlatformKind::Container).then_some(match cpu.virtualization {
        Virtualization::IntelVtX => "intel_vt_x",
        Virtualization::AmdV => "amd_v",
        Virtualization::Absent => "absent",
        Virtualization::NotApplicable => "not_applicable",
    });
    CpuJson {
        model,
        logical: cpu.logical,
        usable: facts.usable,
        quota: facts.quota,
        sockets: cpu.topology.map(|t| t.sockets),
        cores: cpu.topology.map(|t| t.cores),
        mhz: cpu.mhz,
        caches: cpu
            .caches
            .iter()
            .map(|(name, size)| CacheJson {
                name: name.clone(),
                size_bytes: *size,
            })
            .collect(),
        aes: cpu.aes,
        virtualization,
    }
}

fn build_memory(mem: MemoryFacts) -> MemoryJson {
    MemoryJson {
        total_bytes: mem.total,
        used_bytes: mem.used,
        used_estimated: mem.estimated,
        swap_total_bytes: mem.swap_total,
        swap_used_bytes: mem.swap_used,
    }
}

fn build_storage(storage: &StorageFacts, kind: PlatformKind) -> StorageJson {
    let raid = storage
        .raid
        .as_ref()
        .map(|cards| cards.iter().map(build_raid).collect());
    let disks = if kind == PlatformKind::Container {
        Vec::new()
    } else {
        storage
            .disks
            .iter()
            .map(|d| build_disk(d, kind == PlatformKind::Dedicated))
            .collect()
    };
    let filesystems: Vec<FilesystemJson> = if kind == PlatformKind::Container {
        storage
            .filesystems
            .first()
            .into_iter()
            .map(|fs| build_filesystem(fs, storage.work_dir, true))
            .collect()
    } else {
        storage
            .filesystems
            .iter()
            .map(|fs| build_filesystem(fs, storage.work_dir, false))
            .collect()
    };
    StorageJson {
        raid,
        disks,
        filesystems,
    }
}

fn build_raid(card: &Card) -> RaidCardJson {
    RaidCardJson {
        address: card.address.clone(),
        name: card.name.clone(),
        driver: card.driver.clone(),
        disks: card
            .disks
            .iter()
            .map(|(name, att)| RaidDiskJson {
                name: name.clone(),
                attachment: match att {
                    Attachment::Array => "array",
                    Attachment::PassThrough => "pass_through",
                    Attachment::Unknown => "unknown",
                },
            })
            .collect(),
    }
}

fn build_disk(disk: &Disk, dedicated: bool) -> DiskJson {
    DiskJson {
        name: disk.name.clone(),
        interface: disk.interface,
        size_bytes: disk.size,
        rotational: dedicated.then_some(disk.rotational).flatten(),
        model: disk.model.clone(),
        uses: disk
            .uses
            .iter()
            .map(|item| match item {
                Use::Mount(mount) => mount.clone(),
                Use::Swap => "swap".to_owned(),
                Use::ZfsPool(pool) => format!("zfs:{pool}"),
            })
            .collect(),
    }
}

fn build_filesystem(
    fs: &Filesystem,
    work_dir: Option<(u32, u32)>,
    container: bool,
) -> FilesystemJson {
    let (total_bytes, used_bytes, available_bytes) = match fs.space {
        Some(space) if fs.zfs_pool.is_none() || container => {
            (Some(space.total), Some(space.used), Some(space.available))
        }
        Some(space) => (None, None, Some(space.available)),
        None => (None, None, None),
    };
    FilesystemJson {
        mount_point: fs.mount_point.clone(),
        fs_type: fs.fs_type.clone(),
        zfs_pool: fs.zfs_pool.clone(),
        disks: if container {
            Vec::new()
        } else {
            fs.disks.clone()
        },
        test_directory: work_dir.is_some_and(|dev| fs.devices.contains(&dev)),
        total_bytes,
        used_bytes,
        available_bytes,
    }
}

fn build_os(sys: &SystemFacts) -> OsJson {
    OsJson {
        distribution: sys.distribution.clone(),
        kernel: sys.kernel.clone(),
        uptime_seconds: sys.uptime.map(|seen| seen.value.as_secs()),
        uptime_from_host: sys.uptime.map(|seen| seen.host),
        load: sys.load.map(|seen| seen.value),
        load_from_host: sys.load.map(|seen| seen.host),
        congestion_control: sys.congestion_control.clone(),
        qdisc: sys.qdisc.clone(),
    }
}

fn build_card(card: &NetworkCard) -> CardJson {
    let bus = match &card.bus {
        Bus::Pci { .. } => "pci",
        Bus::Usb => "usb",
        Bus::Platform(_) => "platform",
        Bus::Other(_) => "other",
    };
    let (link_up, speed_mbps, duplex) = match card.link {
        Link::Up { speed, duplex } => (
            true,
            speed,
            duplex.map(|d| match d {
                Duplex::Full => "full",
                Duplex::Half => "half",
            }),
        ),
        Link::Down => (false, None, None),
    };
    CardJson {
        interface: card.interface.clone(),
        bus,
        model: card.model.clone(),
        driver: card.driver.clone(),
        link_up,
        speed_mbps,
        duplex,
        mtu: card.mtu,
        pcie_degraded: card.pcie.as_ref().is_some_and(Pcie::degraded),
        virtual_functions: card.virtual_functions,
        master: card.master.as_ref().map(|m| m.name.clone()),
        master_kind: card.master.as_ref().map(|m| match m.kind {
            MasterKind::Bridge => "bridge",
            MasterKind::Bond => "bond",
            MasterKind::Other => "other",
        }),
        default_ipv4: card.default_route.ipv4,
        default_ipv6: card.default_route.ipv6,
    }
}

fn build_family(family: &Family, name: &str, show_ip: bool) -> FamilyJson {
    let (public_address, source) = match &family.address {
        Address::Found { address, source } => {
            let shown = if show_ip {
                address.to_string()
            } else {
                network::mask(*address)
            };
            (Some(shown), Some(*source))
        }
        Address::NotAsked | Address::NotFound(_) => (None, None),
    };
    FamilyJson {
        reachable: family.reachable(),
        public_address,
        source,
        text: terminal::family_text(family, name, show_ip),
    }
}
