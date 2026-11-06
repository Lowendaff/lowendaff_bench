//! Platform detection against captured hosts (tests/fixtures/README.md) and against small
//! synthetic hosts for the rules no capture covers.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use bench_api::ContainerKind;
use bench_api::Hypervisor;
use bench_api::Platform;
use bench_host::Root;
use bench_host::platform;
use bench_host::platform::Cpuid;
use tempfile::TempDir;

/// A captured host: its files, and its CPUID as recorded in the fixture's `cpuid` file.
fn fixture(name: &str) -> (Root, Option<Cpuid>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let cpuid = fs::read_to_string(dir.join("cpuid"))
        .ok()
        .map(|text| cpuid(text.trim()));
    (Root::new(dir.join("root")), cpuid)
}

/// `signature` is the hypervisor signature, or `-` when the hypervisor bit is clear.
fn cpuid(signature: &str) -> Cpuid {
    Cpuid {
        hypervisor: (signature != "-").then(|| signature.to_owned()),
    }
}

fn detect_fixture(name: &str) -> Platform {
    let (root, cpuid) = fixture(name);
    platform::detect(&root, cpuid.as_ref())
}

/// Files (host path, contents) and empty directories of a synthetic host.
type Layout = (
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
);

fn host((files, dirs): Layout) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    for (path, contents) in files {
        let path = dir.path().join(path.trim_start_matches('/'));
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, contents).expect("write file");
    }
    for path in dirs {
        fs::create_dir_all(dir.path().join(path.trim_start_matches('/'))).expect("create dir");
    }
    dir
}

/// `signature` as for [`cpuid`], or `None` for an architecture without CPUID.
fn detect(dir: &TempDir, signature: Option<&str>) -> Platform {
    let cpuid = signature.map(cpuid);
    platform::detect(&Root::new(dir.path()), cpuid.as_ref())
}

#[test]
fn captured_hosts_match_systemd_detect_virt() {
    // systemd-detect-virt printed "kvm" and "none" on these hosts at capture time.
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
}

#[test]
fn without_cpuid_a_kvm_guest_looks_like_plain_qemu() {
    // DMI on this guest only says "QEMU"; CPUID is what tells KVM from software emulation.
    let (root, _) = fixture("kvm-debian13-x86_64");
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
    symlink("pid:[4026533244]", dir.path().join("proc/self/ns/pid")).expect("symlink");
    assert_eq!(
        detect(&dir, Some("-")),
        Platform::Container(ContainerKind::Other)
    );

    let dir = host((&[], &["/proc/self/ns"]));
    symlink("pid:[4026531836]", dir.path().join("proc/self/ns/pid")).expect("symlink");
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
