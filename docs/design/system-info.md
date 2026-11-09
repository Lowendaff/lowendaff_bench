# System Information Test Items (系统基础信息)

- Status: **Accepted** 2026-11-07. The owner decided S-01..S-08 (§2). The hardware of dedicated servers (§9) followed on 2026-11-09 with S-09..S-21, and the owner accepted its risks (§9.8), as revised after the captures, the same day. S-22 followed a real Proxmox VE container on 2026-11-10.
- Scope: the first test items. One plugin per kind of platform in `plugins/system`. They read the host through `crates/bench-host` and rely on the orchestrator's platform detection (plugin system design, D-16). §9 adds the machine, firmware, memory modules, ECC, BMC and RAID cards to the dedicated variant (disk health design, DH-01).

## 1. Goals and non-goals

Goals:

1. Show the server's basic facts in Chinese: CPU, memory, disks and filesystems, distribution, kernel, uptime, load, TCP congestion control and IPv4/IPv6 reachability.
2. Work on every Linux distribution the binary runs on (§4).
3. Never run a host program (design P-11). Every value comes from `/proc`, `/sys`, `/etc`, `/run/udev/data`, a system call (`statvfs`, `stat`, CPUID), or, for dedicated servers, the host's `pci.ids` and, as a last resort, `/dev/mem` (§9.6).
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
| S-05 | Distribution coverage in CI | Run the item in images of Debian 11/13, Ubuntu 20.04/24.04, CentOS 7, Rocky 9, AlmaLinux 8, Fedora 45, Alpine 3.22, Arch (x86_64 only), openSUSE Leap 16.0, Amazon Linux 2023 and Oracle Linux 9, on both architectures (`scripts/distros.sh`). Images are listed in `scripts/distros.txt`, pinned by digest, and come from AWS ECR Public, except openSUSE, which publishes on its own registry. CI pulls copies with the same digests from `ghcr.io/lowendaff/mirror` (README "Gates"): ECR Public allows one anonymous pull per second per address, and on 2026-11-07 that limit failed the x86_64 job on the runners' shared addresses. The step belongs to the Linux CI jobs, which are required checks. |
| S-06 | Fixtures | The owner's KVM guest and dedicated server were sampled read-only. The captures may be published after trimming (`crates/bench-host/tests/fixtures/README.md`). |
| S-07 | One environment-aware plugin, or one plugin per environment | **One plugin per platform kind**, with the shared parts extracted as far as possible. LXC is in scope and tested with Docker first; a real Proxmox VE container followed on 2026-11-10 (§7). |
| S-08 | Plugins that do not apply here | One 本机不适用 line in the header. Titles that run here in another variant are left out. |
| S-09 | How much of the memory modules to show (dedicated servers, §9) | **A table of every slot,** empty slots included. The alternatives were one summary line, and a table of the installed modules only. |
| S-10 | The memory table's columns | **插槽, 容量, 类型, 速度, 厂商, 型号.** The type is the generation and the module kind (DDR4 LRDIMM). The speed is the configured speed, with the rated speed in parentheses when they differ. An empty slot shows its locator and 空 only. The alternatives added a rank column, or left out the manufacturer and part number. |
| S-11 | ECC | **One line:** the error correction that SMBIOS states, and EDAC's corrected and uncorrected errors summed over the memory controllers, since boot. The alternatives were counts per module, which EDAC cannot attribute reliably (§9.1), and whether there is ECC only. |
| S-12 | Machine, board and firmware | **Three lines,** 整机, 主板 and BIOS. Placeholder strings are dropped, the product version is left out, the board version goes in parentheses and dates become year-month-day. The alternatives were two lines without the machine, and one combined line. |
| S-13 | BMC | **Vendor, firmware and IPMI version** from sysfs, the vendor named from its IANA enterprise number by a built-in table. Without the IPMI driver, the interface from SMBIOS type 38 or ACPI's SPMI table. 未检测到 only when every source was checked. The alternatives showed the enterprise number unnamed, or SMBIOS's interface only. |
| S-14 | The RAID card's model | **The host's `pci.ids`,** by vendor, device and subsystem, else vendor and device. Without the file, the PCI ids and the driver. The alternatives were a name table generated into the program, and the ids only. |
| S-15 | Which controllers, and how their disks attach | **RAID-class controllers only,** one line each, listing their disks as 阵列, 直通 or 未知: 没有连接硬盘 for a card without disks, and 无 without RAID cards. The alternatives listed every storage controller with disks, or marked the disk table instead. |
| S-16 | Where the new lines go | **Next to what they belong to:** 整机, 主板, BIOS and BMC first; ECC and the memory table after Swap; RAID cards before the disk table. The alternatives put them all at the end, or all at the start. |
| S-17 | How to parse the SMBIOS table | **`smbios-lib` 0.9.4.** The owner first chose the `dmidecode` crate. Its source showed a header read that can pass the end of the buffer, and a panic, on malformed tables, so the owner switched to `smbios-lib`, whose parsing is safe Rust that stops at bad lengths. A parser of our own was the third alternative. |
| S-18 | When the usual sources are missing, as on ARM boards and old kernels | **A chain of sources for each field** (§9.6), so that the data is as complete as possible: the SMBIOS table also from `/sys/firmware/dmi/entries` and, last, on x86, from `/dev/mem`; the device tree on boards whose SMBIOS lacks the data or that have none; ACPI's SPMI table and the `IPI0001` device for the BMC; EDAC for the ECC mode. Kernel modules are never loaded. The captures (§9.1) limited `/dev/mem` to x86 and extended the device tree to boards whose SMBIOS has no memory structures. |
| S-19 | A real run of §9 | **Once, on the owner's dedicated server,** as an exception to read-only probing (§9.7). Only the new lines are published. |
| S-20 | A slot that firmware reports as empty but that carries a module's data | **容量 shows 未启用,** the other columns show the module's data, and a 说明 line under the table names the slots. The owner's server has two such slots (§9.1). The alternatives were the same without the 说明 line, and showing the slot as 空. |
| S-21 | A real run on the owner's ARM board | **Once, `--only system.dedicated`,** by S-19's procedure (§9.7). The alternatives were adding the disk health item, to read the board's NVMe SSD for real, and no run. |
| S-22 | A container's uptime and load, which `/proc` may show for the host: Docker shares the kernel's, and lxcfs passes the load through | **Compared with what the kernel reports itself:** `CLOCK_BOOTTIME` for the uptime, which LXD and Incus do not intercept as they can sysinfo(2), and sysinfo(2) for the load. In a container, a value equal to the kernel's is the host's and gets （宿主机）; a different one is the container's own and shows plain. OpenVZ is left out, because its kernel answers both with the container's own values. Decided 2026-11-10 after the Proxmox VE container's run (§7). The alternatives were documenting it only, and leaving both lines out in containers. |

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
| 运行时间 | fact | `/proc/uptime` | "3 天 4 小时 12 分钟". In a container, （宿主机） follows when it is the host's (S-22). |
| 负载 | fact | `/proc/loadavg` | In a container, （宿主机） follows when it is the host's (S-22). |
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
- **Containers.** LXC with lxcfs shows container values in `/proc`; on Proxmox VE the container's own cgroup files say `max`, because the limits sit above its namespace. Docker without lxcfs shows the host's, so containers read their cgroup (v2, else v1) for memory and CPU limits.
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
  - the owner's KVM guest, dedicated server and ARM board;
  - an unprivileged Proxmox VE container on that server;
  - Docker on Apple silicon, plain and with limits on its own cgroup.

  Expectations come from the host's own tools at capture time (`truth/`). Small synthetic hosts cover rules no capture shows.
- **`plugins/system/tests`** checks the records each variant prints for those captures.
- **CI, distribution matrix (S-05).** The static binary runs `system.container` inside each image on both architectures. The 系统 line must match the image's os-release, and no field may fail. All containers share the runner's kernel, so the matrix covers distribution files; kernels and platforms are covered by the fixtures.
- **CI runners.** The smoke test runs every test item on the x86_64 and aarch64 runners themselves, which are virtual machines. It fails when an item fails or times out, and prints the output, including 运行环境, to the CI log. On 2026-11-07 both runners reported Hyper-V 虚拟机 and completed the item:
  - x86_64: AMD EPYC 9V74, 4 vCPUs, AMD-V exposed to the guest, a 150 GiB SCSI "Virtual Disk", Ubuntu 24.04.5.
  - aarch64: ARM Neoverse-N2 (from the part code), Hyper-V recognized through DMI because ARM has no CPUID; CPU 频率 showed 不可用, since the guest has no cpufreq.
- **A Proxmox VE container (2026-11-10).** With the owner's approval, an unprivileged container (2 cores, 1 GiB, its root on ZFS) was made on the dedicated server for one capture and one run of `--skip cpu.geekbench`, then destroyed. The run completed 系统基础信息, CPU 性能 and 硬盘性能, with 硬盘健康 本机不适用. Its 负载 was the host's, passed through by lxcfs, which led to S-22.
- **Mutation check.** Each mutation was applied to a copy of the repository, and the tests had to fail. All 66 were killed, the last 16 for S-22 on 2026-11-10, two of them in a Linux container because the code they change is built for Linux only:

  | Area | Mutations | Examples |
  |------|-----------|----------|
  | Platform detection | 9 | containers checked after VMs; an OpenVZ host taken for a container; a non-initial PID namespace ignored; the Hyper-V signature trusted at once; EC2 `.metal` ignored; Xen dom0 ignored; CPUID ignored; an empty `container=` ignored; DMI fields read in another order |
  | Plan, registry check, header | 5 | every plugin kept; variants' titles listed as not applicable; an empty `platforms` read as no platform; a variant for every platform not seen as overlapping; the 本机不适用 line always printed |
  | CPU, memory, distribution | 14 | wrong `model name` key; core ids ignored; cpufreq off by a factor of 10; data and instruction caches swapped; vendor prefix doubled; VT-x read from `svm`; CPU lists off by one; `Cached` left out of the estimate; kB read as bytes; cgroup v1 "unlimited" taken as a limit; quota and period swapped; `/usr/lib/os-release` skipped; escapes in double quotes ignored; `VERSION_ID` skipped |
  | Storage | 11 | ZFS volumes counted as disks; `nvme0n1p1`-style partitions misread; bind mounts kept; device names in mount sources unresolved; swap partitions ignored; ZFS members unrecognized; the longest mount point kept; `/boot` kept; full model names unread; ATA rotation rate inverted; octal escapes misdecoded |
  | Report | 11 | the 介质 column in VMs; ZFS used space shown; the test directory unmarked; the cgroup memory limit applied when larger; the cgroup swap ignored; two-socket text for one socket; a CPU quota above the cpuset shown; 硬件虚拟化 in containers; TiB with one decimal; hours not wrapped at 24; the system disk unmarked |
  | The host's uptime and load (S-22) | 16 | the uptime's second of tolerance made inclusive or a minute; loads compared with `any`, or within 0.1 or 0.001; OpenVZ compared; containers not compared; VMs and dedicated servers compared; only the reading before /proc used, or only the one after; the mark inverted, renamed, or given without comparing the uptime or the load; on Linux, the load's fixed point read with 11 fractional bits, and the real-time clock read for the uptime |

## 8. Known limitations

- CPUID is x86 only. ARM guests are recognized by DMI or the device tree.
- ARM model names come from the part table. SMBIOS processor strings, such as "Ampere Altra Q80-30", are not decoded.
- A ZFS pool's used space is not shown (§6).
- LXC is tested with one Proxmox VE container: unprivileged, with nesting, its root on ZFS. Privileged containers and other managers, such as LXD or Incus, are not captured.
- The （宿主机） mark (S-22) trusts the kernel's answers. A container in its own time namespace, which shifts `CLOCK_BOOTTIME` as well, has its uptime marked as the host's; so would a load that matches the host's to the hundredth by chance, such as an idle container's under lxcfs with `--enable-loadavg` on an idle host; and so would values from any other kernel that, like OpenVZ's, answers with the container's own.
- Kernels older than the runners' are not exercised in CI; their userlands are, through the distribution matrix.

## 9. Hardware of dedicated servers

DH-01 of the disk health design moved the dedicated server's hardware, other than SMART, into this item. Buyers of a dedicated server want to check it against the offer: the machine and its firmware, the memory and how it is populated, ECC and its errors, the BMC and its firmware, and whether the disks sit on a RAID card. Only `system.dedicated` changes; virtual machines and containers print exactly what they print today.

### 9.1 Background

Read-only probes of the owner's dedicated server, 2026-11-09, with serial numbers, asset tags, UUIDs and GUIDs filtered out on the server:

- **DMI:** machine Inspur `SSINSPURY32-XI5-25D-1` (product version `00001`); board Inspur `YZMB-00870-105`, version `SA5112M5`, which is the model's name; BIOS American Megatrends 4.1.19 of 02/07/2022; SMBIOS 3.2.1.
- **Memory, by SMBIOS:** 24 slots, 2 CPUs with 6 channels of 2 slots each. Eight hold a Samsung M386A4G40DM0-CPB, a 32 GB DDR4-2133 LRDIMM of rank 4 and 72-bit width, but only six count. CPU0_C0D0 and CPU0_C1D0 report a size of 0, "No Module Installed", yet carry the module's type, speed, rank, manufacturer and part number; the truly empty slots say `NO DIMM`. dmidecode prints no speed, manufacturer or part number for a slot of size 0, so it shows nothing of these two modules. NUMA node 0 has 64 GB and node 1 128 GB, and type 19 maps CPU0's memory from 2 devices: the two modules on CPU0's first memory controller are installed but not in use. The 192 GB in use match the kernel's 188.5 GiB. Type 16 states Single-bit ECC.
- **EDAC** (`skx_edac`) reports 8 modules of 32 GB on 4 controllers, typed Unbuffered-DDR4. The count is right, the two unused modules included; the type is wrong. Its mode (SECDED) and its counts (0 and 0) are usable; its inventory is not, and its labels (`CPU_SrcID#0_MC#0_Chan#0_DIMM#0`) do not name SMBIOS's slots.
- **BMC:** sysfs gives firmware 4.18, IPMI 2.0, manufacturer `0x009439` (37945, Inspur) and product `0x0202`, in the platform device `ipmi_bmc.0`; `/sys/class/ipmi/ipmi0/device/bmc` only links to it, and exists only while `ipmi_devintf` is loaded. SMBIOS type 38 says KCS and IPMI 2.0; ACPI's SPMI table says interface 1 (KCS) and revision 0x0200, and ACPI lists the device `IPI0001`. The IPMI modules are loaded, but ipmitool is not installed, and none is needed.
- **Storage controllers:** a MegaRAID `1000:005d` (subsystem `1000:9361`, driver `megaraid_sas`, PCI class 0x0104) and two Intel AHCI controllers without disks. The four disks sit at SCSI addresses `0:0:18:0`, `0:0:21:0`, `0:0:23:0` and `0:0:24:0`, channel 0, with vendor `ATA`: passed through. The driver's sysfs has no model, but the host's `pci.ids` names subsystem `1000:9361` "MegaRAID SAS 9361-8i".
- **Rust crates for SMBIOS** (2026-11-09): `dmidecode` 1.0.1 decodes types 16 and 17 but not 38, and on malformed tables its header read can pass the end of the buffer and a reversed slice can panic (S-17). `smbios-lib` 0.9.4 decodes types 16, 17 and 38 in safe Rust: every length is checked, a bad length ends the table, and its `unsafe` is only in its macOS and Windows loaders, which this design does not call. Its dependencies are `getopts`, `serde` and `serde_json`, plus five Apple crates on macOS; all their licenses pass `deny.toml`. On the owner's server it decodes what dmidecode decodes, and on the ARM board below it reads the same 7 structures and type 0–2 strings as the kernel; cut or malformed copies give the structures before the damage, without a panic. It stops at the end-of-table structure (type 127), as the kernel does with 3.x tables.
- **Fallback sources,** compared with the sysfs table on the server, gave the same bytes: `/dev/mem` at the EFI system table's `SMBIOS3=` and `SMBIOS=` entry points (100 structures, 5798 bytes) and at both entry points found in 0xF0000–0xFFFFF, and the 100 `dmi/entries` joined in `position` order. Lockdown is off, and STRICT_DEVMEM, which is on, allows these ranges.

Read-only probes of the owner's ARM board, a Radxa ROCK 3A (Rockchip RK3568, 4 × Cortex-A55, 2 GB, Debian 13, kernel 6.12), 2026-11-09:

- **SMBIOS:** U-Boot, which boots the board through EFI, provides SMBIOS 3.7 with types 0, 1, 2, 3, 4, 32 and 127: machine and board `radxa` `Radxa ROCK 3A`, BIOS `U-Boot` `2026.07-rc4-ga7830e87555a` of 07/01/2026, and no memory, ECC or BMC structures. The 7 `dmi/entries` join to the table; `/dev/mem` refuses the table's address with EFAULT ("Bad address").
- **Device tree:** `model` `Radxa ROCK 3A`; `compatible` `radxa,rock3a` and `rockchip,rk3568`; `chosen/u-boot,version` equal to the BIOS version; one `memory` node of 2046 MiB from 0x200000, U-Boot keeping the first 2 MiB.
- **Otherwise:** no ACPI, EDAC, IPMI, RAID card or `pci.ids`, and one NVMe SSD. systemd-detect-virt says none, and the program also sees a dedicated machine.

Assumptions confirmed with the owner's understanding, 2026-11-09:

1. Every value comes from a kernel interface or a host data file (§9.6). No host program runs, and no kernel module is loaded.
2. Serial numbers, asset tags and UUIDs are never read from the parsed structures, and the BMC's GUID and network addresses are never read.
3. A disk's attachment follows from its driver and SCSI channel (§9.5); disks on other drivers are 未知 rather than guessed.
4. Capacities are shown as SMBIOS gives them (32 GB), speeds in MT/s, dates as year-month-day.
5. The new reads take a few small files; `pci.ids`, about 1.5 MB, is read only when a RAID-class device exists, and only until the entry is found. The item keeps its 30-second timeout.
6. ARM servers with UEFI and SMBIOS read the same way as x86; boards whose SMBIOS has no memory structures, or that have no SMBIOS, use the device tree (S-18).

The first draft of the assumptions did not read `/dev/mem`; S-18 replaced that.

### 9.2 Structure

Five new modules in `crates/bench-host`, named after what they read, all through `Root` so that tests run against captured hosts:

| Module | Reads | Gives |
|--------|-------|-------|
| `machine` | `/sys/class/dmi/id`, else SMBIOS types 0, 1 and 2, else the device tree | machine, board and firmware fields, placeholders removed |
| `smbios` | the SMBIOS table (§9.6), parsed by `smbios-lib` | memory slots (type 17), the ECC mode (type 16), the BMC interface (type 38) |
| `edac` | `/sys/devices/system/edac/mc/mc*` | corrected and uncorrected errors, summed; the controllers' ECC mode |
| `bmc` | `/sys/bus/platform/devices/ipmi_bmc.*`, else type 38, ACPI's SPMI table and the `IPI0001` device | vendor, firmware and IPMI version, or the interface alone |
| `raid` | PCI devices of class 0x0104, the host's `pci.ids`, and each disk's sysfs path | each card's name, and its disks as 阵列, 直通 or 未知 |

`plugins/system`'s `hardware` module formats what they read, and its report places the records for `system.dedicated` only, as S-16 says. The plugin already reads the host in `spawn_blocking`, which these readers join. The only new dependency is `smbios-lib` 0.9.4, in `bench-host`.

### 9.3 Machine, firmware and BMC (S-12, S-13)

| Line | Fields | On the owner's server |
|------|--------|-----------------------|
| 整机 | `sys_vendor` `product_name` | `Inspur SSINSPURY32-XI5-25D-1` |
| 主板 | `board_vendor` `board_name`（`board_version`） | `Inspur YZMB-00870-105（SA5112M5）` |
| BIOS | `bios_vendor` `bios_version`（`bios_date`） | `American Megatrends Inc. 4.1.19（2022-02-07）` |

- **Placeholders.** After trimming, these count as missing, ignoring case: `To Be Filled By O.E.M.`, `Default string`, `System Product Name`, `System manufacturer`, `System Version`, `Base Board Product Name`, `Not Specified`, `Not Applicable`, `None`, `N/A`, `Unknown`, `0123456789`.
- A missing field drops its part, with its parentheses; a line without any field shows 不可用.
- When the product or board name begins with the vendor's name, ignoring case, the vendor is left out: `Radxa ROCK 3A`, not `radxa Radxa ROCK 3A`.
- **Dates** in the form month/day/year become year-month-day; other forms are shown as they are.
- **BMC**, from the first source that answers:
  1. sysfs, when the IPMI driver is loaded: `Inspur，固件 4.18，IPMI 2.0`. `manufacturer_id` is an IANA enterprise number, named by a built-in table of server and BMC vendors. The numbers were checked against IANA's registry, last updated 2026-11-06, on 2026-11-09: Dell 674; HPE 11 (registered as Hewlett-Packard) and 47196; Supermicro 10876; Lenovo 19046; IBM 2 and 20301 (IBM eServer X); Inspur 37945, 48512 and 60775; Huawei 2011; xFusion 58132; Fujitsu 211 and 10368; Cisco 9 and 5771; Intel 343; Quanta 7244; Inventec 6569 and 20569; Wiwynn 40092; Gigabyte 15370; ASUS 2623; ASRock Rack 49622; Tyan 6653; AMI 20974; Ampere 52538. An unknown number shows as 厂商编号 37945.
  2. SMBIOS type 38, else ACPI's SPMI table: `有（IPMI 2.0，KCS 接口）；厂商和固件不可用：内核没有加载 IPMI 驱动`. Interfaces are KCS, SMIC, BT and SSIF.
  3. The ACPI device `IPI0001` alone: `有；接口、厂商和固件不可用：内核没有加载 IPMI 驱动`.
  4. Otherwise `未检测到`. Only the first `ipmi_bmc` device is read: a server has one BMC.

### 9.4 Memory modules and ECC (S-09, S-10, S-11)

The table, id `memory-modules` and title 内存条, has one row per type 17 structure of the arrays that type 16 marks as system memory, in table order:

| Column | Rule |
|--------|------|
| 插槽 | The device locator. When locators repeat, as on boards that call every channel's slot `DIMM 0`, the bank locator goes before it; without either, 插槽 N. |
| 容量 | The size, or the extended size when the size field says so. Whole multiples of 1024 MB as GB (32 GB), else MB. 空 without a module; 未知 when the field says unknown. 未启用 when the size field says no module is installed but the manufacturer and the part number are real values (S-20). |
| 类型 | The generation and the module kind from the type detail: Registered is RDIMM, LRDIMM is LRDIMM, Unbuffered is UDIMM; without one, the generation alone (DDR4 LRDIMM, DDR5 RDIMM, LPDDR4). |
| 速度 | The configured speed in MT/s, with the rated speed in parentheses when it differs: `2666 MT/s（额定 2933）`. Old tables without a configured speed: `2933 MT/s（额定）`. Neither: —. The extended fields are read when the 16-bit fields say so. |
| 厂商, 型号 | Trimmed. Placeholders such as `Unknown`, `Not Specified`, `Undefined`, `NO DIMM`, all zeros, `PartNum0` or `Manufacturer0` show —. JEDEC codes such as `80CE` are shown as they are. |

An empty slot shows its locator, 空 and four —. A slot shown as 未启用 keeps its type, speed, manufacturer and part number, and one 说明 line follows the table: `说明  CPU0_C0D0、CPU0_C1D0 插着内存条但没有启用：固件报告容量为 0，没有计入内存总量`. On the owner's server, 24 rows: 6 such as `CPU0_C3D0  32 GB  DDR4 LRDIMM  2133 MT/s  Samsung  M386A4G40DM0-CPB`, 2 such as `CPU0_C0D0  未启用  DDR4 LRDIMM  2133 MT/s  Samsung  M386A4G40DM0-CPB`, and 16 空.

The ECC line comes before the table:

- **Mode,** from type 16 of the system-memory arrays: Single-bit ECC → 有（单比特纠错）; Multi-bit ECC → 有（多比特纠错）; CRC → 有（CRC 校验）; Parity → 无纠错（只有奇偶校验）; None → 无; anything else → 未知. Different arrays with different modes are listed in turn.
- **Errors,** EDAC's `ce_count` and `ue_count` summed over all controllers: `ECC  有（单比特纠错）；本次开机以来已纠正 0 次，不可纠正 0 次`.
- Without EDAC: `……；错误次数不可用：内核没有加载 EDAC 驱动`. With the mode 无, the line is `ECC  无`.

### 9.5 RAID cards (S-14, S-15)

- **Cards** are the PCI devices whose `class` starts with `0x0104`, in PCI address order.
- **Name,** from the first of `/usr/share/hwdata/pci.ids`, `/usr/share/misc/pci.ids` and `/usr/share/pci.ids`: the subsystem's name for vendor, device and subsystem (`MegaRAID SAS 9361-8i`), else the vendor's and device's names (`Broadcom / LSI MegaRAID SAS-3 3108 [Invader]`). Without a file or an entry: `PCI 1000:005d（子系统 1000:9361，驱动 megaraid_sas）`, or 未加载驱动 in place of the driver.
- **Disks.** A disk of the 硬盘 table belongs to the card whose PCI address is a part of the path that the link `/sys/block/<disk>` points to. The part before `block` is the SCSI address host:channel:target:lun. The kernel source (2026-11-09) gives each driver's channels:

  | Driver | 阵列 | 直通 |
  |--------|------|------|
  | `megaraid_sas` | channel 2 and above (`MEGASAS_IS_LOGICAL`) | 0 and 1 |
  | `hpsa` | 1 (RAID volumes), 2 (external volumes) | 0, 3 (HBA modes) |
  | `smartpqi` | 1 (RAID volumes), 3 (external volumes) | 0, 2 (HBA mode) |
  | `aacraid` | 0 (`CONTAINER_CHANNEL`) | others |
  | `mpt3sas` | 1 (`RAID_CHANNEL`) | 0, 2 (NVMe) |
  | `ahci`, `vmd` | — | all: RST and VROC are software RAID, and their disks are visible one by one |

  Other drivers' disks are 未知.
- **Line:** `RAID 卡  MegaRAID SAS 9361-8i：直通 sda、sdb、sdc、sdd`. Groups in the order 阵列, 直通, 未知, separated by semicolons, disks in the 硬盘 table's order. A card without disks: `……：没有连接硬盘`. No RAID-class device: `RAID 卡  无`.
- The 硬盘 table is unchanged; a RAID volume is already a row of it, usually with the card's name as its model.

### 9.6 Sources, fallbacks and edge cases (S-18)

Each value comes from the first source that answers; all reads are read-only.

| Value | Sources, in order |
|-------|-------------------|
| 整机, 主板, BIOS | `/sys/class/dmi/id`; SMBIOS types 1, 2 and 0; the device tree: `model` for 整机, `compatible` for 主板 (`raspberrypi,4-model-b（brcm,bcm2711）`), and for the firmware, shown as 固件, U-Boot's `/chosen/u-boot,version` or the Raspberry Pi bootloader's `/chosen/bootloader/build-timestamp`, shown as a build date |
| The SMBIOS table | `/sys/firmware/dmi/tables/DMI` with its entry point; `/sys/firmware/dmi/entries/*/raw` joined in `position` order, which exists when the dmi-sysfs module happens to be loaded; on x86 only, `/dev/mem`, reading only the table: its address from the `SMBIOS3=` or `SMBIOS=` line of `/sys/firmware/efi/systab`, else from the entry point found in 0xF0000–0xFFFFF, as dmidecode does on old kernels. A 3.x table is read up to its maximum size, and `smbios-lib` stops at its end-of-table structure (type 127), where the kernel ends it; a 2.x table has the length its entry point gives |
| 内存条 when SMBIOS has no type 17, or there is no SMBIOS | One fact instead of the table: `内存条  板载内存 2.0 GiB（来自设备树）；没有可插拔内存条的信息`, the size summed over the device tree's `memory` nodes and written like 内存总量 |
| ECC mode | SMBIOS type 16; EDAC's mode (`SECDED` → 单比特纠错; chipkill modes such as `S4ECD4ED` → 多比特纠错) |
| BMC | §9.3 |
| RAID cards | PCI sysfs, on ARM servers as well; boards without RAID-class devices show 无 |

| Case | Shown |
|------|-------|
| No SMBIOS table from any source | 内存条: the device tree's memory, else `不可用：读不到 SMBIOS`, followed by `（/dev/mem：<error>）` when `/dev/mem` refused the read. ECC: the mode from EDAC; with neither, `ECC  不可用：没有 SMBIOS，也没有 EDAC 驱动`, or `读不到 SMBIOS` in place of `没有 SMBIOS` when `/dev/mem` refused the read |
| A table without type 17 | 内存条: the device tree's memory, else `不可用：SMBIOS 里没有内存条信息` |
| A table without type 16 | ECC: the mode from EDAC; with neither, `ECC  不可用：SMBIOS 里没有内存纠错信息，也没有 EDAC 驱动` |
| EDAC without a usable mode, and no type 16 | `ECC  纠错方式不可用；本次开机以来已纠正 0 次，不可纠正 0 次` |
| A malformed table | what `smbios-lib` parsed before it stopped |
| A table over 1 MiB | treated as unreadable |
| A file that exists but cannot be read | 不可用, with the system's error |

- **`/dev/mem`** is the last SMBIOS source, on x86 only, and runs only when neither sysfs source exists, as on kernels before 4.2 without dmi-sysfs. It is a plain read of the table's range, without `unsafe`. Kernel lockdown or STRICT_DEVMEM may refuse it, which counts as unreadable. Some security tools flag reads of `/dev/mem`. ARM has no 0xF0000 segment, arm64 kernels since 4.2 have the sysfs table, and the owner's ARM board refused the read anyway (§9.1).
- No kernel module is loaded, so no dmi-sysfs, `ipmi_si` or `ee1004`. Reading an SPD chip, or sending an IPMI command, would need one of them or an ioctl that `unsafe_code = "forbid"` excludes.
- The ARM board of §9.1 will show:

```
  整机              Radxa ROCK 3A
  主板              Radxa ROCK 3A
  BIOS              U-Boot 2026.07-rc4-ga7830e87555a（2026-07-01）
  BMC               未检测到
  ECC               不可用：SMBIOS 里没有内存纠错信息，也没有 EDAC 驱动
  内存条            板载内存 2.0 GiB（来自设备树）；没有可插拔内存条的信息
  RAID 卡           无
```

- A Raspberry Pi started by its own firmware has no SMBIOS: 整机 shows `model` (`Raspberry Pi 4 Model B Rev 1.4`), 主板 `compatible` (`raspberrypi,4-model-b（brcm,bcm2711）`), and a 固件 line the bootloader's build date (`树莓派引导程序，2024-04-15 构建`).

### 9.7 Testing and the real runs (S-19, S-21)

- **Captures,** read-only and through pipes only, on 2026-11-09:
  - the owner's server: the SMBIOS table and its entry point, with serial numbers, asset tags and UUIDs overwritten on the server, by perl, with placeholders of the same length, so that no structure moves; the server then looks for the serial numbers, asset tags and UUIDs that dmidecode, the kernel's DMI files and the device tree report, and leaves the table out unless none is left. Also EDAC, the BMC's sysfs without its GUID, the storage controllers' PCI files and drivers, the `/sys/block` links, ACPI's SPMI table and `IPI0001` device, the SMBIOS lines of the EFI system table, and the few lines of `pci.ids` that are used. They joined the existing fixture `dedicated-pve-x86_64`;
  - the owner's ARM board, in full, as the fixture `rock3a-debian13-aarch64`: its SMBIOS table scrubbed the same way and checked against the kernel and the device tree, because the board has no dmidecode; and the device tree's `model`, `compatible`, cell sizes, `chosen/u-boot,version` and `memory` nodes, never its `serial-number` or the boot arguments and seeds under `chosen`.
- **Fallback sources** were compared on both hosts by read-only probes (§9.1). The tests rebuild them from the captured tables: `dmi/entries` split from a table, a sparse `/dev/mem` with the entry point and table at the captured EFI address, and a 0xF0000 segment holding an entry point.
- **Synthetic hosts:**
  - SMBIOS structures built in the tests: DDR5 with extended speeds, extended sizes, sizes in KB, configured speeds below rated ones, placeholders, repeated locators, empty slots with placeholder data, arrays that are not system memory, and truncated or malformed tables;
  - the fallbacks: a 2.x entry point, a 3.x table followed by bytes after its type 127 structure, and an ARM host, whose `/dev/mem` is never read;
  - the BMC: type 38 only, SPMI with SSIF only, `IPI0001` only, nothing;
  - RAID: a Dell PERC with a volume and pass-through disks, hpsa volumes, aacraid, mpt3sas with IR firmware, two cards, an unknown driver, no `pci.ids`, and chipset RAID modes (`ahci`, `vmd`);
  - device-tree boards: a Raspberry Pi without SMBIOS (`model`, `compatible`, the bootloader's timestamp, `memory` nodes); the U-Boot board is the ARM capture;
  - ECC: no EDAC, and EDAC's mode only.
- **Two levels:** `bench-host` tests each reader; `plugins/system` tests the dedicated variant's records, their order (S-16) and their exact text, and that the VM and container variants are unchanged.
- **Mutation round** on the key logic: placeholders and dates; sizes, types and speeds; the 未启用 rule; the ECC mapping and EDAC's sums; the vendor table; type 38 and SPMI; the `pci.ids` lookup order; disk-to-card mapping and the channel rules; the order of the fallbacks; and the `/dev/mem` address.
- **CI** cannot exercise these paths: the runners are virtual machines, and the distribution matrix runs containers. The gates are unchanged.
- **The real runs** follow DH-07's procedure, once on each host, after the PR passes its gates: the binary for the host's architecture is piped into `/root/lowendaff-bench-test` and runs `--only system.dedicated`; the directory is removed afterwards, and `/root`'s listing is compared before and after, on the owner's server also the MegaRAID node's birth time. The owner sees the whole output; the PR publishes only the new lines, because the filesystem table shows mount points and dataset names.

### 9.8 Risks and limitations

- **Untested hardware:** ARM servers with UEFI, ACPI and IPMI; boards without SMBIOS, such as the Raspberry Pi; AMD's EDAC; and the hpsa, smartpqi, aacraid and mpt3sas rules are covered by synthetic hosts only. The `dmi/entries` and `/dev/mem` paths were checked on the owner's server, U-Boot's SMBIOS and the device tree on the owner's ARM board.
- **Firmware data can be wrong,** as EDAC's module type is on the owner's server. The item shows what the firmware states; the placeholder list cannot be complete.
- **未启用** (S-20) relies on firmware that keeps a module's data in a slot it reports as empty, as the owner's server does. Firmware that leaves stale data in an empty slot would show it as 未启用, and firmware that clears the data shows a disabled module as 空.
- **`pci.ids` varies by host:** without it, cards show their ids; an old file can miss a new card.
- **The attachment rules** cover six drivers and follow channel numbers that have been stable for years; other drivers show 未知.
- **`/dev/mem`** is the riskiest interface the program reads. It runs only on x86 and only without the sysfs sources, reads only the ranges the firmware gives, and may be refused or flagged by security tools.
- **`smbios-lib`** adds `getopts` and `serde_json`, and Apple crates on macOS. Its safety on malformed tables was checked by reading its source and by cutting and corrupting the captured tables, not by fuzzing.
- **The BMC's firmware and vendor** need the IPMI driver, because the program neither loads modules nor sends IPMI commands.
- **The vendor table** is a short list; other vendors show their number.
- **Large servers** print long memory tables, one row per slot (S-09).
- **EDAC's counts** restart when its driver loads, normally at boot.
- **Boards' memory** comes from the device tree, which leaves out what the firmware keeps: 2 MiB on the ROCK 3A, the GPU's share on a Raspberry Pi. A Raspberry Pi's firmware is shown by its build date, the only value it exposes.
- **The real runs** are two more exceptions to read-only probing (S-19, S-21).

### 9.9 Implementation plan

1. Capture the owner's server and ARM board, and compare the fallback sources on both (§9.7). Done 2026-11-09.
2. One PR: this section; the `bench-host` readers with `smbios-lib`, their fixtures and tests; the report's changes and their tests; a README line for 系统基础信息.
3. Mutation round, then each commit's gates in a worktree.
4. Push, CI, the real runs (S-19, S-21), and the owner's approval before merging.
