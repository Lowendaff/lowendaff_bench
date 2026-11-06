# Host fixtures

Each directory is a read-only capture of one real host, made with `capture.sh` and unpacked with `unpack.sh`:

- `root/` holds the captured files at their host paths. Tests read it through `bench_host::Root`.
- `truth/` holds the output of the host's own tools (`systemd-detect-virt`, `lscpu`, `lsblk`, `df`, `free`, `nproc`, `uname`). Tests use these values as expectations; lowendaff-bench itself never runs these tools (design P-11).
- `cpuid` exists only for x86 captures. It holds the hypervisor signature from CPUID leaf `0x4000_0000`, or `-` when the hypervisor bit is clear. A shell cannot read CPUID, so the value is taken from the "Hypervisor vendor" line of `truth/lscpu.txt` (`KVM` is the signature `KVMKVMKVM`).

| Fixture | Host | Captured | `systemd-detect-virt` |
|---------|------|----------|-----------------------|
| `kvm-debian13-x86_64` | KVM guest, Debian 13, kernel 6.12, 1 vCPU, one virtio disk | 2026-11-07, owner's VM | `kvm` |
| `dedicated-pve-x86_64` | Inspur server, 2 × Xeon Gold 6130, Proxmox VE on Debian 13 (kernel 7.0); system SSD with LVM, three SSDs in the ZFS pool `Volume1` | 2026-11-07, owner's server | `none` |
| `docker-debian13-aarch64` | `debian:13-slim` in Docker Desktop on Apple silicon (linuxkit kernel 7.0, no DMI) | 2026-11-07 | not installed |
| `lxc-limits-debian13-aarch64` | as above, with `--memory 512m --cpus 1.5 -e container=lxc`, standing in for LXC until a real LXC capture exists | 2026-11-07 | not installed |

## Adding a capture

```sh
ssh root@host sh -s < capture.sh > host.capture
./unpack.sh host.capture <name>
```

`capture.sh` writes nothing on the host. It never reads hostnames, addresses, serial numbers, WWNs or UUIDs. From the environment of PID 1 it keeps only the `container` variable, and from the udev database only the `ID_TYPE`, `ID_BUS`, `ID_FS_TYPE`, `ID_FS_LABEL` and `ID_ATA_ROTATION_RATE_RPM` keys.

The repository is public, so review every capture before committing it:

- Search it for hostnames, IP addresses and serial numbers.
- Trim mount points that reveal how the host is used.

Trimmed so far:

- `dedicated-pve-x86_64`: mount points and `df` lines under `/opt/CI`, `/var/lib/docker`, `/var/lib/containerd` and `/run/docker`, and the dataset `Volume1/docker`. Also the udev entries of the ZFS volumes (`zd*`), which carried partition labels of the guests' disks.
- `kvm-debian13-x86_64`: Docker's overlay mounts.
