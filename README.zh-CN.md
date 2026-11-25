# lowendaff-bench _(lowendaff_bench)_

[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)
[![CI](https://img.shields.io/github/actions/workflow/status/Lowendaff/lowendaff_bench/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/Lowendaff/lowendaff_bench/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Lowendaff/lowendaff_bench?style=flat-square)](https://github.com/Lowendaff/lowendaff_bench/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-linux--x86__64%20%7C%20linux--aarch64-lightgrey.svg?style=flat-square)](https://github.com/Lowendaff/lowendaff_bench/releases)

> 面向 Linux 服务器的一站式硬件基准跑分、全球及三网测速、路由追踪、BGP 拓扑与 IP 流媒体体检工具。

[English](README.md) | **简体中文** | [日本語](README.ja.md)

`lowendaff-bench` 将 CPU 多维度算力、缓存与内存层次带宽延迟、硬盘 SMART 健康与混合随机 I/O、全球与国内三网吞吐测速、去程路由追踪、BGP 自治域上游拓扑、IP 纯净度画像以及五大洲流媒体解锁整合进单个静态编译二进制，支持 `x86_64` 与 `aarch64` 架构下的独立物理服务器、虚拟机（KVM、Xen 等）与容器环境（Docker、LXC、OpenVZ），兼具全交互终端界面（TUI）与非交互命令行（CLI），可输出终端表格、GitHub 风格 Markdown 或结构化 JSON 报告。

说明：GitHub 仓库与根目录名称为 `lowendaff_bench`，发布的 Linux 独立可执行文件名称为 `lowendaff-bench`（由工作区主控包 `bench` 构建）。

## 内容列表

- [安全](#安全)
- [背景](#背景)
- [安装](#安装)
  - [依赖](#依赖)
  - [从源码编译](#从源码编译)
  - [更新](#更新)
- [用法](#用法)
  - [CLI](#cli)
  - [命令行参数](#命令行参数)
  - [可选值参考](#可选值参考)
- [检测项目一览](#检测项目一览)
- [主要维护人员](#主要维护人员)
- [致谢](#致谢)
- [参与贡献方式](#参与贡献方式)
- [许可证](#许可证)

## 安全

- **固定版本工具与双重完整性校验**：运行中按需拉取的外部测试程序（`sysbench`、`7zz`、`UnixBench`、`HPL`、`PassMark`、`fio`、`iperf3`、`NextTrace`、`Geekbench 7`、`smartctl`、`STREAM`、`multichase`、`y-cruncher`、`mprime`）均锁定精确版本，执行前强制校验文件字节长度与 `SHA-256` 摘要，绝不调用宿主机上预装的同名旧程序。
- **最小权限隔离与中断自清理**：无需特权的计算型子工具在独立进程组与临时目录内降权运行；收到 `Ctrl-C`、`SIGTERM` 或 `SIGHUP` 时自动终止子进程组、卸载并删除临时 Swap 文件、恢复终端原始模式并清理运行目录。
- **纯下载代理严格隔离**：`--socks5-proxy` 与 `--github-proxy` 仅作用于工具与静态节点列表下载；程序主动忽略并剥离 `HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY` 等代理环境变量，确保公网 IP 识别、测速、路由、BGP 与流媒体探测始终反映本机真实直连网络。
- **默认隐私脱敏保护**：除非显式指定 `--show-ip`，所有输出格式（纯文本、Markdown、JSON）以及上传的 BGP SVG 拓扑图均默认隐去本机公网 IPv4/IPv6 地址、反向解析主机名、网卡 MAC 与硬件序列号，便于直接发帖分享。

如发现安全隐患或隐私泄露问题，请通过 [@Lowendaff](https://github.com/Lowendaff) 或 [GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues) 反馈（请勿在公开工单中贴出真实凭据或未脱敏私密地址）。

## 背景

在选购或验收 Linux 服务器时，传统跑分方式往往需要拼凑多份脚本、依赖各发行版编译参数不一的系统软件包，且容易在 512 MiB / 1 GiB 小内存机型上触发 OOM 死机，或在晒单时意外暴露本机真实 IP。`lowendaff-bench` 通过统一的 Rust 编排器解决上述痛点：

- **单文件即下即用，零依赖、零配置文件**：下载单个静态可执行文件即可在主流 Linux 发行版运行；不带参数默认进入交互式 TUI，传入任意参数即进入非交互 CLI。
- **跨发行版跑分口径严格一致**：所有外部基准工具均采用固定静态构建版本（定义于 `crates/bench-tool`），消除不同发行版软件源带来的跑分偏差。
- **自适应硬件裁剪与小内存保护**：自动跳过当前环境不适用的检测项（如虚拟机跳过物理盘 SMART 体检、单节点机器跳过多路 NUMA 矩阵、ARM64 主机跳过仅限 `x86_64` 的烤机子项）；内存、7-Zip 字典与 HPL 矩阵规模按可用内存及容器 cgroup 配额自动缩放，Geekbench 7 在内存吃紧时自动挂载临时 Swap 文件防止崩溃。
- **默认归档留存**：未指定 `-o` / `--output` 时，每次运行均自动在 `/tmp/lowendaff-bench-<时间戳>.<txt|md|json>` 额外保存一份不折行的完整报告副本。

各检测项背后的测量公式、参数推导与判定准则详见 [`docs/README.md`](docs/README.md)；运行时动态测速与路由节点列表维护于 [`Lowendaff/bench_data`](https://github.com/Lowendaff/bench_data)。

## 安装

前往 [GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) 下载对应 CPU 架构的最新可执行文件（常规 Intel/AMD 服务器选择 `lowendaff-bench-x86_64`，ARM64 服务器选择 `lowendaff-bench-aarch64`），赋予执行权限即可：

```sh
# 以 x86_64 服务器为例（ARM64 请将文件名换为 lowendaff-bench-aarch64）
curl -fLO https://github.com/Lowendaff/lowendaff_bench/releases/latest/download/lowendaff-bench-x86_64
chmod +x lowendaff-bench-x86_64
```

### 依赖

- **操作系统与架构**：Linux `x86_64` 或 `aarch64`（无需在宿主机预装任何跑分包；其中 `passmark` 子项需 `glibc >= 2.28`，`prime95` 子项需 `x86_64` 且 `glibc >= 2.18`，`geekbench` 子项需 `glibc >= 2.35` 且具备 IPv4 连接，不满足时自动平滑跳过）。
- **运行权限**：完整硬件画像（`SMBIOS`、`SMART`、原始套接字 `ICMP` 路由追踪及临时 Swap 管理）需使用 `sudo` 或 `root` 运行；仅查询列表或版本（`--list`、`--taier-list`、`--help`、`--version`）无需 `root` 权限。

### 从源码编译

安装 [`rust-toolchain.toml`](rust-toolchain.toml) 指定版本的 Rust 工具链与 `musl-tools`（提供 `musl-gcc`），即可自行编译静态可执行文件：

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

编译产物位于 `target/x86_64-unknown-linux-musl/release/lowendaff-bench`（ARM64 请将目标三元组换为 `aarch64-unknown-linux-musl`）。

### 更新

直接从 [GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) 下载最新二进制覆盖原文件即可，无需迁移任何配置或状态文件。

## 用法

不带任何参数运行即可进入交互式终端界面（TUI），携带 `--all`（或 `--only` / `--skip`）则以非交互命令行模式运行：

```sh
sudo ./lowendaff-bench-x86_64        # 无参数默认进入交互式 TUI
sudo ./lowendaff-bench-x86_64 --all  # 非交互命令行直接运行全套检测
```

未显式传入 `-o` / `--output` 时，每次运行结束后除了在屏幕展示外，还会自动归档保存至 `/tmp/lowendaff-bench-<时间戳>.<txt|md|json>`。运行途中按一次 `Ctrl-C` 可随时安全中止并输出已完成项目的报告。

**运行注意事项：**

- **资源占用与合规声明**：测试过程中会短暂高负载占用 CPU 与网络带宽（及内存、磁盘 I/O）资源，请务必遵守服务器商家的公平使用条款（Acceptable Use Policy）和服务条款（Terms of Service）。
- **测试耗时与后台运行建议**：全量测试包含多项算力基准、多节点测速与路由追踪，会占用一定时间；建议使用 `tmux`、`screen` 等软件在后台会话中执行，避免 SSH 连接意外断开导致测试中断。

### CLI

以下示例假设已将二进制重命名为 `lowendaff-bench`：

```sh
# 1. 查看支持的所有 15 个测试项目列表（无需 root 权限）
./lowendaff-bench --list

# 2. 启动全交互式终端界面（不带任何参数时的默认模式：勾选项目与子项联动、改参数、看实时进度与报告）
sudo ./lowendaff-bench

# 2b. 以命令行非交互方式直接跑全套完整体检（CPU 套件 + 内存 + 硬盘 + 测速 + 路由 + BGP + IP + 流媒体）
sudo ./lowendaff-bench --all

# 3. 日常快速验机：CPU 只跑轻量快速的 4 个子项（不跑耗时较长的 UnixBench 与 Geekbench 7），并跳过路由追踪
sudo ./lowendaff-bench --cpu-bench sysbench,7zip,hpl,passmark --skip network.trace

# 4. 新开 VPS 只想快速看看 IP 干不干净、能不能看流媒体，或者单独画一张本机 BGP 上游拓扑图
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only network.bgp

# 5. 只挑自己关心的单项组合测试，并在 x86_64 独服上开启 y-cruncher 与 Prime95 烤机稳定性压测
sudo ./lowendaff-bench --only cpu.bench,memory.stream,disk.fio --cpu-bench sysbench,7zip,ycruncher,prime95

# 6. 跑整个大类，但剔除其中某一项（例如只测 IP 信誉度，不测流媒体解锁）
sudo ./lowendaff-bench --only ip --skip ip.media

# 7. 将测试报告保存为 Markdown（适合直接粘贴到论坛/博客）或 JSON（适合脚本处理）
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 8. 测试挂载在其他目录的数据盘读写速度（注意不要指定 /tmp，很多系统里 /tmp 是内存盘）
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 9. 按地区筛选：只测日本和美国的带宽节点、中国三网泰尔测速节点、Cloudflare/Apple CDN、三网去程路由或亚洲流媒体
./lowendaff-bench --taier-list guangdong
sudo ./lowendaff-bench --only network.iperf3 --countries JP,US
sudo ./lowendaff-bench --only network.taier --taier beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct
sudo ./lowendaff-bench --only network.cloudflare,network.inetspeed --inetspeed-threads 8
sudo ./lowendaff-bench --only network.trace --trace-groups china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 10. 在默认封禁 25 邮件端口的云服务器上，跳过邮局连通性检查跑 IP 质量体检
sudo ./lowendaff-bench --only ip.quality --no-mail

# 11. 机器拉取 GitHub 或外部工具较慢时，使用 GitHub 镜像代理或 SOCKS5 代理加速文件下载，并在报告中展示完整 IP
#     （程序不读取环境变量代理；代理仅作用于工具与节点列表文件下载，绝不干预公网 IP 识别、测速、路由、BGP 与流媒体检测）
sudo ./lowendaff-bench --all --github-proxy https://ghfast.top/
sudo ./lowendaff-bench --all --socks5-proxy socks5h://127.0.0.1:1080 --download-connections 16 --show-ip
```

### 命令行参数

| 参数 | 写法 | 默认行为 | 通俗说明 |
|---|---|---|---|
| *（无参数）* | — | 默认进入 TUI | 不带任何参数运行 `sudo ./lowendaff-bench` 时默认进入交互式终端界面（支持总开关与子项双向联动、参数编辑、实时进度面板与多格式报告浏览） |
| `-a, --all` | 无需带值 | 关闭 | 在命令行非交互模式下运行全部 15 个测试项（等效于 `--only all`） |
| `--only` | `<名称,...>` | 测试全部项目 | 命令行模式下只运行指定的分类、单项 ID 或 `all`（多个名称用英文逗号隔开） |
| `--skip` | `<名称,...>` | 不跳过任何项 | 跳过指定的分类、单项 ID 或 `all`（可与 `--all` 或 `--only` 搭配做减法） |
| `-o, --output` | `<文件路径>` | 屏幕 + `/tmp` 归档 | 将最终报告仅写入指定文件（未指定 `-o` 时，输出到终端/预览器并自动归档保存至 `/tmp/lowendaff-bench-<时间戳>.<ext>`） |
| `-f, --format` | `<格式>` | `text` | 报告格式：可选 `text`（纯文本表格）、`markdown`（或简写 `md`）、`json` |
| `--work-dir` | `<目录路径>` | 当前目录 `.` | 存放临时测试文件的目录，跑完会自动清理。测哪块盘就指到哪块盘的挂载目录（请避开 `/tmp`） |
| `--cpu-bench` | `<子套件,...>` | 默认启用 6 项 | 选择 `cpu.bench` 要运行的 CPU 子测试（可选 `sysbench`、`7zip`、`unixbench`、`hpl`、`passmark`、`ycruncher`、`prime95`、`geekbench` 或 `all`） |
| `--countries` | `<国家代码,...>` | 默认精选节点 | 限制 `network.iperf3` 只测指定国家/地区的节点（如 `JP,SG,US`），填 `all` 测全部节点 |
| `--taier` | `<选择器,...>` | 北上广 × 三网 | 按 `<地区>[:<运营商>[+<运营商>...]]` 精确组合测速节点（如 `beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct,all:cm`） |
| `--taier-list` | `[省份]` | — | 列出全部泰尔测速省级/直辖市区域（或指定省份下的全部城市、选择器与运营商节点数）后退出（无需 `root`） |
| `--taier-cities` | `<城市/省份,...>` | 北上广默认节点 | 限制 `network.taier` 仅测指定城市、省份英文名或节点 ID（如 `Beijing,Jiangsu`），填 `all` 测全部节点 |
| `--taier-operators` | `<运营商,...>` | `telecom,unicom,mobile` | 选择 `network.taier` 要测的运营商（可选 `telecom`、`unicom`、`mobile`、`cernet`、`cbn`、`other` 或 `all`） |
| `--no-cloudflare-udp` | 无需带值 | 关闭 | 在 `network.cloudflare` 中跳过 RFC 5389 STUN UDP 丢包与抖动探测 |
| `--inetspeed-threads` | `<1..=64>` | `4` | 设置 `network.inetspeed` 多线程轮次的并发工作线程数 |
| `--inetspeed-endpoint` | `<节点 IP>` | 双 DoH 自动优选 | 在 `network.inetspeed` 中强制指定固定 Apple CDN 节点 IP（跳过 DoH 发现） |
| `--trace-groups` | `<分组,...>` | 缓存节点 + 默认线路 | 选择 `network.trace` 要追踪哪些方向的路由目标 |
| `--no-trace-tcp` | 无需带值 | 关闭 | 在 `network.trace` 中跳过中国大陆目标的 TCP SYN 大包去程追踪 |
| `--trace-packet-size` | `<64..=1500>` | `1024` | 设置 `network.trace` 中国大陆目标 TCP 大包探测整包字节数（含 IP + TCP 头） |
| `--media-groups` | `<区域,...>` | `all`（全部区域） | 选择 `ip.media` 要测试哪些地区的流媒体与在线服务 |
| `--no-dnsbl` | 无需带值 | 关闭 | 在 `ip.quality` 中跳过 25 个邮件/垃圾信息黑名单（DNSBL）查询 |
| `--no-mail` | 无需带值 | 关闭 | 在 `ip.quality` 中跳过 25 端口各大邮箱服务连通性测试 |
| `--no-bgp-upload` | 无需带值 | 关闭 | 在 `text` 模式下跳过将 `network.bgp` 拓扑 SVG 上传至 `imageupload.app` 图床 |
| `--download-connections` | `<1..=256>` | `32` | 下载外部测试工具时的最大并发连接数，网络不稳定时可适当调小 |
| `--socks5-proxy` | `<代理地址>` | 无 | 仅用于文件与工具下载的 SOCKS5 代理（支持 `socks5://` 与 `socks5h://`；不读取代理环境变量，也不影响任何检测项） |
| `--github-proxy` | `<镜像前缀 URL>` | 无 | 仅用于 GitHub 工具与列表下载的前缀代理（如 `https://ghfast.top/`，实际请求 `<前缀>/https://github.com/...`，可与 `--socks5-proxy` 同时配置分流） |
| `--show-ip` | 无需带值 | 关闭（默认打码） | 在最终报告里显示完整的本机公网 IPv4/IPv6 地址和反向解析域名 |
| `-v, --verbose` | 无需带值 | 关闭 | 打印更详细的下载过程与子工具运行日志（方便排查卡住或报错原因） |
| `--list` | 无需带值 | — | 列出所有支持的测试项目、适用条件与工具版本后直接退出（不需要 `root`） |
| `-h, --help` | 无需带值 | — | 查看命令行帮助提示 |
| `-V, --version` | 无需带值 | — | 查看当前程序版本号 |

### 可选值参考

| 参数 | 可填写的值 | 含义说明 |
|---|---|---|
| `--only` / `--skip` *（按大类）* | `cpu` | 统一 CPU 综合基准套件：`cpu.bench` |
| | `memory` | 包含全部 4 项内存测试：`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa` |
| | `disk` | 包含全部 2 项硬盘测试：`disk.health`、`disk.fio` |
| | `network` | 包含全部 6 项网络测试：`network.iperf3`、`network.taier`、`network.cloudflare`、`network.inetspeed`、`network.trace`、`network.bgp` |
| | `ip` | 包含全部 2 项 IP 测试：`ip.quality`、`ip.media` |
| `--only` / `--skip` *（按单项）* | `disk.health`、`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa`、`disk.fio`、`network.iperf3`、`network.taier`、`network.cloudflare`、`network.inetspeed`、`network.trace`、`network.bgp`、`ip.quality`、`ip.media`、`cpu.bench` | 精确指定某一个测试项目（无论怎么填，都会按固定的合理顺序依次执行） |
| `-f, --format` | `text` | 适合直接在 SSH 终端阅读的对齐纯文本表格 |
| | `markdown` *（或 `md`）* | 对齐 `lowendaff.com` 测评规范的 GitHub 风格 Markdown（双列属性表、引用块、上下文感知转义，以及 BGP 拓扑与虚拟机 `%si`/`%st` 的 Mermaid 图表） |
| | `json` | 结构化 JSON 数据，适合自动化汇总、监控脚本或二次开发读取 |
| `--cpu-bench` | `sysbench` | 单线程与满线程 20 秒素数筛查运算 *（默认启用）* |
| | `7zip` | 7-Zip LZMA2 单线程与满线程压缩/解压 MIPS 吞吐率 *（默认启用）* |
| | `unixbench` | Byte-UnixBench 6.0.1 完整 12 项系统综合指数（1 并发与满核并发）*（默认启用）* |
| | `hpl` | High-Performance Linpack（OpenBLAS 双精度稠密矩阵 LU 求解）GFLOPS *（默认启用）* |
| | `passmark` | PassMark PerformanceTest Linux CPU Mark 总分与 8 项细分跑分 *（默认启用；需 `glibc >= 2.28`）* |
| | `ycruncher` | y-cruncher 多线程高精度圆周率 Pi 极限计算 *（按需开启；仅 `x86_64`）* |
| | `prime95` | Prime95 (`mprime`) 65 秒 Small-FFT 重负载稳定性烤机检测 *（按需开启；仅 `x86_64`）* |
| | `geekbench` | 官方 Geekbench 7.1.0 单核/多核综合跑分与在线报告链接 *（默认启用；需 `glibc >= 2.35` 且有 IPv4）* |
| | `all` | 一次启用上述全部 8 个 CPU 子套件（不满足当前主机架构或环境要求的子项会自动平滑跳过） |
| `--trace-groups` | `services` | 只测离当前机器最近的 YouTube 和 Netflix 视频缓存节点路由 |
| | `china` | 中国大陆三网线路（北上广电信 163 / CN2、联通 169 / 9929、移动 CMNET / CMIN2） |
| | `international` | 海外主流国际骨干网、Telegram 数据中心以及 AWS / GCP 云节点 |
| | `all` | 以上全部目标一起测（共 73 个目标） |
| `--media-groups` | `global` | 全球通用平台（Netflix、YouTube Premium、Disney+、Prime Video、Spotify、TikTok 等） |
| | `asia` | 亚洲地区平台（Bilibili 港澳台、巴哈姆特动画疯、Abema、U-NEXT、TVer、Viu 等） |
| | `europe` | 欧洲地区平台（BBC iPlayer、ITVX、Channel 4、Canal+、ZDF、France TV、RAI Play 等） |
| | `americas` | 美洲地区平台（Hulu、Max、Peacock、Paramount+、Starz、Crave 等） |
| | `oceania` | 大洋洲地区平台（澳大利亚与新西兰的 Stan、9Now、7plus、ABC iview、TVNZ+ 等） |
| | `all` | 一次测完五大区域共 68 个平台（默认） |
| `--countries` | 两位国家/地区代码 *（如 `US`、`JP`、`DE`、`SG`、`GB`）* | 只连接对应国家/地区的公共 `iperf3` 测速节点（大小写均可） |
| | `all` | 连接节点列表里的所有可用公共 `iperf3` 服务器 |
| `--taier` | `<地区>[:<运营商>[+<运营商>...]]` *（如 `beijing:cu`、`guangdong/shenzhen:ct+cm`、`jiangsu:ct`、`all:cm`）* | 按直辖市、`省份/城市` 或全省粒度自由多选运营商（支持简写 `ct`/`cu`/`cm`/`edu`/`cbn`/`other`/`all`） |
| `--taier-cities` | 城市/省份英文名或节点 ID *（如 `Beijing`、`Jiangsu`、`guangdong-shenzhen`）* | 连接匹配的中国信通院全球网测（泰尔）节点（不区分大小写） |
| | `all` | 测试泰尔节点列表中的全部可用地区 |
| `--taier-operators` | `telecom`、`unicom`、`mobile`、`cernet`、`cbn`、`other` 或 `all` | 按运营商筛选泰尔测速节点（默认测试电信、联通、移动三网） |
| `--socks5-proxy` | `socks5://[user:pass@]host[:port]` | 在本机解析下载域名后通过 SOCKS5 代理传输文件 |
| | `socks5h://[user:pass@]host[:port]` | 将下载域名交由远端 SOCKS5 代理服务器解析并传输文件 |
| `--github-proxy` | `https://<镜像域名>/` *（或 `http://<镜像域名>/`）* | 为 `github.com` 与 `raw.githubusercontent.com` 下载自动拼接前缀（如 `https://ghfast.top/https://github.com/...`，直连镜像站，可与 `--socks5-proxy` 分流搭配） |

## 检测项目一览

在正式跑分前，程序会先只读采集 **系统概览（`[System]`）**：识别独立物理机、虚拟机或容器环境，读取 CPU 型号与各级缓存、内存与 Swap、系统内核与 TCP 拥塞算法（如 BBR）、磁盘分区、网卡协商速率（含 PCIe 降速提示）及双栈连通性；在独立物理服务器上还会列出内存插槽、ECC 报错计数、IPMI/BMC 与硬件 RAID 卡信息，而在虚拟机环境下则会在整个测试期间每 5 秒采样一次 `/proc/stat` 的 `%si`（软中断）与 `%st`（宿主机争抢），并在末尾输出统计汇总及时间线（`text` 模式为逐 5 秒明细表，`markdown` 模式为 Mermaid `xychart-beta` 双折线趋势图，`json` 模式为 `vm_cpu_telemetry`；详见 [`docs/system.md`](docs/system.md)）。

随后按固定注册顺序执行以下 15 个检测项目：

| 分类 | 项目名称 (`ID`) | 适用环境 | 这项测试能帮你看出什么？ | 原理详解 |
|---|---|---|---|---|
| `disk` | `disk.health`<br>硬盘健康体检 | 独立物理服务器 | 读取硬盘 SMART 信息（支持穿透常见硬件 RAID 卡），查看硬盘通电时长、温度、SSD 寿命磨损、累计写入量以及有没有坏道，快速识别二手老盘或故障隐患 | [`docs/disk.md`](docs/disk.md) |
| `memory` | `memory.sysbench`<br>内存连续读写 | 所有服务器 | 测量单线程与多线程连续读写内存的速度，直观反映内存通道配置是否正常 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream`<br>内存持续带宽 | 所有服务器 | 采用业界标准的 STREAM 测试法，测量大块数据搬运与计算时的真实内存极限带宽 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency`<br>缓存与内存延迟 | 所有服务器 | 逐级测量 CPU 一/二/三级缓存和主内存的随机访问延迟（纳秒级），能看出虚拟化开销或跨插槽延迟问题 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa`<br>多路内存互通 | 双路/多 NUMA 节点机器 | 在多颗 CPU 或多 NUMA 节点的机器上，测试访问“本地内存”与“另一颗 CPU 旁边的内存”之间的速度和延迟差距 | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio`<br>硬盘随机读写 | 所有服务器 | 采用与 YABS 相同的测试标准（50% 读 + 50% 写），测试 `4K`、`64K`、`512K`、`1M` 四种块大小下的真实读写速度与 IOPS，建站跑数据库重点看 `4K`，大文件吞吐看 `512K`/`1M` | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3`<br>全球带宽测速 | 所有联网机器 | 默认连接美西、美东、东京、法兰克福 5 个公共节点，用多线程同时测 IPv4 和 IPv6 的上传、下载真实带宽与延迟 | [`docs/network.md`](docs/network.md) |
| `network` | `network.taier`<br>中国三网测速（泰尔） | 有 IPv4 公网连接的机器 | 纯 Rust 原生实现的中国信通院“全球网测（泰尔测速）”多线程 HTTP/1.1 上下行带宽测速（默认测北京、上海、广州 × 电信、联通、移动，繁忙节点自动降级候选） | [`docs/network.md`](docs/network.md) |
| `network` | `network.cloudflare`<br>Cloudflare 测速 | 所有联网机器 | 纯 Rust 原生实现的双栈 `speed.cloudflare.com` 稳态上下行吞吐、空载与负载延迟、Waveform 缓冲膨胀（Bufferbloat）/ 稳定性评级及 STUN UDP 丢包与语音 MOS 评估 | [`docs/network.md`](docs/network.md) |
| `network` | `network.inetspeed`<br>Apple CDN 测速 | 所有联网机器 | 纯 Rust 原生实现的 `mensura.cdn-apple.com` 测速，支持双 DoH（Cloudflare + 阿里 DNS）最低 RTT 边缘节点优选绑定、空载延迟与单线程/多线程上下行吞吐测试 | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace`<br>去程路由追踪 | 支持原始套接字的机器 | 追踪本机到 YouTube/Netflix 节点、国内三网（163/CN2/9929/CMIN2）及国际骨干网的每一跳路径（独立 `ASN` 与运营商名称六列展示并标注 IX 交换中心），看清线路是直连还是绕路 | [`docs/network.md`](docs/network.md) |
| `network` | `network.bgp`<br>BGP 上游拓扑图 | 有公网 IP 的机器 | 查询本机 IP 所在网段的 BGP 宣告前缀、RPKI 签名状态与直连上游占比，在终端模式生成 `imageupload.app` 高清 SVG 拓扑图链接（不含本机 IP；Markdown 模式生成 Mermaid 矢量图，JSON 模式内置独立 SVG 源码） | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality`<br>IP 质量与纯净度 | 有公网 IP 的机器 | 汇总 9 家风控数据库结果，判断 IP 是**原生 IP** 还是**广播 IP**、属于机房还是家宽、欺诈风险高低、历史暴露端口、是否在 25 个黑名单里，以及 25 邮件端口是否开放 | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media`<br>流媒体解锁检测 | 所有联网机器 | 实测全球五大区域共 68 个流媒体与在线平台（Netflix、Disney+、YouTube、B站港澳台、巴哈姆特、Abema 等）能否正常解锁及识别到的所在地区 | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.bench`<br>CPU 综合基准与压测 | 所有服务器 *（按子项适配）* | 统一调度 8 项可选 CPU 子套件（`--cpu-bench`）：默认运行 `sysbench`、`7zip`、`unixbench`、`hpl`、`passmark` 与 `geekbench`，并支持按需开启 `ycruncher` 圆周率竞速与 `prime95` 烤机稳定性压测 | [`docs/cpu.md`](docs/cpu.md) |

## 主要维护人员

- [@Lowendaff](https://github.com/Lowendaff)

## 致谢

`lowendaff-bench` 的设计思路、测试标准与底层能力离不开以下优秀的开源项目、跑分工具与公开数据平台，向所有原作者与维护者致以诚挚感谢：

- **[YABS (Yet Another Bench Script)](https://github.com/masonr/yet-another-bench-script)**：`disk.fio` 四档块大小混合随机读写参数与速率展示格式，以及 `network.iperf3` 多线程上下行测速方案的重要参考。
- **[IPQuality (`xykt/IPQuality`)](https://github.com/xykt/IPQuality)**：`ip.quality` 多数据源交叉比对、原生/广播 IP 识别、风险标签汇总、DNSBL 黑名单检查与 25 端口邮局测试的重要参考。
- **[RegionRestrictionCheck (`lmc999/RegionRestrictionCheck`)](https://github.com/lmc999/RegionRestrictionCheck)** 与 **[MediaUnlockTest (`HsukqiLee/MediaUnlockTest`)](https://github.com/HsukqiLee/MediaUnlockTest)**：`ip.media` 各地区流媒体解锁检测的重要参考。
- **[NextTrace (`nxtrace/NTrace-core`)](https://github.com/nxtrace/NTrace-core)**：为 `network.trace` 提供精准轻量的路由追踪引擎与节点归属地信息。
- **[peer.as (`Archeb/peer.as`)](https://github.com/Archeb/peer.as)**：`network.bgp` 自治域路径分层无交叉布局算法、Tier-1 截断、Prepend 折叠与 SVG 拓扑图绘制的重要参考。
- **[speedtaier (`transflo/speedtaier`)](https://github.com/transflo/speedtaier)**：`network.taier` 中国信通院全球网测（泰尔测速）`dovalid` 鉴权协议、多线程 HTTP/1.1 上下行测速以及全国三网节点列表的重要参考。
- **[cloudflare-speed-cli (`kavehtehrani/cloudflare-speed-cli`)](https://github.com/kavehtehrani/cloudflare-speed-cli)**：`network.cloudflare` 稳态 HTTPS 上下行测速、负载延迟采样、Waveform Bufferbloat / 吞吐稳定性评级以及 STUN UDP 丢包与 MOS 计算的重要参考。
- **[iNetSpeed-CLI (`nxtrace/iNetSpeed-CLI`)](https://github.com/nxtrace/iNetSpeed-CLI)**：`network.inetspeed` 双 DoH 节点优选绑定与 Apple CDN（`mensura.cdn-apple.com`）单线程/多线程测速协议的重要参考。
- **上游测试程序与公开数据源**：[`sysbench`](https://github.com/akopytov/sysbench)（Alexey Kopytov）、[`7-Zip`](https://www.7-zip.org/)（Igor Pavlov）、[`Byte-UnixBench`](https://github.com/kdlucas/byte-unixbench)、[`HPL`](https://www.netlib.org/benchmark/hpl/) 与 [`OpenBLAS`](https://www.openblas.net/)、[`PassMark PerformanceTest`](https://www.passmark.com/products/pt_linux/)、[`y-cruncher`](http://www.numberworld.org/y-cruncher/)（Alexander J. Yee）、[`Prime95 / GIMPS`](https://www.mersenne.org/)、[`Geekbench`](https://www.geekbench.com/)（Primate Labs）、[`STREAM`](https://www.cs.virginia.edu/stream/)（John D. McCalpin）、[`multichase`](https://github.com/google/multichase)（Google）、[`smartmontools`](https://www.smartmontools.org/)（`smartctl`）、[`fio`](https://github.com/axboe/fio)（Jens Axboe）、[`iperf3`](https://github.com/esnet/iperf)（ESnet）、[`RIPEstat`](https://stat.ripe.net/)（RIPE NCC）、[`iperf3serverlist.net`](https://iperf3serverlist.net/) 以及 [`PeeringDB`](https://www.peeringdb.com/)。

## 参与贡献方式

欢迎在 [GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues) 中提问、反馈缺陷或提出改进建议。本项目接受 Pull Request；在提交代码或文档前，请先阅读 [`CONTRIBUTING.md`](CONTRIBUTING.md) 了解模块划分、Rust 代码规范、`ast-grep` 检查、提交信息格式以及 CI 门禁要求。

补充说明：修改本仓库 `README` 时，请遵循 [standard-readme](https://github.com/RichardLitt/standard-readme) 编写规范。

## 许可证

[MIT](LICENSE) © 2026 Lowendaff
