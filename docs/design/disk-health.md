# Disk Health Test Item (硬盘健康)

- Status: **Accepted** 2026-11-08. The owner decided DH-01..DH-07 (§2), confirmed the design section by section, and accepted the risks (§9); DH-08 followed on 2026-11-09.
- Implementation (M3): the smartctl builds and the release `smartctl-7.5-1`, then the plugin in `plugins/disk-health`.
- Scope: one test item, `disk.health`, which reads the SMART health of every physical disk of a dedicated server with a pinned smartctl, and the smartctl builds it needs (§4). The other checks for dedicated servers join 系统基础信息 (DH-01).

## 1. Goals and non-goals

Goals:

1. Show the health of each physical disk of a dedicated server as smartctl judges it, with the values that show wear and damage: power-on time, temperature, endurance used, data written, bad blocks and interface errors.
2. Reach the disks behind RAID cards, where dedicated servers often keep them.
3. Use only the smartctl that this repository builds and pins (P-10, P-11).
4. Only read: no self-tests, and no change to a disk's settings.
5. Leave nothing behind on the host, including the device nodes that smartctl creates (§6).
6. Degrade per disk: a disk that cannot be read keeps its row, with the reason.

Non-goals:

- Self-tests, full attribute lists, and health scores of our own.
- The state of RAID arrays, and IPMI sensors and events (DH-01).
- Virtual machines and containers, whose disks are virtual. The item does not apply there.
- Disks on RAID cards that smartctl's scan does not list (§9).
- Keeping disks asleep: as with smartctl's default, reading wakes a disk in standby.

## 2. Owner's decisions

| ID | Question | Decision |
|----|----------|----------|
| DH-01 | Which checks for dedicated servers, and where | **Split A.** SMART becomes this item. Board and BIOS, memory modules, memory ECC mode and error counts, the BMC (firmware, IPMI version, vendor) and RAID controllers (model; disks passed through or in an array) join the dedicated variant of 系统基础信息, read from kernel interfaces; they are designed next. Deferred: IPMI sensors and the event log, which need a pinned ipmitool because `unsafe_code = "forbid"` rules out IPMI ioctls of our own, and the state of RAID arrays, which only the proprietary storcli reads. This refines DK-04. |
| DH-02 | What the item shows | **One table, one row per physical disk** (§5.5), with the columns 设备 健康 通电时间 温度 已用寿命 累计写入 坏块 接口错误. 健康 is smartctl's own verdict. |
| DH-03 | smartctl, or a native Rust implementation | **smartctl 7.5.** No Rust crate covers SATA, SAS and NVMe, and none reaches the disks behind RAID cards (§3.2). |
| DH-04 | How smartctl is built and released | **A script and a workflow of its own,** like fio's (§4), with the release `smartctl-7.5-1`. The alternative was one generic workflow for every tool, replacing fio's; that becomes worthwhile with a third tool. |
| DH-05 | The device nodes that smartctl creates | **Removed after the item** when they did not exist before it (§6). The alternatives were leaving them and documenting it, or finding disks without smartctl's scan and skipping RAID-card disks while the node is missing. |
| DH-06 | Test data, with no other hardware at hand | **The owner's four SATA SSDs,** read directly and through the RAID card. NVMe comes from GitHub's runners; hard disks and SAS are written by hand from smartmontools 7.5's source, and marked so (§7.2). |
| DH-07 | A real run on dedicated hardware | **Once, on the owner's dedicated server,** as an exception to read-only probing (§8). |
| DH-08 | Where the item runs among the others | **Before 硬盘性能**, after 系统基础信息 (the registry's order is the run order). SMART is read before fio heats the disks and writes up to about 10 GiB, so the temperature is the idle one, 累计写入 leaves out the program's own writes, and a failing disk shows before the stress test. The health result neither skips nor changes 硬盘性能. |

## 3. Background

### 3.1 The owner's dedicated server

Read-only probes, 2026-11-08:

- Proxmox VE on Debian 13 (kernel 7.0.14-15-pve), on an Inspur server with two Xeon Gold 6130.
- Four Samsung SATA SSDs (an 860 EVO 250GB, two 870 EVO 1TB and an 860 EVO 1TB) sit behind a Broadcom MegaRAID SAS 9361-8i (SAS3108, driver `megaraid_sas`), which passes them through: the kernel sees them as `sda` to `sdd`.
- `smartctl --scan-open` lists `/dev/sda` to `/dev/sdd` with type `sat`, and `/dev/bus/0` with types `sat+megaraid,18`, `21`, `23` and `24`. Serials compared on the server: 18, 21, 23 and 24 are `sda`, `sdb`, `sdc` and `sdd`.
- smartd runs, and created `/dev/megaraid_sas_ioctl_node` 17 seconds after boot (§6).
- None of the four disks has Device Statistics page 3, and `sda` has no attribute 197. All four have page 7; smartctl's estimate from attribute 177 differs from it by 0 to 4 points. The two 870 EVO exit with status 64 (entries in the error log) and are otherwise read normally.
- Through the RAID card, the SCT commands fail (exit status bit 2) and every value the item uses is the same as read directly. The disk's own SMART status does not come through the card either, so smartctl judges health from the attributes' thresholds, with the warning "This result is based on an Attribute check" (`ataprint.cpp`).
- Timing, 2026-11-09: the scan took 46 ms, `-i` 34 ms and `-x` 46 to 50 ms per disk, directly and through the RAID card alike.
- With the rules of §5, they show:

```
  设备  健康  通电时间    温度   已用寿命  累计写入  坏块     接口错误
  sda   通过  46164 小时  25 °C  23%       48.2 TB   0 / —    3
  sdb   通过  38109 小时  28 °C  49%       169.0 TB  380 / —  0
  sdc   通过  53541 小时  27 °C  79%       277.2 TB  0 / —    0
  sdd   通过  35120 小时  27 °C  30%       155.6 TB  13 / —   0
```

### 3.2 Alternatives to smartctl

Surveyed on crates.io and GitHub, 2026-11-08:

| Candidate | License | Covers | Why not |
|-----------|---------|--------|---------|
| `hdd` 0.10.3 | MPL-2.0 | ATA and SCSI, with Device Statistics | No NVMe and no RAID cards. Its README calls it practically unmaintained since 2020, because smartctl gained JSON output. |
| `siomon` 0.2.3 | MIT | ATA through SG_IO, NVMe | A monitoring tool with disks as one part; no RAID cards. |
| `nvme-telem` 0.3.3 | MIT | NVMe | NVMe only; its build needs libclang. |
| `libatasmart` 0.3.3, `libglacierdisk` 0.7.0 | MIT | ATA | Wrappers of the C library libatasmart (LGPL). |
| `smart_sat` | not stated | Five ATA attributes through SAT | No NVMe and no RAID cards. |
| `smartmon` 0.1.0 | GPL-2.0-or-later | What smartctl covers | Bindings to smartmontools built as a library, not a native implementation. |
| `rigtop-core`, `diskwatch`, `smartctl-rs` | MIT | — | They run smartctl. |
| `smartmontools/smartmontools-builds` | GPL-2.0-or-later | Upstream's static builds | Development snapshots only (all 50 releases are pre-releases), for x86_64 and i686 but not aarch64. |

A native implementation of our own would need ioctls (SG_IO, the NVMe admin command, MegaRAID's pass-through), which `unsafe_code = "forbid"` rules out in this repository's crates.

### 3.3 smartctl 7.5

From its source (`smartmontools-7.5.tar.gz`) and trial runs:

- **JSON:** the same fields for every protocol: `smart_status.passed`, `smart_support.available` and `.enabled`, `power_on_time.hours`, `temperature.current`, `endurance_used.current_percent`, `serial_number` and `logical_block_size`. For NVMe, `passed` is true when the critical warning is 0, and `smart_support` is always available and enabled.
- **Exit status,** by bit:
  - 0: the command line did not parse;
  - 1: the device could not be opened or identified;
  - 2: a SMART or other command failed;
  - 3: the disk reports DISK FAILING;
  - 4: a prefail attribute is at or below its threshold;
  - 5: an attribute was at or below its threshold in the past;
  - 6: the error log has entries;
  - 7: the self-test log has errors.
- **Messages:** errors raised with `jerr` appear in `smartctl.messages[]` as `{string, severity}`, with severity `information`, `warning` or `error`. Errors printed as plain output, such as `Read Device Identity failed`, do not.
- **Scan on Linux,** in this order:
  1. `/dev/hd[a-t]`;
  2. `/dev/sd[a-z]` and `/dev/sd[a-z][a-z]`;
  3. disks on MegaRAID cards: name `/dev/bus/<host>`, type `megaraid,<id>` or `sat+megaraid,<id>`, where `<id>` is the card's device ID for the disk (MegaCli's Device Id, storcli's DID), not a slot;
  4. disks on 3SNIC cards: name `/dev/bsg/sssraid<host>`, type `sssraid,<enclosure>,<slot>`;
  5. NVMe controllers, `/dev/nvme0` to `/dev/nvme99`.

  The JSON lists `devices[]` with `name`, `info_name`, `type` and `protocol`, plus `open_error` when opening failed.
- **Device nodes:** when `/proc/devices` lists `megaraid_sas_ioctl`, the scan, and every open of a MegaRAID disk, run `mknod /dev/megaraid_sas_ioctl_node` (`os_linux.cpp`); with the old `megaraid_mm` driver, `/dev/megadev0`. The driver creates no node itself. The other `mknod` calls, for aacraid and 3ware, belong to device types that the scan never produces.
- **Drive database:** built in (r5706, 2025-04-27). By default smartctl also reads `drivedb.h` and `smart_drivedb.h` from the host's directories.
- **Raw values:** the built-in database formats attribute 5 as `raw16(raw16)` in 42 places. `raw.value` then holds all 48 bits, while `raw.string` starts with the counter that smartctl shows.
- **NVMe namespaces:** `nvme_number_of_namespaces` is the controller's maximum (`NN` in Identify Controller), not the number that exist.

## 4. smartctl builds (DH-04, P-10)

- **Files:** `scripts/build-smartctl.sh` and `.github/workflows/smartctl.yml`. They work like fio's (`docs/design/disk.md` §4).
  - A pull request that changes either file builds both architectures without publishing.
  - Only a manual run on `main` publishes, after checking that the x86_64 binary reports the pinned version.
- **Sources,** pinned in the script and checked before use. The signatures were checked once, when pinning; the script checks the SHA-256.

| Source | Version | License | URL | SHA-256 | Provenance |
|--------|---------|---------|-----|---------|------------|
| smartmontools | 7.5 | GPL-2.0-or-later | `https://github.com/smartmontools/smartmontools/releases/download/RELEASE_7_5/smartmontools-7.5.tar.gz` | `690b83ca331378da9ea0d9d61008c4b22dde391387b9bbad7f29387f2595f76e` (1122317 bytes) | Released 2025-05-12, the latest release (checked 2026-11-08). Signed by key `0C95 77FD 2C4C FCB4 B9A5 9964 0A30 812E FF3A EFF5` ("Smartmontools Signing Key (through 2025)"), the key that smartmontools.org's download page names. The MD5 matches the published `.md5`. |
| musl | 1.2.5 | MIT | `https://musl.libc.org/releases/musl-1.2.5.tar.gz` | `a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4` (1080786 bytes) | Only its `COPYRIGHT` is used: Alpine 3.22 links musl 1.2.5 statically but ships no copy of the notice. Signed by key `8364 8929 0BB6 B70F 99FF DA05 56BC DB59 3020 450F` (musl libc), as published at `https://musl.libc.org/musl.pub`. |

- **Build environment:** Alpine 3.22, pinned by digest and pulled from `ghcr.io/lowendaff/mirror`, natively on `ubuntu-24.04` and `ubuntu-24.04-arm`. The packages are `curl g++ linux-headers make`. smartmontools is not modified.
- **Configuration:** `./configure LDFLAGS=-static --sysconfdir=/proc/nonexistent --with-drivedbdir=no --with-update-smart-drivedb=no --with-nvme-devicescan=yes --with-libsystemd=no --with-libcap-ng=no --with-selinux=no`, then `make smartctl`.
  - `--with-drivedbdir=no` and `--sysconfdir=/proc/nonexistent` leave only the built-in drive database. The first drops `drivedb.h`; the second puts the path of `smart_drivedb.h` under `/proc`, where no such file can exist. A trial build ignored deliberately broken files in `/usr/local/etc` and `/etc`.
  - `--with-nvme-devicescan=yes` is the default already; it is stated so that the scan's NVMe coverage does not rest on a default.
  - The other options turn off what smartctl does not need here.
- **Checks before release:**
  - the binary is statically linked;
  - `smartctl --version` reports 7.5;
  - `smartctl --json --scan-open` exits with 0 and prints JSON;
  - with broken `smart_drivedb.h` files in `/usr/local/etc` and `/etc`, the scan reports no error.
- **Release:**
  - The tag is `smartctl-7.5-1`. The release is created as a draft, the assets are uploaded, and then it is published, passing `--latest=false` both times.
  - Assets: `smartctl-x86_64`, `smartctl-aarch64`, `smartmontools-7.5.tar.gz`, `build-smartctl.sh`, `COPYING-smartmontools`, `COPYRIGHT-musl` and `SHA256SUMS`.
  - Offering the source from the same place as the binaries meets GPL-2.0 §3. musl's MIT license asks for its notice in copies, hence `COPYRIGHT-musl`. libstdc++ and libgcc are linked statically too, under the GCC Runtime Library Exception, which allows that without further conditions. `fio-3.43-1` lacks musl's notice (§9).
- **Permissions:** as for fio, the build jobs only read, and only the release job has `contents: write`.
- **Pins:** the plugin declares `ToolSpec { name: "smartctl", version: "7.5", assets }`.
- **Trial build** (2026-11-08, aarch64, in local Docker): static, 7359592 bytes with debug information, `smartctl 7.5 2025-04-30 r5714`; JSON output and `--scan-open` worked.
- **Order:** this workflow lands first and runs on `main`. The plugin and its pins follow in a second PR.

## 5. The test item

### 5.1 Identity

- Crate `plugins/disk-health` (package `plugin-disk-health`); one plugin, `disk.health`, titled 硬盘健康.
- Requirements: the tool smartctl 7.5, no network, and only the platform kind `Dedicated`. Elsewhere the item appears in 本机不适用.
- Timeout: 5 minutes. Each disk gets 60 seconds.
- Order: after 系统基础信息 and before 硬盘性能 (DH-08).
- No configuration fields.

### 5.2 Flow

1. Note which of the device nodes of §6 exist. This comes before any smartctl call, because the scan itself creates the node.
2. **Scan** with `smartctl --scan-open --json`.
   - No devices: the item is skipped with smartctl 没有找到硬盘.
   - Exit status bit 0 or 1, or output that is not JSON: the item fails with smartctl 扫描硬盘失败：<reason>, the reason as in §5.6.
3. **Read** each device in scan order with `smartctl --json -x -d <type> <name>`, taking the type and name from the scan. `-x` reads all SMART data and logs, Device Statistics included; all of it is read-only. A read over 60 seconds is stopped.
4. **Classify** each result (§5.6) and take its values (§5.4).
5. **Remove duplicates, name and sort** (§5.3).
6. **Emit** the table, then one 说明 line for each disk that needs one.
7. **Remove** the device nodes that the item created (§6).
8. **Outcome** (§5.6).

smartctl runs from an argument list (no shell) with `kill_on_drop(true)`, so cancellation, by Ctrl-C or the item's timeout, ends it at once. The table is emitted only after every disk has been read, so a cancelled item shows no table. The device nodes are removed on every path, cancellation included.

### 5.3 Names, duplicates and order

- **Names:**
  - `/dev/sdX` and `/dev/hdX` show as `sdX` and `hdX`, as in 系统基础信息.
  - An NVMe controller `/dev/nvmeN` with exactly one namespace shows as that namespace's block device, for example `nvme0n1`; otherwise as `nvmeN`. The namespaces are the entries of `/sys/class/nvme/nvmeN/` named `nvme<a>n<b>`, or `nvme<a>c<c>n<b>` for one path of a multipath namespace, whose block device is `nvme<a>n<b>`. 系统基础信息 leaves the per-path devices out in the same way.
  - A disk reached only through a RAID card shows as `RAID 卡 <id> 号盘` (MegaRAID) or `RAID 卡 <enclosure>:<slot> 号盘` (3SNIC). When such disks sit on more than one card, the card's SCSI host is added, for example `RAID 卡 host1 的 18 号盘`.
- **Duplicates:** results with the same `serial_number` are one disk. The one with a device name is kept; among the rest, the first in scan order. Results without a serial are never merged. Serials are compared in memory and never shown. Duplicates are removed before naming: whether RAID-card disks show their card's host depends on the RAID-card disks that remain.
- **Order:** disks with a device name first, sorted by name like 系统基础信息's disk table (plain string order, so `nvme0n1` comes before `sda`); then the RAID-card disks, by host and number.

### 5.4 Values

From the JSON of `-x`; a missing value shows `—`.

| Column | SATA, also behind RAID cards | NVMe | SAS, also behind RAID cards |
|--------|------------------------------|------|-----------------------------|
| 通电时间 | `power_on_time.hours` | `power_on_time.hours` | `power_on_time.hours`, from the background scan log (or Seagate's vendor page); some disks have neither |
| 温度 | `temperature.current` | `temperature.current` | `temperature.current` |
| 已用寿命 | `endurance_used.current_percent`: Device Statistics page 7 when present, else smartctl's estimate from attributes named `SSD_Life_Left*` or `Wear_Leveling*`; hard disks have neither | `endurance_used.current_percent` (the health log's `percentage_used`, which may exceed 100) | `endurance_used.current_percent`, from the Solid State Media log; hard disks have none |
| 累计写入 | Device Statistics page 1, offset 0x018 (logical sectors written), × `logical_block_size`. Attribute 241 is not used: its unit differs by vendor. | `nvme_smart_health_information_log.data_units_written` × 512000 bytes (units of 1000 × 512 bytes) | `scsi_error_counter_log.write.gigabytes_processed`, a string with three decimals, × 10^9 bytes |
| 坏块 | `<reallocated> / <pending>`: Device Statistics page 3 offsets 0x020 and 0x038, else attributes 5 and 197 | `nvme_smart_health_information_log.media_errors` | `scsi_grown_defect_list` |
| 接口错误 | Device Statistics page 6 offset 0x018 (interface CRC errors), else attribute 199 | `—` | `—` |

- Device Statistics entries are found by page `number` and `offset`. An entry without `value` is invalid and counts as missing.
- An attribute's count is the leading number of its `raw.string`, never `raw.value` (§3.3). A string that does not start with a digit counts as missing.
- 坏块 for SATA: a missing side shows `—`, as in `0 / —`. When both sides are missing, the cell is `—`.

### 5.5 Output

An example; only the `sda` row is real, from the owner's server:

```
【硬盘健康】
  硬盘
  设备             健康      通电时间    温度   已用寿命  累计写入  坏块       接口错误
  nvme0n1          通过      12034 小时  38 °C  4%        21.7 TB   0          —
  sda              通过      46164 小时  25 °C  23%       48.2 TB   0 / —      3
  sdb              通过      35678 小时  31 °C  —         112.4 TB  8 / 0      0
  sdc              读取失败  —           —      —         —         —          —
  sdd              不支持    —           —      —         —         —          —
  RAID 卡 23 号盘  未通过    51208 小时  44 °C  —         —         1532 / 16  0
  说明              sdc 读取失败：打不开设备，或读不到设备识别信息
  说明              sdd 不支持 SMART
  完成，用时 6.2 秒
```

- The table's id is `disks` and its title 硬盘. The 说明 lines follow it, in row order.
- **Formats:** 通电时间 `46164 小时`, 温度 `25 °C`, 已用寿命 `23%`.
- **累计写入,** in decimal units: below 1000 GB in GB, below 1000 TB in TB, otherwise in PB, with one decimal. Extra digits are cut, not rounded, with integer arithmetic, like the free space shown by 硬盘性能: 999,999,999,999 bytes show as `999.9 GB`, never `1000.0 GB`.

### 5.6 Health, errors and outcomes

| 健康 | When | 说明 line |
|------|------|-----------|
| 通过 / 未通过 | `smart_status.passed` | none |
| 未知 | no `smart_status.passed` | `<设备> 的 SMART 功能已关闭，本程序不会替它打开` when `smart_support.enabled` is false; otherwise none |
| 不支持 | `smart_support.available` is false | `<设备> 不支持 SMART` |
| 读取失败 | exit status bit 0 or 1; a read over 60 seconds; output that is not JSON | `<设备> 读取失败：<reason>` |

- **Precedence:** 读取失败, then 不支持, then smartctl's verdict.
- **The reason for 读取失败:**
  - a read over 60 seconds: 超过 60 秒仍未读完，已停止, as 硬盘性能 words its time limit;
  - output that is not JSON: smartctl 的输出无法解析;
  - otherwise smartctl's first `error` message, verbatim, for example `Smartctl open device: /dev/sdc [SAT] failed: No such device`;
  - without one, by exit status: bit 1 打不开设备，或读不到设备识别信息; bit 0 smartctl 不接受命令参数.
- Exit status bits 2 to 7 are not failures: whatever smartctl read is shown.
- **The item's outcome:**
  - **skipped** when the scan finds no devices: smartctl 没有找到硬盘;
  - **failed** when the scan fails: smartctl 扫描硬盘失败：<reason>;
  - **failed** when every disk is 读取失败, after the table and its 说明 lines: 所有硬盘都读取失败, as 硬盘性能 fails when no block size has a result. 不支持 is not a failure;
  - **completed** otherwise, also when a disk is 未通过: that is a result, not a failure of the test.
- A device node that cannot be removed (§6) adds a 说明 line: 无法删除 `<path>`：`<error>`.

## 6. Device nodes (DH-05)

- **Why:** smartctl creates `/dev/megaraid_sas_ioctl_node`, or `/dev/megadev0` with the old driver, to reach MegaRAID disks (§3.3). Everything else the program creates lives in its run directory and is removed with it, so these nodes would be the only thing left behind.
- **Rule:** before the scan, note which of the two paths exist. After the last read, remove each path that did not exist before and exists now. A node that existed before is never touched.
- **Every path:** a guard performs the removal, so it happens on completion, failure, timeout and Ctrl-C alike. Only a forced end of the program, such as `kill -9`, leaves a node behind.
- **Safety:** removing the name does not affect a process that has the node open. smartctl and smartd recreate the node whenever they next open a MegaRAID disk, because the `mknod` runs on every open, so removing a node that another program created meanwhile does no lasting harm.
- On the owner's server, smartd created the node at boot, so the item leaves it alone; the real run checks this (§8).

## 7. Testing

### 7.1 Tests

- **Parsing:** fixtures for every column of every protocol, and for 读取失败, 不支持 and SMART turned off.
- **Pure logic:** names, duplicates, order, formats (including boundaries such as `999.9 GB`) and the item's outcome.
- **The flow:** the few lines that run smartctl are kept apart from the rest, and tests replace them with a fake that returns fixtures. This covers the scan, the reads, duplicates and the table from end to end, and checks:
  - the arguments passed to smartctl;
  - that a disk taking over 60 seconds shows 读取失败;
  - that Ctrl-C stops the item at once, without a table.

  Time is simulated with tokio's paused clock.
- **Device nodes:** in a temporary directory, regular files stand in for the nodes. One created during the item is removed; one present before is kept.
- **Mutation round** (2026-11-09): each mutation was applied to a fresh copy of the repository, and the tests had to fail. All 52 were killed. The one that first survived, attribute 198 instead of 197 for pending sectors, led to a test with attribute 197 and no page 3.

  | Area | Mutations |
  |------|-----------|
  | Exit status and messages | only bit 0 fails a read; bit 2 fails a read; messages of the wrong severity; the bits' texts swapped; JSON without an exit status accepted |
  | Scan and reads | `-a` instead of `-x`; a scan that does not open the devices; a failed scan accepted; a 30-second limit; a failed read accepted |
  | Health | 不支持 inverted; SMART off inverted |
  | Values | four wrong Device Statistics offsets; attributes 9, 198 and 197 instead of 5, 197 and 199; no CRC fallback; NVMe units of 512 bytes; SAS's read log; sectors without their size; hexadecimal counted; the fraction padded on the left; finer than a byte accepted; ATA disks not read |
  | Names and order | megaraid without its comma; every card on host 0; the first of several namespaces; a path's numbers swapped; no card in names; no NVMe controller recognized; not sorted; sorted backwards |
  | Merging and output | a card path kept over a named device; readings without a serial merged; several cards inverted; the GB to TB boundary; rounded instead of cut; note ids from 0 |
  | Device nodes | any error counts as missing; removal without checking existence; nothing removed on drop; the noted nodes not cleared; nodes noted after the scan; the node note's id |
  | Flow and registry | Ctrl-C taken for a timeout; one failed disk fails the item; no skip without disks; every platform; 硬盘健康 after 硬盘性能 |

### 7.2 Fixtures (DH-06)

- **SATA:** the owner's four SSDs, each read directly and through the RAID card, so 8 files, taken through pipes only. On the server, each serial is replaced by a placeholder, the same for both paths of one disk, so that duplicates are tested on real data. WWNs are removed on the server too.
- **Scan:** the owner's `--scan-open` output, which contains no serials.
- **NVMe and a virtual disk:** with `smartctl-7.5-1`, on 2026-11-09, a temporary branch whose workflow ran on its own push read the disks of seven GitHub runners; a branch was enough, and no draft PR was needed. Two x86_64 runners had Azure's virtual NVMe disk ("MSFT NVMe Accelerator v1.0"): a real NVMe sample, with every count at 0, and one namespace although `nvme_number_of_namespaces` is 127. The others had Hyper-V's virtual SCSI disk, which turned out to be a real sample of exit status 2 without a message, not of 不支持. NVMe with real counts and 不支持 are written by hand.
- **Hard disks and SAS:** written by hand from the fields in smartmontools 7.5's source, and marked so in the fixtures' README.
- **读取失败:** this repository's smartctl opening a missing disk in a container (`Smartctl open device: /dev/sdz [SAT] failed: No such device`, exit status 2).

### 7.3 CI

- The runners are virtual machines, so the item does not apply there: it neither runs nor downloads smartctl, and smoke cannot catch a wrong URL, SHA-256 or size among smartctl's pins.
- **A new step, "Tool downloads",** in the Linux job on both runners.
  - A test that is skipped by default downloads and verifies every tool in the registry for the runner's architecture, with the existing provisioning code (`provision::required_tools` and `provision::provision_all`). Today that is fio and smartctl; later tools are included automatically.
  - The README's table of gates gets a row. No required check is added, since it is a step of an existing job.
- Smoke, smoke mutants and the distribution matrix are unchanged. On the runners they show 本机不适用：硬盘健康.

## 8. Real run on the owner's dedicated server (DH-07)

- **When:** once the plugin passes its local and CI gates, before the merge is requested.
- **Steps:**
  1. Record the listing of `/root` and the birth time of `/dev/megaraid_sas_ioctl_node`.
  2. Create `/root/lowendaff-bench-test` and copy the x86_64 binary into it through a pipe.
  3. In it, run `./lowendaff-bench --only disk.health`. The program creates its run directory there, downloads smartctl from this repository's release, reads every disk (8 reads, all read-only) and removes its run directory.
  4. Remove `/root/lowendaff-bench-test`, also if the program crashed.
  5. Compare: `/root` must list the same entries as before, and the node's birth time must be unchanged.
- 硬盘性能 does not run, so no test file is written and the disks get no load.
- The output contains no serials, host names or IP addresses by design, and is shown to the owner as it is.
- **Result** (2026-11-09): the x86_64 build of commit `f82d404` ran as above. It downloaded and verified smartctl, read the four disks directly and through the RAID card in 0.3 seconds, and showed the four rows of §3.1 (with 46167, 38112, 53544 and 35124 hours) without a 说明 line; the item completed and the program exited with 0. Afterwards the test directory held only the binary, `/root` listed the same entries as before, and the node's birth time was unchanged.

## 9. Risks and limitations

- **Untested hardware:** NVMe on real hardware (at most a runner's virtual NVMe), SAS, hard disks, 3SNIC cards, several RAID cards in one server, and multipath NVMe. Some of their fixtures are written by hand (§7.2).
- **RAID cards the scan does not cover:** smartctl's scan lists the disks of MegaRAID and 3SNIC cards only. Disks on HP Smart Array (`cciss`), Adaptec (`aacraid`), Areca or 3ware cards need explicit device types that the scan does not produce, so the item shows only what the scan lists.
- **RAID volumes:** a volume that a card presents as a disk appears as a row of its own, usually 不支持. Its member disks appear separately when the scan lists them.
- **Health through RAID cards:** for a SATA disk read only through a MegaRAID card, 通过 and 未通过 come from smartctl's check of the attributes' thresholds, not from the disk's own SMART status, which does not come through the card (§3.1).
- **Virtual disks:** some answer smartctl so badly that the read fails, as Hyper-V's virtual SCSI disk does (exit status 2, §7.2). The item applies to dedicated servers only, where such disks are rare.
- **Duplicates by serial:** two different disks that report the same serial, as some cheap disks and bridges do, show as one. A disk reached through two paths shows twice when one of the reads fails.
- **Estimated endurance:** for SATA SSDs without Device Statistics page 7, 已用寿命 is smartctl's estimate from vendor attributes. On the owner's disks, the estimate differed from the standard value by up to 4 points.
- **Waking disks:** reading wakes a disk in standby, as smartctl does by default.
- **Timeout:** when 5 or more disks each hang for 60 seconds, the item times out at 5 minutes and shows no table.
- **A forced end of the program** leaves behind a device node that the item created (§6).
- **musl's notice:** `fio-3.43-1` does not include musl's `COPYRIGHT`. Fixing that needs a new release, `fio-3.43-2`, and new pins. The proposal is to add the notice at fio's next rebuild.
- **Builds are not bit-for-bit reproducible,** and a published immutable release cannot be fixed, as for fio.
- **The real run** (§8) is one exception to read-only probing of the owner's servers.

## 10. Implementation plan

1. **First PR:** this document, the M3 line in the plugin system design, `scripts/build-smartctl.sh`, `.github/workflows/smartctl.yml`, and a row in the README's third-party tools.
2. **After the owner merges it:**
   - run the workflow on `main`;
   - verify the assets, `SHA256SUMS`, `gh release verify` and `gh release verify-asset`.
3. **Fixtures:** the owner's disks through pipes, and NVMe and a virtual disk from a temporary branch on GitHub's runners.
4. **Second PR:** `plugins/disk-health`, its registry line, the "Tool downloads" step and the README updates. Then the real run (§8).
5. **Merge:** after the owner's approval.
