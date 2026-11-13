# Glossary

The program prints its results in Chinese. This table gives the English term that the README,
the code and the commit messages use for each word that it prints often. The Chinese column is
copied from the program's source, character for character, and CI checks that each term still
appears there (README "Gates", Rules).

| Chinese | English | Where it appears |
|---------|---------|------------------|
| 硬盘健康 | Disk health | an item's title |
| CPU 性能 | CPU performance | an item's title; the report's CPU section, with Geekbench 7 |
| 内存性能 | Memory performance | an item's title; the report's memory section |
| 硬盘性能 | Disk performance | an item's title |
| 系统信息 | System | the report's first section, with system information |
| 汇总 | Summary | the report's last section: how each item ended |
| 准备工具 | Tool preparation | the progress log: the tools' downloads, before the items run |
| 运行环境 | Environment | the line that says where the program runs |
| 独立服务器 | Dedicated server | `--list`: the platforms that an item applies to |
| 虚拟机 | Virtual machine | the environment line; `--list` |
| 容器 | Container | the environment line; `--list` |
| 未检测到虚拟化 | No virtualization detected | the environment line of a dedicated server |
| 本机不适用 | Not applicable here | the summary: the items made for other platforms |
| 就绪 | Ready | tool preparation: a tool that downloaded and verified |
| 开始 | Started | the progress log: an item that starts |
| 完成 | Completed | an item's last line; the summary; the progress log |
| 跳过 | Skipped | an item's last line; the summary; the progress log |
| 失败 | Failed | an item's last line; the summary; the progress log, also for a tool |
| 超时 | Timed out | an item's last line; the summary; the progress log |
| 已中断 | Interrupted | an item's last line; the progress log (中断 in the summary) |
| 用时 | Elapsed | an item's last line; the progress log |
| 总用时 | Total time | the summary |
| 说明 | Note | a line under a table that explains it |
| 不可用 | Unavailable | a value that could not be read, followed by the reason |
| 未知 | Unknown | a value that its source does not give |
| 运行时间 | Uptime | system information |
| 负载 | Load | system information |
| 宿主机 | Host | system information: an uptime or load that is the host's, seen from a container |
| TCP 拥塞控制 | TCP congestion control | system information |
| 测试目录 | Test directory | system information; disk performance |
| 公网地址 | Public address | system information: the IPv4 and IPv6 lines |
| 系统盘 | System disk | system information: the disk table |
| 文件系统 | Filesystem | system information: the filesystem table |
| 内存条 | Memory modules | system information on dedicated servers |
| 插槽 | Slot | system information: the memory module table |
| 容量 | Capacity | system information: the disk and memory module tables |
| 未启用 | Not in use | system information: a module that firmware reports as not in use |
| 整机 | Machine | system information on dedicated servers |
| 主板 | Board | system information on dedicated servers |
| RAID 卡 | RAID card | system information; disk health |
| 硬盘 | Disk | the report's disk section; the disk tables of system information and disk health |
| 健康 | Health | disk health: the column with SMART's verdict |
| 通过 | Passed | disk health: SMART's verdict |
| 未通过 | Not passed | disk health: SMART's verdict |
| 读取失败 | Read failed | disk health |
| 通电时间 | Power-on time | disk health |
| 温度 | Temperature | disk health |
| 已用寿命 | Endurance used | disk health |
| 累计写入 | Data written | disk health |
| 坏块 | Bad blocks | disk health |
| 接口错误 | Interface errors | disk health |
| 单线程 | Single thread | CPU and memory performance |
| 多线程 | Multiple threads | CPU and memory performance |
| 每秒事件数 | Events per second | CPU performance |
| 延迟 | Latency | memory performance |
| 带宽 | Bandwidth | memory performance: the NUMA tables |
| 单核 | Single-core | Geekbench 7 |
| 多核 | Multi-core | Geekbench 7 |
| 结果链接 | Result link | Geekbench 7 |
