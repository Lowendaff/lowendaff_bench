# 磁盘健康与性能检测（`disk.health`、`disk.fio`）

[返回文档目录](README.md) | [简体中文首页](../README.zh-CN.md)

磁盘章节兼顾物理介质可靠性与实际文件系统 I/O 吞吐：**`disk.health`** 在独立物理服务器上读取底层硬盘的 SMART 健康指标（含硬件 RAID 卡穿透）；**`disk.fio`** 在所有平台上执行与 YABS 完全一致的混合随机读写基准测试。

---

## 1. 物理磁盘 SMART 健康检查：`disk.health`

| 属性 | 说明 |
|---|---|
| 插件 ID | `disk.health`（所属分组：`disk`） |
| 适用平台 | **仅独立物理服务器（`Dedicated`）**（虚拟机与容器不暴露真实物理盘 SMART，自动归入不适用） |
| 依赖工具 | 静态构建 `smartctl 7.5`（来自 `smartmontools`） |
| 超时上限 | `5 分钟` |
| 配置表 | 无自定义配置项 |

### 测试逻辑与隐私保护
1. **全自动设备发现与硬件 RAID 穿透**：
   - 先运行 `smartctl --scan-open --json` 让 `smartctl` 主动探测所有块设备与阵列控制器的驱动类型（如 NVMe、SATA `sat`、SCSI，以及 MegaRAID `megaraid,N`、3SNIC / SSSRAID 等硬件阵列卡背后的物理槽位）。
   - 对扫描到的每个目标逐一执行 `smartctl --json -x -d <type> <device>`，**全程仅发送只读查询指令**，不触发任何 SMART 自检写入或固件操作。
2. **临时设备节点回收**：
   - `smartctl` 在穿透部分 RAID 卡时可能按需创建底层字符设备节点；插件会记录运行前后的节点变化并在结束时自动清理，清理失败时在报告备注中列出路径。
3. **跨接口统一指标提炼**：
   - 汇总展示每块物理盘的健康判定（`passed` / `failed` / `unknown` / `SMART unsupported` / `read failed`）、通电小时数（Power-on）、当前温度（`°C`）、SSD 寿命损耗百分比（Worn）、累计主机写入量（按十进制 `GB` / `TB` / `PB` 截断一位小数显示）、坏块计数（SATA 展示“重映射扇区 / 待映射扇区”，NVMe/SAS 展示坏块总数）与接口传输错误计数。
4. **隐私红线**：
   - 解析器刻意丢弃硬盘序列号（Serial Number）、WWN 与固件唯一标识；即便开启 `-v` 详细日志模式，`smartctl` 的原始标准输出也不会打印到日志中，杜绝泄露硬件序列号。

---

## 2. 文件系统混合随机读写基准：`disk.fio`

| 属性 | 说明 |
|---|---|
| 插件 ID | `disk.fio`（所属分组：`disk`） |
| 适用平台 | 独立服务器、虚拟机、容器（全平台） |
| 依赖工具 | 静态链接 `libaio 0.3.113` 的 `fio 3.43` |
| 超时上限 | `10 分钟` |
| 配置表 | 无自定义配置项（通过全局 `--work-dir <DIR>` 选择待测分区） |

### 测试逻辑（100% 对齐 YABS 标准）

为了使测试结果与社区广泛参考的 YABS（Yet Another Bench Script）磁盘数据直接可比，`disk.fio` 从参数、超时到数字截断规则均严格复刻 YABS：

1. **前置安全与空间检查**：
   - **拒绝内存文件系统**：通过 `statfs(2)` 检查测试目录的文件系统魔数；若位于 `tmpfs` 或 `ramfs`（如部分发行版的 `/tmp`），立即跳过并提示更换工作目录（`--work-dir`），避免把内存当成硬盘测速。
   - **架构自适应文件大小**：在 `aarch64`、`arm`、`riscv64` 架构上使用 **`512 MiB`** 测试文件，在 `x86_64` 等其余架构上使用 **`2 GiB`** 测试文件。
   - **剩余空间与 ZFS 膨胀系数检查**：确保分区可用空间足以容纳测试文件；若测试目录位于 ZFS 数据集上，额外读取 `/sys/module/zfs/parameters/spa_asize_inflation` 检查空间富余度并按需给出提示。
   - **Direct I/O 支持检查**：启用 `--direct=1` 绕过操作系统页缓存；若底层文件系统（如部分未开启直通的容器存储或特殊配置）拒绝 `O_DIRECT`，清晰报告不支持 Direct I/O 而非回退到失真的缓存读写。
2. **预铺设 + 四档块大小混合随机读写**：
   - **预铺设阶段（Setup）**：`--ioengine=libaio --rw=read --bs=64k --iodepth=64 --numjobs=2 --runtime=1 --direct=1`，预先在磁盘上完整分配并铺好测试文件。
   - **四轮正式压测**：依次针对 **`4K`、`64K`、`512K`、`1M`** 四种块大小执行 **50% 读 / 50% 写混合随机 I/O**：
     ```text
     fio --name=rand_rw_<bs> --ioengine=libaio --rw=randrw --rwmixread=50 \
         --bs=<bs> --iodepth=64 --numjobs=2 --size=<2G|512M> --runtime=30 \
         --gtod_reduce=1 --direct=1 --group_reporting --minimal
     ```
   - 每轮运行 30 秒（外部硬限时 **35 秒**，与 YABS 的 `timeout 35` 一致）。每完成一个块大小立即发布阶段性结果；单个块大小超时或报错不会抹掉其余块大小的成绩。
3. **速率与 IOPS 汇总格式**：
   - 报告按 `4K & 64K`、`512K & 1M` 两张表展示 `Read`、`Write` 与 `Total`（先将原始 `read_kib + write_kib`、`read_iops + write_iops` 相加，再按 YABS 同款十进制 `KB/s`、`MB/s`、`GB/s` 与 `k` 截断规则格式化），退出后随临时运行目录一并删除测试文件。
