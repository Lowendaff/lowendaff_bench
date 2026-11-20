# 检测逻辑与架构文档（中文）

返回首页：[English](../README.md) | [简体中文](../README.zh-CN.md) | [日本語](../README.ja.md)

本目录详细说明 `lowendaff-bench` 在执行前的主机环境采集机制，以及全部 5 大分组、12 个检测插件的测试方法、参数选择依据、异常处理逻辑与可配置项。

## 模块导航

| 模块文档 | 对应报告章节 | 包含内容 / 插件 ID |
|---|---|---|
| [主机画像与环境探测](system.md) | `[System]` | 虚拟化与容器识别、CPU/内存/cgroup 采集、SMBIOS/EDAC/BMC/RAID 卡硬件识别、存储与网卡拓扑、双栈公网 IP 直连探测 |
| [CPU 性能检测](cpu.md) | `[CPU performance]` | `cpu.bench`（统一调度 `sysbench`、`7zip`、`unixbench`、`hpl`、`passmark`、`ycruncher`、`prime95`、`geekbench` 共 8 项可选子基准） |
| [内存与缓存性能检测](memory.md) | `[Memory performance]` | 共享工作集规划器、`memory.sysbench`（顺序读写）、`memory.stream`（STREAM 四核函数）、`memory.latency`（各级缓存与主存延迟）、`memory.numa`（跨节点带宽与延迟矩阵） |
| [磁盘健康与性能检测](disk.md) | `[Disks]` | `disk.health`（物理盘只读 SMART 与硬件 RAID 穿透）、`disk.fio`（对齐 YABS 的四块大小混合随机读写） |
| [网络吞吐、路由追踪与 BGP 拓扑](network.md) | `[Network]` | `network.iperf3`（双向多线程公网测速与节点池巡检）、`network.trace`（最小权限 ICMP 路由追踪与 IX 识别）、`network.bgp`（全球 RIS 采集点 BGP 自治域路径拓扑、RPKI 校验与三端图形绘制） |
| [IP 信誉度与流媒体解锁](ip.md) | `[IP]` | `ip.quality`（9 源情报聚合、原生/广播判定、DNSBL 黑名单与 25 端口邮局连通性）、`ip.media`（五大洲 68 平台双栈直连解锁检测） |

## 统一执行生命周期

每次运行 `lowendaff-bench` 时，编排器（Orchestrator）按以下固定流程工作：

1. **模式选择与输出前置校验**：不带任何参数运行时默认启动交互式 TUI（在界面内完成总开关与子项双向联动配置、实时进度观测与报告浏览）；传入任意命令行参数（如 `-a` / `--all`、`--only`、`--skip`）则运行非交互 CLI 模式。若指定了 `-o` / `--output <PATH>`，在下载工具或启动测试前立即创建输出文件，路径不可写时直接以退出码 `2` 报错退出。
2. **并行主机画像与网络探测**：只读扫描内核接口（`/proc`、`/sys` 等）采集硬件与系统信息，同时不经任何代理直连探测 IPv4 / IPv6 公网连通性与出口地址。
3. **按平台裁剪与按需拉取工具**：自动剔除不适用于当前平台的检测项（如容器/虚拟机上的 `disk.health`、单 NUMA 节点机器上的 `memory.numa`），仅下载本次实际可运行插件所需的外部工具，逐一校验文件大小与固定 `SHA-256` 摘要（绝不调用宿主机预装的同名程序）。
4. **隔离目录与超时受控执行**：在 `--work-dir`（默认当前目录 `.`，避开 `/tmp` 内存盘以免干扰磁盘跑分）下创建权限受限的临时运行目录，按注册顺序逐个运行插件；每个插件均设有最坏情况超时上限、阶段性结果实时发布（`Ctrl-C` 中断或超时时保留已完成子阶段数据）与任务级 `panic` 隔离，运行结束后自动清理临时目录。
5. **统一报告渲染与 `/tmp` 默认归档**：将全部结果按 `text`（默认终端对齐）、`markdown`（`md`）或 `json` 渲染输出；当未显式指定自定义输出文件路径时，TUI 与 CLI 模式均会自动在 `/tmp/lowendaff-bench-<时间戳>.<txt|md|json>` 额外保存一份不折行的完整报告副本。除非显式传入 `--show-ip`，所有格式均默认对本机公网 IP 与反向解析主机名脱敏。
