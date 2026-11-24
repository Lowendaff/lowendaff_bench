# 检测逻辑与架构文档 _(docs)_

[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../LICENSE)

> lowendaff-bench 主机画像采集与全部 15 个检测插件的测量原理、参数推导及异常处理技术文档。

返回仓库首页：[English](../README.md) | [简体中文](../README.zh-CN.md) | [日本語](../README.ja.md)

本目录（`docs/`）存放 `lowendaff-bench`（仓库 `lowendaff_bench`）的模块级中文技术说明，涵盖执行前的主机环境只读采集机制，以及全部 5 大分组、15 个检测插件的测试方法、参数选择依据、异常处理逻辑与可配置项。软件安装与命令行参数速查请参阅仓库根目录 [`README.zh-CN.md`](../README.zh-CN.md)。

## 内容列表

- [背景](#背景)
- [模块文档导航](#模块文档导航)
- [统一执行生命周期](#统一执行生命周期)
- [主要维护人员](#主要维护人员)
- [参与贡献方式](#参与贡献方式)
- [许可证](#许可证)

## 背景

服务器跑分工具若缺乏透明的采样公式与边界条件说明，读者往往难以判断某项指标是由真实硬件瓶颈、虚拟化调度开销还是测试参数差异所导致。本目录按报告输出章节逐一拆解每个插件的底层命令、工作集推导、超时控制与结果判定规则，确保每一项输出指标均可复核、可解释。

## 模块文档导航

| 模块文档 | 对应报告章节 | 包含内容 / 插件 ID |
|---|---|---|
| [主机画像与环境探测](system.md) | `[System]` | 虚拟化与容器识别、CPU/内存/cgroup 采集、SMBIOS/EDAC/BMC/RAID 卡硬件识别、存储与网卡拓扑、双栈公网 IP 直连探测 |
| [CPU 性能检测](cpu.md) | `[CPU performance]` | `cpu.bench`（统一调度 `sysbench`、`7zip`、`unixbench`、`hpl`、`passmark`、`ycruncher`、`prime95`、`geekbench` 共 8 项可选子基准） |
| [内存与缓存性能检测](memory.md) | `[Memory performance]` | 共享工作集规划器、`memory.sysbench`（顺序读写）、`memory.stream`（STREAM 四核函数）、`memory.latency`（各级缓存与主存延迟）、`memory.numa`（跨节点带宽与延迟矩阵） |
| [磁盘健康与性能检测](disk.md) | `[Disks]` | `disk.health`（物理盘只读 SMART 与硬件 RAID 穿透）、`disk.fio`（对齐 YABS 的四块大小混合随机读写） |
| [网络吞吐、中国三网、Cloudflare/Apple CDN 测速、路由与 BGP](network.md) | `[Network]` | `network.iperf3`（全球公网测速）、`network.taier`（纯 Rust 中国三网泰尔测速）、`network.cloudflare`（纯 Rust Cloudflare 双栈测速与 Bufferbloat/STUN 评估）、`network.inetspeed`（纯 Rust 双 DoH Apple CDN 单/多线程测速）、`network.trace`（ICMP 路由追踪与 IX 识别）、`network.bgp`（全球 RIS 采集点 BGP 拓扑图） |
| [IP 信誉度与流媒体解锁](ip.md) | `[IP]` | `ip.quality`（9 源情报聚合、原生/广播判定、DNSBL 黑名单与 25 端口邮局连通性）、`ip.media`（五大洲 68 平台双栈直连解锁检测） |

## 统一执行生命周期

每次运行 `lowendaff-bench` 时，编排器（Orchestrator）按以下固定流程工作：

1. **模式选择与输出前置校验**：不带任何参数运行时默认启动交互式 TUI（在界面内完成总开关与子项双向联动配置、实时进度观测与报告浏览）；传入任意命令行参数（如 `-a` / `--all`、`--only`、`--skip`）则运行非交互 CLI 模式。若指定了 `-o` / `--output <PATH>`，在下载工具或启动测试前立即以非阻塞安全方式创建并校验输出文件，路径不可写时直接以退出码 `2` 报错退出。
2. **并行主机画像与网络探测**：只读扫描内核接口（`/proc`、`/sys` 等）采集硬件与系统信息，同时强制忽略一切代理环境变量（`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY`）与下载代理参数，直连探测 IPv4 / IPv6 公网连通性与出口地址。
3. **按平台裁剪与按需拉取工具（支持纯下载代理分流）**：自动剔除不适用于当前平台的检测项（如容器/虚拟机上的 `disk.health`、单 NUMA 节点机器上的 `memory.numa`），仅下载本次实际可运行插件所需的外部工具与静态节点列表（支持通过 `--github-proxy` 直连镜像前缀改写与 `--socks5-proxy` `socks5://`/`socks5h://` 加速下载，且代理严格局限于文件下载，子工具进程亦自动剥离代理环境变量，绝不干预任何检测流量），逐一校验文件大小与固定 `SHA-256` 摘要（绝不调用宿主机预装的同名程序）。
4. **隔离目录、超时受控执行与虚拟机 `%si` / `%st` 全程采样**：在 `--work-dir`（默认当前目录 `.`，避开 `/tmp` 内存盘以免干扰磁盘跑分）下创建权限受限的临时运行目录，按注册顺序逐个运行插件；当主机识别为虚拟机（`Vm`）时，编排器在整个执行期间每 5 秒读取一次 `/proc/stat` 采集内核软中断（`%si`）与宿主机 CPU 争抢（`%st`）时间线。每个插件均设有最坏情况超时上限、阶段性结果实时发布（`Ctrl-C` 中断或超时时保留已完成子阶段数据）与任务级 `panic` 隔离，运行结束后自动清理临时目录。
5. **统一报告渲染与 `/tmp` 默认归档**：将全部结果按 `text`（默认终端对齐）、`markdown`（`md`，对齐 `lowendaff.com` 测评文档规范：双列 `Item | Value` 属性表、`> **Note:**` 引用块、`#`..`####` 严格标题层级与上下文感知转义，不含冗余耗时调试行）或 `json` 渲染输出；当未显式指定自定义输出文件路径时，TUI 与 CLI 模式均会自动在 `/tmp/lowendaff-bench-<时间戳>.<txt|md|json>` 额外保存一份不折行的完整报告副本。除非显式传入 `--show-ip`，所有格式均默认对本机公网 IP 与反向解析主机名脱敏。

## 主要维护人员

- [@Lowendaff](https://github.com/Lowendaff)

## 参与贡献方式

欢迎在 [GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues) 提交文档勘误或问题反馈。当修改插件检测逻辑、参数范围或默认行为时，请按照 [`CONTRIBUTING.md`](../CONTRIBUTING.md) 同步更新本目录下的对应说明，并遵守 [standard-readme](https://github.com/RichardLitt/standard-readme) 结构约定。

## 许可证

[MIT](../LICENSE) © 2026 Lowendaff
