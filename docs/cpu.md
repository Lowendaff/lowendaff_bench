# CPU 性能检测（`cpu.bench`）

[返回文档目录](README.md) | [简体中文首页](../README.zh-CN.md)

CPU 章节由统一插件 **`cpu.bench`** 驱动，按固定顺序调度 **8 项互补的 CPU 子基准套件**，既覆盖快速整数/浮点吞吐、字典压缩、Unix 内核与进程吞吐、稠密线性代数（AVX2/AVX-512/NEON）与跨平台综合跑分，也支持在 `x86_64` 物理机与高性能 VPS 上按需开启高精度圆周率竞速与重负载稳定性烤机检测。

---

## 概览与子套件选择机制

| 属性 | 说明 |
|---|---|
| 插件 ID | `cpu.bench`（所属分组：`cpu`） |
| 适用平台 | 独立服务器、虚拟机、容器（各子套件在运行前独立判定架构、`glibc` 与网络前置条件） |
| 命令行参数 | `--cpu-bench <SUITE,...>`（可逗号分隔或重复传入，支持 `all`） |
| TUI 配置 | 无参数进入交互式 TUI 后选中 `cpu.bench`，可逐一勾选子套件（子套件与总开关双向联动） |
| 默认启用（6 项） | `sysbench`、`7zip`、`unixbench`、`hpl`、`passmark`、`geekbench` |
| 按需启用（2 项） | `ycruncher`、`prime95`（均为 `x86_64` 专属高负载测试；在 `aarch64` 上即使显式勾选或传入 `all` 也会平滑标记为跳过，不会中断其余子套件） |
| 超时上限 | `320 分钟`（覆盖全部子套件顺序运行及 Geekbench 上传重试的最坏耗时） |

### 统一调度与容错设计

1. **按需精准拉取工具（`tools_for`）**：编排器在准备阶段结合用户勾选的子套件与当前主机画像（CPU 架构、`glibc` 版本、IPv4 连通性），仅下载本次实际可运行子套件所需的工具包并校验大小与 `SHA-256`，绝不为已被前置跳过的子套件浪费带宽。
2. **前置条件联动**：当用户只选择了依赖 `glibc` 或外网的子集（例如 `--only cpu.bench --cpu-bench geekbench` 运行在 Alpine `musl` 或纯 IPv6 环境）时，`host_requirements` 会在下载开始前直接将整项 `cpu.bench` 判定为前置跳过；当混合选择多项时，不满足条件的单项在子套件内部记录跳过原因，其余子套件照常运行。
3. **增量快照发布（`ctx.publish`）**：每完成一个子套件（以及 `sysbench`、`7zip`、`unixbench`、`geekbench` 内部的子阶段），插件都会立即向编排器发布最新快照。中途按 `Ctrl-C` 或总时长触顶时，已完成的所有子套件成绩均完整保留在最终报告中。

---

## 1. 素数筛查基础算力：`sysbench`

| 属性 | 说明 |
|---|---|
| 子套件代码 | `sysbench`（默认启用） |
| 依赖工具 | 静态构建 `sysbench 1.0.20`（`x86_64` / `aarch64`） |
| 环境要求 | 全平台离线可跑 |

1. **标准参数**：执行 `sysbench cpu --cpu-max-prime=10000 --time=20 --threads=<N> run`，统计每秒完成的素数筛查事件数（`events/s`）。
2. **单核与满核扩展比**：先以 **1 线程**运行 20 秒并即时发布单核成绩；若可用线程数 `N > 1`（容器内尊重 `cpuset` 与 CPU 配额上限），再以 **`N` 线程**运行 20 秒并计算多核相对单核的加速倍率（`multi / single`）。
3. **容错重试**：单次异常退出或缺少速率输出时最多自动重试 **4 次**，重试成功后在备注中记录重试次数与上次错误。

---

## 2. 字典压缩与整数分支吞吐：`7zip`

| 属性 | 说明 |
|---|---|
| 子套件代码 | `7zip`（默认启用） |
| 依赖工具 | 静态构建 `7-Zip 26.06`（`7zz`，剥离非自由 unRAR 代码，`x86_64` / `aarch64`） |
| 环境要求 | 全平台离线可跑 |

1. **测试内容**：分别以 **1 线程**（`-mmt1`）和 **全部可用线程 `N`**（`-mmt<N>`）运行内置 LZMA2 基准 `7zz b`，采集汇总行的**压缩吞吐率（Compressing MIPS）**、**解压吞吐率（Decompressing MIPS）**与**综合评分（Total MIPS）**。该负载对整数流水线、分支预测以及 L3 缓存/内存延迟非常敏感。
2. **小内存自适应字典**：默认使用标准字典运行；当检测到主机当前可用内存低于 **1 GiB** 时，自动附加 `-md18`（256 KiB 字典）限制内存占用，避免在 512 MiB / 1 GiB 小内存 VPS 上触发 OOM。

---

## 3. Unix 系统综合基准：`unixbench`

| 属性 | 说明 |
|---|---|
| 子套件代码 | `unixbench`（默认启用） |
| 依赖工具 | 静态构建 `Byte-UnixBench 6.0.1`（`x86_64` / `aarch64`） |
| 环境要求 | 全平台离线可跑 |

1. **完整 12 项 System Index**：严格复刻上游 `UnixBench/Run` 默认 `system` 测试集的全部 **12 个子项**，全面衡量用户态算力与 Linux 内核系统调用/上下文切换开销：
   - 纯计算：`dhry2reg`（Dhrystone 2 整数与字符串）、`whetstone-double`（双精度浮点 MWIPS）
   - 进程与管道：`execl`（进程映像替换吞吐）、`pipe`（管道读写吞吐）、`context1`（基于管道的上下文切换）、`spawn`（子进程创建销毁）
   - 文件系统缓存与缓冲：`fstime`（1024 B 块 / 2000 块文件拷贝）、`fsbuffer`（256 B 块 / 500 块文件拷贝）、`fsdisk`（4096 B 块 / 8000 块文件拷贝）
   - Shell 负载与系统调用：`shell1`（1 路并发脚本流水线）、`shell8`（8 路并发脚本流水线）、`syscall`（混合系统调用开销）
2. **单并发与满核并发两轮**：第一轮运行 **1 Copy** 测量单核与单进程内核路径表现；当可用线程数 `N > 1` 时，第二轮为每个并发副本准备独立隔离目录并同时启动 **`N` Copies**。
3. **官方几何平均计分**：按上游 `index.base`（以 SPARCstation 20-61 为 `10.0` 分基准）计算每项得分 `Index_i = (Rate_i / Baseline_i) × 10.0`，再对 12 项比值取几何平均得出最终 **1-Copy Index** 与 **N-Copy Index**。

---

## 4. 双精度稠密线性代数峰值：`hpl`

| 属性 | 说明 |
|---|---|
| 子套件代码 | `hpl`（默认启用） |
| 依赖工具 | 静态构建 `HPL 2.3` + `OpenBLAS 0.3.34`（`DYNAMIC_ARCH=1` + OpenMP + 单机 MPI 桩，`x86_64` / `aarch64`） |
| 环境要求 | 全平台离线可跑（动态适配各代微架构 SIMD 指令集：AVX2 / AVX-512 / ARMv8 NEON / SVE） |

1. **动态矩阵规模与二维网格规划**：
   - 启动时读取可用内存，按约 **25% 可用内存**反推双精度矩阵阶数 `N`（`8 × N²` 字节），对齐分块大小 `NB = 192`，下限 `N >= 1920`，上限封顶 `N <= 20000`，确保单机测试在合理时间内完成且绝不挤占过多内存。
   - 单机单进程运行，将 `OMP_NUM_THREADS` 与 `OPENBLAS_NUM_THREADS` 同步设为可用线程数 `N_threads`（形状记录为 `1 × 1` 进程网格配 OpenMP 多线程，矩阵参数与理论耗时随 `HPL.dat` 自动生成）。
2. **数值残差校验**：不仅提取峰值算力（`GFLOPS`），还强制解析 HPL 尾部的残差检验行 `||Ax-b||_oo / (eps*(||A||_oo*||x||_oo+||b||_oo)*N)`；仅在残差 `PASSED` 时记为有效成绩，若因超频不稳或宿主机异常导致矩阵分解计算失真则直接判为失败。

---

## 5. 跨平台综合对比评分：`passmark`

| 属性 | 说明 |
|---|---|
| 子套件代码 | `passmark`（默认启用） |
| 依赖工具 | 官方 `PassMark PerformanceTest Linux 11.0 Build 1004` + 仓库构建的 `passmark-libs 1.0` 运行库补丁包（`x86_64` / `aarch64`） |
| 环境要求 | `glibc >= 2.28`（无需外网上传即可在本地生成完整评分） |

1. **自带运行库免装依赖**：上游 PassMark Linux 二进制动态依赖旧版 `libncurses.so.5`、`xterm` 终端描述与 `libcurl.so.4`，在 Debian 12/13、Ubuntu 24.04 等新发行版上直接运行会缺库报错。`lowendaff-bench` 通过独立构建的 `passmark-libs` 在私有目录提供兼容库与 `TERMINFO` 环境变量，实现零宿主机污染开箱即跑。
2. **全量 CPU 子项提取**：以 `-r 1`（仅运行全部 CPU 测试集）无交互执行，解析生成的 `results_cpu.yml`，完整输出 **CPU Mark 总分**、**Single-Threaded 单线程分**以及全部 **8 项细分负载成绩**（Integer Math 整数、Floating Point 浮点、Prime Numbers 素数、Extended Instructions SSE/NEON 扩展指令、Compression 压缩、Encryption 加密、Physics 物理模拟、Sorting 字符串排序）。

---

## 6. 高精度圆周率竞速：`ycruncher`（按需启用）

| 属性 | 说明 |
|---|---|
| 子套件代码 | `ycruncher`（默认关闭，通过 `--cpu-bench ycruncher` / `all` 或交互式 TUI 勾选启用） |
| 依赖工具 | 官方静态版 `y-cruncher 0.8.7 Build 9547`（仅 `x86_64`） |
| 环境要求 | `x86_64` 架构（`aarch64` 主机自动跳过） |

1. **自动档位匹配**：根据主机可用内存与线程数自动选择圆周率计算位数（内存较小时跑 `25,000,000` 位，常规多核跑 `100,000,000` 位，大内存多核机器跑 `500,000,000` 位），非交互执行 `y-cruncher bench <tier> -od:0`。
2. **微架构优化与指标**：`y-cruncher` 会根据 CPU 支持的指令集自动分派专用二进制内核（如 Zen4/Sapphire Rapids AVX-512、AVX2 等），报告中同时记录选定的计算规模、核心计算耗时（Computation Time）与端到端总耗时（Total Time），直观检验高负载 AVX 与内存子系统协同吞吐。

---

## 7. Small-FFT 稳定性烤机测试：`prime95`（按需启用）

| 属性 | 说明 |
|---|---|
| 子套件代码 | `prime95`（默认关闭，通过 `--cpu-bench prime95` / `all` 或交互式 TUI 勾选启用） |
| 依赖工具 | 官方 `Prime95 / mprime 30.19 Build 20`（仅 `x86_64`） |
| 环境要求 | `x86_64` 架构且 `glibc >= 2.18`（`aarch64` 或纯 `musl` 主机自动跳过） |

1. **定长无交互 Small-FFT 压测**：在隔离工作目录自动写入 `prime.txt`（禁用联网 GIMPS 任务领取，配置纯本地 Small-FFT 高热满载模式，充分驻留 L1/L2/L3 缓存以压榨 FPU/AVX 供电与散热稳定性），以 `mprime -t -d` 启动全部工作线程持续压测 **65 秒**后发送优雅中断信号（`SIGINT`）汇总结果。
2. **硬件错误零容忍校验**：逐行解析各 Worker 线程启停日志、实际选用的 FFT 实现描述，以及最终汇总的 **`errors`（硬件舍入/计算错误）与 `warnings` 计数**——方便独立服务器验机时快速排查体质缩水、电压不稳或散热降频隐患。

---

## 8. 业界通用单核/多核跑分：`geekbench`

| 属性 | 说明 |
|---|---|
| 子套件代码 | `geekbench`（默认启用） |
| 依赖工具 | 官方 `Geekbench 7.1.0`（`x86_64` 正式版 / `aarch64` Linux ARM Preview） |
| 环境要求 | `glibc >= 2.35` 且**具备 IPv4 公网连接**（成绩上传至 Geekbench Browser 换取公开结果链接） |

Geekbench 7 工作集较大，在低配 VPS（如 1 GiB / 2 GiB 内存机型）上直接运行极易触发 Linux OOM Killer。`geekbench` 子套件内置了完整的内存评估与保护流程：

1. **动态内存需求估算**：启动时实时读取 `/proc/meminfo`、容器 cgroup 剩余内存配额与测试目录所在分区的剩余磁盘空间，按 `所需内存 = 1 GiB + 1 GiB × 可用线程数` 估算峰值需求。
2. **内核 Swap 有效性预判（MGLRU + `swappiness=0` 检测）**：在开启了 Linux Multi-Gen LRU（`/sys/kernel/mm/lru_gen/enabled` 第 0 位为 `1`）且 `vm.swappiness == 0` 的内核上，内核仅回收页缓存而绝不换出匿名进程内存，此时挂载 Swap 文件也无法防止 OOM。子套件会主动识别该组合，跳过无意义的磁盘写入并在备注中说明原因。
3. **自动创建与清理临时 Swap 文件**：当现有“可用内存 + 空闲 Swap”小于估算需求且内核会使用 Swap 时，自动在工作目录下创建并启用临时 `swapfile`（向上取整至整数 GiB，最多补充 **8 GiB**，同时为磁盘保底预留至少 **1 GiB** 空间），子套件结束或中断后自动卸载并删除。
4. **OOM 优先级保护与上传重试**：调高 `geekbench` 子进程 OOM 分数以保护主程序与宿主机服务，前后对比 `/proc/vmstat` 的 `oom_kill` 计数精准识别内存耗尽；若本地计算已完成、仅最后上传 Geekbench Browser 因瞬时网络波动失败，最多自动重试 **4 次**。
