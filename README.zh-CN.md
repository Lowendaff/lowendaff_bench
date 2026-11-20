# lowendaff-bench

[English](README.md) | **简体中文** | [日本語](README.ja.md)

使用 Rust 编写的 Linux 服务器综合基准测试与硬件/网络诊断工具（全面支持独立物理服务器、KVM/Xen 等虚拟机以及 Docker/LXC 等容器环境）：

- **单静态二进制，内置 12 项检测插件**：发布产物为纯静态链接的 `musl` 二进制（支持 `x86_64` 与 `aarch64`），每个检测项均为拥有独立强类型结果、超时控制与异常隔离的插件。
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

# 8. 仅选择日本与德国的公共 iperf3 节点测速（节省按量计费主机的流量）
sudo ./lowendaff-bench --only network.iperf3 --countries JP,DE

# 9. 在连接 GitHub 受限的机器上通过代理加速工具下载，并调整单文件并发分片连接数
#    （注意：环境变量需写在 sudo 之后；公网 IP、信誉度与流媒体探测仍强制本机直连）
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16

# 10. 加载自定义 TOML 配置文件、在报告中展示完整公网 IP，并开启详细调试日志
sudo ./lowendaff-bench --config bench.toml --show-ip -v
```

### `bench.toml` 配置文件示例

支持配置自定义参数的插件均通过 `[plugins."<id>"]` 表读取设置：

```toml
[plugins."network.iperf3"]
countries = ["US", "JP"]            # 按 ISO 3166 国家码筛选节点；命令行 --countries 优先级更高
servers = ["tokyo"]                 # 按节点 ID 精确指定测速服务器；非空时优先于 countries
seconds = 10                        # 单向（上传/下载）压测秒数
streams = 8                         # 并发 TCP 连接流数
rate = 100                          # 单流最大发送速率限制（Mbit/s）；不填则不限速

[plugins."network.trace"]
groups = ["china"]                  # 目标分组：services、china、international 或 all
targets = ["pek-4809", "youtube"]   # 按目标 ID 精确指定追踪对象；非空时优先于 groups
geo = true                          # 设为 false 可关闭跳点 ASN 与地理位置查询

[plugins."ip.quality"]
dnsbl = true                        # 设为 false 可跳过 25 个公共 DNSBL 黑名单查询
mail = true                         # 设为 false 可跳过 TCP 25 端口邮局入站连通性探测

[plugins."ip.media"]
groups = ["global", "asia"]         # 区域分组：global、asia、europe、americas、oceania 或 all
services = ["netflix", "bahamut"]   # 按平台 ID 精确指定流媒体服务；非空时优先于 groups
```

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

## 命令行参数、隐私边界与退出码

| 参数 | 说明 |
|---|---|
| `--only <LIST>` | 仅运行指定的分组或插件 ID（逗号分隔或多次传入，如 `--only cpu,disk.fio`） |
| `--skip <LIST>` | 略过指定的分组或单个插件 ID（如 `--skip cpu.geekbench,ip.media`） |
| `-o, --output <PATH>` | 将最终报告直接写入 `<PATH>` 而非 `stdout`（在下载与跑分开始前即校验文件可写性） |
| `-f, --format <FMT>` | 报告格式：`text`（默认）、`markdown`（别名 `md`）或 `json`；写入文件或非 `text` 格式时不按终端宽度折行 |
| `--config <PATH>` | 指定 TOML 配置文件路径（表结构为 `[plugins."<id>"]`） |
| `--work-dir <DIR>` | 在 `<DIR>` 下创建临时运行目录并在退出时自动清理（默认当前目录 `.`；请避开 `/tmp`） |
| `--countries <CODES>` | 为 `network.iperf3` 指定测速服务器所在国家/地区的两位 ISO 代码（如 `JP,DE`）或 `all` |
| `--download-connections <N>` | 单个大文件（`>= 2 MiB`）分片并发下载的最大连接数，范围 `1..=256`，默认 `32` |
| `--show-ip` | 在报告中显示完整的本机公网 IPv4/IPv6 地址与反向解析主机名（默认打码脱敏） |
| `-v, --verbose` | 输出详细进度日志（含下载分片/重试及外部工具命令与退出码，自动隐去序列号与敏感链接） |
| `--list` | 按报告分组顺序列出所有检测项及其适用条件与工具版本后退出 |

- **隐私保护一致性**：无论选择 `text`、`markdown` 还是 `json` 格式，只要未传入 `--show-ip`，报告中的本机公网 IPv4（如 `203.0.*.*`）、IPv6（如 `2001:db8:*`）、路由追踪中的本机跳点均会自动打码，且 `ip.quality` 中的反向解析 `hostname` 会被隐藏（JSON 中为 `null`）。
- **退出码约定**：
  - `0`：整轮流程正常结束（单个插件的跳过或失败记录在报告内部）；
  - `2`：命令行参数错误、配置文件语法/字段错误、`--output` 无法创建或未以 `root` 身份运行；
  - `130`：收到 `Ctrl-C` 中断信号（已完成部分的测试结果仍会完整输出到报告；连续第二次 `Ctrl-C` 立即退出）。

---

## 从源码构建与版本发布

项目工具链版本固定于 [`rust-toolchain.toml`](rust-toolchain.toml)。构建静态 `musl` 二进制需安装 `musl-tools`（提供编译 `ring` 所需的 `musl-gcc`）：

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

当向仓库推送符合语义化版本且与根 `Cargo.toml` 中 `[workspace.package] version` 一致的 `v*` 标签（如 `v0.1.0`）时，GitHub Actions 发布流水线（[`.github/workflows/release.yml`](.github/workflows/release.yml)）会自动构建 `lowendaff-bench-x86_64` 与 `lowendaff-bench-aarch64` 纯静态二进制，生成 `SHA256SUMS` 并发布至 GitHub Releases。

---

## 外部评测工具与质量门禁

除闭源且禁止再分发的 Geekbench 7.1.0（运行时直连官方 `cdn.geekbench.com` 下载并校验固定 `SHA-256`）外，其余所有外部工具（`fio 3.43`、`iperf3 3.22`、`smartctl 7.5`、`sysbench 1.0.20`、`STREAM 5.10`、`NextTrace 1.7.6`、`multichase`）均由本仓库 [`.github/workflows/tools.yml`](.github/workflows/tools.yml) 在固定摘要的容器镜像中可重现地静态编译，并连同源码包、许可证文本与 `SHA256SUMS` 发布为不可变 Release。完整的本地与 CI 门禁命令列表请参阅英文版 [`README.md` "Gates"](README.md#gates) 及 [`CONTRIBUTING.md`](CONTRIBUTING.md)。
