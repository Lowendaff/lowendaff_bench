//! Platform detection against captured hosts (tests/fixtures/README.md) and against small
//! synthetic hosts for the rules no capture covers.

mod common;

use bench_host::Root;
use bench_host::platform;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use common::Layout;
use tempfile::TempDir;

fn detect_fixture(name: &str) -> Platform {
    platform::detect(&common::fixture(name), common::fixture_cpuid(name).as_ref())
}

fn host(layout: Layout) -> TempDir {
    common::host(layout)
}

/// `signature` is the hypervisor signature, `-` for a clear hypervisor bit, or `None` for
/// an architecture without CPUID.
fn detect(dir: &TempDir, signature: Option<&str>) -> Platform {
    let cpuid = signature.map(common::cpuid);
    platform::detect(&Root::new(dir.path()), cpuid.as_ref())
}

#[test]
fn captured_hosts_match_systemd_detect_virt() {
    // systemd-detect-virt printed "kvm", "none" and "lxc" on these hosts at capture time.
    assert_eq!(
        detect_fixture("kvm-debian13-x86_64"),
        Platform::Vm(Hypervisor::Kvm)
    );
    assert_eq!(detect_fixture("dedicated-pve-x86_64"), Platform::Dedicated);
    assert_eq!(
        detect_fixture("docker-debian13-aarch64"),
        Platform::Container(ContainerKind::Docker)
    );
    // PID 1 carries container=lxc, which wins over the /.dockerenv of the Docker test bed.
    assert_eq!(
        detect_fixture("lxc-limits-debian13-aarch64"),
        Platform::Container(ContainerKind::Lxc)
    );
    // A container on Proxmox VE, captured on the dedicated server.
    assert_eq!(
        detect_fixture("lxc-pve-debian13-x86_64"),
        Platform::Container(ContainerKind::Lxc)
    );
}

#[test]
fn without_cpuid_a_kvm_guest_looks_like_plain_qemu() {
    // DMI on this guest only says "QEMU"; CPUID is what tells KVM from software emulation.
    let root = common::fixture("kvm-debian13-x86_64");
    assert_eq!(
        platform::detect(&root, None),
        Platform::Vm(Hypervisor::Qemu)
    );
}

#[test]
fn containers_are_recognized_by_their_markers() {
    let cases: [(Layout, Platform); 7] = [
        (
            (&[], &["/proc/vz"]),
            Platform::Container(ContainerKind::Openvz),
        ),
        (
            (
                &[(
                    "/proc/sys/kernel/osrelease",
                    "5.15.167.4-microsoft-standard-WSL2\n",
                )],
                &[],
            ),
            Platform::Container(ContainerKind::Wsl),
        ),
        (
            (&[("/run/systemd/container", "systemd-nspawn\n")], &[]),
            Platform::Container(ContainerKind::SystemdNspawn),
        ),
        (
            (&[("/run/host/container-manager", "podman\n")], &[]),
            Platform::Container(ContainerKind::Podman),
        ),
        (
            (
                &[
                    ("/proc/1/environ", "PATH=/bin\0container=oci\0"),
                    ("/run/.containerenv", ""),
                ],
                &[],
            ),
            Platform::Container(ContainerKind::Podman),
        ),
        (
            (&[("/proc/1/environ", "container=oci\0")], &[]),
            Platform::Container(ContainerKind::Other),
        ),
        (
            (&[("/proc/1/environ", "container=rkt\0")], &[]),
            Platform::Container(ContainerKind::Other),
        ),
    ];
    for (layout, expected) in cases {
        assert_eq!(detect(&host(layout), None), expected, "{layout:?}");
    }
}

#[test]
fn an_openvz_host_and_an_empty_container_variable_are_not_containers() {
    // `/proc/bc` exists only on the OpenVZ host node.
    assert_eq!(
        detect(&host((&[], &["/proc/vz", "/proc/bc"])), Some("-")),
        Platform::Dedicated
    );
    // An empty `container` variable means "not a container", even with a stray marker.
    let dir = host((
        &[("/proc/1/environ", "container=\0"), ("/.dockerenv", "")],
        &[],
    ));
    assert_eq!(detect(&dir, Some("-")), Platform::Dedicated);
}

#[test]
fn an_unknown_pid_namespace_is_some_container() {
    let dir = host((&[], &["/proc/self/ns"]));
    common::link(&dir, "/proc/self/ns/pid", "pid:[4026533244]");
    assert_eq!(
        detect(&dir, Some("-")),
        Platform::Container(ContainerKind::Other)
    );

    let dir = host((&[], &["/proc/self/ns"]));
    common::link(&dir, "/proc/self/ns/pid", "pid:[4026531836]");
    assert_eq!(detect(&dir, Some("-")), Platform::Dedicated);
}

#[test]
fn a_container_inside_a_virtual_machine_is_a_container() {
    // Docker on a KVM guest, as on CI runners: the container is what the tests run in.
    let dir = host((&[("/.dockerenv", "")], &[]));
    assert_eq!(
        detect(&dir, Some("KVMKVMKVM")),
        Platform::Container(ContainerKind::Docker)
    );
}

#[test]
fn virtual_machines_are_recognized_by_cpuid_and_dmi() {
    let cases: [(Layout, Option<&str>, Platform); 11] = [
        (
            (&[], &[]),
            Some("VMwareVMware"),
            Platform::Vm(Hypervisor::Vmware),
        ),
        (
            (&[], &[]),
            Some("TCGTCGTCGTCG"),
            Platform::Vm(Hypervisor::Qemu),
        ),
        (
            (&[], &[]),
            Some("SomeNewHv"),
            Platform::Vm(Hypervisor::Other),
        ),
        // KVM offering Hyper-V enlightenments shows Microsoft's signature; DMI decides.
        (
            (&[("/sys/class/dmi/id/sys_vendor", "QEMU\n")], &[]),
            Some("Microsoft Hv"),
            Platform::Vm(Hypervisor::Qemu),
        ),
        (
            (
                &[
                    ("/sys/class/dmi/id/sys_vendor", "Microsoft Corporation\n"),
                    (
                        "/sys/class/dmi/id/product_version",
                        "Hyper-V UEFI Release v4.1\n",
                    ),
                ],
                &[],
            ),
            Some("Microsoft Hv"),
            Platform::Vm(Hypervisor::HyperV),
        ),
        (
            (&[("/sys/class/dmi/id/sys_vendor", "innotek GmbH\n")], &[]),
            Some("KVMKVMKVM"),
            Platform::Vm(Hypervisor::VirtualBox),
        ),
        (
            (
                &[
                    ("/sys/class/dmi/id/sys_vendor", "Amazon EC2\n"),
                    ("/sys/class/dmi/id/product_name", "m5.large\n"),
                ],
                &[],
            ),
            Some("KVMKVMKVM"),
            Platform::Vm(Hypervisor::AmazonEc2),
        ),
        (
            (
                &[
                    ("/sys/class/dmi/id/sys_vendor", "Amazon EC2\n"),
                    ("/sys/class/dmi/id/product_name", "c5.metal\n"),
                ],
                &[],
            ),
            Some("-"),
            Platform::Dedicated,
        ),
        (
            (
                &[("/sys/class/dmi/id/product_name", "Google Compute Engine\n")],
                &[],
            ),
            Some("KVMKVMKVM"),
            Platform::Vm(Hypervisor::GoogleCompute),
        ),
        (
            (
                &[("/sys/class/dmi/id/product_name", "Google Compute Engine\n")],
                &[],
            ),
            Some("-"),
            Platform::Dedicated,
        ),
        (
            (
                &[
                    ("/sys/class/dmi/id/product_name", "KVM Virtual Machine\n"),
                    ("/sys/class/dmi/id/sys_vendor", "QEMU\n"),
                ],
                &[],
            ),
            None,
            Platform::Vm(Hypervisor::Kvm),
        ),
    ];
    for (layout, cpuid, expected) in cases {
        assert_eq!(
            detect(&host(layout), cpuid),
            expected,
            "{layout:?} {cpuid:?}"
        );
    }
}

#[test]
fn xen_guests_and_the_xen_host_are_told_apart() {
    let guest = host((&[("/proc/xen/capabilities", "\n")], &[]));
    assert_eq!(detect(&guest, None), Platform::Vm(Hypervisor::Xen));
    let control_domain = host((&[("/proc/xen/capabilities", "control_d\n")], &[]));
    assert_eq!(detect(&control_domain, Some("-")), Platform::Dedicated);
    let by_feature_bit = host((
        &[("/sys/hypervisor/properties/features", "00000801\n")],
        &["/proc/xen"],
    ));
    assert_eq!(detect(&by_feature_bit, Some("-")), Platform::Dedicated);
}

#[test]
fn arm_guests_are_recognized_by_the_device_tree() {
    let kvm = host((
        &[("/proc/device-tree/hypervisor/compatible", "linux,kvm\0")],
        &[],
    ));
    assert_eq!(detect(&kvm, None), Platform::Vm(Hypervisor::Kvm));
    let qemu = host((&[], &["/proc/device-tree/fw-cfg@9020000"]));
    assert_eq!(detect(&qemu, None), Platform::Vm(Hypervisor::Qemu));
    let generic = host((
        &[("/proc/device-tree/compatible", "linux,dummy-virt\0")],
        &[],
    ));
    assert_eq!(detect(&generic, None), Platform::Vm(Hypervisor::Other));
    let board = host((
        &[(
            "/proc/device-tree/compatible",
            "raspberrypi,4-model-b\0brcm,bcm2711\0",
        )],
        &[],
    ));
    assert_eq!(detect(&board, None), Platform::Dedicated);
}

#[test]
fn cpuid_exists_exactly_on_x86_64() {
    assert_eq!(
        platform::cpuid().is_some(),
        cfg!(target_arch = "x86_64"),
        "the platform probe reads CPUID only where the instruction exists"
    );
}

#[test]
fn platforms_are_named_in_chinese() {
    let names = [
        (Platform::Dedicated, "未检测到虚拟化"),
        (Platform::Vm(Hypervisor::Kvm), "KVM 虚拟机"),
        (Platform::Vm(Hypervisor::Qemu), "QEMU 虚拟机（纯软件模拟）"),
        (Platform::Vm(Hypervisor::Xen), "Xen 虚拟机"),
        (Platform::Vm(Hypervisor::Vmware), "VMware 虚拟机"),
        (Platform::Vm(Hypervisor::HyperV), "Hyper-V 虚拟机"),
        (Platform::Vm(Hypervisor::VirtualBox), "VirtualBox 虚拟机"),
        (Platform::Vm(Hypervisor::Parallels), "Parallels 虚拟机"),
        (Platform::Vm(Hypervisor::AmazonEc2), "Amazon EC2 虚拟机"),
        (
            Platform::Vm(Hypervisor::GoogleCompute),
            "Google Compute Engine 虚拟机",
        ),
        (Platform::Vm(Hypervisor::Bhyve), "bhyve 虚拟机"),
        (Platform::Vm(Hypervisor::Other), "虚拟机（类型未知）"),
        (Platform::Container(ContainerKind::Lxc), "LXC 容器"),
        (Platform::Container(ContainerKind::Openvz), "OpenVZ 容器"),
        (Platform::Container(ContainerKind::Docker), "Docker 容器"),
        (Platform::Container(ContainerKind::Podman), "Podman 容器"),
        (
            Platform::Container(ContainerKind::SystemdNspawn),
            "systemd-nspawn 容器",
        ),
        (Platform::Container(ContainerKind::Wsl), "WSL"),
        (
            Platform::Container(ContainerKind::Other),
            "容器（类型未知）",
        ),
    ];
    for (platform, name) in names {
        assert_eq!(platform.to_string(), name);
    }
    assert_eq!(Hypervisor::Other.to_string(), "未知");
    assert_eq!(ContainerKind::Other.to_string(), "未知");
    let kinds = [
        (PlatformKind::Dedicated, "独立服务器"),
        (PlatformKind::Vm, "虚拟机"),
        (PlatformKind::Container, "容器"),
    ];
    for (kind, name) in kinds {
        assert_eq!(kind.to_string(), name);
    }
    assert_eq!(Platform::default(), Platform::Dedicated);
}
