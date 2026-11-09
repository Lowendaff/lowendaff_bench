//! Platform detection (design D-16), following the order systemd-detect-virt uses:
//! containers first, then virtual machines. Only files and CPUID are read.

use bench_api::ContainerKind;
use bench_api::Hypervisor;
use bench_api::Platform;

use crate::Root;

/// What CPUID reports about a hypervisor. Only x86 has CPUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpuid {
    /// The vendor signature from leaf `0x4000_0000`, such as `KVMKVMKVM`, when the
    /// hypervisor bit is set; `None` when it is clear.
    pub hypervisor: Option<String>,
}

/// Reads CPUID; `None` on architectures without it.
#[cfg(target_arch = "x86_64")]
#[must_use]
pub fn cpuid() -> Option<Cpuid> {
    use std::arch::x86_64::__cpuid;

    // Bit 31 of ECX in leaf 1 is reserved for hypervisors to announce themselves.
    if __cpuid(1).ecx & (1 << 31) == 0 {
        return Some(Cpuid { hypervisor: None });
    }
    let leaf = __cpuid(0x4000_0000);
    let mut signature = Vec::with_capacity(12);
    for register in [leaf.ebx, leaf.ecx, leaf.edx] {
        signature.extend_from_slice(&register.to_le_bytes());
    }
    let signature = String::from_utf8_lossy(&signature);
    Some(Cpuid {
        hypervisor: Some(signature.trim_end_matches('\0').to_owned()),
    })
}

/// Reads CPUID; `None` on architectures without it.
#[cfg(not(target_arch = "x86_64"))]
#[must_use]
pub fn cpuid() -> Option<Cpuid> {
    None
}

/// The platform of the host whose files are under `root`. `cpuid` is `None` where the
/// architecture has no CPUID.
#[must_use]
pub fn detect(root: &Root, cpuid: Option<&Cpuid>) -> Platform {
    if let Some(container) = container(root) {
        return Platform::Container(container);
    }
    match vm(root, cpuid) {
        Some(hypervisor) => Platform::Vm(hypervisor),
        None => Platform::Dedicated,
    }
}

fn container(root: &Root) -> Option<ContainerKind> {
    // `/proc/vz` exists inside and outside OpenVZ containers, `/proc/bc` only outside.
    if root.exists("/proc/vz") && !root.exists("/proc/bc") {
        return Some(ContainerKind::Openvz);
    }
    // Microsoft's documented way to detect WSL.
    if let Some(release) = root.read_line("/proc/sys/kernel/osrelease")
        && (release.contains("Microsoft") || release.contains("WSL"))
    {
        return Some(ContainerKind::Wsl);
    }
    // Container managers name themselves in these places, most reliable first.
    let named = match root
        .read_line("/run/host/container-manager")
        .or_else(|| root.read_line("/run/systemd/container"))
    {
        Some(name) => Some(name),
        None => match pid1_container(root) {
            // Set but empty means "not in a container".
            Some(name) if name.is_empty() => return None,
            name => name,
        },
    };
    if let Some(name) = named {
        return Some(match name.as_str() {
            // Some images hardcode `oci`, which names no manager.
            "oci" => container_files(root).unwrap_or(ContainerKind::Other),
            "lxc" | "lxc-libvirt" => ContainerKind::Lxc,
            "systemd-nspawn" => ContainerKind::SystemdNspawn,
            "docker" => ContainerKind::Docker,
            "podman" => ContainerKind::Podman,
            "wsl" => ContainerKind::Wsl,
            _ => ContainerKind::Other,
        });
    }
    // Checked last: `/.dockerenv` can end up in an image built for another manager.
    if let Some(kind) = container_files(root) {
        return Some(kind);
    }
    // The initial PID namespace has inode 0xEFFFFFFC since Linux 3.8.
    let in_pid_namespace = root
        .link("/proc/self/ns/pid")
        .is_some_and(|target| target != "pid:[4026531836]");
    in_pid_namespace.then_some(ContainerKind::Other)
}

/// The `container` variable in the environment of PID 1, if it is set.
fn pid1_container(root: &Root) -> Option<String> {
    let environ = root.read("/proc/1/environ")?;
    environ
        .split('\0')
        .find_map(|entry| entry.strip_prefix("container="))
        .map(str::to_owned)
}

fn container_files(root: &Root) -> Option<ContainerKind> {
    if root.exists("/run/.containerenv") {
        Some(ContainerKind::Podman)
    } else if root.exists("/.dockerenv") {
        Some(ContainerKind::Docker)
    } else {
        None
    }
}

fn vm(root: &Root, cpuid: Option<&Cpuid>) -> Option<Hypervisor> {
    let dmi = dmi(root);
    // These run on KVM or Xen internally but name themselves only in DMI.
    match dmi {
        Some(
            found @ (Hypervisor::VirtualBox
            | Hypervisor::Xen
            | Hypervisor::AmazonEc2
            | Hypervisor::Parallels),
        ) => return Some(found),
        // Without CPUID a Google VM cannot be told from Google bare metal; report a VM.
        Some(found @ Hypervisor::GoogleCompute) if cpuid.is_none() => return Some(found),
        _ => {}
    }

    // Xen PV guests show no CPUID hypervisor, so `/proc/xen` comes first. The control
    // domain (dom0) is the host itself, unless CPUID reveals an outer hypervisor.
    let xen_dom0 = root.exists("/proc/xen") && xen_dom0(root);
    if root.exists("/proc/xen") && !xen_dom0 {
        return Some(Hypervisor::Xen);
    }

    let mut hyperv = false;
    let mut other = false;
    if let Some(signature) = cpuid.and_then(|cpuid| cpuid.hypervisor.as_deref()) {
        match from_cpuid(signature) {
            // QEMU offers the Hyper-V signature for its enlightenments; decide last.
            Some(Hypervisor::HyperV) => hyperv = true,
            Some(Hypervisor::Kvm) if dmi == Some(Hypervisor::GoogleCompute) => {
                return Some(Hypervisor::GoogleCompute);
            }
            Some(found) => return Some(found),
            None => other = true,
        }
    }
    if xen_dom0 {
        return fallback(hyperv, other);
    }

    match dmi {
        Some(Hypervisor::Other) => other = true,
        // Google DMI without a KVM CPUID signature is Google bare metal.
        Some(Hypervisor::GoogleCompute) | None => {}
        Some(found) => return Some(found),
    }
    match root.read_line("/sys/hypervisor/type").as_deref() {
        Some("xen") => return Some(Hypervisor::Xen),
        Some(_) => other = true,
        None => {}
    }
    match device_tree(root) {
        Some(Hypervisor::Other) => other = true,
        Some(found) => return Some(found),
        None => {}
    }
    fallback(hyperv, other)
}

fn fallback(hyperv: bool, other: bool) -> Option<Hypervisor> {
    if hyperv {
        Some(Hypervisor::HyperV)
    } else if other {
        Some(Hypervisor::Other)
    } else {
        None
    }
}

fn from_cpuid(signature: &str) -> Option<Hypervisor> {
    match signature {
        "XenVMMXenVMM" => Some(Hypervisor::Xen),
        // "Linux KVM Hv" is KVM offering Hyper-V enlightenments.
        "KVMKVMKVM" | "Linux KVM Hv" => Some(Hypervisor::Kvm),
        "TCGTCGTCGTCG" => Some(Hypervisor::Qemu),
        "VMwareVMware" => Some(Hypervisor::Vmware),
        "Microsoft Hv" => Some(Hypervisor::HyperV),
        "bhyve bhyve " => Some(Hypervisor::Bhyve),
        _ => None,
    }
}

/// Vendor prefixes in DMI fields, as in systemd's table.
const DMI_VENDORS: [(&str, Hypervisor); 18] = [
    ("KVM", Hypervisor::Kvm),
    ("OpenStack", Hypervisor::Kvm),
    ("KubeVirt", Hypervisor::Kvm),
    ("Alibaba Cloud ECS", Hypervisor::Kvm),
    ("Amazon EC2", Hypervisor::AmazonEc2),
    ("QEMU", Hypervisor::Qemu),
    ("VMware", Hypervisor::Vmware),
    ("VMW", Hypervisor::Vmware),
    ("innotek GmbH", Hypervisor::VirtualBox),
    ("VirtualBox", Hypervisor::VirtualBox),
    ("Oracle Corporation", Hypervisor::VirtualBox),
    ("Xen", Hypervisor::Xen),
    ("Bochs", Hypervisor::Other),
    ("Parallels", Hypervisor::Parallels),
    ("BHYVE", Hypervisor::Bhyve),
    ("Hyper-V", Hypervisor::HyperV),
    ("Apple Virtualization", Hypervisor::Other),
    ("Google Compute Engine", Hypervisor::GoogleCompute),
];

fn dmi(root: &Root) -> Option<Hypervisor> {
    // `product_name` first, so "KVM" wins over a "QEMU" vendor.
    let fields = [
        "/sys/class/dmi/id/product_name",
        "/sys/class/dmi/id/sys_vendor",
        "/sys/class/dmi/id/board_vendor",
        "/sys/class/dmi/id/bios_vendor",
        "/sys/class/dmi/id/product_version",
    ];
    let found = fields.iter().find_map(|field| {
        let value = root.read_line(field)?;
        DMI_VENDORS
            .iter()
            .find(|(prefix, _)| value.starts_with(prefix))
            .map(|(_, hypervisor)| *hypervisor)
    })?;
    if found == Hypervisor::AmazonEc2 && amazon_metal(root) {
        return None;
    }
    Some(found)
}

/// EC2 bare-metal instance types end in `.metal`, optionally followed by `-<size>`.
fn amazon_metal(root: &Root) -> bool {
    root.read_line("/sys/class/dmi/id/product_name")
        .and_then(|name| name.split_once(".metal").map(|(_, rest)| rest.to_owned()))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
}

fn xen_dom0(root: &Root) -> bool {
    // Feature bit 11 (XENFEAT_dom0) marks the hardware domain.
    if let Some(features) = root.read_line("/sys/hypervisor/properties/features")
        && let Ok(features) = u64::from_str_radix(&features, 16)
    {
        return features & (1 << 11) != 0;
    }
    root.read_line("/proc/xen/capabilities")
        .is_some_and(|capabilities| capabilities.split(',').any(|cap| cap == "control_d"))
}

fn device_tree(root: &Root) -> Option<Hypervisor> {
    let hypervisor = crate::device_tree_strings(root, "/proc/device-tree/hypervisor/compatible");
    if let Some(compatible) = hypervisor.first() {
        return Some(if compatible == "linux,kvm" {
            Hypervisor::Kvm
        } else if compatible.contains("xen") {
            Hypervisor::Xen
        } else if compatible.contains("vmware") {
            Hypervisor::Vmware
        } else {
            Hypervisor::Other
        });
    }
    if root
        .list("/proc/device-tree")
        .iter()
        .any(|name| name.contains("fw-cfg"))
    {
        return Some(Hypervisor::Qemu);
    }
    let compatible = crate::device_tree_strings(root, "/proc/device-tree/compatible");
    match compatible.first().map(String::as_str) {
        Some("qemu,pseries") => Some(Hypervisor::Qemu),
        Some("linux,dummy-virt") => Some(Hypervisor::Other),
        _ => None,
    }
}
