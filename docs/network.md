# 网络吞吐、中国三网、Cloudflare / Apple CDN 测速、路由追踪与 BGP 拓扑
# （`network.iperf3`、`network.taier`、`network.cloudflare`、`network.inetspeed`、`network.trace`、`network.bgp`）

[返回文档目录](README.md) | [简体中文首页](../README.zh-CN.md)

网络章节包含六项测试：**`network.iperf3`** 测量本机与全球公网节点之间的双向多线程 TCP 吞吐带宽及往返延迟；**`network.taier`** 基于中国信通院“全球网测（泰尔测速）”协议，使用纯 Rust 原生套接字测量中国大陆三网及教育网/广电节点的双向多线程上下行吞吐带宽；**`network.cloudflare`** 使用纯 Rust 原生客户端双栈测量 `speed.cloudflare.com` 稳态上下行吞吐、空载/负载延迟、Waveform Bufferbloat 与 CV 稳定性评级以及 RFC 5389 STUN UDP 丢包与 MOS 语音质量；**`network.inetspeed`** 使用纯 Rust 原生实现双 DoH 边缘节点优选绑定与 Apple CDN（`mensura.cdn-apple.com`）单线程/多线程上下行吞吐测试；**`network.trace`** 执行逐跳 ICMP 路由追踪，识别去程骨干网线路、运营商 AS、流媒体边缘缓存服务器与互联网交换中心（IX）；**`network.bgp`** 通过全球全表采集点查询本机公网前缀的控制面 BGP `AS_PATH` 传播路径、直连上游占比与 RPKI 路由签名状态，并直接绘制自治域拓扑图。

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

## 2. 中国三网测速（泰尔）：`network.taier`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.taier`（所属分组：`network`） |
| 适用平台 | 全平台（需具备 IPv4 公网连接） |
| 依赖工具 | **零外部二进制依赖**（100% 纯 Rust 原生异步直连 IPv4 `TcpSocket` 实现，绝不经过下载代理） |
| 超时上限 | `30 分钟` |
| 快捷参数 | `--taier-cities <城市/省份列表或 all>`、`--taier-operators <运营商列表或 all>` |

### 测试逻辑
1. **全国三网节点库与自动巡检同步（`servers/taier.json`）**：
   - 节点列表由 `scripts/taier-servers.sh` 将上游中国信通院“全球网测（`com.cnspeedtest.globalspeed`）”全国服务器表归一化转换为纯 ASCII JSON（`<= 64 KiB`），涵盖全国 30+ 省级行政区、250+ 地级市/自治州及中国电信（`telecom`）、中国联通（`unicom`）、中国移动（`mobile`）、教育网（`cernet`）、中国广电（`cbn`）等 600+ 个测速节点，并通过 `.github/workflows/data.yml` 每日自动同步更新。
   - **默认范围与按需勾选**：默认测试 **北京、上海、广州 × 电信、联通、移动** 共 9 个核心槽位；既支持在命令行通过 `--taier-cities Beijing,Jiangsu`、`--taier-operators telecom,unicom,cernet` 按城市/省份/运营商筛选，也支持在交互式 TUI 中直接勾选运营商、编辑城市、调节单向时长与并发流数。
2. **原生 `dovalid` 队列鉴权、繁忙自动回退与租约释放**：
   - 内置纯 Rust RFC 1321 MD5 摘要、HTTP `chunked` 分块响应解帧与 Luhn 校验算法生成 15 位合规合成 Android IMEI，向节点发起 `GET /speed/dovalid` 鉴权握手：
     - 返回 `1-<uuid>`：获得测速排队租约，进入测速；
     - 返回 `-1`（服务端队列繁忙）：自动在同一 `(城市, 运营商)` 槽位的候选节点池中无缝回退切换至下一台节点；
     - 返回 `0`（老版本节点签名策略差异）：自动回退空 `token` 重试一次；
     - 无论测速成功、报错还是 `Ctrl-C` 中断，结束前均保证发送 `POST /speed/dovalid?key=<uuid>` 立即归还服务端并发槽位。
3. **多线程 HTTP/1.1 纯上下行吞吐测量（完全绕过代理，不做重复 RTT 探测）**：
   - 所有测速套接字均直接绑定 `0.0.0.0:0`（`SO_RCVBUF`/`SO_SNDBUF = 512 KiB`、`TCP_NODELAY`），不受任何环境变量或 `--socks5-proxy` / `--github-proxy` 影响。
   - 默认开启 **8 条并发流、每方向持续 4 秒**：先并发请求 `/speed/File(1G).dl` 测量持续下载吞吐带宽，再并发通过 `multipart/form-data` 流式写入 `/speed/doAnalsLoad.do` 测量持续上传吞吐带宽，专注于高并发带宽吞吐测试，不再额外执行 RTT 延迟探测。

### 常用命令行示例
```sh
# 按默认配置测试北上广 × 电信/联通/移动
sudo ./lowendaff-bench --only network.taier

# 指定测试江苏全省与深圳的电信、联通、移动及教育网节点
sudo ./lowendaff-bench --only network.taier --taier-cities Jiangsu,guangdong-shenzhen --taier-operators telecom,unicom,mobile,cernet
```

---

## 3. Cloudflare 双栈测速与连接质量：`network.cloudflare`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.cloudflare`（所属分组：`network`） |
| 适用平台 | 全平台（需具备 IPv4 或 IPv6 公网连接） |
| 依赖工具 | **零外部二进制依赖**（100% 纯 Rust 原生 HTTPS + RFC 5389 STUN UDP 实现，直连绕过一切代理） |
| 超时上限 | `10 分钟` |
| 快捷参数 | `--no-cloudflare-udp`（跳过 STUN UDP 丢包探测） |

### 测试逻辑
1. **双栈独立测速与边缘机房识别**：
   - 自动针对本机可达的每一个协议栈（IPv4 与 IPv6）分别绑定本地地址直连 `speed.cloudflare.com`，依次读取 `/meta`、回退 `/cdn-cgi/trace` 与 `cf-meta-*` / `cf-ray` 响应头，并结合 `/locations` 映射出具体接入的三字码边缘数据中心（如 `LAX - Los Angeles, United States`、`NRT - Tokyo, Japan`），直观对比 IPv4 与 IPv6 是否绕路至不同机房。
2. **稳态慢启动剔除与 429 自适应退避**：
   - 先采样 2 秒空载 HTTP RTT（计算 `min`、`median`、`p25/p75/p95/p99` 与样本标准差抖动）；
   - 随后默认以 **6 个并发工作协程、每方向持续 10 秒** 分别执行下载（`GET /__down`；遇到 HTTP 429 自动将单次请求字节数折半退避）与流式分块上传（`POST /__up`，64 KiB 零字节流，请求异常自动回滚已计数字节），每 200 ms 采样一次瞬时吞吐并自动剔除前 `max(20% 时长, 1 秒)` 的 TCP 慢启动爬坡期，避免起速阶段拉低真实稳态带宽；同时在后台并发探测负载下延迟（`during=download` / `during=upload`）。
3. **RFC 5389 STUN UDP 丢包乱序探测与连接质量评级**：
   - 向 `stun.cloudflare.com:3478` 发送默认 50 个原生 STUN Binding Request 报文（支持延迟到达包按事务 ID 归位、去重与乱序统计，连续 10 包无响应自动判定 UDP 被防火墙拦截并以非致命提示记录），按简化 ITU-T G.107 E-model 计算语音 MOS 分（`1.0..=5.0`）；
   - 基于空载与双向负载中位数延迟增量计算 Waveform 标准 **Bufferbloat 缓冲膨胀等级（`A+` 至 `F`）**，并基于稳态吞吐变异系数（CV%）给出 **吞吐稳定性等级（`A` 至 `F`）**。
   - 默认对 Cloudflare 返回的客户端公网 IP 自动脱敏（传入 `--show-ip` 时显示完整地址）。

---

## 4. Apple CDN 优选测速（iNetSpeed）：`network.inetspeed`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.inetspeed`（所属分组：`network`） |
| 适用平台 | 全平台（需具备 IPv4 或 IPv6 公网连接） |
| 依赖工具 | **零外部二进制依赖**（100% 纯 Rust 原生实现 `mensura.cdn-apple.com` / `networkQuality` 协议，直连绕过一切代理） |
| 超时上限 | `15 分钟` |
| 快捷参数 | `--inetspeed-threads <1..=64>`、`--inetspeed-endpoint <固定节点 IP>` |

### 测试逻辑
1. **双 DoH 并发发现与保留 TLS SNI 的最低延迟 IP 固定**：
   - 同时向 Cloudflare DoH（`cloudflare-dns.com/dns-query`）与阿里 DoH（`dns.alidns.com/resolve`）并发查询本机可达协议栈下的 `mensura.cdn-apple.com` 候选边缘 IP（双 DoH 不可达时自动回退系统 DNS）；
   - 逐一对候选边缘 IP 建立保留 `mensura.cdn-apple.com` TLS SNI 的直连探测（`GET /api/v1/gm/small`），按实测 HTTP RTT 升序排列并自动锁定延迟最低的健康边缘节点用于后续全部测试（亦可通过 `--inetspeed-endpoint` 手动指定节点 IP）。
2. **空载延迟与单线程 / 多线程四轮吞吐对比**：
   - 先采集默认 20 次空载延迟样本（统计 `min`、`avg`、`median`、`max` 与 `jitter`）；
   - 随后依次运行 **下载单线程（1T）**、**下载多线程（默认 4T）**、**上传单线程（1T）** 与 **上传多线程（默认 4T）** 共四轮测试（上传使用流式 `PUT /api/v1/gm/slurp` 并携带 `Upload-Draft-Interop-Version: 6` 与 `Upload-Complete: ?1` 协议头，非 2xx/3xx 响应自动回滚字节计数），每轮均在后台同步采集负载延迟与抖动，清晰展现单流限速与多流聚合能力。

---

## 5. 去程路由追踪：`network.trace`

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

## 6. BGP 自治域路径拓扑：`network.bgp`

| 属性 | 说明 |
|---|---|
| 插件 ID | `network.bgp`（所属分组：`network`） |
| 适用平台 | 全平台（需具备公网 IPv4 或 IPv6 地址） |
| 数据来源 | RIPE NCC 公开免认证接口 RIPEstat Data API（`bgp-state`、`rpki-validation`、`as-overview`） |
| 超时上限 | `2 分钟` |
| 快捷参数 | `--no-bgp-upload`（跳过 `text` 模式图床上传） |

### 测试与绘图逻辑
1. **全球全表采集点路由聚合（`bgp-state`）**：
   - 针对本机公网 IPv4 与 IPv6 地址直连请求 RIPEstat `bgp-state`，汇总全球数十个 RIPE RIS（`rrc00..rrc26`）全表采集点对端观测到的真实路由表。
   - 若同一地址同时被大段超网与更具体子网覆盖，自动选取**掩码最长（最具体）**的宣告前缀统计其 Origin ASN（支持识别多源宣告 MOAS）、直连上游占比（紧邻 Origin 前一跳的自治域）与全部去重 `AS_PATH`。
2. **RPKI 路由起源签名校验与 ASN 简称补齐**：
   - 调用 `rpki-validation` 验证 `(Origin ASN, 宣告前缀)` 是否具备合法的 ROA 签名（区分 `valid`、`invalid`、`not found`）；
   - 内置 23 个全球 Tier-1 骨干网（如 `3356` Lumen、`1299` Arelion、`174` Cogent、`2914` NTT 等）与常见国内外运营商简称表，并对图中其余未知 ASN 并发查询 `as-overview` 提取机构简称。
3. **Sugiyama 紧凑跳数分层与重心降交叉布局**：
   - **起源侧首跳 Tier-1 截断与去重跳数分列**：优先筛选经过 Tier-1 骨干网的路径，并在自 Origin 向外到达的**首个** Tier-1 骨干网处截断（剔除采集点侧冗余的 Tier-1 互联跳数，同时保证高扇出上游如 `AS6939` 不会挤占多层级备份上游路径）；按去重后的真实 AS 跳数（`origin, 1, 2, ...`）划分纵向列，连续重复的同一 ASN（AS-PATH Prepend）折叠为单框并在连线上以虚线与 `×N` 标注；
   - **主干居中 DFS + 重心法（Barycenter）降交叉 + 邻接对齐行分配**：先将子树规模最大的主干排在纵向中心，再经多轮正反向重心扫掠最小化多上游网状交叉，最后以最密列为锚点向两侧做邻接吸附单调行号投影，消除同列重叠与悬空绕线。
4. **三种报告格式的拓扑图输出**：
   - **`text` 终端模式**：输出 IPv4 / IPv6 双栈路由摘要表与主要观测 `AS_PATH` 列表，并将生成的匿名 SVG 矢量拓扑图以 `unlisted`（不公开收录）方式上传至 `imageupload.app`，直接在终端展示不折行的可点击图床链接（如 `https://imageupload.app/i/<key>`）；若图床上传超时或限流，仅追加一行 `Note` 说明而不会影响测试成功状态，也可通过 `--no-bgp-upload`（或交互式 TUI 中的 `Upload topology image` 开关）关闭上传；
   - **`markdown` 模式**：输出原生 ```` ```mermaid ````（`flowchart LR`）代码块（不触发外部图床上传），在 GitHub、GitLab、Obsidian 等平台可直接渲染为矢量图；
   - **`json` 模式**：在 `ipv4.svg` / `ipv6.svg` 字段中直接内嵌完整自包含的三次贝塞尔曲线 `<svg>` 矢量图源码（内联配色与样式，不触发外部图床上传，可直接保存为 `.svg` 查看）。
   - **默认隐私脱敏**：未传入 `--show-ip` 时，宣告前缀自动打码保留掩码长度（如 `203.0.*.*/24`、`2001:db8:*/48`）；上传与内嵌的 SVG 拓扑图内部仅包含自治域号（ASN）、运营商简称、节点权重与 Prepend 倍数，**绝不包含本机公网 IP 或前缀**。

```sh
# 单独查询本机 BGP 路由并生成可点击的 SVG 拓扑图链接
sudo ./lowendaff-bench --only network.bgp

# 仅在终端查看 BGP 摘要与 AS_PATH 表，不上传拓扑图到图床
sudo ./lowendaff-bench --only network.bgp --no-bgp-upload

# 导出包含独立 SVG 源码的 JSON 文件（不会向外部图床发起上传）
sudo ./lowendaff-bench --only network.bgp -f json -o bgp.json
```
