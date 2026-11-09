# CPU Test Items (CPU 性能 and Geekbench 7)

- Status: **Accepted** 2026-11-09. The owner decided CP-01..CP-17 (§2), confirmed the design section by section, and accepted the risks (§10).
- Implementation (M4): one workflow for every pinned tool build, with the releases `sysbench-1.0.20-1` and `fio-3.43-2` (§4); then the test items in `plugins/cpu` and `plugins/geekbench`, and the crate `crates/swap-file` (§5–§8).
- Scope: two test items, `cpu.sysbench` (CPU 性能) and `cpu.geekbench` (Geekbench 7), and the tool builds they need.

## 1. Goals and non-goals

Goals:

1. Measure single- and multi-thread CPU throughput with sysbench, with the parameters of the owner's earlier `linux_bench` script, so that the numbers compare with the owner's earlier measurements (CP-04).
2. Show Geekbench 7's single- and multi-core scores and the public result link, as the site's review templates ask (CP-01).
3. Run Geekbench on small machines too, with temporary swap, while the host's own processes stay protected (CP-06, CP-13).
4. Leave nothing behind: the tools, the swap file and Geekbench's processes go when the item ends (§6, §7).
5. Download nothing that cannot run: glibc and IPv4 are checked before the download (CP-11).

Non-goals:

- Geekbench Pro, with its offline results and its license for automation and commercial use (CP-05).
- Geekbench's GPU and AI benchmarks.
- Geekbench 6 on systems whose glibc is too old for Geekbench 7 (CP-11).
- The claim link that Geekbench prints after an upload: whoever has it can add the result to their own account.
- Memory bandwidth and sysbench's other tests. Memory performance is a test item of its own, later.

## 2. Owner's decisions

| ID | Question | Decision |
|----|----------|----------|
| CP-01 | The next test item | **CPU performance with sysbench, plus Geekbench 7** (2026-11-09). The owner added Geekbench 7, to be downloaded as Primate Labs builds it rather than built from source (CP-02). |
| CP-02 | Where Geekbench comes from | **Primate Labs' own builds, downloaded at run time** from `cdn.geekbench.com` and pinned by URL, SHA-256 and size (§6.1). They are neither built nor re-hosted here: Geekbench is closed source, and its license forbids redistribution (EULA v7 §4d). This is the one exception to P-10. |
| CP-03 | One test item or two | **Two.** CPU 性能 runs sysbench, and Geekbench 7 is an item of its own. A failed Geekbench download or upload cannot cost the sysbench results then, and `--skip` can leave out Geekbench alone. In one item, either missing tool would skip both. |
| CP-04 | sysbench's parameters | **As in the owner's `linux_bench`:** `--cpu-max-prime=10000`, 20 seconds with one thread, then 20 seconds with every usable thread; the result is events per second. The alternatives were runs of 10 seconds, and the multi-thread run alone. |
| CP-05 | Geekbench's license | **The free version, which uploads every result** (§3.2). The owner does not use it commercially. Pro licenses are not supported. The alternatives were supporting both, and Pro only. The free license's terms are a risk that the owner accepts (§10). |
| CP-06 | Machines with little memory | **Temporary swap,** like `linux_bench`, removed after the item. Its size follows the measured need (§7.1): the estimate `1 GiB + 1 GiB × threads` against the available memory plus free swap; the shortfall rounded up to whole GiB, at most 8 GiB, leaving 1 GiB free on the disk; no swap below 1 GiB. The first rule, `linux_bench`'s 2 GB of swap when memory and swap make less than 2 GB, was corrected after the runners measured 3.6 GiB with 4 threads (§3.2). The alternatives were running as is, with Geekbench killed first when memory runs out, and skipping below 2 GB. |
| CP-07 | How the swap is turned on and off | **`swapon` and `swapoff` through `libc`,** in a small crate of their own (§7.2): the only exception to `unsafe_code = "forbid"`. The swap header is written in safe Rust. The alternatives were the host's `swapon` and `swapoff` programs, against P-11, and no swap. |
| CP-08 | Where the items run | **系统基础信息, 硬盘健康, CPU 性能, 硬盘性能, Geekbench 7.** SMART still reads idle disks (DH-08), and sysbench runs before fio's writes. Geekbench, the longest item, with a download, an upload and perhaps swap, runs last, where it cannot disturb the disk test. |
| CP-09 | How CPU 性能 shows its results | **A two-row table and a ratio line** (§5.3): 单线程 and 多线程 with their thread counts, then "多线程是单线程的 N 倍". The alternatives were two plain lines, and the table alone. |
| CP-10 | Whether Geekbench runs by default | **Yes,** like YABS; `--skip cpu.geekbench` leaves it out. The output shows the scores and the link, without a line about the public upload (the owner's change on 2026-11-09). The alternative was an item that is off by default, which the plugin system does not have. |
| CP-11 | Systems whose glibc is too old | **Checked before the download, and skipped** (§6.2). Geekbench 6 as a fallback was rejected, because its scores do not compare with Geekbench 7's; so was downloading 0.5 GB to fail. |
| CP-12 | Where to capture Geekbench's online output | **On GitHub's runners,** from a temporary branch (§3.2). The alternatives were the owner's hosts, and emulated runs on the developer's Mac. |
| CP-13 | The host's own processes when memory runs out | **Geekbench is killed first:** it starts with `oom_score_adj` 1000 (§7.3). |
| CP-14 | sysbench errors | **Each run is retried up to 3 times** (the owner, 2026-11-09), so it runs at most 4 times. A 说明 line reports retries that succeeded. |
| CP-15 | Geekbench errors | **Only a failed upload is retried,** up to 3 times, 30 seconds apart (§6.4). Nothing was published then, and the cause is usually the network. A kill for lack of memory and a timeout are not retried, as a rerun would most likely end the same way. The alternatives were retrying every error, and no retries. |
| CP-16 | Real runs | **On all three of the owner's hosts,** once each: the VM, the dedicated server and the ARM board, with `--only cpu.sysbench,cpu.geekbench` (§9.5). Each run publishes one Geekbench result. |
| CP-17 | How the tools are built | **One workflow for every pinned tool,** as DH-04 foresaw for a third tool (§4). fio is rebuilt as `fio-3.43-2`, with musl's notice. |

## 3. Background

### 3.1 sysbench 1.0.20

- **Release:** 1.0.20 of 2020-04-24 is the latest. sysbench is GPL-2.0-or-later and bundles LuaJIT 2.1 (MIT) and Concurrency Kit (BSD-2-Clause). Distributions link LuaJIT and the MariaDB and PostgreSQL client libraries dynamically, so their packages cannot be copied to other systems.
- **Source:** GitHub's archive of the tag `1.0.20`, `https://github.com/akopytov/sysbench/archive/refs/tags/1.0.20.tar.gz`, 1509630 bytes, SHA-256 `e8ee79b1f399b2d167e6a90de52ccc90e52408f7ade1b9b7135727efe181347f`. Homebrew's formula pins the same SHA-256, and the SHA-512 matches Arch Linux's PKGBUILD (checked 2026-11-09).
- **Trial build** (2026-11-09, aarch64, Alpine 3.22 in local Docker): `./autogen.sh` (which needs bash), `./configure --without-mysql --without-pgsql`, `make`. A static link needs care: `LDFLAGS=-all-static` for the whole build reaches the makefile of the bundled LuaJIT, whose gcc rejects it. Relinking `src/sysbench` alone with `-all-static` after the normal build gave a static binary. In Docker on the developer's Mac it ran 12842 events per second with 1 thread and 49848 with 4. The x86_64 build is left to the workflow (§4).
- **`linux_bench`** runs `sysbench --threads=N --time=20 --cpu-max-prime=10000 cpu run` with one thread and with all of them, and reads the line `events per second:`.

### 3.2 Geekbench 7.1.0

- **Release:** Geekbench 7.0.0 came out on 2026-07-23. The 7.1.0 files on the CDN are dated 2026-09-18. On Linux, the x86_64 build is a release and the aarch64 build a preview ("Preview builds require an active Internet connection and automatically upload benchmark results to the Geekbench Browser").
- **License** (EULA v7, `https://www.primatelabs.com/legal/eula-v7.html`, read 2026-11-09):
  - The free "Geekbench License" (§2a) is personal and non-commercial, for one named user, "solely for personally owned devices of yours or your household's", and "requires that all Benchmark Results be uploaded".
  - Without a Pro license, no commercial use (§4a), and no access "through any automated means, including any scripts", except as the documentation allows (§4g). No redistribution (§4d).
  - Published results identify the make and model of the device, not the person (§5).
  - Pro costs 99 US dollars per user and adds offline results, automation and commercial use.
- **Downloads:**

  | Architecture | URL | Size | SHA-256 | Unpacked |
  |---|---|---|---|---|
  | x86_64 | `https://cdn.geekbench.com/Geekbench-7.1.0-Linux.tar.gz` | 484515924 | `411b72a9c851b69e6f91d390649d419e2e625f475e3983880169651ca27a8672` | 6 files, 1009518416 bytes |
  | aarch64 | `https://cdn.geekbench.com/Geekbench-7.1.0-LinuxARMPreview.tar.gz` | 419635785 | `bb6abc61a68a9b33fecf1f3c0b800050dd76e93666a13eaabfa52cbd26ad3319` | 5 files, 826968228 bytes |

  - Each archive holds one directory, `Geekbench-7.1.0-Linux/` or `Geekbench-7.1.0-LinuxARMPreview/`, with the launcher `geekbench7`, the workers (`geekbench_x86_64` and `geekbench_avx2`, or `geekbench_aarch64`), `geekbench.plxr`, `geekbench-workload.plxr` and `libtensorrt_rtx.so`, all regular files.
  - The hashes are from downloads on the developer's Mac; the runners' downloads matched them. Primate Labs publishes no checksums.
- **glibc:** the workers need the symbol version `GLIBC_2.35` (the x86_64 launcher only 2.34). They are linked dynamically, with the loader `/lib64/ld-linux-x86-64.so.2` or `/lib/ld-linux-aarch64.so.1`.
- **Network:** `cdn.geekbench.com` has IPv4 addresses only; `browser.geekbench.com`, where results go, has both.
- **Command line:** the help lists `--cpu` and `--sysinfo`. `--upload` is accepted. The free version refuses `--export-json`, `--json`, `--save` and `--no-upload` at once, with "only available in Geekbench 7 Pro" and exit status 255, so the console is its only output.
- **Offline**, both builds ran the whole benchmark, then failed to upload: they printed `console_upload_failed unable to resolve host browser.geekbench.com`, exited with 255, and showed no scores (238 seconds for aarch64 on the developer's Mac, 344 for x86_64 under Rosetta).
- **Online** (CP-12; 2026-11-09, `ubuntu-24.04` and `ubuntu-24.04-arm` runners with 4 vCPUs and 16 GB): both uploaded and exited with 0. After the progress lines, the output ends with:

  ```
  Upload succeeded. Visit the following link and view your results online:

    https://browser.geekbench.com/v7/cpu/840201

  Visit the following link and add this result to your profile:

    https://browser.geekbench.com/v7/cpu/840201/claim?key=<secret>

  Benchmark Summary
    Single-Core Score             1771
    Multi-Core Score              4347
  ```

  - x86_64 (AMD EPYC 9V74, 4 threads): 1771 and 4347, result 840201; aarch64 (4 cores): 1463 and 5098, result 840200. Both results are public.
  - About 400 seconds each: about 205 for the single-core part and 190 for the multi-core part. The downloads took 8 to 9 seconds.
  - The worker's peak resident memory was 3654 MiB on x86_64 and 3589 MiB on aarch64. The single-core part peaked at about 1.3 GiB; the multi-core Photo Editor workload reached the peak with 4 threads.
  - Geekbench wrote nothing outside its own directory.
  - The result page, and the same URL with `.gb7` and `.json`, answered 403 with Cloudflare's "Just a moment..." page: the scores must come from the console.
- **Processes:** the launcher starts one worker (`geekbench_avx2` on the runner) with `--argv0`. After a `kill -9` of the launcher, the worker went on running under PID 1.
- **Threads:** in a container limited by cpuset to 2 of 10 CPUs, `--sysinfo` still reported 10 cores. The single-core part ran 1 thread, and the multi-core workloads 2, except Ray Tracer with 10 or 11.
- **Locale:** the output stayed English with `LANG` and `LC_ALL` set to `C`, `zh_CN.UTF-8` and `de_DE.UTF-8`, but the image had no such locales installed.

### 3.3 Distributions and glibc

| Distribution (`scripts/distros.txt`) | glibc | Geekbench 7 runs |
|---|---|---|
| Debian 11, Ubuntu 20.04 | 2.31 | no |
| CentOS 7 | 2.17 | no |
| AlmaLinux 8 | 2.28 | no |
| Rocky Linux 9, Oracle Linux 9, Amazon Linux 2023 | 2.34 | no |
| Alpine 3.22 | none (musl) | no |
| Debian 13 (2.41), Ubuntu 24.04 (2.39), Fedora 45, openSUSE Leap 16.0, Arch Linux | 2.35 or later | yes |

Geekbench 6.7.2, the rejected fallback, needs only glibc 2.27 (its x86_64 workers' newest symbol version), and would have run on all but CentOS 7 and Alpine.

## 4. Tool builds (CP-17, P-10)

- **Workflow:** `.github/workflows/tools.yml` replaces `fio.yml` and `smartctl.yml`, which were the same but for names, versions and texts.
  - A pull request that changes `scripts/build-*.sh` or the workflow builds all three tools for both architectures, six jobs, without publishing.
  - A manual run on `main` builds the chosen tool (`fio`, `smartctl` or `sysbench`) for both architectures and publishes the release named by the script's `RELEASE` line.
  - The release job writes `SHA256SUMS` over all assets, creates a draft, uploads the assets and then publishes, passing `--latest=false` both times. The notes are the same for every tool, apart from its name and version.
  - The build jobs run in the Alpine 3.22 image of `scripts/distros.txt`, pinned by digest and pulled from `ghcr.io/lowendaff/mirror`, natively on `ubuntu-24.04` and `ubuntu-24.04-arm`. They only read; the release job alone has `contents: write`. The workflow is not a required check.
- **Scripts:** each begins with `RELEASE=<tool>-<version>-<build>` and checks that the version in it is the version of its pinned source. Changing a release name thus goes through a pull request, as the `TAG` of the old workflows did. The scripts build in `/build` instead of a directory from `mktemp -d`: two builds of smartctl differed only in that directory's name and in the build ID that follows from it. The fixed directory should make builds reproducible bit for bit; two builds are compared before the docs say so.
- **`scripts/build-sysbench.sh`** (`RELEASE=sysbench-1.0.20-1`):

  | Source | Version | License | URL | SHA-256 |
  |---|---|---|---|---|
  | sysbench | 1.0.20 | GPL-2.0-or-later | `https://github.com/akopytov/sysbench/archive/refs/tags/1.0.20.tar.gz` | `e8ee79b1f399b2d167e6a90de52ccc90e52408f7ade1b9b7135727efe181347f` (1509630 bytes, §3.1) |
  | musl | 1.2.5 | MIT | `https://musl.libc.org/releases/musl-1.2.5.tar.gz` | `a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4`; only its `COPYRIGHT` is used, as for smartctl |

  - Packages: `autoconf automake bash curl gcc libtool linux-headers make musl-dev pkgconf`.
  - Build: `./autogen.sh`, `./configure --without-mysql --without-pgsql`, `make`, then `src/sysbench` is removed and relinked with `make -C src sysbench LDFLAGS=-all-static` (§3.1).
  - Checks: the binary is static, `sysbench --version` prints `sysbench 1.0.20`, and a one-second `sysbench cpu --cpu-max-prime=10000 run` prints `events per second:`.
  - Assets: `sysbench-x86_64`, `sysbench-aarch64`, `sysbench-1.0.20.tar.gz`, `build-sysbench.sh`, `COPYING-sysbench`, `COPYRIGHT-luajit`, `COPYING-concurrency-kit`, `COPYRIGHT-musl` and `SHA256SUMS`. The source tarball includes the sources of LuaJIT and Concurrency Kit, which meets GPL-2.0 §3 for the whole binary.
- **`scripts/build-fio.sh`** (`RELEASE=fio-3.43-2`): adds musl's `COPYRIGHT`, which `fio-3.43-1` lacks (disk-health §9), and the fixed build directory. fio and libaio are unchanged.
- **`scripts/build-smartctl.sh`** (`RELEASE=smartctl-7.5-1`): only the fixed build directory. `smartctl-7.5-1` stays in use; a manual run would fail, because the tag exists.
- **Order:** the workflow lands first and runs on `main`, for `sysbench-1.0.20-1` and `fio-3.43-2`. The items and the new pins follow in a second pull request.

## 5. CPU 性能 (`cpu.sysbench`)

### 5.1 Identity

- Id `cpu.sysbench`, title `CPU 性能`, version `0.1.0`, in the crate `plugins/cpu`. The plugin `Sysbench` has no settings.
- Requirements: the tool `sysbench` 1.0.20 from the release `sysbench-1.0.20-1`; no network; every platform.
- Timeout: 10 minutes, for two runs of up to 4 attempts of up to 60 seconds each (CP-14).

### 5.2 Flow

1. **Threads:** the logical CPUs, and in a container the CPUs of its cpuset, when it has one; the same number as 系统基础信息's CPU line, from one function in `bench-host` that both items call. If neither can be read, `std::thread::available_parallelism`.
2. **Single thread:** `sysbench cpu --cpu-max-prime=10000 --threads=1 --time=20 run`, without a shell, with an empty standard input and `LC_ALL=C`, killed on cancellation and after 60 seconds.
3. **All threads:** with more than one thread, the same with `--threads=N`.
4. **Retries (CP-14):** an attempt fails when sysbench exits with a status other than 0, prints no `events per second:` line, or runs over 60 seconds. Each run then gets up to 3 more attempts. A cancellation ends the item without retries.

### 5.3 Output

```
【CPU 性能】
  sysbench 1.0.20：素数上限 10000，每项 20 秒
  项目    线程数  每秒事件数
  单线程  1       12842.07
  多线程  4       49847.67
  倍数              多线程是单线程的 3.88 倍
  完成，用时 40.6 秒
```

- Table `results` with the title `sysbench 1.0.20：素数上限 10000，每项 20 秒` and the headers `项目 线程数 每秒事件数`. Events per second as sysbench prints them, with two decimals.
- Fact `ratio`, label `倍数`: the multi-thread result divided by the single-thread one, rounded to two decimals.
- With one thread, the table has the single-thread row only, and there is no ratio.
- A run that succeeded after retries adds a note (label `说明`), such as `多线程测试失败 2 次后成功，最后一次失败的原因：sysbench 退出码 1：…`.

### 5.4 Errors and outcomes

- Reasons: `sysbench 没有输出每秒事件数`, `sysbench 退出码 N：<the last line of output>`, `超过 60 秒仍未结束，已停止`. After the fourth failed attempt, `（共试了 4 次）` follows the reason.
- The single-thread run fails: the item fails with the reason, without a table.
- The multi-thread run fails: the table shows the single-thread row, without a ratio, and the item fails with `多线程测试失败：<reason>`.

## 6. Geekbench 7 (`cpu.geekbench`)

### 6.1 Identity

- Id `cpu.geekbench`, title `Geekbench 7`, version `0.1.0`, in the crate `plugins/geekbench`. The plugin `Geekbench` has no settings.
- Requirements: the tool `geekbench` 7.1.0, an archive whose executable is `geekbench7` (§6.3), pinned as in §3.2; IPv4; glibc 2.35 or later; every platform.
- Timeout: 4 hours 10 minutes, for up to 4 runs of up to 60 minutes each (CP-15), plus the swap.

### 6.2 Checks before the download (CP-11)

- **glibc:** read once at the start, from files only (P-11):
  1. The loader that Geekbench names: `/lib64/ld-linux-x86-64.so.2` on x86_64, `/lib/ld-linux-aarch64.so.1` on aarch64. If it does not exist, the system has no glibc (Alpine and other musl systems, or a system without the usual loader path).
  2. Otherwise, with its links resolved, `libc.so.6` in the loader's directory, and in it the text `release version <major>.<minor>` that glibc prints about itself (`release version 2.41` on Debian 13).
  3. A version that cannot be read lets the item run; Geekbench then reports any problem itself.
- **Order of the checks:** platform (D-16), glibc, network, then tools. The orchestrator checks glibc and network before the downloads, and downloads no tool of an item that fails them. Until now, every planned tool was downloaded before any check, so that a host without IPv4 tried to download Geekbench from an IPv4-only server.
- Reasons: `需要 glibc 2.35 或更新，本机是 2.34`, `需要 glibc 2.35 或更新，本机没有 glibc`, and the existing `需要 IPv4 网络连接`.

### 6.3 Archives

- A tool's declaration gains its form: one executable, as for fio, smartctl and sysbench, or a gzip-compressed tar archive with the name of the executable inside, as for Geekbench.
- An archive is downloaded to `tools/geekbench.part` under the size limit, and its size and SHA-256 are checked before anything is unpacked. Then:
  1. It is unpacked into `tools/geekbench.unpack/`, without its one top-level directory. Only regular files and directories are accepted: entries that are links or devices, or whose paths are absolute or contain `..`, fail the tool. Executable bits are kept.
  2. The directory is renamed to `tools/geekbench/`, and the archive is removed.
  3. On any failure, the partial files go, as for single executables.
- The tool's path is `tools/geekbench/geekbench7`. While unpacking, Geekbench takes about 1.5 GB of the disk, and 1 GB afterwards.
- New dependencies: `flate2` (with the Rust backend `miniz_oxide`) and `tar` (without default features).
- The registry check requires an archive's executable to be a plain file name.

### 6.4 Flow

1. **Memory:** the threads as in §5.2; the estimate, and swap if needed (§7).
2. **Run:** `tools/geekbench/geekbench7 --cpu --upload`, without a shell, in the item's work directory, with an empty standard input and `LC_ALL=C` and `LANG=C`, in a process group of its own, with `oom_score_adj` 1000 (§7.3). The `oom_kill` count of `/proc/vmstat` is noted before the run.
3. **Limit:** at most 60 minutes per run. On a timeout or a cancellation, the whole process group gets SIGKILL: killing the launcher alone leaves the worker running (§3.2).
4. **Result** (§6.5): on a failed upload, 30 seconds of waiting and another run, up to 3 times (CP-15). The swap stays during the retries.
5. **Clean-up,** whatever the outcome: the process group is killed if any of it is left, then the swap is turned off and removed (§7.2).
6. **Records** (§6.6).

### 6.5 Reading the output

- **Result link:** after the line that begins with `Upload succeeded`, the first line that, trimmed, begins with `https://browser.geekbench.com/` and does not contain `/claim`.
- **Scores:** after `Benchmark Summary`, the numbers at the end of the lines `Single-Core Score` and `Multi-Core Score`.
- **Claim link:** never kept or shown. A reason that quotes a line of output skips lines with `claim?key=`.
- **Failed upload:** a line that begins with `console_upload_failed `, whose rest is the reason, or a run that printed `Uploading results` but no `Upload succeeded`, whose last line is the reason.
- **Lack of memory:** the run failed, and the `oom_kill` count of `/proc/vmstat` rose during it. The kernel usually kills the worker, so the launcher's own exit does not show it.

### 6.6 Output

```
【Geekbench 7】
  单核              1771
  多核              4347
  结果链接          https://browser.geekbench.com/v7/cpu/840201
  完成，用时 412.3 秒
```

- Facts `single` (单核), `multi` (多核) and `url` (结果链接).
- Notes (label `说明`), when they apply:
  - `可用内存不足（估计需要 5 GiB，可用 1.6 GiB），临时加了 4 GiB swap，分数可能偏低`
  - `可用内存可能不足（估计需要 5 GiB，可用 1.6 GiB），无法临时加 swap：<reason>`
  - `无法关闭临时 swap：<error>；请手动执行 swapoff <path>，再删除这个文件`
  - `无法让 Geekbench 在内存耗尽时优先被结束：<error>`
  - `上传失败 1 次后重跑成功，失败原因：<reason>`
- Skips: `需要 IPv4 网络连接`, `需要 glibc 2.35 或更新，本机是 2.34`, `需要 glibc 2.35 或更新，本机没有 glibc`.
- Failures:
  - `上传失败（<reason>），免费版只有上传成功才给出分数`, after the last retry;
  - `运行中因内存不足被系统终止`;
  - `Geekbench 没有输出分数`, for an exit status of 0 without scores;
  - `Geekbench 退出码 N：<the last line of output>`;
  - `超过 60 分钟仍未结束，已停止`.

## 7. Memory, temporary swap and OOM protection

### 7.1 Estimate and size (CP-06)

- **Need:** `1 GiB + threads × 1 GiB`, the threads as in §5.2. The runners measured 3.6 GiB with 4 threads and about 1.3 GiB with one (§3.2); the cpuset test showed that the memory-heavy workloads follow the cpuset.
- **Available:** `MemAvailable` plus `SwapFree` from `/proc/meminfo`; in a cgroup with a memory limit, at most the limit minus the usage.
- **Swap size:** the shortfall rounded up to whole GiB, at most 8 GiB, and at most the free space of the work directory's filesystem minus 1 GiB, rounded down. Below 1 GiB, no swap, and a note.

### 7.2 The swap file (CP-07)

- **Crate `crates/swap-file`,** type `SwapFile`. `unsafe_code` is `deny` there instead of `forbid`; the two functions that call `libc::swapon` and `libc::swapoff` allow it, with a `SAFETY` comment each, and `clippy::undocumented_unsafe_blocks` is on.
- **Creation:**
  1. `swapfile` in the item's work directory, created exclusively, with mode 0600.
  2. Filled with zeros, since swap cannot have holes, in writes of 1 MiB that check for cancellation; then `fsync`.
  3. The header (`SWAPSPACE2`, version 1), in safe Rust. The page size comes from the kernel (`rustix::param::page_size()`): 4 KiB on x86_64, and 4, 16 or 64 KiB on aarch64. From byte 1024: the version `1`, the number of the last page, 0 bad pages, a UUID of zeros and an empty label, each in the machine's byte order; `SWAPSPACE2` in the last 10 bytes of the first page. This is what `mkswap --uuid clear` writes (§9.1).
  4. `swapon(path, 0)`, with the kernel's default priority, below the host's own swap.
  5. On an error, the file is removed. `EPERM` reads `没有权限（容器里通常不允许）` and `EINVAL` `运行目录所在的文件系统不支持 swap 文件` (ZFS, tmpfs, overlay, and btrfs files that are not NOCOW); other errors read as the system words them.
- **Removal:** `swapoff`, which reads the pages that are still swapped out back into memory, then the file goes. When `swapoff` fails, for example with `ENOMEM`, a note gives the command to run by hand. A drop without removal, after a panic or an abort, does the same as well as it can.
- **A forced end of the program** (`kill -9`) leaves the swap on until a reboot or a manual `swapoff`, and its file in the run directory (§10).

### 7.3 OOM protection (CP-13)

- Before Geekbench starts, the program reads its own `/proc/self/oom_score_adj`, writes 1000, starts Geekbench, and writes back the old value. Geekbench and its worker inherit 1000; restoring a lower value needs `CAP_SYS_RESOURCE`, which root has.
- If the value cannot be written, Geekbench runs anyway, with a note (§6.6).

## 8. Changes to the plugin system

- **bench-api:** `Requirements` gains `glibc`, the minimum version or none; `Env` gains `libc` (glibc of a version, glibc of an unknown version, or none); `ToolSpec` gains its form (§6.3).
- **bench:** the order and timing of the checks (§6.2); unpacking archives; the registry adds `plugin_cpu::Sysbench` after `plugin_disk_health::Smartctl` and `plugin_geekbench::Geekbench` last (CP-08). `--list` then prints `共 7 个测试项`.
- **bench-host:** the thread count (§5.2) and the glibc reader (§6.2).
- **New crates:** `crates/swap-file`, `plugins/cpu` and `plugins/geekbench`.
- **Dependencies:** `flate2` and `tar` for bench, `libc` for swap-file.
- **README:** the status line, the two items, sysbench and Geekbench among the third-party tools, and the new CI step (§9.3).

## 9. Testing

### 9.1 Tests

None needs the network or root.

- **CPU 性能:** parsing real sysbench output; the ratio; one thread only; retries, with a stand-in for running sysbench: success after 2 failures, 4 failures, and a cancellation that is not retried.
- **Geekbench:** parsing the runners' output (§3.2) and the offline output: link and scores; failed uploads; no scores; retries after failed uploads only; the memory kill. No record and no reason may contain `claim?key=`.
- **Memory:** need, available memory (also under a cgroup limit) and swap size: rounding, the 8 GiB cap, the 1 GiB of free space, no swap below 1 GiB.
- **Swap header:** byte for byte against `mkswap --uuid clear` from util-linux, for pages of 4 KiB and 64 KiB.
- **glibc:** fixture roots with and without the loader, links across directories, and a libc without a readable version.
- **Archives,** made in the test: unpacking without the top-level directory and with executable bits; nothing unpacked after a failed check; links and `..` rejected; nothing left after a failure.
- **Orchestrator:** no download for an item that fails its glibc or network check; the order of the checks.

### 9.2 Fixtures

- Geekbench: the runners' standard output for both architectures, with the claim key replaced by `FIXTURE`, and the offline outputs.
- sysbench: output of the trial build, and of the workflow's builds.
- Swap headers: the first page of `mkswap --uuid clear` for 4 KiB and 64 KiB pages, made in a container.

### 9.3 CI

- **Smoke:** runs with `--skip cpu.geekbench`, since every run would publish a result and take minutes. On the runners it then prints `共 3 项：完成 3，跳过 0，…`; 硬盘健康 does not apply there, and CPU 性能 adds about 45 seconds.
- **Smoke mutants:** also skip `cpu.sysbench` and `cpu.geekbench`.
- **Distributions:** each image also runs `--only cpu.geekbench` without a network. The reason must match the container's glibc: `本机没有 glibc` on Alpine, `本机是 2.31` and the like on old systems, and `需要 IPv4 网络连接` on new ones. Without a network, nothing is downloaded or uploaded, and the glibc reader meets 13 real distributions on both architectures.
- **Tool downloads:** now also sysbench, and Geekbench with its unpacking (about 10 seconds on a runner).
- **Swap round trip,** a new step: an example program, run as root on the runner, makes a 64 MiB swap file, turns it on, finds it in `/proc/swaps`, turns it off and removes it.

### 9.4 Mutations

A mutation round covers the parsers, the retries, the estimate and the swap size, the swap header, the glibc reader, unpacking, and the order of the checks.

### 9.5 Real runs (CP-16)

- **When:** once the items pass their local and CI gates, before the merge is requested.
- **Where:** the VM, the dedicated server and the ARM board, once each.
- **Steps** on each host:
  1. Record the listing of `/root` and `/proc/swaps`.
  2. Create `/root/lowendaff-bench-test` and copy the binary of the host's architecture into it through a pipe; compare its SHA-256.
  3. In it, run `./lowendaff-bench --only cpu.sysbench,cpu.geekbench`.
  4. Remove `/root/lowendaff-bench-test`, also if the program crashed.
  5. Compare `/root` and `/proc/swaps` with the records.
- **Expected,** from the hosts' `/proc/meminfo` on 2026-11-09:
  - the ARM board: 4 threads and 1.69 GiB available, without swap; it needs 5 GiB and gets 4 GiB of swap on its ext4 root;
  - the VM: 1 thread and 0.61 GiB available, without swap; it needs 2 GiB and gets 2 GiB of swap on its ext4 root, and sysbench runs only with one thread;
  - the dedicated server: 64 threads, 131 GiB available and 4.1 GiB of free swap; it needs 65 GiB and gets no swap.
- The owner sees the whole output. The pull request quotes only the lines of the two items.

## 10. Risks and limitations

1. **Geekbench's license.** The free license is for personally owned devices, non-commercial, with every result uploaded, and it forbids automated use (§3.2). This program runs Geekbench automatically, mostly on rented servers. The owner decided on the free version (CP-05).
2. **Public results.** Every run publishes the machine's make and model, CPU, memory and system on the Geekbench Browser. Beyond what the console shows, the upload's content is unknown: it is encrypted. Only `--skip cpu.geekbench` avoids it.
3. **A closed-source vendor.** The download depends on `cdn.geekbench.com`, which has IPv4 addresses only, and on 7.1.0 staying there. The aarch64 build is a preview. Primate Labs may change Geekbench, its output or its terms; a new version needs new pins and new captures.
4. **Console text.** The free version has no machine-readable output, so the item reads English console lines, as checked for 7.1.0 on both architectures.
5. **glibc 2.35.** Geekbench 7 skips on 8 of the 13 distributions of the matrix (§3.3), among them Rocky Linux 9 and Amazon Linux 2023.
6. **The memory estimate** rests on two measurements, with 1 and 4 threads. Hosts with many threads may need much more or much less; at the cap of 8 GiB of swap, memory can still run out, and Geekbench is then killed.
7. **Temporary swap** writes up to 8 GiB of zeros to the disk of the run directory, and swapping lowers the scores. Without swap support (ZFS, tmpfs, overlay, btrfs without NOCOW, containers without permission), there is no swap.
8. **A forced end of the program** leaves the swap on until a reboot or a manual `swapoff`, with its file in the run directory, and lets Geekbench's processes finish and perhaps upload.
9. **`unsafe`:** two foreign function calls in `crates/swap-file`.
10. **OOM:** for a moment, the program itself has `oom_score_adj` 1000. The memory kill is recognised by a count for the whole host, so another process killed for memory during the run is taken for Geekbench.
11. **Load:** sysbench loads every usable CPU for 20 seconds, and Geekbench for minutes, with memory that grows with the threads (3.6 GiB with 4). Guests of a hypervisor slow down meanwhile, as on the owner's server (CP-16).
12. **Time:** Geekbench takes about 7 minutes on a 4-thread runner, far longer on slow machines and with swap, and up to 4 times as long with retries.
13. **Disk:** Geekbench takes about 1.5 GB while it is unpacked and 1 GB afterwards, plus the swap.
14. **Retries** in sysbench can hide unstable runs; the 说明 line shows them.
15. **sysbench 1.0.20** dates from 2020, and its LuaJIT's aarch64 support is old. Its source is an archive that GitHub generates; should its hash change, the build fails, and the release keeps the copy that was built.
16. **Threads in containers** follow the cpuset; a CPU quota is not counted, so the multi-thread result shows the throttling.
17. **The real runs** (CP-16) are three more exceptions to read-only access to the owner's hosts. They publish three results, load the dedicated server's guests for minutes, and write 2 GiB and 4 GiB of temporary swap to the VM's and the ARM board's disks (§9.5).

## 11. Implementation plan

1. **First pull request:** this document, the M4 line in the plugin system design, `.github/workflows/tools.yml` in place of `fio.yml` and `smartctl.yml`, `scripts/build-sysbench.sh`, and the changes to `build-fio.sh` and `build-smartctl.sh`.
2. **After the owner merges it:** manual runs on `main` for sysbench and fio; then the assets, `SHA256SUMS`, `gh release verify` and `gh release verify-asset` are checked.
3. **Second pull request:** the changes to the plugin system (§8), `crates/swap-file`, `plugins/cpu` and `plugins/geekbench`, the pins of `sysbench-1.0.20-1` and `fio-3.43-2`, the CI changes (§9.3), the README and the design notes on `fio-3.43-2`. Then the real runs (§9.5).
4. **Merge:** after the owner's approval.
