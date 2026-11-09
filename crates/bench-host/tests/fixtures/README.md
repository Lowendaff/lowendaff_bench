# Host fixtures

Each directory is a read-only capture of one real host, made with `capture.sh` and unpacked with `unpack.sh`:

- `root/` holds the captured files at their host paths. Tests read it through `bench_host::Root`.
- `truth/` holds the output of the host's own tools (`systemd-detect-virt`, `lscpu`, `lsblk`, `df`, `free`, `nproc`, `uname`, and `dmidecode` without its identifier lines). Tests use these values as expectations; lowendaff-bench itself never runs these tools (design P-11).
- `cpuid` exists only for x86 captures. It holds the hypervisor signature from CPUID leaf `0x4000_0000`, or `-` when the hypervisor bit is clear. A shell cannot read CPUID, so the value is taken from the "Hypervisor vendor" line of `truth/lscpu.txt` (`KVM` is the signature `KVMKVMKVM`).

| Fixture | Host | Captured | `systemd-detect-virt` |
|---------|------|----------|-----------------------|
| `kvm-debian13-x86_64` | KVM guest, Debian 13, kernel 6.12, 1 vCPU, one virtio disk | 2026-11-07, owner's VM | `kvm` |
| `dedicated-pve-x86_64` | Inspur server, 2 × Xeon Gold 6130, Proxmox VE on Debian 13 (kernel 7.0); system SSD with LVM, three SSDs in the ZFS pool `Volume1` | 2026-11-07, owner's server; hardware files 2026-11-09 | `none` |
| `docker-debian13-aarch64` | `debian:13-slim` in Docker Desktop on Apple silicon (linuxkit kernel 7.0, no DMI) | 2026-11-07 | not installed |
| `lxc-limits-debian13-aarch64` | as above, with `--memory 512m --cpus 1.5 -e container=lxc`: a container whose own cgroup carries its limits, which a Proxmox VE container's does not | 2026-11-07 | not installed |
| `rock3a-debian13-aarch64` | Radxa ROCK 3A board (Rockchip RK3568, 4 × Cortex-A55, 2 GB), Debian 13 (kernel 6.12); U-Boot with EFI and SMBIOS, a device tree, one NVMe SSD | 2026-11-09, owner's board | `none` |
| `lxc-pve-debian13-x86_64` | unprivileged Proxmox VE container on the dedicated server above (`cores 2`, `memory 1024`, `swap 512`, nesting on), from the Debian 13 template 13.6-1, root on a ZFS subvolume; lxcfs serves `/proc/meminfo`, `/proc/cpuinfo`, `/proc/swaps`, `/proc/uptime` and `/sys/devices/system/cpu`, while the container's cgroup files say `max`. Made for the capture and destroyed after it | 2026-11-10, owner's server | `lxc` |

## Adding a capture

```sh
ssh root@host sh -s < capture.sh > host.capture
./unpack.sh host.capture <name>
```

`capture.sh` writes nothing on the host. It never captures hostnames, addresses, serial numbers, WWNs or UUIDs. From the environment of PID 1 it keeps only the `container` variable, and from the udev database only the `ID_TYPE`, `ID_BUS`, `ID_MODEL_ENC`, `ID_FS_TYPE`, `ID_FS_LABEL` and `ID_ATA_ROTATION_RATE_RPM` keys. The SMBIOS table leaves the host only through `scrub_smbios`, which overwrites the serial numbers, asset tags and UUID of the types the program reads, and the whole of every other type, with placeholders of the same length; it leaves the table out if any serial number, asset tag or UUID that dmidecode reports is still in it. The BMC's GUID is never read.

The repository is public, so review every capture before committing it:

- Search it for hostnames, IP addresses and serial numbers.
- Trim mount points that reveal how the host is used.

Edits after capture:

- `dedicated-pve-x86_64`:
  - Trimmed the mount points and `df` lines under `/opt/CI`, `/var/lib/docker`, `/var/lib/containerd` and `/run/docker`, and the dataset `Volume1/docker`.
  - Trimmed the udev entries of the ZFS volumes (`zd*`), which carried partition labels of the guests' disks.
  - Added `ID_MODEL_ENC` to the udev entries of the four disks the same day, read from the same host; the key joined `capture.sh` after the first capture.
  - Added the hardware files of a second capture of the same server on 2026-11-09: the SMBIOS table and entry point, the SMBIOS lines of the EFI system table, ACPI's SPMI table, the `IPI0001` device, the BMC, EDAC, the storage controllers' PCI files, the `/sys/block` links, the used lines of `pci.ids`, `board_version`, `bios_date`, and `truth/dmidecode.txt`. The other files are from 2026-11-07.
- `kvm-debian13-x86_64`: trimmed Docker's overlay mounts.
- The two Docker captures were redone after `capture.sh` started reading the cgroup's memory usage.
