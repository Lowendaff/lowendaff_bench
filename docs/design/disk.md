# Disk Performance Test Item (硬盘性能)

- Status: **Accepted** 2026-11-08. The owner decided DK-01..DK-07 (§2), which include the plugin system's P-10.
- Scope: one test item, `disk.fio`, which measures the filesystem of the test directory with fio the way YABS does, and the pinned fio builds it needs (§4). Hardware checks for dedicated servers are separate work (DK-04).

## 1. Goals and non-goals

Goals:

1. Measure the test directory's filesystem with fio exactly as YABS v2026-10-31 does, so that a result can be compared with a YABS result from the same machine.
2. Use only the fio that this repository builds and pins (P-10, P-11): never a host fio, and no other host program.
3. Never report a number that is not the disk's. RAM disks and filesystems without direct I/O are skipped with the reason.
4. Degrade per block size: a block size that times out or fails shows 超时 or 出错, and the others still print.

Non-goals:

- YABS's dd fallback when fio fails: dd is a host program (P-11).
- Disks or mount points other than the test directory's.
- Improving on YABS's method, for example by running each test for a fixed time (DK-01).
- A report file (P-06).
- Hardware checks for dedicated servers (SMART, SMBIOS, RAID controllers, IPMI, memory ECC errors), which come after this item (DK-04).

## 2. Owner's decisions

| ID | Question | Decision |
|----|----------|----------|
| DK-01 | What to measure | **The YABS method:** one mixed random read and write test (50/50) at each of 4k, 64k, 512k and 1m. The alternatives were linux_bench's four tests unchanged, or the same four tests run for a fixed time. Rationale: results can be compared with the YABS results posted in the LowEnd community, and linux_bench's test measures the wrong thing on many hosts (§3). |
| DK-02 | How closely to follow YABS | **Identical** parameters, file sizes, free-space rules and number formatting (§5), one plugin for every platform, and fio 3.43. The only differences follow from this design: the test directory is the run directory (D-14), fio is the pinned build (P-11), there is no dd fallback, and the output is Chinese. |
| DK-03 | P-10: where the fio builds come from | **Built and released by this repository** (§4). A manual workflow builds static binaries from checksum-pinned sources and publishes them, together with those sources, as an immutable release. The alternatives were a separate tools repository, and YABS's own fio builds; the latter fall short of this project's standards (§3). |
| DK-04 | Dedicated-server checks (SMART, dmidecode and so on) | **Separate from this item, and designed after it.** SMART becomes the item 硬盘健康 (dedicated servers only, with a pinned smartctl). SMBIOS facts (board, BIOS, memory modules) join the dedicated variant of 系统基础信息. RAID controllers, IPMI and memory ECC errors follow. |
| DK-05 | Output layout | **YABS's structure:** rows 读取, 写入 and 合计, one column per block size, two block sizes per table (§5.4). |
| DK-06 | Test directory on a RAM disk | **Skip** when the test directory is on tmpfs or ramfs. YABS has no such check. The method is unchanged; the number would just not be the disk's. |
| DK-07 | CI | **Smoke runs the full test** on both runners. Smoke mutants skip it with `--skip disk.fio`, since they check only the root check and the run directory's removal. |

## 3. Background

**linux_bench** (`Lowendaff/linux_bench`, function `run_disk_test`):

- It runs four fio jobs: 4K random write, 4K random read, 128K sequential write and 128K sequential read. Each uses `--size=50m --runtime=10 --iodepth=32 --direct=1` and libaio, or sync when libaio is missing. fio comes from the package manager. Each result prints as `X MB/s (N IOPS)`.
- Without `--time_based`, each job ends after 50 MB. With these parameters on Docker Desktop (Apple silicon, kernel 7.0.14-linuxkit), the four jobs ran for 358, 44, 54 and 5 ms, and the 128K read reported 9.77 GiB/s: a cache, not a disk.
- The test file is created under `mktemp -d "${TMPDIR:-/tmp}/linux_bench.XXXXXX"`. Both of the owner's sampled hosts mount `/tmp` as tmpfs (the fixtures' mountinfo). tmpfs accepts O_DIRECT since Linux 6.6 (commit e88e0d3, "tmpfs: trivial support for direct IO"), and fio reported 1471 MiB/s with `direct=1` on tmpfs on kernel 7.0. On such hosts, linux_bench reports the speed of memory.

**YABS** v2026-10-31 (`masonr/yet-another-bench-script`, `yabs.sh`):

- **Setup:** `fio --name=setup --ioengine=libaio --rw=read --bs=64k --iodepth=64 --numjobs=2 --size=$FIO_SIZE --runtime=1 --gtod_reduce=1 --direct=1 --minimal`. fio lays out the whole file before this one-second read.
- **Tests:** `fio --name=rand_rw_$BS --ioengine=libaio --rw=randrw --rwmixread=50 --bs=$BS --iodepth=64 --numjobs=2 --size=$FIO_SIZE --runtime=30 --gtod_reduce=1 --direct=1 --group_reporting --minimal`, under `timeout 35`, for each of 4k, 64k, 512k and 1m. There is no `--time_based`.
- **File:** `FIO_SIZE` is 2G, or 512M on aarch64, arm and riscv64. The file is in `$YABS_PATH/disk`, under the current directory.
- **Before testing:**
  - The disk test is skipped when `df -k .` shows less than 2097152 KiB available (524288 KiB on ARM).
  - On ZFS (`/sys/module/zfs/parameters/spa_asize_inflation` exists), YABS warns that results may be inaccurate when the free space is below twice that value in GB, as printed by `df -h`.
- **fio:** a local fio when one is installed, otherwise YABS's own static build from its `fio-3.43` release. When the download or the tests fail, YABS runs a dd test instead.
- **Values:** fields 7, 8, 48 and 49 of the terse line (read KiB/s, read IOPS, write KiB/s, write IOPS). The totals are their sums.
- **YABS's fio builds** (release `fio-3.43`):
  - They are static-pie musl binaries with the libaio and io_uring engines, and they ran YABS's parameters correctly in a test.
  - `build-fio.yml` builds them with actions pinned to tags, downloads the fio archive without verification and libaio over plain HTTP, and takes its toolchains from YABS's own release or musl.cc.
  - The release includes fio's source but not libaio's, and it is mutable.

## 4. fio builds (DK-03, P-10)

- **Files:** the build script `scripts/build-fio.sh` and the workflow `.github/workflows/fio.yml`.
  - A pull request that changes either file runs the build jobs only, so both architectures are built and checked before merging.
  - Only a manual run on `main` publishes the release. Its tag is the workflow's `TAG`, and the release job first checks that the x86_64 binary reports the same fio version.
- **Sources,** pinned in the script and checked before use:

| Source | Version | License | URL | SHA-256 | Note |
|--------|---------|---------|-----|---------|------|
| fio | 3.43 | GPL-2.0 | `https://brick.kernel.dk/snaps/fio-3.43.tar.gz` | `6f57893c7a73d9ed72e9e2b6e15f136a99c137bd6a7645fdeeb4a40b3e9da78d` (8117085 bytes) | Upstream publishes no signature. The tarball is identical, file by file, to the `fio-3.43` tag of `axboe/fio` (commit `d2dcd7e053d1902d2420f151c83990e6acdd9f8c`, 736 files; checked 2026-11-08). |
| libaio | 0.3.113 | LGPL-2.1 | `https://deb.debian.org/debian/pool/main/liba/libaio/libaio_0.3.113.orig.tar.gz` | `2c44d1c5fd0d43752287c9ae1eb9c023f04ef848ea8d4aafa46e9aedb678200b` | The upstream tarball as kept by Debian; the hash matches Debian's `libaio_0.3.113-10.dsc`. The pagure release URL returned 404. |

- **Build environment:** Alpine 3.22, pinned by digest and pulled from `ghcr.io/lowendaff/mirror`. Each architecture builds natively, on `ubuntu-24.04` (x86_64) and `ubuntu-24.04-arm` (aarch64).
  - libaio is built as a static library, then fio with `./configure --build-static --disable-native`.
  - Compilers come from Alpine's signed packages and follow the 3.22 branch, so builds are not bit-for-bit reproducible. Two builds from the same script on the same day, one in local Docker and one in CI, already produced different binaries (2026-11-08).
- **Checks before release:**
  - the binary is statically linked;
  - `fio --version` prints `fio-3.43`;
  - `fio --enghelp` lists libaio;
  - a short run with YABS's parameters succeeds on the runner.
- **Release:**
  - The tag is `fio-3.43-1`. The trailing build number allows a rebuild, because an immutable release's tag cannot be reused.
  - The release is created as a draft, all assets are uploaded, and only then is it published. It is not marked as the latest release.
  - Assets: `fio-x86_64`, `fio-aarch64`, both source tarballs, `build-fio.sh`, both license texts and `SHA256SUMS`.
  - Offering the sources from the same place as the binaries satisfies GPL-2.0 §3. LGPL-2.1's relinking requirement is met because the complete sources of fio and libaio are offered.
- **Immutable releases:** enabled in the repository settings before the first release. Assets and the tag cannot change after publishing, and GitHub adds a release attestation.
- **Permissions:** the build jobs only read (plus `packages: read` for the image); only the release job has `contents: write`.
- **Pins:** the plugin declares `ToolSpec { name: "fio", version: "3.43", assets }`, with each asset's release URL, SHA-256 and size (plugin system design, §9).
- **Order:** this workflow lands first and runs on `main`. The plugin and its pins follow in a second PR, as the image mirror did before the distribution matrix used it.

## 5. The test item

### 5.1 Identity

- Crate `plugins/disk` (package `plugin-disk`); one plugin, `disk.fio`, titled 硬盘性能.
- Requirements: the tool fio 3.43, no network, every platform.
- Timeout: 10 minutes, which leaves room to lay out 2 GiB on a slow disk.
- No configuration fields: every parameter is YABS's (DK-02).

### 5.2 Flow

1. The test file is `test.fio` in the plugin's private directory (`Context::work_dir`). That directory is in the run directory, never in `/tmp`.
2. **RAM disk (DK-06).** `statfs` gives the filesystem type. tmpfs (`0x01021994`) or ramfs (`0x858458f6`) skips the item: 测试目录在内存盘上，测到的会是内存速度；请在硬盘上的目录运行，或用 --work-dir 指定.
3. **Free space.** `statvfs` gives the space available. Less than 2 GiB on x86_64, or 512 MiB on aarch64, skips the item, for example: 测试目录可用空间不足 2 GiB（只有 1.2 GiB）. Exactly the threshold passes, as in YABS.
4. **ZFS.** For ZFS (`0x2fc12fc1`), when `spa_asize_inflation` is readable and the space available is below twice that value in GiB, a 说明 fact precedes the tables: 测试目录在 ZFS 上，可用空间少于 48 GiB，结果可能不准（与 YABS 的提示相同）.
5. **Setup,** with YABS's parameters.
   - When fio prints "does not support direct=1/buffered=0" or "does not support O_DIRECT" (`filesetup.c` and `ioengines.c` in fio 3.43), the item is skipped: 测试目录所在的文件系统不支持直接 I/O.
   - Any other failure fails the item, with the last lines of fio's error output.
6. **Tests,** in the order 4k, 64k, 512k, 1m, each with YABS's parameters and a 35-second limit.
   - A test over the limit shows 超时.
   - A fio error, a nonzero error field, or no terse line for the job shows 出错.
   - A 说明 line after the table gives the reason, and testing continues.
   - When no block size has a result, the item fails.
7. **Cleanup.** The test file is deleted at once, to free the space for later items. A failure to delete is ignored, because the run directory is removed later anyway.

fio is started with an argument list (no shell) and `kill_on_drop(true)`, so cancellation (Ctrl-C or the item's timeout) ends it at once.

### 5.3 Values

Parsing and formatting are pure functions.

- **Parsing:** the line that starts with `3;` and whose third field is the job name. Field 5 is the error, and fields 7, 8, 48 and 49 hold read KiB/s, read IOPS, write KiB/s and write IOPS.
- **Totals:** read plus write, taken from the raw KiB/s and IOPS before formatting.
- **Speed,** as YABS's `format_speed` (all values truncated to two decimals, not rounded):
  - 976563 KiB/s or more: GB/s, computed as × 1024 / 10^9;
  - 977 KiB/s or more: MB/s, computed as × 1024 / 10^6;
  - otherwise: KB/s, computed as × 1024 / 10^3.
- **IOPS,** as YABS's `format_iops`: 1000 or more is divided by 1000, truncated to one decimal and suffixed with `k`; a smaller value is printed as is.
- **A cell** is `<speed>（<IOPS>）`, for example `92.82 MB/s（23.2k）`.

### 5.4 Output

An example. The numbers are made up, and the widest line is 72 columns:

```
【硬盘性能】
  测试方式          与 YABS 相同：读写各半的随机测试，测试文件 2 GiB
  4K 与 64K
  块大小  4K                    64K
  读取    92.82 MB/s（23.2k）   1.09 GB/s（17.0k）
  写入    93.07 MB/s（23.2k）   1.09 GB/s（17.1k）
  合计    185.89 MB/s（46.4k）  2.19 GB/s（34.2k）
  512K 与 1M
  块大小  512K               1M
  读取    1.52 GB/s（2.9k）  1.61 GB/s（1.5k）
  写入    1.60 GB/s（3.1k）  1.72 GB/s（1.6k）
  合计    3.13 GB/s（6.1k）  3.33 GB/s（3.2k）
  完成，用时 151.6 秒
```

- Each table appears as soon as its two block sizes are done.
- The ZFS 说明 line comes before the first table. A 说明 line about 超时 or 出错 follows the table it concerns.

## 6. Testing

- **Unit tests:**
  - **Parsing:** fixtures from real fio `--minimal` output on both architectures, including warning lines before the terse line.
  - **Formatting:** expected values come from YABS's own `format_speed` and `format_iops` at the boundaries (976 and 977 KiB/s, 976562 and 976563 KiB/s, 999 and 1000 IOPS), and the output must match character for character.
  - **Decisions:**
    - the free-space threshold, including exactly the threshold;
    - recognizing RAM disks and ZFS, and the ZFS 说明 threshold;
    - recognizing a filesystem without direct I/O;
    - 超时 and 出错 cells, and the item failing when no block size has a result;
    - the tables' contents.
- **Mutation rounds:** on parsing, formatting and the decisions. Every mutant must be killed.
- **Comparison with YABS's fio:** one machine runs YABS's full procedure with this repository's fio and with YABS's fio, alternating, twice each.
  - Both architectures run on GitHub runners, in a temporary draft PR that is never merged.
  - Local Docker proved unsuitable (2026-11-08, Docker Desktop on Apple silicon). Caches served the 512 MiB file at 6 to 15 GiB/s, so the 512k and 1m tests ended after about 40 ms. The same binary's results then varied by up to 78% between runs, which hid any difference between the two builds.
- **Real runs:** the full test in local Docker, with the test directory on overlayfs (tested) and on tmpfs (skipped).
- **CI (DK-07):** Smoke runs the full test on the x86_64 and aarch64 runners, which also exercises downloading and verifying fio. Smoke mutants pass `--skip disk.fio`.

## 7. Risks and limitations

- **Inherited from YABS:**
  - Tests are not time-based, so a fast disk finishes a test early. On a cache-backed disk, the 512k and 1m tests ended after about 40 ms (§6).
  - The 512 MiB file on aarch64 fits more easily in caches.
- **ZFS before 2.3** accepts O_DIRECT without bypassing the ARC, so reads can come from memory. YABS has the same limitation, and the 说明 line covers only free space.
- **Builds are not bit-for-bit reproducible** (§4).
- **A published immutable release cannot be fixed.** A rebuild needs a new tag (`fio-3.43-2`) and new pins.
- **The comparison with YABS's fio** runs on GitHub runners, not on the owner's hardware.
- **CI time:** each Linux CI job takes about 2.5 minutes longer (DK-07).
- **Kernel support:** fio needs the kernel's AIO system calls (`io_setup`). A kernel or sandbox without them fails at setup, with fio's error.

## 8. Implementation plan

1. **First PR:** this document, P-10 in the plugin system design, `scripts/build-fio.sh`, `.github/workflows/fio.yml`, and a README section on third-party tools.
2. **After the owner merges it:**
   - enable immutable releases;
   - run the workflow on `main`;
   - verify the assets, the hashes and the attestation.
3. **Second PR:** `plugins/disk`, its registry line, `--skip disk.fio` for smoke mutants, and README and documentation updates. A temporary draft PR runs the comparison with YABS's fio on both architectures.
4. **Merge:** after the owner's approval.
