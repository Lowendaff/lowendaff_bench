# lowendaff-bench

[English](README.md) | **简体中文** | [日本語](README.ja.md)

`lowendaff-bench` 是一款面向 Linux 服务器的一站式硬件跑分、网络测速与 IP 体检工具，支持独立物理机、常见云服务器/VPS（KVM、Xen 等）以及容器环境（Docker、LXC 等）。

### 为什么用它？

- **单文件即下即用，无需安装依赖，也不用写配置文件**：下载一个静态编译好的可执行文件就能直接运行，支持 `x86_64` 和 `aarch64`（ARM64）架构，所有功能都通过命令行参数控制。
- **测试标准统一，不同机器分数可直接对比**：运行过程中需要用到的测试工具（如 `sysbench`、`fio`、`iperf3`、`NextTrace`、`Geekbench 7` 等）都会按需自动下载固定版本并校验完整性，不会调用系统里自带的旧版本软件，避免因系统环境不同导致跑分偏差。
- **自动适配机器配置，照顾小内存机型**：自动跳过当前环境不支持的项目（例如虚拟机自动跳过物理阵列卡硬盘体检，单路 CPU 自动跳过多路 NUMA 测试）；内存测试最多只使用当前可用内存的一半，跑 Geekbench 7 时如果内存不足还会自动创建临时交换文件（Swap），防止小内存 VPS 卡死崩溃。
- **默认给隐私信息打码，方便直接发帖分享**：支持在终端直接查看对齐表格，也支持一键导出 **Markdown** 或 **JSON** 文件。默认自动隐去本机真实公网 IP、主机名与硬件序列号，复制结果发帖晒单无需手动打码。

如果想深入了解每一项测试背后的计算方法和判定逻辑，可以查阅 [`docs/` 测试原理说明](docs/README.md)。

---

## 三步快速开始

前往 [GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) 下载对应架构的最新文件（常规服务器选 `lowendaff-bench-x86_64`，ARM 服务器选 `lowendaff-bench-aarch64`），赋予执行权限后用 `root` 运行即可：

```sh
# 以 x86_64 服务器为例（ARM64 请将文件名换为 lowendaff-bench-aarch64）
curl -fLO https://github.com/Lowendaff/lowendaff_bench/releases/latest/download/lowendaff-bench-x86_64
chmod +x lowendaff-bench-x86_64
sudo ./lowendaff-bench-x86_64
```

> 运行期间，实时进度会显示在屏幕上；所有选定项目跑完后，会统一输出排版整齐的完整报告。按一次 `Ctrl-C` 可随时中止，并保留已经跑完的部分结果。

---

## 常见使用场景示例

下面演示实际买机器、验机时最常用的几种跑法（假设已将二进制重命名为 `lowendaff-bench`）：

```sh
# 1. 查看支持的所有测试项目列表（无需 root 权限）
./lowendaff-bench --list

# 2. 跑全套完整体检（CPU + 内存 + 硬盘 + 全球测速 + 路由追踪 + IP 质量 + 流媒体解锁）
sudo ./lowendaff-bench

# 2b. 启动全交互式终端界面（勾选要测的项目、修改各项参数、查看实时进度面板并滚动阅读报告）
sudo ./lowendaff-bench --tui

# 3. 日常快速验机：跳过耗时较长、且会把跑分上传到官网公开页面的 Geekbench 7 和路由追踪
sudo ./lowendaff-bench --skip cpu.geekbench,network.trace

# 4. 新开 VPS 只想快速看看 IP 干不干净、能不能看流媒体，或者单独画一张本机 BGP 上游拓扑图
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only network.bgp

# 5. 只挑自己关心的几个单项组合测试（分类名和具体项目名可以自由混搭）
sudo ./lowendaff-bench --only cpu.sysbench,memory.stream,disk.fio

# 6. 跑整个大类，但剔除其中某一项（例如只测 IP 信誉度，不测流媒体解锁）
sudo ./lowendaff-bench --only ip --skip ip.media

# 7. 将测试报告保存为 Markdown（适合直接粘贴到论坛/博客）或 JSON（适合脚本处理）
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 8. 测试挂载在其他目录的数据盘读写速度（注意不要指定 /tmp，很多系统里 /tmp 是内存盘）
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 9. 按地区筛选：只测日本和美国的带宽节点、只看三网回程/去程路由、只测全球与亚洲流媒体
sudo ./lowendaff-bench --only network.iperf3 --countries JP,US
sudo ./lowendaff-bench --only network.trace --trace-groups china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 10. 在默认封禁 25 邮件端口的云服务器上，跳过邮局连通性检查跑 IP 质量体检
sudo ./lowendaff-bench --only ip.quality --no-mail

# 11. 机器拉取 GitHub 文件较慢时，通过代理加速下载测试工具，并在报告中展示完整 IP
#     （注：代理变量需放在 sudo 后面；公网 IP 识别、测速与流媒体检测依然会走本机直连）
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16 --show-ip
```

---

## 命令行参数与可选值说明

### 参数一览

| 参数 | 写法 | 默认行为 | 通俗说明 |
|---|---|---|---|
| `--tui` | 无需带值 | 关闭 | 启动全流程交互式终端界面（勾选各测试项开关、设置每项参数与全局选项、查看实时进度并滚动浏览报告；**不可与任何其他参数同时使用**） |
| `--only` | `<名称,...>` | 测试全部项目 | 只运行你指定的分类或具体项目（多个名称用英文逗号隔开，也可重复写多次） |
| `--skip` | `<名称,...>` | 不跳过任何项 | 跳过你指定的分类或具体项目（可与 `--only` 搭配做减法） |
| `-o, --output` | `<文件路径>` | 直接打印到屏幕 | 把最终报告保存到指定文件，屏幕上只显示进度 |
| `-f, --format` | `<格式>` | `text` | 报告格式：可选 `text`（纯文本表格）、`markdown`（或简写 `md`）、`json` |
| `--work-dir` | `<目录路径>` | 当前目录 `.` | 存放临时测试文件的目录，跑完会自动清理。测哪块盘就指到哪块盘的挂载目录（请避开 `/tmp`） |
| `--countries` | `<国家代码,...>` | 默认精选节点 | 限制 `network.iperf3` 只测指定国家/地区的节点（如 `JP,SG,US`），填 `all` 测全部节点 |
| `--trace-groups` | `<分组,...>` | 缓存节点 + 默认线路 | 选择 `network.trace` 要追踪哪些方向的路由目标 |
| `--media-groups` | `<区域,...>` | `all`（全部区域） | 选择 `ip.media` 要测试哪些地区的流媒体与在线服务 |
| `--no-dnsbl` | 无需带值 | 关闭 | 在 `ip.quality` 中跳过 25 个邮件/垃圾信息黑名单（DNSBL）查询 |
| `--no-mail` | 无需带值 | 关闭 | 在 `ip.quality` 中跳过 25 端口各大邮箱服务连通性测试 |
| `--no-bgp-upload` | 无需带值 | 关闭 | 在 `text` 模式下跳过将 `network.bgp` 拓扑 SVG 上传至 `imageupload.app` 图床 |
| `--download-connections` | `<1..=256>` | `32` | 下载外部测试工具时的最大并发连接数，网络不稳定时可适当调小 |
| `--show-ip` | 无需带值 | 关闭（默认打码） | 在最终报告里显示完整的本机公网 IPv4/IPv6 地址和反向解析域名 |
| `-v, --verbose` | 无需带值 | 关闭 | 打印更详细的下载过程与子工具运行日志（方便排查卡住或报错原因） |
| `--list` | 无需带值 | — | 列出所有支持的测试项目、适用条件与工具版本后直接退出（不需要 `root`） |
| `-h, --help` | 无需带值 | — | 查看命令行帮助提示 |
| `-V, --version` | 无需带值 | — | 查看当前程序版本号 |

### 各参数支持的可选值

| 参数 | 可填写的值 | 含义说明 |
|---|---|---|
| `--only` / `--skip` *（按大类）* | `cpu` | 包含全部 2 项 CPU 测试：`cpu.sysbench`、`cpu.geekbench` |
| | `memory` | 包含全部 4 项内存测试：`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa` |
| | `disk` | 包含全部 2 项硬盘测试：`disk.health`、`disk.fio` |
| | `network` | 包含全部 3 项网络测试：`network.iperf3`、`network.trace`、`network.bgp` |
| | `ip` | 包含全部 2 项 IP 测试：`ip.quality`、`ip.media` |
| `--only` / `--skip` *（按单项）* | `disk.health`、`cpu.sysbench`、`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa`、`disk.fio`、`network.iperf3`、`network.trace`、`network.bgp`、`ip.quality`、`ip.media`、`cpu.geekbench` | 精确指定某一个测试项目（无论怎么填，都会按固定的合理顺序依次执行） |
| `-f, --format` | `text` | 适合直接在 SSH 终端阅读的对齐纯文本表格 |
| | `markdown` *（或 `md`）* | GitHub 风格 Markdown 文档，复制即可直接发到支持 Markdown 的社区或工单 |
| | `json` | 结构化 JSON 数据，适合自动化汇总、监控脚本或二次开发读取 |
| `--trace-groups` | `services` | 只测离当前机器最近的 YouTube 和 Netflix 视频缓存节点路由 |
| | `china` | 中国大陆三网线路（北上广电信 163 / CN2、联通 169 / 9929、移动 CMNET / CMIN2） |
| | `international` | 海外主流国际骨干网、Telegram 数据中心以及 AWS / GCP 云节点 |
| | `all` | 以上全部目标一起测（共 73 个目标） |
| `--media-groups` | `global` | 全球通用平台（Netflix、YouTube Premium、Disney+、Prime Video、Spotify、TikTok 等） |
| | `asia` | 亚洲地区平台（Bilibili 港澳台、巴哈姆特动画疯、Abema、U-NEXT、TVer、Viu 等） |
| | `europe` | 欧洲地区平台（BBC iPlayer、ITVX、Channel 4、Canal+、ZDF、ARTE、RAI Play 等） |
| | `americas` | 美洲地区平台（Hulu、Max、Peacock、Paramount+、Starz、Crave、Globoplay 等） |
| | `oceania` | 大洋洲地区平台（澳大利亚与新西兰的 Stan、9Now、7plus、ABC iview、TVNZ+ 等） |
| | `all` | 一次测完五大区域共 68 个平台（默认） |
| `--countries` | 两位国家/地区代码 *（如 `US`、`JP`、`DE`、`SG`、`GB`）* | 只连接对应国家/地区的公共 `iperf3` 测速节点（大小写均可） |
| | `all` | 连接节点列表里的所有可用公共 `iperf3` 服务器 |

---

## 每个检测项都在测什么？

在正式跑分前，程序会先快速读取 **系统概览（`[System]`）**：帮你看清这台机器是独立服务器、KVM 虚拟机还是容器，CPU 具体型号与缓存大小、真实内存与 Swap 容量、系统内核与 TCP 拥塞算法（如 BBR）、硬盘分区、网卡速率（含 PCIe 插槽是否降速提示）以及 IPv4/IPv6 联网状态；如果是独立物理服务器，还会列出每根内存条插槽信息、ECC 内存报错统计、IPMI 带外管理与硬件 RAID 卡型号（详见 [`docs/system.md`](docs/system.md)）。

随后按顺序运行以下 13 个测试项目：

| 分类 | 项目名称 (`ID`) | 适用环境 | 这项测试能帮你看出什么？ | 原理详解 |
|---|---|---|---|---|
| `disk` | `disk.health`<br>硬盘健康体检 | 独立物理服务器 | 读取硬盘 SMART 信息（支持穿透常见硬件 RAID 卡），查看硬盘通电时长、温度、SSD 寿命磨损、累计写入量以及有没有坏道，快速识别二手老盘或故障隐患 | [`docs/disk.md`](docs/disk.md) |
| `cpu` | `cpu.sysbench`<br>CPU 基础算力 | 所有服务器 | 分别用单核与全部核心进行 20 秒素数运算，快速检验 CPU 单核响应速度以及多核性能有没有被宿主机限制（超售限速） | [`docs/cpu.md`](docs/cpu.md) |
| `memory` | `memory.sysbench`<br>内存连续读写 | 所有服务器 | 测量单线程与多线程连续读写内存的速度，直观反映内存通道配置是否正常 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream`<br>内存持续带宽 | 所有服务器 | 采用业界标准的 STREAM 测试法，测量大块数据搬运与计算时的真实内存极限带宽 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency`<br>缓存与内存延迟 | 所有服务器 | 逐级测量 CPU 一/二/三级缓存和主内存的随机访问延迟（纳秒级），能看出虚拟化开销或跨插槽延迟问题 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa`<br>多路内存互通 | 双路/多 NUMA 节点机器 | 在多颗 CPU 或多 NUMA 节点的机器上，测试访问“本地内存”与“另一颗 CPU 旁边的内存”之间的速度和延迟差距 | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio`<br>硬盘随机读写 | 所有服务器 | 采用与 YABS 相同的测试标准（50% 读 + 50% 写），测试 `4K`、`64K`、`512K`、`1M` 四种块大小下的真实读写速度与 IOPS，建站跑数据库重点看 `4K`，大文件吞吐看 `512K`/`1M` | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3`<br>全球带宽测速 | 所有联网机器 | 默认连接美西、美东、东京、法兰克福 5 个公共节点，用多线程同时测 IPv4 和 IPv6 的上传、下载真实带宽与延迟 | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace`<br>去程路由追踪 | 支持原始套接字的机器 | 追踪本机到 YouTube/Netflix 节点、国内三网（163/CN2/9929/CMIN2）及国际骨干网的每一跳路径，看清线路是直连还是绕路 | [`docs/network.md`](docs/network.md) |
| `network` | `network.bgp`<br>BGP 上游拓扑图 | 有公网 IP 的机器 | 查询本机 IP 所在网段的 BGP 宣告前缀、RPKI 签名状态与直连上游占比，在终端模式生成 `imageupload.app` 高清 SVG 拓扑图链接（不含本机 IP；Markdown 模式生成 Mermaid 矢量图，JSON 模式内置独立 SVG 源码） | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality`<br>IP 质量与纯净度 | 有公网 IP 的机器 | 汇总 9 家风控数据库结果，判断 IP 是**原生 IP** 还是**广播 IP**、属于机房还是家宽、欺诈风险高低、历史暴露端口、是否在 25 个黑名单里，以及 25 邮件端口是否开放 | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media`<br>流媒体解锁检测 | 所有联网机器 | 实测全球五大区域共 68 个流媒体与在线平台（Netflix、Disney+、YouTube、B站港澳台、巴哈姆特、Abema 等）能否正常解锁及识别到的所在地区 | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.geekbench`<br>Geekbench 7 跑分 | `glibc >= 2.35` 且有 IPv4 | 运行官方 Geekbench 7.1.0 综合基准测试，给出大家最熟悉的单核、多核跑分和官方网页报告链接 | [`docs/cpu.md`](docs/cpu.md) |

---

## 从源码自行编译

普通用户直接前往 [Releases](https://github.com/Lowendaff/lowendaff_bench/releases) 下载现成的二进制即可。如果你想自己动手编译静态版本，只需安装好 Rust 工具链和 `musl-tools`（提供 `musl-gcc`）：

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

编译完成后，可执行文件位于 `target/x86_64-unknown-linux-musl/release/lowendaff-bench`。参与项目开发与提交代码的说明请参阅 [`CONTRIBUTING.md`](CONTRIBUTING.md)。

---

## 致谢

`lowendaff-bench` 的设计思路、测试标准与底层能力离不开以下优秀的开源项目、跑分工具与公开数据平台，向所有原作者与维护者致以诚挚感谢：

- **[YABS (Yet Another Bench Script)](https://github.com/masonr/yet-another-bench-script)**：`disk.fio` 四档块大小混合随机读写参数与速率展示格式，以及 `network.iperf3` 多线程上下行测速方案的重要参考。
- **[IPQuality (`xykt/IPQuality`)](https://github.com/xykt/IPQuality)**：`ip.quality` 多数据源交叉比对、原生/广播 IP 识别、风险标签汇总、DNSBL 黑名单检查与 25 端口邮局测试的重要参考。
- **[RegionRestrictionCheck (`lmc999/RegionRestrictionCheck`)](https://github.com/lmc999/RegionRestrictionCheck)** 与 **[MediaUnlockTest (`HsukqiLee/MediaUnlockTest`)](https://github.com/HsukqiLee/MediaUnlockTest)**：`ip.media` 各地区流媒体解锁检测的重要参考。
- **[NextTrace (`nxtrace/NTrace-core`)](https://github.com/nxtrace/NTrace-core)**：为 `network.trace` 提供精准轻量的路由追踪引擎与节点归属地信息。
- **[peer.as (`Archeb/peer.as`)](https://github.com/Archeb/peer.as)**：`network.bgp` 自治域路径分层无交叉布局算法、Tier-1 截断、Prepend 折叠与 SVG 拓扑图绘制的重要参考。
- **上游测试程序与公开数据源**：[`sysbench`](https://github.com/akopytov/sysbench)（Alexey Kopytov）、[`STREAM`](https://www.cs.virginia.edu/stream/)（John D. McCalpin）、[`multichase`](https://github.com/google/multichase)（Google）、[`smartmontools`](https://www.smartmontools.org/)（`smartctl`）、[`fio`](https://github.com/axboe/fio)（Jens Axboe）、[`iperf3`](https://github.com/esnet/iperf)（ESnet）、[`Geekbench`](https://www.geekbench.com/)（Primate Labs）、[`RIPEstat`](https://stat.ripe.net/)（RIPE NCC）、[`iperf3serverlist.net`](https://iperf3serverlist.net/) 以及 [`PeeringDB`](https://www.peeringdb.com/)。
