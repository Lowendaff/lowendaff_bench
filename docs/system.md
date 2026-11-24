# 主机画像与环境探测（`[System]`）

[返回文档目录](README.md) | [简体中文首页](../README.zh-CN.md)

在任何测试插件启动前，`lowendaff-bench` 会先采集一次完整的主机画像（`HostProfile`），既用于在报告开头的 `[System]` 章节展示软硬件全貌，也供后续编排器和各插件进行平台适用性裁剪、线程数计算与工作集容量规划。

## 1. 设计原则：零外部命令、并行限时采集

- **纯文件与系统调用读取**：除 x86 架构在内核未暴露 DMI 表时回退读取 `/dev/mem` 中的固件 SMBIOS 表外，所有主机信息均直接从 `/proc`、`/sys`、`/etc`、`/run/udev`、`/usr/share/hwdata/pci.ids` 以及 `statvfs` / CPUID 读取，不执行 `lscpu`、`dmidecode`、`lspci`、`ip` 等任何宿主机外部命令，避免因极简发行版缺少工具包而产生差异。
- **30 秒总预算与单点故障隔离**：CPU、内存、NUMA、存储、系统内核、网卡和独立服务器硬件分别在独立线程中并行读取，整体最多等待 **30 秒**。若某个子项阻塞（例如死掉的网络挂载点导致 `statvfs` 卡死），仅将该子项标记为 `timed out`，其余采集结果与后续基准测试正常继续。

## 2. 运行环境识别（独立服务器 / 虚拟机 / 容器）

识别顺序遵循 `systemd-detect-virt` 的判定思路由内向外检测（先容器，后虚拟机，均未命中则归为独立物理服务器 `Dedicated`）：

1. **容器环境（`Container`）**：
   - OpenVZ / Virtuozzo：存在 `/proc/vz` 且不存在宿主机侧的 `/proc/bc`。
   - WSL：`/proc/sys/kernel/osrelease` 含 `Microsoft` 或 `WSL`。
   - 容器管理器显式标识：依次读取 `/run/host/container-manager`、`/run/systemd/container` 与 PID 1 的环境变量 `container=`（识别 `lxc`、`docker`、`podman`、`systemd-nspawn`、`wsl` 等）。
   - 标志文件与命名空间兜底：检查 `/run/.containerenv`、`/.dockerenv`，以及 `/proc/self/ns/pid` 是否偏离 Linux 初始 PID 命名空间 inode（`4026531836`）。
2. **虚拟机环境（`Vm`）**：
   - DMI 特征匹配：优先读取 `/sys/class/dmi/id/` 识别 VirtualBox、Xen、Amazon EC2、Parallels、KVM、OpenStack、Alibaba Cloud ECS、QEMU、VMware、Hyper-V、bhyve、Google Compute Engine 等；若 EC2 实例型号以 `.metal` 结尾（如 `c5n.metal`），判定为裸金属物理机而非虚拟机。
   - Xen 半虚拟化：检查 `/proc/xen`，并通过 `XENFEAT_dom0` 特性位或 `control_d` 排除宿主机控制域（dom0）。
   - x86 CPUID 与设备树：在 `x86_64` 上读取 CPUID Leaf `1` 的 Hypervisor 位与 Leaf `0x4000_0000` 签名（如 `KVMKVMKVM`、`Microsoft Hv`、`VMwareVMware`）；在 ARM 等无 CPUID 架构上结合 `/sys/hypervisor/type` 与 `/proc/device-tree/hypervisor/compatible` 判定。

## 3. 各子项采集逻辑

### CPU 与可用线程数
- **型号与 ARM 部件码解码**：x86 直接读取 `/proc/cpuinfo` 的 `model name`；ARM 因内核通常不提供型号字符串，内置了 `util-linux` 同款厂商代码（`CPU implementer`）与核心代码（`CPU part`）映射表，自动还原 `ARM Neoverse-N1`、`HiSilicon Kunpeng-920`、`Ampere-1`、`Apple Firestorm-M1` 等真实微架构名称。
- **物理拓扑与频率**：遍历 `/sys/devices/system/cpu/cpu*/topology/` 统计在线核心的物理插槽（Sockets）与物理核数（自动考虑多 Die 封装）；主频优先取 `cpufreq/cpuinfo_max_freq`，回退至当前频率。
- **多级缓存与指令集**：按 `/sys/devices/system/cpu/cpu0/cache/index*` 汇总 L1d / L1i / L2 / L3 容量，检测 `aes` 硬件加速与硬件虚拟化标志（`vmx` / `svm`；容器内隐藏宿主机虚拟化标志以免误导）。
- **容器配额感知**：在容器中综合 cgroup v1/v2 的 `cpuset` 与 CPU CFS/max 配额（如 `1.5 vCPUs`），后续多线程测试均严格按容器实际可用核心数运行。

### 内存、Swap 与 C 库（glibc）
- **内存与 cgroup 修正**：解析 `/proc/meminfo`；若运行在未挂载 `lxcfs` 的容器（如 Docker）且 cgroup 内存上限低于宿主机物理内存，自动采用容器 cgroup 的上限与已用内存。
- **glibc 版本探测**：直接读取架构对应的动态链接器（`/lib64/ld-linux-x86-64.so.2` 或 `/lib/ld-linux-aarch64.so.1`）及其同级 `libc.so.6` 中的版本标记（如 Alpine musl 系统识别为 `NoGlibc`，从而在下载前即跳过依赖 glibc 的 Geekbench 7）。

### 独立服务器专属硬件详情（DMI / SMBIOS / EDAC / BMC / RAID）
仅当平台识别为 `Dedicated` 时采集并展示：
- **整机、主板与固件**：依次从 `/sys/class/dmi/id`、SMBIOS 表或设备树（U-Boot / 树莓派引导时间戳）提取厂商、型号与 BIOS 日期，自动过滤 `"To Be Filled By O.E.M."`、`"Default string"` 等固件占位符。
- **ECC 与内存插槽表**：结合 SMBIOS Type 16/17 与 `/sys/devices/system/edac/mc` 控制器信息，列出每根内存插槽的容量、代数（DDR3/DDR4/DDR5/LPDDR5/HBM 等）、规格（RDIMM/LRDIMM/UDIMM）、额定与运行频率、厂商及料号（**不采集序列号**），并统计 EDAC 已纠正（CE）与未纠正（UE）错误数，对插在槽位上但被禁用的内存条给出告警备注。
- **BMC 带外管理**：非侵入式读取 IPMI 平台驱动、SMBIOS Type 38 或 ACPI `SPMI` 表（不发送任何 IPMI 指令，不读取 BMC GUID），将 IANA 企业号映射为 Supermicro、Dell、HPE、Inspur、ASRock Rack 等厂商名称。
- **硬件 RAID 控制器**：扫描 PCI Class `0x0104` 设备并匹配 `pci.ids` 名称，根据内核驱动（`megaraid_sas`、`hpsa`、`smartpqi`、`aacraid`、`mpt3sas`、`ahci`、`vmd`）与 SCSI Channel 编号自动区分磁盘挂载模式是硬件阵列卷（`Array`）还是直通盘（`PassThrough`）。

### 存储设备、文件系统与网卡
- **块设备与分区溯源**：从 `/sys/class/block` 过滤内存盘、回环设备、光驱及虚拟卷，通过 `slaves` 递归追溯软 RAID（`md`）、LVM（`dm-*`）和 ZFS 池成员背后的真实物理盘，识别接口类型（NVMe、SATA、SCSI、USB、virtio、Xen、MMC）与完整型号（解码 udev `\x20` 转义以突破 sysfs 16 字符截断）。容器环境下仅展示容器自身根文件系统 `/` 的配额空间。
- **文件系统归并**：解析 `/proc/self/mountinfo` 合并同一块设备或同一 ZFS 存储池的重复挂载，自动标注 `--work-dir` 所在的测试目录分区；对于共享剩余空间的 ZFS 数据集，仅显示可用容量（`Available`）以避免误导。
- **物理网卡与 PCIe 降速检测**：列出具备底层硬件设备的网卡接口、型号、驱动、协商速率/双工、MTU、所属网桥/Bond 及默认路由（IPv4/IPv6）；若检测到 PCIe 网卡当前协商代数或通道宽度低于硬件最大值（如 `x8` 网卡插在 `x4` 槽位上），自动输出降速提示。

## 4. 双栈连通性与公网出口地址探测

主机网络探测模块运行在 `bench` 主程序中，**强制忽略 `HTTPS_PROXY` / `ALL_PROXY` 等代理环境变量**，确保测得的是机器真实的直连网络能力：

1. **TCP 握手可达性探测**：IPv4（`1.1.1.1:443`、`223.5.5.5:443`）与 IPv6（`[2606:4700:4700::1111]:443`、`[2400:3200::1]:443`）分别独立尝试直连握手，区分连接成功、无路由（`NoRoute`）、超时或拒绝。
2. **公网出口地址获取**：对可达协议族依次直接请求 `Cloudflare`、`ipify` 与 `icanhazip`，采用首个合法公网单播地址并记录来源。
3. **默认脱敏规则**：
   - 实时进度日志（`stderr`）**始终**对公网地址打码。
   - 最终报告（无论是 `text`、`markdown` 还是 `json`）默认仅保留 IPv4 前两段（如 `203.0.*.*`）与 IPv6 前 32 位（如 `2001:db8:*`），仅在显式传入 `--show-ip` 时才输出完整地址。

## 5. 虚拟机全程 `%si` 与 `%st` 周期采样（`[CPU steal & softirq]`）

在 VPS / 云主机测评中，宿主机超售导致的 CPU 争抢（`steal`）与高带宽测速期间的内核网络软中断开销（`softirq`）往往直接决定真实负载表现：

1. **仅在虚拟机环境启用（`PlatformKind::Vm`）**：
   - 仅当运行环境被识别为虚拟机（如 KVM、Xen、VMware、Hyper-V、EC2 非 `.metal` 实例、Apple Virtualization 等）时启动后台采样；独立物理服务器（`Dedicated`）与容器环境（`Container`）不采集、不在报告中渲染该章节。
2. **全程每 5 秒 `/proc/stat` 差分计算**：
   - 自编排器启动测试生命周期起至全部测试结束，后台任务每 **5 秒** 读取一次 `/proc/stat` 首行聚合 `cpu` 计数器，并关联当前正在执行的检测项名称（工具准备阶段记为 `Setup`；若单次运行总时长不足 5 秒，在结束时自动补采一次全程区间样本）。
   - 由于 Linux 内核在 `user` 与 `nice` 字段中已包含 `guest` 与 `guest_nice` 时钟节拍，分母严格取前 8 项基础状态增量之和（`user + nice + system + idle + iowait + irq + softirq + steal`），并对每个计数器使用饱和减法（`saturating_sub`）规避无滴答内核（`NO_HZ`）偶发的单调微抖动，分别计算窗口内的 **`%si`（`softirq / total`）** 与 **`%st`（`steal / total`）**。
3. **汇总与逐 5 秒时间线双表展示**：
   - **`text` 与 `markdown` 模式**：在报告末尾输出 `[CPU steal & softirq]` / `## CPU steal & softirq` 章节，包含 **`Summary` 统计表**（`Average` 均值、`P95` 分位数、`Peak` 峰值以及峰值发生时的测试项与时间点 `Peak during`）与 **`Samples` 明细表**（`Time | Item | %si | %st`）；
   - **`json` 模式**：在顶层 `vm_cpu_telemetry` 对象中输出采样周期、样本数、`si_pct` / `st_pct` 汇总指标及完整样本数组（非虚拟机环境该字段为 `null`）。
