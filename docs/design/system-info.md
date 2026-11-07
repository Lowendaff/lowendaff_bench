# System Information Test Items (系统基础信息)

- Status: **Accepted** 2026-11-07. The owner decided S-01..S-08 (§2).
- Scope: the first test items. One plugin per kind of platform in `plugins/system`. They read the host through `crates/bench-host` and rely on the orchestrator's platform detection (plugin system design, D-16).

## 1. Goals and non-goals

Goals:

1. Show the server's basic facts in Chinese: CPU, memory, disks and filesystems, distribution, kernel, uptime, load, TCP congestion control and IPv4/IPv6 reachability.
2. Work on every Linux distribution the binary runs on (§4).
3. Never run a host program (design P-11). Every value comes from `/proc`, `/sys`, `/etc`, `/run/udev/data` or a system call (`statvfs`, `stat`, CPUID).
4. Degrade per field. A value that cannot be read shows a reason (不可用); the rest of the section still prints.

Non-goals:

- IP address, ASN, location and NAT type, which belong to later network items.
- Hostname, serial numbers and UUIDs, which are private.
- Time zone, virtio balloon driver and KSM (S-02).

## 2. Owner's decisions

| ID | Question | Decision |
|----|----------|----------|
| S-01 | Name of the item | Ids `system.dedicated`, `system.vm`, `system.container`; title 系统基础信息; crate `plugins/system` (package `plugin-system`). |
| S-02 | Fields beyond the core set | Add IPv4/IPv6 reachability. No balloon driver, KSM or time zone. |
| S-03 | Disks | Two tables. **硬盘** lists disks: device, interface, size, medium (dedicated servers only), model and usage. **文件系统** lists filesystems: mount point, type, size, used, available and disks. The system disk and the test directory are marked. Rationale: providers split system and data disks, often SSD and HDD. |
| S-04 | Text when no virtualization is found | 未检测到虚拟化, not 独立服务器: finding nothing is no proof of bare metal. |
| S-05 | Distribution coverage in CI | Run the item in images of Debian 11/13, Ubuntu 20.04/24.04, CentOS 7, Rocky 9, AlmaLinux 8, Fedora 45, Alpine 3.22, Arch (x86_64 only), openSUSE Leap 16.0, Amazon Linux 2023 and Oracle Linux 9, on both architectures (`scripts/distros.sh`). Images are pinned by digest and come from AWS ECR Public, except openSUSE, which publishes on its own registry. The step belongs to the Linux CI jobs, which are required checks. |
| S-06 | Fixtures | The owner's KVM guest and dedicated server were sampled read-only. The captures may be published after trimming (`crates/bench-host/tests/fixtures/README.md`). |
| S-07 | One environment-aware plugin, or one plugin per environment | **One plugin per platform kind**, with the shared parts extracted as far as possible. LXC is in scope and tested with Docker first. |
| S-08 | Plugins that do not apply here | One 本机不适用 line in the header. Titles that run here in another variant are left out. |

## 3. Fields

All three variants print the section in this order. The table gives the source of each field; when the source is missing, the field shows 不可用.

| Label | Kind | Source, in fallback order | Notes |
|-------|------|---------------------------|-------|
| CPU 型号 | fact | x86: `model name` in `/proc/cpuinfo`. ARM: `CPU implementer` and `CPU part`, decoded with util-linux's table (ARMv8+ parts only). | Unknown ARM parts show their codes. |
| CPU 核心 | fact | `/sys/devices/system/cpu/online`; else `processor` entries. Topology from `cpu*/topology`. | Dedicated: "64 线程（2 颗 CPU，共 32 核）". VM: "4 个". Container: cpuset count, with "（限额 1.5 个）" when the CPU quota is lower. |
| CPU 频率 | metric (MHz) | cpufreq `cpuinfo_max_freq`; else x86 `cpu MHz`. | Usually missing on ARM VMs. |
| CPU 缓存 | fact | `cpu0/cache/index*`; else x86 `cache size`. | "L1d 32 KiB / L1i 32 KiB / L2 1 MiB / L3 22 MiB". |
| AES 指令 | fact | `aes` in x86 `flags` or ARM `Features`. | |
| 硬件虚拟化 | fact | `vmx` or `svm` in x86 `flags`. | Not in containers. 不适用 on ARM. |
| 内存总量, 内存已用 | metrics | `/proc/meminfo`: used = `MemTotal − MemAvailable`. Kernels before 3.14 use `MemFree + Buffers + Cached` instead, and the label says （估算）. | Containers: the cgroup limit and usage, when the limit is lower. |
| Swap 总量, Swap 已用 | metrics | `/proc/meminfo`. | "Swap：无" without swap. |
| 硬盘 | table | §6 | Not in containers: they see the host's disks. |
| 文件系统 | table | §6 | Containers show their root filesystem only. |
| 系统 | fact | os-release(5): `/etc/os-release`, then `/usr/lib/os-release`; `PRETTY_NAME`, else `NAME VERSION`. Then `/etc/lsb-release`, `/etc/redhat-release`, `/etc/centos-release`, `/etc/system-release`, `/etc/debian_version`, `/etc/alpine-release`. | Quoting and escapes as os-release(5) defines them. |
| 内核 | fact | `/proc/sys/kernel/osrelease` | Same as `uname -r`. |
| 架构 | fact | `Env.arch` | |
| 运行时间 | fact | `/proc/uptime` | "3 天 4 小时 12 分钟". |
| 负载 | fact | `/proc/loadavg` | |
| TCP 拥塞控制 | fact | `/proc/sys/net/ipv4/tcp_congestion_control`, plus `/proc/sys/net/core/default_qdisc` when present. | "bbr（队列 fq）". |
| IPv4 / IPv6 | fact | `Env.ipv4`, `Env.ipv6` from the orchestrator's probe. | |

The platform itself is printed once, in the header (运行环境), by the orchestrator.

## 4. Compatibility across distributions

- **The binary.** A static musl build has no glibc dependency. Rust's standard library needs Linux 3.2 or later on x86_64 and 4.1 or later on aarch64 ([Rust blog, 2022-08-01](https://blog.rust-lang.org/2022/08/01/Increasing-glibc-kernel-requirements/); [platform support](https://doc.rust-lang.org/rustc/platform-support.html)). CentOS 7 (3.10) qualifies; CentOS 6 and OpenVZ 6 (2.6.32) do not.
- **No command output.** Distributions differ in their tools (`lscpu`, `free`, `df`), versions and languages. The kernel interfaces read here differ only by kernel version and configuration.
- **Distribution name.** os-release(5) with fallbacks for the files that predate it (§3).
- **Kernel differences.** Each field has its own fallback:
  - a missing `MemAvailable`;
  - cpufreq and cache sizes missing in VMs (the capture from Docker on Apple silicon has cache directories without sizes);
  - DMI missing on device-tree boards;
  - `default_qdisc` missing inside network namespaces.
- **Containers.** LXC with lxcfs shows container values in `/proc`. Docker without lxcfs shows the host's, so containers read their cgroup (v2, else v1) for memory and CPU limits.
- **Architectures.** x86_64 and aarch64, the release architectures.

## 5. Platform detection

The orchestrator detects the platform once (`crates/bench-host/src/platform.rs`, plugin system D-16). It follows the order of systemd-detect-virt, reimplemented without running it:

1. **Containers.** In order:
   - OpenVZ: `/proc/vz` without `/proc/bc`.
   - WSL: `osrelease`.
   - `/run/host/container-manager`, `/run/systemd/container`.
   - PID 1's `container` variable.
   - `/run/.containerenv`, then `/.dockerenv`.
   - A PID namespace other than the initial one.
2. **Virtual machines.** In order:
   - DMI vendors that win outright (VirtualBox, Xen, Amazon EC2, Parallels).
   - `/proc/xen`, telling the control domain from guests.
   - The CPUID hypervisor signature (x86). This tells KVM from software-only QEMU, which DMI cannot: the owner's KVM guest only says "QEMU" in DMI.
   - DMI again, `/sys/hypervisor/type`, then the device tree (ARM).
3. **Otherwise:** 未检测到虚拟化, which runs the dedicated-server variant.

`core::arch::x86_64::__cpuid` is a safe function in Rust 1.99, so `unsafe_code = "forbid"` stays.

## 6. Storage

- **Disks.** Whole devices in `/sys/class/block`, without:
  - memory, loop, optical, floppy and network block devices;
  - ZFS volumes (`zd*`, the guests' disks on a hypervisor);
  - the logical layers of LVM, RAID and caching (`dm-*`, `md*`, `drbd*`, `bcache*`).
- **Interface.**
  - From the device name: NVMe, virtio, Xen, MMC.
  - Otherwise from udev's `ID_BUS`, or from the sysfs vendor `ATA` when there is no udev.
- **Medium.** Dedicated servers only (S-03). udev's ATA rotation rate, else the kernel's rotational flag. In a VM the flag is whatever the hypervisor claims; the owner's virtio disk reports "rotational".
- **Model.** udev's `ID_MODEL_ENC`, else sysfs `device/model`, which libata truncates to 16 characters.
- **Usage.** Each disk-backed filesystem from mountinfo, and each swap partition, is traced to the disks below it:
  - partitions to their disk, by the kernel's naming rule (`sda1`, `nvme0n1p1`);
  - LVM and RAID devices through `slaves`;
  - ZFS pool members through udev's `zfs_member` entries and their pool label.
- **Filesystems.**
  - Only the disk-backed types (ext2/3/4, xfs, btrfs, zfs, f2fs, jfs, reiserfs, bcachefs, nilfs2, vfat, exfat, ntfs3, simfs), and nothing under `/boot`.
  - One row per device: btrfs subvolumes and bind mounts merge, keeping the shortest mount point. ZFS has one row per pool.
  - Sizes come from `statvfs`, computed as `df` does.
  - A ZFS pool shows only its available space, because its datasets and volumes share it.
- **Test directory.** The row whose device holds the run directory is marked （测试目录）. Disk tests will write there; `--work-dir` moves it, for example to a data disk.

## 7. Testing

- **`crates/bench-host/tests`** runs the readers against captured hosts in `tests/fixtures`:
  - the owner's KVM guest and dedicated server;
  - Docker on Apple silicon;
  - Docker standing in for LXC.

  Expectations come from the host's own tools at capture time (`truth/`). Small synthetic hosts cover rules no capture shows.
- **`plugins/system/tests`** checks the records each variant prints for those captures.
- **CI, distribution matrix (S-05).** The static binary runs `system.container` inside each image on both architectures. The 系统 line must match the image's os-release, and no field may fail. All containers share the runner's kernel, so the matrix covers distribution files; kernels and platforms are covered by the fixtures.
- **CI runners.** The smoke test runs every test item on the x86_64 and aarch64 runners themselves, which are virtual machines. It fails when an item fails or times out, and prints the output, including 运行环境, to the CI log. On 2026-11-07 both runners reported Hyper-V 虚拟机 and completed the item:
  - x86_64: AMD EPYC 9V74, 4 vCPUs, AMD-V exposed to the guest, a 150 GiB SCSI "Virtual Disk", Ubuntu 24.04.5.
  - aarch64: ARM Neoverse-N2 (from the part code), Hyper-V recognized through DMI because ARM has no CPUID; CPU 频率 showed 不可用, since the guest has no cpufreq.
- **Mutation check.** Each mutation was applied to a copy of the repository, and the tests had to fail. All 50 were killed:

  | Area | Mutations | Examples |
  |------|-----------|----------|
  | Platform detection | 9 | containers checked after VMs; an OpenVZ host taken for a container; a non-initial PID namespace ignored; the Hyper-V signature trusted at once; EC2 `.metal` ignored; Xen dom0 ignored; CPUID ignored; an empty `container=` ignored; DMI fields read in another order |
  | Plan, registry check, header | 5 | every plugin kept; variants' titles listed as not applicable; an empty `platforms` read as no platform; a variant for every platform not seen as overlapping; the 本机不适用 line always printed |
  | CPU, memory, distribution | 14 | wrong `model name` key; core ids ignored; cpufreq off by a factor of 10; data and instruction caches swapped; vendor prefix doubled; VT-x read from `svm`; CPU lists off by one; `Cached` left out of the estimate; kB read as bytes; cgroup v1 "unlimited" taken as a limit; quota and period swapped; `/usr/lib/os-release` skipped; escapes in double quotes ignored; `VERSION_ID` skipped |
  | Storage | 11 | ZFS volumes counted as disks; `nvme0n1p1`-style partitions misread; bind mounts kept; device names in mount sources unresolved; swap partitions ignored; ZFS members unrecognized; the longest mount point kept; `/boot` kept; full model names unread; ATA rotation rate inverted; octal escapes misdecoded |
  | Report | 11 | the 介质 column in VMs; ZFS used space shown; the test directory unmarked; the cgroup memory limit applied when larger; the cgroup swap ignored; two-socket text for one socket; a CPU quota above the cpuset shown; 硬件虚拟化 in containers; TiB with one decimal; hours not wrapped at 24; the system disk unmarked |

## 8. Known limitations

- CPUID is x86 only. ARM guests are recognized by DMI or the device tree.
- ARM model names come from the part table. SMBIOS processor strings, such as "Ampere Altra Q80-30", are not decoded.
- A ZFS pool's used space is not shown (§6).
- LXC is tested through Docker until a real LXC capture exists (S-07).
- Kernels older than the runners' are not exercised in CI; their userlands are, through the distribution matrix.
