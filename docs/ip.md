# IP 信誉度与流媒体解锁（`ip.quality`、`ip.media`）

[返回文档目录](README.md) | [简体中文首页](../README.zh-CN.md)

`[IP]` 章节完全由 Rust 原生双栈直连探针驱动（无需下载任何外部工具，且**强制绕过 `HTTPS_PROXY` / `ALL_PROXY` 等代理环境变量**），分别从安全声誉与区域内容可用性两个维度评估主机的公网 IPv4 与 IPv6 地址：**`ip.quality`** 检测 IP 纯净度、原生/广播属性、黑名单与 25 端口发信能力；**`ip.media`** 检测全球五大洲 68 个流媒体与在线平台的解锁状态。

---

## 1. IP 信誉度与网络身份：`ip.quality`

| 属性 | 说明 |
|---|---|
| 插件 ID | `ip.quality`（所属分组：`ip`） |
| 适用平台 | 全平台（需具备至少一个已探明公网出口地址的 IPv4 或 IPv6 协议栈） |
| 依赖工具 | 无（纯内置 HTTPS、UDP/TCP DNS 与 SMTP Socket 探针） |
| 超时上限 | `3 分钟` |

### 四大检测维度

1. **9 源网络身份聚合与“原生 vs 广播 IP”判定**：
   - 并发查询 9 个免 API Key 权威数据源：`Scamalytics`、`ProxyCheck`、`ipapi.is`、`GreyNoise`、`Shodan`（InternetDB）、`RIPEstat`、`IPinfo`、`DB-IP` 与 `IPWHOIS`。
   - **RIR 注册地 vs GeoIP 共识地**：从官方 RIR WHOIS 数据源（`RIPEstat` 与 `ipapi.is`）提取 IP 段注册国家码，同时统计 5 个 GeoIP 库返回的实际运营国家投票分布（如 `US (5/5)` 或 `HK (4/5), US (1/5)`）：
     - 若 RIR 注册国与 GeoIP 多数派共识国一致，判定为 **原生 IP（`native`）**；
     - 若二者不一致（例如 ARIN 美籍注册段在香港机房宣告广播使用），判定为 **广播 IP（`broadcast`）** 并同时列出注册国与落地国。
   - **使用类型分类**：综合多源分类归一为住宅宽带（`residential`）、机房托管（`hosting`）、企业专线（`business`）、移动蜂窝（`mobile`）、教育科研（`education`）、政府机构（`government`）或 CDN（`cdn`）。

2. **多源欺诈评分、威胁情报与 7 类风险因子投票**：
   - **欺诈与安全评分表**：并列展示 IPv4 / IPv6 在 `Scamalytics`（`0-100` 欺诈分）、`ProxyCheck`（`0-100` 风险分）、`ipapi.is`（滥用比率与风险等级）、`GreyNoise`（近 90 天公网扫描行为噪音分类与 RIOT 良性服务标记）以及 `Shodan`（历史暴露开放端口数、关联 CVE 漏洞数与威胁标签）的结果。
   - **7 类风险因子交叉验证**：对 **代理（`Proxy`）、VPN（`Vpn`）、Tor 节点（`Tor`）、数据中心（`Datacenter`）、滥用记录（`Abuser`）、自动化爬虫（`Robot`）、保留/未路由地址（`Bogon`）** 七项指标分别统计“参与给出明确布尔结论的数据源总数”与“标记命中的具体数据源名称”（如 `flagged (2/4: ProxyCheck, ipapi.is)` 或 `clean (0/4)`），避免单一数据库误报定性。

3. **25 个公共 DNSBL 实时黑名单扫描（含递归拒绝去噪）**：
   - 读取本机 `/etc/resolv.conf` 中的最多前 3 个有效递归 DNS 服务器，以 12 并发构造反向 IP 域名查询（IPv4 覆盖 24 个区域如 `Spamhaus ZEN`、`SpamCop`、`Barracuda`、`SORBS`、`UCEPROTECT L1` 等；IPv6 按半字节逆序 nibble 格式覆盖 9 个支持 IPv6 的区域）。
   - **RFC 5782 策略拒绝过滤**：当主机使用 `8.8.8.8` / `1.1.1.1` 等大型公共递归解析器查询 Spamhaus 等区域时，对方常返回 `127.255.255.254` 等策略拒绝码而非真实黑名单命中；插件会将 `127.255.255.0/24` 段及非 `127.0.0.0/8` 劫持响应严格归类为 **`refused`**，仅将合法的 `127.x.y.z` 列表码计为黑名单命中（UDP 截断时自动回退至 TCP DNS 重查）。

4. **TCP 25 端口连通性与 8 大邮局入站 MX 问候探测**：
   - 针对 `Gmail`、`Outlook`、`Yahoo`、`iCloud`、`Proton Mail`、`Fastmail`、`QQ Mail` 与 `163 Mail` 共 8 家全球及中文主流邮箱的入站 MX 服务器，分别在 IPv4 与 IPv6 上向 TCP **25 端口**发起连接（单目标超时 5 秒）。
   - **严格非侵入式握手**：连接建立后仅读取服务端首行三位数字 SMTP Banner 状态码（`2xx` 如 `220` 记为可投递，`4xx`/`5xx` 记为连接期被拒，全部超时/拒绝则判定机房封禁了出站 25 端口），随后**立即发送 `QUIT\r\n` 断开连接**，绝不发送 `HELO`/`EHLO`、`MAIL FROM` 或任何邮件正文。

### 隐私保护与配置示例
- 未指定 `--show-ip` 时，报告（含 `text`、`markdown`、`json`）自动对公网地址打码，并隐藏（在 JSON 中置为 `null`）可能直接包含 IP 数字的反向解析 PTR 主机名（`hostname`）。
```toml
[plugins."ip.quality"]
dnsbl = true   # 设为 false 可跳过 DNSBL 黑名单查询
mail = true    # 设为 false 可跳过出站 TCP 25 端口邮局探测
```

---

## 2. 流媒体与区域服务解锁检测：`ip.media`

| 属性 | 说明 |
|---|---|
| 插件 ID | `ip.media`（所属分组：`ip`） |
| 适用平台 | 全平台（需具备 IPv4 或 IPv6 公网连接） |
| 依赖工具 | 无（纯内置无代理双栈 HTTPS 探针） |
| 超时上限 | `5 分钟` |

### 测试逻辑
1. **五大区域分组、68 个平台双栈直连检测**：
   - 平台名录按地理区域划分为五组：**`global`**（Netflix、YouTube Premium、Disney+、Prime Video、Spotify、TikTok、ChatGPT、Claude、Gemini、Reddit 等全球通用服务）、**`asia`**（Bilibili 港澳台/大陆、巴哈姆特动画疯、KKTV、LINE TV、MyTVSuper、Now E、Viu、Abema、DMM、Niconico、TVer、U-NEXT、Coupang Play、Wavve、TVING 等亚洲平台）、**`europe`**（BBC iPlayer、ITVX、Channel 4、Canal+、ZDF、ARTE、RAI Play、ORF ON 等欧洲平台）、**`americas`**（Hulu、Max、Peacock、Paramount+、Starz、Crave、Globoplay 等美洲平台）与 **`oceania`**（Stan、9Now、7plus、ABC iview、TVNZ+、ThreeNow 等大洋洲平台）。
   - 每个区域内部以最多 **10 个平台并发** 推进，每完成一个大洲分组立即向编排器发布阶段性结果；每个平台均分别绑定 IPv4 客户端与 IPv6 客户端独立探测，互不串栈。
2. **精细化解锁状态分类**：
   - `yes (XX)`：完整解锁非自制版权内容，并在可识别时附带两位地区代码（如 `yes (JP)`、`yes (US)`）。
   - `originals only (XX)`：如 Netflix 仅可观看自制剧集（非自制剧测试片源受限）。
   - `no` / `no (China mainland)` / `coming soon` / `proxy detected`：服务明确不可用、特定地区限制或被平台识别为代理/机房节点。
   - `no IPv6 address`：平台域名本身未配置 `AAAA` 记录（纯 IPv4 服务）。
   - `rate limited`：触发 `HTTP 429` 频率限制。
3. **合规反爬边界（绝不绕过人机验证）**：
   - 当目标平台返回 Cloudflare Turnstile、reCAPTCHA、hCaptcha 等人机验证或 WAF 质询页面时，探针**立即停止对该平台的后续请求并如实标记为 `blocked (WAF)`**，绝不尝试计算工作量证明、模拟浏览器绕过或借用第三方缓存结果。

### 配置示例（`bench.toml`）
```toml
[plugins."ip.media"]
groups = ["global", "asia"]        # 可选 global, asia, europe, americas, oceania 或 all
services = ["netflix", "bahamut"]  # 按平台 id 精确指定待测服务；非空时优先于 groups
```
