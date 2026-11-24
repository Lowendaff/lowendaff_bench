# Host Captures _(fixtures)_

> Read-only scrubbed Linux host captures for testing `bench-host` hardware and platform detection.

This directory (`crates/bench-host/tests/fixtures/`) stores recorded filesystem trees and reference
tool outputs for the `bench-host` crate inside `lowendaff_bench`.

## Table of Contents

- [Security](#security)
- [Background](#background)
- [Usage](#usage)
- [Captured Hosts](#captured-hosts)
- [Post-Capture Edits](#post-capture-edits)
- [Contributing](#contributing)
- [License](#license)

## Security

`capture.sh` writes nothing on the target host and omits hostnames, IP addresses, serial numbers,
WWNs, and UUIDs. From PID 1's environment it keeps only `container`, and from the udev database
only `ID_TYPE`, `ID_BUS`, `ID_MODEL_ENC`, `ID_FS_TYPE`, `ID_FS_LABEL`, and
`ID_ATA_ROTATION_RATE_RPM`. SMBIOS tables pass through `scrub_smbios`, which overwrites serial
numbers, asset tags, and UUIDs with equal-length placeholders and drops the table if any identifier
reported by `dmidecode` remains. BMC GUIDs are never read.

Because this repository is public, inspect every new capture before committing:

- Search the capture for hostnames, IP addresses, and serial numbers.
- Trim mount points or guest interface names that reveal private host workloads.

## Background

Each subdirectory is a read-only snapshot of one real machine or container, captured with
`capture.sh` and unpacked with `unpack.sh`:

- `root/` holds captured files at their original Linux paths, read in tests via `bench_host::Root`.
- `truth/` holds outputs of standard host utilities (`systemd-detect-virt`, `lscpu`, `lsblk`, `df`,
  `free`, `nproc`, `uname`, and scrubbed `dmidecode`) used solely as test expectations;
  `lowendaff-bench` never invokes those utilities at runtime.
- `cpuid` exists on x86 captures and records the hypervisor signature from CPUID leaf `0x4000_0000`
  (or `-` when clear), derived from `truth/lscpu.txt` (`KVM` corresponds to `KVMKVMKVM`).

## Usage

Capture a target Linux host over SSH and unpack it locally:

```sh
ssh root@host sh -s < capture.sh > host.capture
./unpack.sh host.capture <name>
```

## Captured Hosts

| Fixture | Host | Captured | `systemd-detect-virt` |
|---|---|---|---|
| `kvm-debian13-x86_64` | KVM guest, Debian 13, kernel 6.12, 1 vCPU, one virtio disk | 2026-11-07, owner's VM | `kvm` |
| `dedicated-pve-x86_64` | Inspur server, 2 x Xeon Gold 6130, Proxmox VE on Debian 13 (kernel 7.0); system SSD with LVM, three SSDs in ZFS pool `Volume1` | 2026-11-07, owner's server; hardware files 2026-11-09 | `none` |
| `docker-debian13-aarch64` | `debian:13-slim` in Docker Desktop on Apple silicon (linuxkit kernel 7.0, no DMI) | 2026-11-07 | not installed |
| `lxc-limits-debian13-aarch64` | As above, with `--memory 512m --cpus 1.5 -e container=lxc`: a container whose own cgroup carries its limits | 2026-11-07 | not installed |
| `rock3a-debian13-aarch64` | Radxa ROCK 3A board (Rockchip RK3568, 4 x Cortex-A55, 2 GB), Debian 13 (kernel 6.12); U-Boot with EFI and SMBIOS, device tree, one NVMe SSD | 2026-11-09, owner's board | `none` |
| `lxc-pve-debian13-x86_64` | Unprivileged Proxmox VE container on the dedicated server above (`cores 2`, `memory 1024`, `swap 512`, nesting on), Debian 13 template 13.6-1, root on ZFS subvolume; lxcfs serves `/proc/{meminfo,cpuinfo,swaps,uptime}` and `/sys/devices/system/cpu` while cgroup files say `max` | 2026-11-10, owner's server | `lxc` |
| `dedicated-ubuntu24-x86_64` | Inspur i24 server, 2 x Xeon Platinum 8259CL, Ubuntu 24.04 (kernel 6.8), one NVMe SSD; Intel X722 10 GbE controller with one port up at 10 Gbit/s | 2026-11-19, owner's fourth host | `none` |

## Post-Capture Edits

- `dedicated-pve-x86_64`:
  - Trimmed mount points and `df` lines under `/opt/CI`, `/var/lib/docker`, `/var/lib/containerd`,
    and `/run/docker`, plus dataset `Volume1/docker`.
  - Trimmed udev entries of ZFS volumes (`zd*`) carrying guest disk partition labels.
  - Added `ID_MODEL_ENC` to udev entries of the four disks from the same host on 2026-11-07, and
    added SMBIOS, EFI, ACPI SPMI, `IPI0001`, BMC, EDAC, PCI storage controller, `/sys/block`,
    `pci.ids`, `board_version`, `bios_date`, and `truth/dmidecode.txt` captures on 2026-11-09.
- `kvm-debian13-x86_64`: Trimmed Docker overlay mounts.
- Both Docker captures were regenerated after `capture.sh` began reading cgroup memory usage.
- `kvm-debian13-x86_64`, `dedicated-pve-x86_64`, and `rock3a-debian13-aarch64`: Added per-CPU cache
  directories, NUMA `cpulist`/`meminfo`, and `/proc/self/status` on 2026-11-10, plus NIC sysfs,
  udev keys, and default-route captures on 2026-11-19 (alongside `docker-debian13-aarch64`),
  keeping loopback, physical/backed interfaces, and bridges while trimming guest/container `veth`,
  `tap`, firewall, and tunnel interfaces.

## Contributing

Pull requests adding scrubbed host captures are welcome on
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues). Follow the scrubbing rules
above and the conventions in [`CONTRIBUTING.md`](../../../../CONTRIBUTING.md).

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
