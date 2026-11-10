# Memory Test Item (内存性能)

- Status: **Accepted** 2026-11-10. The owner decided ME-01..ME-15 (§2), confirmed the design section by section, and accepted the risks (§10). The implementation settled the points of §12.
- Implementation (M5): builds of STREAM and multichase, with the releases `stream-5.10-1` and `multichase-2026.10.01-1` (§4); then the test item in `plugins/memory`, with new host facts in `crates/bench-host` (§5–§8).
- Scope: one test item, `memory` (内存性能), in four chapters, and the tool builds it needs.

## 1. Goals and non-goals

Goals:

1. Measure read and write bandwidth with sysbench, single- and multi-thread, which the dedicated review template asks for as 读取 and 写入 (ME-02, ME-04).
2. Measure STREAM's Copy, Scale, Add and Triad, single- and multi-thread, with the unmodified STREAM 5.10, so that the numbers compare with other reviews (ME-02, ME-06, ME-07).
3. Measure the latency of each cache level and of memory with multichase (ME-12, ME-13).
4. On machines with more than one NUMA node, measure the bandwidth and the latency between every node the threads run on and every node the memory is on (ME-11, ME-14).
5. Run on every platform (ME-01), within half of the available memory (ME-05).

Non-goals:

- Modifying STREAM's source (ME-06).
- Huge pages.
- The memory modules: 系统基础信息 shows them on dedicated servers.
- sysbench and STREAM per NUMA node: the matrix covers the nodes (ME-14).

## 2. Owner's decisions

| ID | Question | Decision | Alternatives |
|----|----------|----------|--------------|
| ME-01 | Platforms | **All:** dedicated servers, virtual machines and containers. A VPS's memory bandwidth varies with overselling and neighbours. | Dedicated servers only, as in the review templates |
| ME-02 | Tools | **sysbench and STREAM** ("我都要"). sysbench is pinned already and gives read and write; STREAM is the common standard. | sysbench only; STREAM only |
| ME-03 | One item or two | Two items, `memory.sysbench` and `memory.stream`. **Superseded by ME-10** the same day: STREAM is a chapter of 内存性能, not a section of its own. | One item |
| ME-04 | sysbench runs | **Read and write, single- and multi-thread**: four runs, a 2×2 table. | Single-thread only; multi-thread only |
| ME-05 | Too little memory | **Shrink the working set to fit and measure,** with a 说明 line. | Skip the run with a reason; fixed sizes, risking the OOM killer |
| ME-06 | STREAM's array size, fixed at compile time | **The unmodified source, built at six sizes** in one archive, chosen at run time (§5.4). | A source changed to take the size at run time, which its license then requires labelling; the default size only |
| ME-07 | STREAM results | **Copy, Scale, Add and Triad, single- and multi-thread**: a 4×2 table. | Multi-thread only; Triad only |
| ME-08 | NUMA | First: STREAM's multi-thread run spread over the nodes with `OMP_PROC_BIND=spread`, sysbench left to the scheduler. **Changed** the same day to measuring per node; ME-11 and ME-14 say how. The spread binding stays for STREAM's machine-wide run. | Per node (then chosen); no binding |
| ME-09 | Order | **After CPU 性能, before 硬盘性能.** | After 硬盘性能; before CPU 性能 |
| ME-10 | Structure | **One item, 内存性能, in chapters: sysbench, STREAM, latency and NUMA.** All three tools are required: they come from the same GitHub releases and fail together. A chapter that fails leaves the others running; the item then fails, keeping their results. No change to the plugin system. | Optional tools in the plugin contract; several items under one title, merged when rendered |
| ME-11 | NUMA scope | **Local and cross-node, as a matrix.** | The local node only; a latency matrix only |
| ME-12 | Latency tool | **multichase**, with multiload from the same repository for the bandwidth matrix. Both bind memory to a node themselves (`-W`) and pin their threads to the CPUs they are allowed, so no `unsafe` code is needed. | tinymembench, whose buffer is fixed at 64 MiB and which cannot bind memory; both |
| ME-13 | Latency | **One buffer per cache level and one in memory** (§5.5). | Memory only; fixed sizes |
| ME-14 | Matrix | **Every cell:** bandwidth with multiload's `stream-triad` and latency with multichase, about 9 seconds each. | Fewer cells above four nodes; a time cap |
| ME-15 | multichase build | **Unmodified, linked statically with glibc in Debian 13**: it uses glibc's `random_r`, which musl lacks. | A musl build with a compatibility header, in effect a change; another tool |

## 3. Background

### 3.1 sysbench's memory test

sysbench 1.0.20 (`sysbench-1.0.20-1`, built for CPU 性能) has a memory test: each thread reads or writes a buffer of `--memory-block-size` over and over, and the result is MiB/s. Its options:

- `--memory-block-size`, default 1 KiB, a power of two;
- `--memory-total-size`, 0 for no limit, so that `--time` ends the run;
- `--memory-scope=global`, one buffer for all threads, or `local`, one buffer per thread;
- `--memory-oper=read|write`, `--memory-access-mode=seq|rnd`.

A buffer that fits in a cache measures the cache. Checked on 2026-11-10 under Docker Desktop on Apple silicon, limited to four CPUs:

| Block | Read, 1 thread | Write, 1 thread |
|-------|----------------|-----------------|
| 1K (default) | 11973 MiB/s | 10887 MiB/s |
| 1M | 80152 MiB/s | 50842 MiB/s |
| 256M | 61992 MiB/s | 38941 MiB/s |

At 1K the cost of each call dominates; 1M fits the chip's caches; 256M reaches memory. Four threads writing 256M reached 68071 MiB/s with one shared buffer and 56552 MiB/s with one buffer each: the shared buffer flatters the result, so the multi-thread runs use `local`.

### 3.2 STREAM 5.10

- **Source:** `https://www.cs.virginia.edu/stream/FTP/Code/stream.c`, revision 5.10 of 2013-01-17, 19967 bytes, SHA-256 `a52bae5e175bea3f7832112af9c085adab47117f7d2ce219165379849231692b`.
- **License**, in the source's header: use and redistribution are free; modification is free subject to item 3. Published results must follow the STREAM Run Rules to be called "STREAM benchmark results"; results from modified code, or from runs that break the rules, must be labelled as such.
- **Run rules**: each array at least four times the size of all the last-level caches, and a timer resolution well below a kernel's run time.
- **Sizes**: `STREAM_ARRAY_SIZE` (default 10 million elements, three arrays of doubles, 229 MiB) and `NTIMES` (default 10) are fixed when compiled. Each kernel runs `NTIMES` times; the best rate is reported, in MB/s (10⁶ bytes).
- **Threads**: with OpenMP, `OMP_NUM_THREADS` sets the threads at run time, so one program serves both runs. The arrays are initialized in parallel, so each thread first touches, and so places, its part.
- **Build**: in Alpine 3.22, `gcc -O3 -static -fopenmp` links libgomp statically (`/usr/lib/libgomp.a` comes with gcc); the program has no interpreter. A static OpenMP build is 668744 bytes, 281854 compressed. On the Mac under Docker it gave Copy 82514.3, Scale 79882.0, Add 86517.7 and Triad 90974.5 MB/s with four threads.

### 3.3 multichase and multiload

- **Source:** `github.com/google/multichase`, Apache-2.0, maintained by Google. It has no releases or tags; the build pins commit `8b68e918f8d2e84794f2c20cdb6c7ebb9c3952bb` of 2026-10-01 through GitHub's archive of it, 42260 bytes, SHA-256 `38d4adbd7e6cbe9d7ae9d96a48f22aa18d4338b40308d8f36a255c9fc0f2d620`. Two downloads gave the same bytes, but GitHub generates such archives and could change them.
- **multichase** chases pointers through a randomly permuted buffer (`-m`, default 256 MiB; stride `-s`, default 256 bytes) for `-n` half-second samples, and prints one number: the best latency in nanoseconds.
- **multiload**, from the same repository, also measures bandwidth: `-l stream-triad` with `-t` threads and `-m` bytes per thread prints one comma-separated row, whose `LdMaxMibs` is the best sample in MiB/s.
- **Placement:** both pin thread k to the k-th CPU of the set they are allowed (`sched_getaffinity`), unless `-X` is given; and `-W node:weight` binds their memory with `mbind`.
- **Build:** the Makefile links statically. musl lacks the `random_r` and `initstate_r` that `permutation.h` uses, so the build fails in Alpine; in `debian:13` (gcc 14, glibc 2.41) it succeeds, giving two static programs of about 1 MB.
- **Containers:** Docker's default seccomp profile allows `mbind` only with `CAP_SYS_NICE`; in a default container `-W` fails with `mbind: Operation not permitted`.

## 4. Tool builds (P-10)

Both follow `tools.yml` as the other tools do (`docs/design/cpu.md` §4): a manual run on `main` builds both architectures and publishes an immutable release, not marked latest, with `SHA256SUMS`.

- **Build images:** each build script names its image on `IMAGE=` and `IMAGE_DIGEST=` lines, as it names its release on `RELEASE=`; the workflow reads them and accepts only a copy on `ghcr.io/lowendaff/mirror` pinned by digest. fio, smartctl, sysbench and STREAM keep Alpine 3.22; multichase uses Debian 13, both from `scripts/distros.txt`.
- **`scripts/build-stream.sh`**, `RELEASE=stream-5.10-1`:
  - checks the source's SHA-256 and that the image has musl 1.2.5, whose notice it attaches;
  - builds `stream-2m`, `stream-4m`, `stream-8m`, `stream-16m`, `stream-32m` and `stream-64m` with `gcc -O2 -static -fopenmp -DSTREAM_ARRAY_SIZE=<n>`, for 2 to 64 million elements: 46 MiB to 1.4 GiB for the three arrays. No `-march`, so that each runs on every machine of its architecture. The largest size stays below 2 GiB of static arrays, which x86_64's default code model allows;
  - checks that every program is static, and that the smallest validates its results with two threads;
  - writes `stream-<arch>.tar.gz`, whose single directory `stream/` holds the six programs, plus the script, `stream.c`, `LICENSE-stream` (the license from the source's header) and `COPYRIGHT-musl`.
- **`scripts/build-multichase.sh`**, `RELEASE=multichase-2026.10.01-1`, the version being the commit's date:
  - checks the archive's SHA-256, and that Debian's glibc is 2.41 and gcc 14;
  - runs `make CAP= multichase multiload`: on aarch64 the Makefile otherwise adds a `-march` for the CPU of the machine it builds on, read from its `/proc/cpuinfo` (SVE, else LSE atomics). A first build on GitHub's runners, which have SVE, stopped with an illegal instruction on a CPU without it;
  - checks that both programs are static, that the flags gcc recorded in them hold no machine flag beyond its defaults (`-mlittle-endian` and `-mabi=lp64` on aarch64, `-mtune=generic` and `-march=x86-64` on x86_64), that `multichase -m 4m -n 1` prints a latency and that `multiload` measures `stream-triad`;
  - writes `multichase-<arch>.tar.gz`, whose single directory `multichase/` holds both programs, plus the script, the archive, `LICENSE-multichase` (Apache-2.0) and `COPYING-glibc` (LGPL-2.1, which glibc is under; the published source and script let anyone relink).
- Archives are written reproducibly: names sorted, owner 0, time 0, `gzip -n`.

## 5. The item (`memory`)

### 5.1 Identity

- Id `memory`, title 内存性能, crate `plugins/memory` (package `plugin-memory`), every platform.
- Tools: sysbench (the same pin as CPU 性能), `stream` (archive, executable `stream-2m`; the plugin runs the other sizes beside it) and `multichase` (archive, executable `multichase`, with `multiload` beside it).
- Timeout 30 minutes: enough for the matrix of up to 13 nodes (§5.6).
- Runs after CPU 性能 and before 硬盘性能 (ME-09).

### 5.2 Quantities

- **Available memory A:** `MemAvailable`; in a container also at most the cgroup's limit minus its usage. Swap does not count: a memory test that swaps measures the disk. The **budget** B is A ÷ 2 (ME-05). Buffers halved to fit are never halved below a page, 4 KiB.
- **Last-level cache C:** the size of the highest cache level holding data times the number of its instances, counted by the distinct CPU lists that share one. Unknown: 32 MiB, with the 说明 line `没有末级缓存的信息，按 32 MiB 计算`. **Cᵢ** is the same for the instances serving node i's CPUs that the process may use.
- **Threads N:** the usable threads, as for CPU 性能 (the logical CPUs, or a container's cpuset). With one thread, only the single-thread runs happen.
- **Nodes:** `/sys/devices/system/node/node*`, each with its CPUs and its `MemTotal` and `MemFree`. The CPUs the process may use are `Cpus_allowed_list` in `/proc/self/status`: its affinity, within a container's cpuset, in the kernel's numbering, which the nodes use too. The nodes it may allocate on are `Mems_allowed_list`.

### 5.3 sysbench

Four runs of `sysbench memory --memory-total-size=0 --memory-access-mode=seq --time=10`, reading and writing, single- and multi-thread:

- **Single thread:** one buffer, the smallest power of two of at least 4C (C = 44 MiB gives 256 MiB), halved until it fits B.
- **Multi-thread:** `--memory-scope=local`, one buffer per thread, the smallest power of two of at least 4C ÷ N and at least 1 MiB, halved until the N buffers fit B.
- A working set below 4C after halving adds the 说明 line `可用内存不足，测试数据小于末级缓存总和的 4 倍（<4C>），一部分读写落在缓存里，结果偏高`.
- Each run is retried up to three times, as in CPU 性能; a run longer than 60 seconds is stopped. The result is the MiB/s of `MiB transferred (… MiB/sec)`, which must be finite and positive.

### 5.4 STREAM

- **Size:** the smallest of the six whose array is at least 4C; above B for the three arrays, the largest that fits. An array below 4C adds the 说明 line `未完全符合 STREAM 运行规则（每个数组至少为末级缓存总和的 4 倍）`. When not even the smallest fits B, the chapter shows `可用内存不足：三个数组最少要 46 MiB，可用内存的一半只有 <B>`, which is no failure, and the others go on.
- **Runs:** `OMP_NUM_THREADS=1`, then `OMP_NUM_THREADS=N` with `OMP_PROC_BIND=spread` (ME-08).
- **Result:** the best rate of each kernel; the output must contain `Solution Validates`, or the chapter fails. Each run is retried up to three times and stopped after 180 seconds.

### 5.5 Latency

One `multichase -n 5` (2.5 seconds) per level:

- L1d, L2 and L3 at half of one instance of each, and memory at the smallest power of two of at least 4C, halved until it fits B; below 4C, the 说明 line `可用内存不足，内存一档的缓冲区小于末级缓存总和的 4 倍（<4C>），一部分访问落在缓存里，延迟偏低`.
- A level the kernel says nothing of, being hidden, as often in virtual machines, or absent, as the L3 of the owner's board, is skipped with the 说明 line `没有 L3 的信息，跳过这一档`.
- Each run is retried up to three times and stopped after 60 seconds.
- The best latency of the default stride, 256 bytes, without huge pages: the result includes the cost of TLB misses on 4 KiB pages.

### 5.6 NUMA

Only when the matrix has more than one cell, and with every cell:

- **Bandwidth**, threads on node i and memory on node j: `multiload -l stream-triad -n 3 -t <node i's CPUs> -m <per thread> -W j:1`. Per thread: the smallest power of two of at least 4Cᵢ ÷ t and at least 1 MiB, the t buffers at most half of node j's `MemFree` and at most B. The result is `LdMaxMibs`.
- **Latency:** `multichase -n 5 -m <the smallest power of two of at least 4Cᵢ> -W j:1`, its thread on node i, its buffer within the same bounds.
- A node none of whose CPUs the process may use is no row; a node without memory, or one the process may not allocate on, is no column.
- Each run is retried up to three times and stopped after 60 seconds.
- When `mbind` is refused, as in Docker's default containers, the chapter shows `不可用：容器不允许把内存绑定到指定节点` and is no failure; nothing is retried.
- About 9 seconds a cell: 35 seconds for two nodes, 2.5 minutes for four, 9 minutes for eight.

### 5.7 Output

```
【内存性能】
  sysbench          1.0.20：顺序读写，每项 10 秒；单线程一块 256 MiB，多线程每个线程一块 8 MiB
  sysbench 结果
  项目  单线程        多线程（64 线程）
  读取  12345 MiB/s   98765 MiB/s
  写入  10234 MiB/s   87654 MiB/s
  STREAM            5.10：每个数组 3200 万个元素（244 MiB，三个合计 732 MiB），每项取 10 遍中最快的一遍；多线程按节点均匀绑定
  STREAM 结果
  项目   单线程         多线程（64 线程）
  Copy   10234.5 MB/s   80123.4 MB/s
  Scale  …
  Add    …
  Triad  …
  延迟              multichase：随机指针追逐，每档 2.5 秒
  延迟结果
  层级  缓冲区   延迟
  L1    16 KiB   1.2 ns
  L2    512 KiB  4.1 ns
  L3    11 MiB   18.5 ns
  内存  128 MiB  92.3 ns
  NUMA              2 个节点；带宽用 multiload 的 stream-triad，每格 3 次采样取最快；延迟同上
  NUMA 带宽（MiB/s；行是线程所在节点，列是内存所在节点）
          节点 0  节点 1
  节点 0  52000   31000
  节点 1  30500   51800
  NUMA 延迟（ns）
          节点 0  节点 1
  节点 0  90.1    140.2
  节点 1  141.0   89.7
  完成，用时 98.4 秒
```

- Each chapter is a fact named after it, giving its parameters, and a table; the numbers above are invented.
- Units are each tool's own: MiB/s for sysbench and multiload, MB/s for STREAM, nanoseconds for latency.
- With one thread, the tables have the single-thread column only. The NUMA chapter appears when its matrix has more than one cell.
- 说明 lines follow the chapter they concern: a shrunk working set, STREAM's run rule, a retry that succeeded.

### 5.8 Errors and outcomes

- A chapter that fails shows `失败：<run>：<reason>（共试了 4 次）` in place of its parameters, such as `失败：多线程测试：STREAM 退出码 1：…（共试了 4 次）`, and stops; the other chapters run, and the item fails at the end with the chapters that failed, such as `STREAM 出错`, keeping their results, as CPU 性能 does when its multi-thread run fails. A table shows the runs before the one that failed, `—` for the rest.
- Reasons: a tool's exit code with its last line, output that cannot be read, a run stopped at its limit, or STREAM's results failing validation. A run that succeeded after failures adds `<run>失败 N 次后成功，最后一次失败的原因：<reason>`.
- A cancellation stops the running tool and is not retried; the item is 已中断, and the results so far stay.

## 6. Host facts (`crates/bench-host`)

- **Caches** (new module `cache`): every cache instance once, with its level, kind, size and the CPUs sharing it, from `/sys/devices/system/cpu/cpu*/cache/index*/{level,type,size,shared_cpu_list}`, giving L1d, L2, one L3, C and each Cᵢ.
- **NUMA** (new module `numa`): the nodes, each with its CPUs and its `MemTotal` and `MemFree` from `node*/meminfo`, and the nodes the process may allocate on. The CPUs it may run on come from `cpu::allowed`.
- **Memory:** `memory::usable`, the available memory without swap, from `memory` and the cgroup limits that exist already.

## 7. Placing a tool on a node

The matrix runs each tool from a new system thread whose CPU set the plugin sets first, with rustix's `sched_setaffinity`; the child inherits the set, and multichase and multiload pin their threads within it. Waiting, the time limit and cancellation work as for the other runs, and the tools run without a shell. No other chapter pins anything.

## 8. Changes elsewhere

- The registry gains `memory` after `cpu.sysbench`.
- CI's smoke runs the item on the runners, which have one node: about a minute and a half, and no matrix. The smoke mutants skip it. "Tool downloads" fetches and unpacks both new tools.
- `crates/bench-host/tests/fixtures/capture.sh` also reads every CPU's cache directories, the node directories and `/proc/self/status`, and the owner's three hosts are captured again, read-only.
- README: the item's row and the two tools.

## 9. Testing

- **Pure functions:** the working sets, STREAM's size, the latency buffers and the matrix's rows and columns, for many C, B and N, with cpusets and nodes without CPUs or memory; the readers of each tool's output, from real output captured from both architectures, including failures.
- **Flow:** fake tools, as in CPU 性能's tests, through four chapters: a chapter failing, retries, cancellation, `mbind` refused.
- **Placement:** on Linux, a child inherits the CPU set it was started with (CI's runners).
- **Mutations** of the selections and the flow: 59, two of them of the placement, in a Linux container. 56 were killed. The three left change nothing: rounding 4C ÷ t down instead of up gives the same power of two while cache sizes are whole KiB and t is at most 4096; a set's `replace` adds what `insert` adds; and no entry of a cache directory but `index*` holds the four files read.
- **Real runs:** `--only memory` on the owner's three hosts, approved and done on 2026-11-11 with the build of `a7fe052`. All three completed, in 36, 50 and 120 seconds, and left nothing behind. The server's matrix, the first one run, shows its uneven memory: node 0 has two modules in use and node 1 four (system information, S-20), so threads on node 0 get more bandwidth from node 1's memory than from their own (35735 against 23867 MiB/s), and node 1's own is the fastest (46589 MiB/s); the latency is 83 to 87 ns within a node and 156 to 157 ns across. The VM's L3, which QEMU reports as 16 MiB, measured like memory (187 against 182 ns): risk 3.

## 10. Risks and limitations

Accepted by the owner on 2026-11-10:

1. sysbench measures its own loop's throughput, not bus traffic, and compares with STREAM only loosely. On a machine with several nodes its threads are left to the scheduler, mixing local and remote memory.
2. A working set shrunk to fit a small machine can include cache speed; a 说明 line says so, but the number is high.
3. Virtual machines may hide or misstate their caches; unknown counts as 32 MiB. Guests share the host's L3, so busy neighbours move the results.
4. STREAM has six sizes: a machine whose last-level caches exceed about 122 MiB cannot meet the four-times rule and gets a 说明 line. Built with `-O2` and no `-march`, STREAM gives less than tuned builds.
5. STREAM's license requires published results to follow its run rules or say they do not; the item labels only the four-times rule.
6. multichase has no releases; its pin is a commit whose GitHub archive could change. Linking glibc statically brings the LGPL's terms, met by publishing the source and the script. Debian's packages change over time, so rebuilds are identical only while they do not.
7. Docker's default containers refuse `mbind`, so they get no matrix; whether Proxmox VE's containers allow it is untested.
8. The matrix grows with the square of the nodes: above 13 nodes the item reaches its 30-minute limit.
9. The test may use half of the available memory, which other programs then lack; in a container without a cgroup limit `MemAvailable` may overstate what is free.
10. The test saturates memory bandwidth for its duration, slowing neighbours on the same host, and the guests of the owner's server during the real run.
11. Without huge pages, the latency includes TLB misses and is higher than with them; tools such as Intel MLC support huge pages (1 GiB pages since its version 3.8), though whether by default is unchecked.
12. The units differ: MiB/s for sysbench and multiload, MB/s for STREAM; readers may compare across them.
13. A run takes about a minute and a half longer, and two more pinned tools need maintaining.
14. The real runs are more exceptions to writing on the owner's hosts, each to be approved.

## 11. Implementation plan

1. **First pull request:** this document, the M5 line in the plugin system design, `scripts/build-stream.sh`, `scripts/build-multichase.sh`, and `tools.yml` reading each script's image.
2. **Publish** `stream-5.10-1` and `multichase-2026.10.01-1` from `main`, and verify them.
3. **Second pull request:** the host facts and fixtures, `plugins/memory`, the registry, CI and the README; then the real runs (§9).
4. The owner merges.

## 12. Changes during implementation

Points the sections above settled while the item was built, on 2026-11-10 and 2026-11-11; none changes a decision of §2:

1. **The CPUs and nodes the process may use** come from `Cpus_allowed_list` and `Mems_allowed_list` in `/proc/self/status`, rather than from the online CPUs and a container's cpuset: these are the numbers `sched_setaffinity` and the nodes use (§5.2).
2. **A node the process may not allocate on is no column** of the matrix, where `mbind` would fail (§5.6).
3. **The matrix appears when it has more than one cell**, which more than one node does not ensure: a node may have neither usable CPUs nor memory (§5.6).
4. **A cache level the kernel says nothing of** gets `没有 L3 的信息，跳过这一档`: the owner's board has no L3 at all, where `读不到 L3 的大小` would mislead (§5.5).
5. **multichase and multiload are retried** up to three times and stopped after 60 seconds, like the other tools; a refused `mbind` is not retried (§5.5, §5.6).
6. **No buffer is halved below a page**, and the matrix's buffers also stay within B (§5.2, §5.6).
7. **The item's failure names the chapters that failed**, such as `STREAM 出错`; each chapter's line gives its reason (§5.8).
8. **multichase is built with `make CAP=`**, so that the Makefile does not target the build machine's CPU (§4); the first build on GitHub's runners needed SVE.
