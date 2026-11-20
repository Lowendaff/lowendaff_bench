# lowendaff-bench

[English](README.md) | **简体中文** | [日本語](README.ja.md)

使用 Rust 编写的零配置文件、单静态二进制 Linux 服务器综合基准测试与诊断工具（全面支持独立物理服务器、KVM/Xen 等虚拟机以及 Docker/LXC 等容器环境）：

- **单静态二进制、内置 12 项插件、零本地配置文件**：发布产物为纯静态链接的 `musl` 二进制（支持 `x86_64` 与 `aarch64`），所有筛选项均通过类型安全的命令行枚举参数控制，开箱即用。
- **外部工具版本与哈希强锁定**：需要调用外部评测程序（`sysbench`、`STREAM`、`multichase`、`smartctl`、`fio`、`iperf3`、`NextTrace`、`Geekbench 7`）时，主程序按需自动下载受控构建版本并强制校验文件大小与 `SHA-256` 摘要，**绝不调用宿主机已安装的同名程序**，确保任何发行版上测得的结果一致可比。
- **平台自适应与内存安全**：自动根据硬件环境裁剪不适用项（如仅在独立物理机运行 SMART 检测、仅在双节点以上环境运行 NUMA 矩阵）；内存测试动态将单轮占用限制在可用内存的 `50%` 以内，同时保证工作集达到末级 CPU 缓存的 `4 倍` 以上。
- **多格式报告输出与默认隐私脱敏**：实时进度输出至 `stderr`；完整报告支持终端对齐纯文本（`text`）、GitHub 风格 `Markdown`（`markdown` / `md`）与结构化 `JSON`（`json`），可直接写入 `-o` / `--output` 指定文件；未显式传入 `--show-ip` 时，所有格式均自动隐藏本机完整公网 IP 与反向解析主机名。

各检测项的底层测试方法、公式与判定规则详见 [`docs/` 模块详解文档](docs/README.md)；代码结构与贡献规范详见 [`CONTRIBUTING.md`](CONTRIBUTING.md)。

---

## 快速上手与常用案例

从 [GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) 下载对应架构的静态二进制文件（`lowendaff-bench-x86_64` 或 `lowendaff-bench-aarch64`），核对 `SHA256SUMS` 后以 `root` 权限运行：

```sh
# 1. 查看全部 12 个检测项、所属分组名、适用平台及锁定的外部工具版本（无需 root）
./lowendaff-bench --list

# 2. 执行全套综合评测（stderr 实时显示进度，全部完成后在 stdout 打印对齐报告）
sudo ./lowendaff-bench

# 3. 日常快速体检：略过耗时长或会公开上传跑分的项目（支持逗号分隔或重复传参）
sudo ./lowendaff-bench --skip cpu.geekbench,network.trace

# 4. 仅运行指定分组或单个检测插件（分组名与插件 ID 可自由混用）
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only cpu.sysbench,memory.stream,disk.fio

# 5. 运行某个分组但排除其中某一项（例如只测 IP 信誉度，跳过流媒体解锁）
sudo ./lowendaff-bench --only ip --skip ip.media

# 6. 将报告以 Markdown 或 JSON 格式直接保存到文件（不重复输出到终端 stdout）
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 7. 指定数据盘目录执行磁盘 fio 测试（请勿使用可能挂载为内存盘或 noexec 的 /tmp）
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 8. 使用命令行枚举参数按国家/区域筛选测速节点、路由追踪分组与流媒体大洲
sudo ./lowendaff-bench --only network.iperf3 --countries JP,DE
sudo ./lowendaff-bench --only network.trace --trace-groups services,china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 9. 在封禁 25 端口或敏感风控的云主机上仅跳过邮局端口检测执行 IP 信誉度体检
sudo ./lowendaff-bench --only ip.quality --no-mail

# 10. 通过代理加速工具下载，展示完整公网 IP 并开启详细调试日志
#     （注意：环境变量需写在 sudo 之后；公网 IP、信誉度与流媒体探测仍强制本机直连）
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16 --show-ip -v
```

---

## 命令行参数与枚举值详解

### 参数总表

| 参数 | 取值语法 | 默认值 | 功能说明 |
|---|---|---|---|
| `--only` | `<ITEM,...>` | *全部检测项* | 仅运行指定的分组名或插件 ID（支持逗号分隔或多次传入） |
| `--skip` | `<ITEM,...>` | *无* | 略过指定的分组名或单个插件 ID（在 `--only` 筛选结果基础上剔除） |
| `-o, --output` | `<PATH>` | `stdout` | 在测试开始前创建 `<PATH>` 并将最终报告写入该文件（不再重复打印到 `stdout`；路径不可写时立即以退出码 `2` 报错） |
| `-f, --format` | `<FORMAT>` | `text` | 报告输出格式（`text`、`markdown` / `md`、`json`）；写入文件或使用 `markdown`/`json` 时禁用终端列宽折行 |
| `--work-dir` | `<DIR>` | `.` | 在 `<DIR>` 下创建临时运行目录并在退出时自动清理（请勿指定 `/tmp`，以免落入内存盘或 `noexec` 挂载） |
| `--countries` | `<CODE,...>` | *列表默认节点* | 为 `network.iperf3` 按两位 ISO 3166 国家/地区代码（不区分大小写）或 `all` 筛选测速服务器 |
| `--trace-groups` | `<GROUP,...>` | *缓存节点 + 默认目标* | 为 `network.trace` 指定路由追踪目标分组枚举（逗号分隔） |
| `--media-groups` | `<GROUP,...>` | `all` | 为 `ip.media` 指定流媒体解锁检测区域枚举（逗号分隔） |
| `--no-dnsbl` | *开关* | `false` | 在 `ip.quality` 中跳过 25 个公共 DNSBL 黑名单反查 |
| `--no-mail` | *开关* | `false` | 在 `ip.quality` 中跳过出站 TCP 25 端口邮局 MX 连通性探测 |
| `--download-connections` | `<1..=256>` | `32` | 单个大文件（`>= 2 MiB`）HTTP 分片并发下载的最大连接数（支持断点续传与 4 次重试） |
| `--show-ip` | *开关* | `false` | 在报告中显示完整的本机公网 IPv4/IPv6 地址与反向解析主机名（默认打码脱敏） |
| `-v, --verbose` | *开关* | `false` | 输出详细进度日志（含下载分片/重试及外部工具命令与退出码，自动隐去序列号、敏感链接与本机 IP） |
| `--list` | *开关* | `false` | 按报告章节顺序列出所有注册插件、适用平台与锁定工具版本后退出（无需 `root`） |
| `-h, --help` | *开关* | — | 打印命令行帮助信息并退出 |
| `-V, --version` | *开关* | — | 打印程序版本号并退出 |

### 参数枚举值速查表

| 参数 | 可选枚举值 | 枚举含义说明 |
|---|---|---|
| `--only` / `--skip` *（按分组）* | `cpu` | 包含全部 2 项 CPU 测试：`cpu.sysbench`、`cpu.geekbench` |
| | `memory` | 包含全部 4 项内存测试：`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa` |
| | `disk` | 包含全部 2 项磁盘测试：`disk.health`、`disk.fio` |
| | `network` | 包含全部 2 项网络测试：`network.iperf3`、`network.trace` |
| | `ip` | 包含全部 2 项 IP 测试：`ip.quality`、`ip.media` |
| `--only` / `--skip` *（按单插件 ID）* | `disk.health`、`cpu.sysbench`、`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa`、`disk.fio`、`network.iperf3`、`network.trace`、`ip.quality`、`ip.media`、`cpu.geekbench` | 按精确插件 ID 选择或排除任意单项（保持默认执行顺序） |
| `-f, --format` | `text` | 终端对齐纯文本格式（当标准输出为交互终端时自动按列宽折行） |
| | `markdown` *（别名 `md`）* | GitHub 风格 Markdown（多级标题、列表、数值列右对齐管道表格，并自动转义掩码 IP 中的 `\*`） |
| | `json` | 格式化结构化 JSON（包含 `version`、`environment`、`show_ip`、`system`、`runs`、`not_applicable`、`summary`） |
| `--trace-groups` | `services` | 仅追踪距离当前主机最近的 YouTube 与 Netflix 边缘缓存服务器 |
| | `china` | 中国大陆三网骨干与精品网目标（北上广电信 163 / CN2、联通 169 / 9929、移动 CMNET / CMIN2） |
| | `international` | 国际 Tier-1 骨干网运营商、Telegram 数据中心、AWS 与 GCP 目标 |
| | `all` | 同时追踪流媒体缓存节点（`services`）以及列表内的全部国内外目标（共 73 项） |
| `--media-groups` | `global` | 全球跨国流媒体平台（Netflix、YouTube Premium、Disney+、Prime Video、Spotify、TikTok 等） |
| | `asia` | 亚洲区域平台（Bilibili 港澳台、巴哈姆特动画疯、Abema、U-NEXT、TVer、Viu、Coupang Play 等） |
| | `europe` | 欧洲区域平台（BBC iPlayer、ITVX、Channel 4、Canal+、ZDF、ARTE、RAI Play 等） |
| | `americas` | 北美与拉美区域平台（Hulu、Max、Peacock、Paramount+、Starz、Crave、Globoplay 等） |
| | `oceania` | 大洋洲澳大利亚与新西兰平台（Stan、9Now、7plus、ABC iview、TVNZ+、ThreeNow 等） |
| | `all` | 检测全部五大区域共 68 个平台（未指定 `--media-groups` 时的默认行为） |
| `--countries` | `<ISO-3166-1 两位代码>` *（如 `US`、`JP`、`DE`、`SG`、`GB`）* | 仅测试 `servers/iperf3.json` 中对应国家/地区的全部公共测速节点（不区分大小写） |
| | `all` | 测试列表中的全部可用公共 `iperf3` 节点 |

### 隐私保护与退出码

- **隐私保护一致性**：无论选择 `text`、`markdown` 还是 `json` 格式，只要未传入 `--show-ip`，报告中的本机公网 IPv4（如 `203.0.*.*`）、IPv6（如 `2001:db8:*`）及路由追踪中的本机跳点均会自动打码，且 `ip.quality` 中的反向解析 `hostname` 会被隐藏（JSON 中为 `null`）。
- **退出码约定**：
  - `0`：整轮流程正常结束（单个插件的跳过或失败记录在报告内部）；
  - `2`：命令行参数或枚举值非法、`--output` 无法创建，或未以 `root` 身份运行；
  - `130`：收到 `Ctrl-C` 中断信号（已完成部分的测试结果仍会完整输出到报告；连续第二次 `Ctrl-C` 立即退出）。

---

## 检测项一览与原理索引

在任何插件启动前，程序会先在 30 秒预算内只读采集 **主机画像（`[System]`）**：自动识别独立物理机 / 虚拟机 / 容器环境、CPU 微架构（含 ARM 厂商与核心代码解码）与各级缓存、内存/Swap 与容器 cgroup 配额、发行版/内核/运行时间/负载/TCP 拥塞控制算法、物理磁盘与文件系统、网卡速率及 PCIe 槽位降速告警、双栈直连可达性与公网出口地址；在独立物理机上还会额外读取 DMI/SMBIOS 内存插槽明细、ECC/EDAC 纠错计数、BMC 带外管理芯片与硬件 RAID 阵列卡（详见 [`docs/system.md`](docs/system.md)）。

| 分组 | 插件 ID | 名称 | 适用范围 | 检测内容简介 | 详细测试逻辑 |
|---|---|---|---|---|---|
| `disk` | `disk.health` | 磁盘健康 | 仅独立物理机 | 使用 `smartctl` 只读扫描 NVMe/SATA/SAS 硬盘并穿透 MegaRAID / 3SNIC 阵列卡，汇总健康状态、通电时间、温度、磨损度、累计写入量、坏块与链路错误（绝不记录序列号与 WWN） | [`docs/disk.md`](docs/disk.md) |
| `cpu` | `cpu.sysbench` | CPU 基础性能 | 全平台 | 运行 `sysbench` 素数筛查（上限 `10000`，单轮 20 秒），分别测试单线程与全部可用线程速率（events/s）及多核加速比 | [`docs/cpu.md`](docs/cpu.md) |
| `memory` | `memory.sysbench` | 内存顺序读写 | 全平台 | 在穿透末级缓存（`>= 4x LLC`）且不超过可用内存一半的缓冲区上，测量单线程与满线程顺序读、顺序写吞吐率（`MiB/s`） | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream` | 内存 STREAM | 全平台 | 从预编译的 6 档双精度数组规模中自动匹配缓存与内存预算，测量 `Copy`、`Scale`、`Add`、`Triad` 四项持续内存带宽（`MB/s`） | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency` | 缓存与主存延迟 | 全平台 | 基于 `multichase` 随机指针追逐，逐级测量 `L1d`、`L2`、`L3` 缓存命中延迟与主存随机访问延迟（`ns`） | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa` | 内存 NUMA 矩阵 | `>= 2` 个可用 NUMA 节点 | 构建“CPU 节点 × 内存节点”二维矩阵，使用 `multiload` 与 `multichase` 测量本地与跨插槽互联的带宽及访问延迟 | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio` | 磁盘读写性能 | 全平台 | 严格对齐 YABS 参数（`libaio` + `O_DIRECT`、50/50 混合随机读写），在 `4K`、`64K`、`512K`、`1M` 四档块大小下测量吞吐速度与 IOPS（自动拒绝内存盘） | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3` | 公网带宽测速 | 全平台（需联网） | 对齐 YABS 的双向 8 并发流 × 10 秒 TCP 上传/下载带宽与最小 RTT 测速（默认覆盖美西、美东、东京、法兰克福 5 大节点，支持 IPv4/IPv6 双栈） | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace` | 去程路由追踪 | 全平台（需 `CAP_NET_RAW`） | 以最小权限（`cap_net_raw+ep` + `nobody`）调用 `NextTrace` 进行 ICMP 逐跳追踪，覆盖最近的 YouTube/Netflix 边缘节点与国内外 71 个骨干网目标，并识别 Netflix IX 交换中心 | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality` | IP 信誉度 | 全平台（需公网 IP） | 聚合 9 个免认证情报源（ASN、RIR **原生/广播**判定、用途分类、欺诈评分、Shodan 暴露端口/漏洞、7 类风险因子投票）、25 个 DNSBL 黑名单（过滤公共 DNS 策略拒绝）及 8 大邮局 25 端口连通性 | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media` | 流媒体解锁 | 全平台（需联网） | 不经代理直连探测全球五大区域（全球/亚洲/欧洲/美洲/大洋洲）共 68 个流媒体与在线平台的 IPv4/IPv6 解锁状态、解锁地区码、仅自制剧或 WAF 拦截情况 | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.geekbench` | Geekbench 7 | `glibc >= 2.35` 且有 IPv4 | 运行官方 Geekbench 7.1.0 输出单核/多核跑分与公开结果链接；低内存机型自动按缺口创建临时 Swap（上限 8 GiB）并配置 OOM 保护 | [`docs/cpu.md`](docs/cpu.md) |

---

## 从源码构建与版本发布

项目工具链版本固定于 [`rust-toolchain.toml`](rust-toolchain.toml)。构建静态 `musl` 二进制需安装 `musl-tools`（提供编译 `ring` 所需的 `musl-gcc`）：

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

当向仓库推送符合语义化版本且与根 `Cargo.toml` 中 `[workspace.package] version` 一致的 `v*` 标签（如 `v0.1.0`）时，GitHub Actions 发布流水线（[`.github/workflows/release.yml`](.github/workflows/release.yml)）会自动构建 `lowendaff-bench-x86_64` 与 `lowendaff-bench-aarch64` 纯静态二进制，生成 `SHA256SUMS` 并发布至 GitHub Releases。完整的本地与 CI 门禁命令请参阅英文版 [`README.md` "Gates"](README.md#gates) 及 [`CONTRIBUTING.md`](CONTRIBUTING.md)。

---

## 致谢（Acknowledgments）

`lowendaff-bench` 在测试方法论、指标设计与工具链构建上参考并受益于以下优秀开源项目、基准套件与开放数据平台，特此致谢：

- **[YABS (Yet Another Bench Script)](https://github.com/masonr/yet-another-bench-script)**：`disk.fio` 四块大小混合随机读写参数、速率/IOPS 截断格式，以及 `network.iperf3` 双向多线程测速流程的核心参考。
- **[IPQuality (`xykt/IPQuality`)](https://github.com/xykt/IPQuality)**：`ip.quality` 多源 IP 信誉度聚合、RIR 原生/广播来源比对、风险因子投票、DNSBL 扫描与 25 端口邮局连通性体检的设计参考。
- **[RegionRestrictionCheck (`lmc999/RegionRestrictionCheck`)](https://github.com/lmc999/RegionRestrictionCheck)** 与 **[MediaUnlockTest (`HsukqiLee/MediaUnlockTest`)](https://github.com/HsukqiLee/MediaUnlockTest)**：`ip.media` 全球各区域流媒体平台版权地域解锁检测的重要参考。
- **[NextTrace (`nxtrace/NTrace-core`)](https://github.com/nxtrace/NTrace-core)**：为 `network.trace` 提供底层 ICMP 路由追踪引擎与跳点 ASN / 地理位置查询服务。
- **上游基准评测工具与开放数据源**：[`sysbench`](https://github.com/akopytov/sysbench)（Alexey Kopytov）、[`STREAM`](https://www.cs.virginia.edu/stream/)（John D. McCalpin）、[`multichase`](https://github.com/google/multichase)（Google）、[`smartmontools`](https://www.smartmontools.org/)（`smartctl`）、[`fio`](https://github.com/axboe/fio)（Jens Axboe）、[`iperf3`](https://github.com/esnet/iperf)（ESnet）、[`Geekbench`](https://www.geekbench.com/)（Primate Labs）、[`iperf3serverlist.net`](https://iperf3serverlist.net/) 以及 [`PeeringDB`](https://www.peeringdb.com/)。
