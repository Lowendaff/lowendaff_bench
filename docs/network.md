# 网络吞吐、路由追踪与 BGP 拓扑（`network.iperf3`、`network.trace`、`network.bgp`）

[返回文档目录](README.md) | [简体中文首页](../README.zh-CN.md)

网络章节包含三项测试：**`network.iperf3`** 测量本机与全球公网节点之间的双向多线程 TCP 吞吐带宽及往返延迟；**`network.trace`** 执行逐跳 ICMP 路由追踪，识别去程骨干网线路、运营商 AS（如 163 / CN2 / 9929 / CMIN2）、流媒体边缘缓存服务器与互联网交换中心（IX）；**`network.bgp`** 通过全球全表采集点查询本机公网前缀的控制面 BGP `AS_PATH` 传播路径、直连上游占比与 RPKI 路由签名状态，并直接绘制自治域拓扑图。

---

## 1. 公网带宽测速：`network.iperf3`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.iperf3`（所属分组：`network`） |
| 适用平台 | 全平台（需具备 IPv4 或 IPv6 公网连接） |
| 依赖工具 | 静态构建 `iperf3 3.22`（裁剪了 OpenSSL 认证模块与 SCTP，体积精简且无攻击面） |
| 超时上限 | `45 分钟` |
| 快捷参数 | `--countries <ISO 代码列表或 all>`、`--skip network.iperf3` |

### 测试逻辑
1. **动态节点列表与自动巡检机制**：
   - 每次运行时从仓库主分支实时拉取经过校验的服务器列表（`servers/iperf3.json`，限制 `<= 64 KiB` 且最多 100 台节点，严格校验主机名、端口区间与控制字符）。
   - 仓库每日两次通过自动化流水线拉取 `iperf3serverlist.net` 实时状态，从人工审核过的候选池（`servers/iperf3-candidates.json`）中为每个地区挑选首个在线且支持反向下载（`-R`）的节点并自动提交更新，无需发版即可剔除宕机节点。
   - 默认测试列表标记为 `default = true` 的 5 个核心地区（洛杉矶、纽约、弗吉尼亚、东京、法兰克福）；可通过 `--countries JP,DE` 或 `--countries all` 按国家/地区筛选。
2. **双向双栈测量流程（对齐 YABS）**：
   - 先对所有选中节点跑完 **IPv4**，若本机支持 IPv6 再跑 **IPv6**。
   - 对每个节点先测 **上传（Upload）**：`iperf3 -4|-6 -c <host> -p <port> -P 8 -t 10 -J`（默认 8 条并发流、持续 10 秒），取服务端实际接收速率（`receiver`）作为上传带宽，并提取上传期间内核 TCP 协议栈记录的最小 RTT（`min_rtt`）作为节点延迟；停顿 1 秒后再加 `-R` 测 **下载（Download）**，取本机接收速率。
3. **随机起始端口与跨端口重试**：
   - 若节点开放了端口段（如 `5201-5210`），插件使用系统随机种子随机选择起始端口以分散并发客户端；遇到服务端正忙（Busy）或瞬时连接失败时，等待 2 秒后换用同节点的下一个端口重试（最多 **3 次**）。若上传全部失败，则直接跳过该节点的下载测试以节省时间。
4. **最小权限与流量提醒**：
   - 因对端均为第三方公共服务器，`iperf3` 强制降权至非特权用户 **`65534`（`nobody`）** 运行，且 `-v` 模式不回显含本机 IP 的 `iperf3` 原始输出。
   - **流量估算**：在 `1 Gbit/s` 带宽下，单个双栈节点双向约消耗 `5 GB` 流量，默认 5 节点约消耗 `25 GB`（`10 Gbit/s` 机器可达 `250 GB`）。按流量计费的机器建议用 `--countries` 缩减测速国家/地区，或用 `--skip network.iperf3` 跳过。

### 常用命令行示例
```sh
# 仅测试日本与德国的公共 iperf3 节点
sudo ./lowendaff-bench --only network.iperf3 --countries JP,DE

# 测试列表中的全部可用国家节点
sudo ./lowendaff-bench --only network.iperf3 --countries all
```

---

## 2. 去程路由追踪：`network.trace`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.trace`（所属分组：`network`） |
| 适用平台 | 全平台（需具备公网连接且系统允许授予 `CAP_NET_RAW` 文件能力） |
| 依赖工具 | 静态构建 `NextTrace 1.7.6`（Tiny 构建，内置根证书） |
| 超时上限 | `60 分钟` |

### 测试逻辑
1. **最小权限原始套接字（`cap_net_raw+ep` + `nobody`）**：
   - 发送与接收原始 ICMP 报文需要 `CAP_NET_RAW`。主程序**绝不以 `root` 身份运行第三方追踪工具**，而是为临时目录中的 `nexttrace` 二进制单独写入文件能力 `cap_net_raw+ep`，然后切换为非特权用户 `65534`（`nobody`）并以 `/` 为工作目录执行。
   - 若宿主机开启了 `no_new_privs`、容器裁剪了 `CAP_NET_RAW` 能力边界集，或运行目录位于 `nosuid` 挂载分区上，插件会在启动前精准检测并以清晰原因安全跳过（提示可通过 `--work-dir` 换至非 `nosuid` 分区）。
2. **三类追踪目标**：
   - **流媒体边缘缓存节点**：分别在 IPv4 与 IPv6 下直连请求 Google `redirector.googlevideo.com/report_mapping?di=no`（还原出 `r1---sn-<code>.googlevideo.com` 边缘域名）和 Netflix `fast.com` API，自动定位离当前主机最近的 YouTube 与 Netflix 缓存服务器（且不保存或展示接口响应中回显的本机 IP）。
   - **中国大陆三网与精品网目标**：从 `servers/traceroute.json` 拉取北京、上海、广州等地的电信（163 / CN2 GT / CN2 GIA `AS4809`）、联通（169 / CUII `AS9929`）、移动（CMNET / CMIN2 `AS58807`）标准测试目标。
   - **国际骨干与云服务目标**：涵盖 Telegram 数据中心、AWS、GCP 以及十大国际 Tier-1 骨干网运营商，共计 71 个预设目标（默认运行精简代表集或按配置展开）。
3. **逐跳解析与 Netflix IX 交换中心标注**：
   - 执行参数：`nexttrace --traceroute -j -M -C -g en -q 3 -m 30 --timeout 1000 -4|-6 <target>`（每跳 3 探针、最多 30 跳、单探针超时 1 秒、始终禁用 `-M` 轨迹地图上传，单目标限时 60 秒）。
   - 结合仓库自动同步 PeeringDB 生成的 `servers/netflix-ix.json`（`net_id=457`），当路由经过 Netflix 在全球各大互联网交换中心（如 Equinix、DE-CIX、HKIX、BBIX 等）的对等互联端口 IP 时，自动在跳点网络名旁标注所属 IX 名称。
   - **本机跳点脱敏**：未传入 `--show-ip` 时，路由路径中凡是匹配本机公网 IPv4 / IPv6 的跳点均自动打码（中间运营商路由器 IP 原样保留）。

### `--trace-groups` 命令行枚举与示例
- `services`：仅追踪距离最近的 YouTube 与 Netflix 边缘缓存服务器
- `china`：中国大陆三网及精品网目标（163 / CN2 / 169 / 9929 / CMNET / CMIN2）
- `international`：国际 Tier-1 骨干网、Telegram 数据中心、AWS 与 GCP 目标
- `all`：同时追踪流媒体缓存节点与列表内的全部国内外目标

```sh
# 仅追踪流媒体边缘缓存节点与中国大陆三网目标
sudo ./lowendaff-bench --only network.trace --trace-groups services,china
```

---

## 3. BGP 自治域路径拓扑：`network.bgp`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.bgp`（所属分组：`network`） |
| 适用平台 | 全平台（需具备公网 IPv4 或 IPv6 地址） |
| 数据来源 | RIPE NCC 公开免认证接口 RIPEstat Data API（`bgp-state`、`rpki-validation`、`as-overview`） |
| 超时上限 | `2 分钟` |

### 测试与绘图逻辑
1. **全球全表采集点路由聚合（`bgp-state`）**：
   - 针对本机公网 IPv4 与 IPv6 地址直连请求 RIPEstat `bgp-state`，汇总全球数十个 RIPE RIS（`rrc00..rrc26`）全表采集点对端观测到的真实路由表。
   - 若同一地址同时被大段超网与更具体子网覆盖，自动选取**掩码最长（最具体）**的宣告前缀统计其 Origin ASN（支持识别多源宣告 MOAS）、直连上游占比（紧邻 Origin 前一跳的自治域）与全部去重 `AS_PATH`。
2. **RPKI 路由起源签名校验与 ASN 简称补齐**：
   - 调用 `rpki-validation` 验证 `(Origin ASN, 宣告前缀)` 是否具备合法的 ROA 签名（区分 `valid`、`invalid`、`not found`）；
   - 内置 23 个全球 Tier-1 骨干网（如 `3356` Lumen、`1299` Arelion、`174` Cogent、`2914` NTT 等）与常见国内外运营商简称表，并对图中其余未知 ASN 并发查询 `as-overview` 提取机构简称。
3. **对齐 `peer.as` 的两阶段无交叉布局算法**：
   - **Tier-1 截断与 Prepend 折叠**：优先筛选经过 Tier-1 骨干网的路径并在外侧首跳 Tier-1 处截断收集器边缘节点；连续重复的同一 ASN（AS-PATH Prepend）折叠为单框，跨列连线以虚线及 `×N` 标注；
   - **主干居中树序 DFS + 稀疏列优先整行量化**：以第 `0` 列（`origin`）为起点向右展开上游树，将子树规模最大的主干排在纵向中心、细碎分支分列上下两侧，再按列节点密度由稀到密分配整数行号并预留跨列通道，确保连线清晰无杂乱交叉。
4. **三种报告格式的拓扑图输出**：
   - **`text` 终端模式**：在二维 Unicode 字符画布上绘制圆角节点框（`╭─╮│╰─╯`）与正交连线（主路径双线 `═▸`、普通路径细线 `─▸`、Prepend 跨列 `┄×N▸`）；若终端窗口过窄装不下多列方框，自动降级为缩进树状分支图防止折行错位；
   - **`markdown` 模式**：输出原生 ```` ```mermaid ````（`flowchart LR`）代码块，在 GitHub、GitLab、Obsidian 等平台无需外部图片文件即可直接渲染为矢量图；
   - **`json` 模式**：在 `ipv4.svg` / `ipv6.svg` 字段中直接内嵌完整自包含的三次贝塞尔曲线 `<svg>` 矢量图源码（内联配色与样式，可直接保存为 `.svg` 查看）。
   - **默认隐私脱敏**：未传入 `--show-ip` 时，宣告前缀自动打码保留掩码长度（如 `203.0.*.*/24`、`2001:db8:*/48`），拓扑图内部不携带原始 IP。

```sh
# 单独查看本机 BGP 路由拓扑图（终端直接画图）
sudo ./lowendaff-bench --only network.bgp

# 导出包含独立 SVG 源码的 JSON 文件
sudo ./lowendaff-bench --only network.bgp -f json -o bgp.json
```
