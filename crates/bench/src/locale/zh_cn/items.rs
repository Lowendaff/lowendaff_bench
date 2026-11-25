//! 简体中文：各测试项详情章节文案。

use std::path::Path;

use super::SimplifiedChinese;
use crate::locale::BgpField;
use crate::locale::BgpIssueRef;
use crate::locale::BgpWords;
use crate::locale::CloudflareChoice;
use crate::locale::CloudflareField;
use crate::locale::CloudflareIssueRef;
use crate::locale::CloudflareWords;
use crate::locale::CpuWords;
use crate::locale::DiskWords;
use crate::locale::FioRow;
use crate::locale::InetspeedChoice;
use crate::locale::InetspeedProbeFaultRef;
use crate::locale::InetspeedRoundIssueRef;
use crate::locale::InetspeedWarningRef;
use crate::locale::InetspeedWords;
use crate::locale::IpChoice;
use crate::locale::IpField;
use crate::locale::IpTable;
use crate::locale::IpWords;
use crate::locale::ListRule;
use crate::locale::LookupProblem;
use crate::locale::MediaCell;
use crate::locale::MediaChoice;
use crate::locale::MediaWords;
use crate::locale::MemoryWords;
use crate::locale::NetworkWords;
use crate::locale::NoSwapWhy;
use crate::locale::Probe;
use crate::locale::TaierCell;
use crate::locale::TaierChoice;
use crate::locale::TaierListProblem;
use crate::locale::TaierRule;
use crate::locale::TaierSelectorProblem;
use crate::locale::TaierWords;
use crate::locale::TraceChoice;
use crate::locale::TraceList;
use crate::locale::TraceListProblem;
use crate::locale::TraceRule;
use crate::locale::TraceWords;
use crate::locale::Verdict;
use crate::locale::WayCell;

fn version(version: Option<&str>) -> String {
    version.map_or_else(String::new, |version| format!("{version}："))
}

impl CpuWords for SimplifiedChinese {
    fn cpu_suite_step(&self, suite: &str, stage: Option<&str>, threads: Option<usize>) -> String {
        let name = self.cpu_suite_title(suite);
        match (stage, threads) {
            (Some(stage), Some(1)) => format!("{name}：{stage}（单并发）"),
            (Some(stage), Some(n)) => format!("{name}：{stage}（{n} 线程/并发）"),
            (Some(stage), None) => format!("{name}：{stage}"),
            (None, Some(1)) => format!("{name}：单线程测试"),
            (None, Some(n)) => format!("{name}：多线程测试（{n} 线程）"),
            (None, None) => format!("{name}：运行中"),
        }
    }

    fn cpu_selected_suites(&self, suites: &[String]) -> String {
        format!("启用子套件：{}", suites.join(", "))
    }

    fn cpu_suite_title(&self, suite: &str) -> &'static str {
        match suite {
            "sysbench" => "sysbench",
            "7zip" => "7-Zip",
            "unixbench" => "UnixBench",
            "hpl" => "HPL (Linpack)",
            "passmark" => "PassMark",
            "ycruncher" => "y-cruncher",
            "prime95" => "Prime95",
            "geekbench" => "Geekbench 7",
            _ => "CPU 子套件",
        }
    }

    fn cpu_suite_skipped(&self, suite: &str, why: &str) -> String {
        format!("{}：已跳过（{why}）", self.cpu_suite_title(suite))
    }

    fn cpu_suite_failed(&self, suite: &str, why: &str) -> String {
        format!("{}：失败（{why}）", self.cpu_suite_title(suite))
    }

    fn cpu_skip_arch(&self, arch: &str) -> String {
        format!("仅支持 x86_64，当前主机架构为 {arch}")
    }

    fn cpu_missing_lib(&self, lib: &str) -> String {
        format!("缺少系统动态链接库 {lib}")
    }

    fn cpu_setup_failed(&self, reason: &str) -> String {
        format!("无法准备测试工作目录：{reason}")
    }

    fn sysbench_parameters(&self, version: Option<&str>, max_prime: u64, seconds: u64) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("sysbench{version}：素数上限 {max_prime}，每轮 {seconds} 秒")
    }

    fn cpu_headers(&self) -> [&'static str; 3] {
        ["测试模式", "线程数", "事件/秒"]
    }

    fn cpu_row(&self, multi: bool) -> &'static str {
        if multi { "多线程" } else { "单线程" }
    }

    fn cpu_test(&self, multi: bool) -> &'static str {
        if multi {
            "多线程测试"
        } else {
            "单线程测试"
        }
    }

    fn ratio_label(&self) -> &'static str {
        "多核倍率"
    }

    fn ratio(&self, ratio: f64) -> String {
        format!("多线程为单线程的 {ratio:.2} 倍")
    }

    fn no_rate(&self) -> String {
        "sysbench 未输出每秒事件速率".to_owned()
    }

    fn seven_zip_parameters(&self, version: Option<&str>) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        format!("7-Zip{version}：LZMA2 整数压缩与解压吞吐")
    }

    fn seven_zip_headers(&self) -> [&'static str; 5] {
        [
            "测试模式",
            "线程数",
            "压缩 (MIPS)",
            "解压 (MIPS)",
            "总计 (MIPS)",
        ]
    }

    fn seven_zip_no_scores(&self) -> String {
        "7-Zip 未输出基准评分".to_owned()
    }

    fn unixbench_parameters(&self, version: &str) -> String {
        format!("Byte-UnixBench {version}：12 项系统综合指数（SPARCstation 20-61 = 10.0）")
    }

    fn unixbench_index_label(&self, copies: usize) -> String {
        if copies <= 1 {
            "综合指数（单并发）".to_owned()
        } else {
            format!("综合指数（{copies} 并发）")
        }
    }

    fn unixbench_headers(&self, multi_copies: Option<usize>) -> Vec<String> {
        match multi_copies {
            Some(n) => vec![
                "子项".to_owned(),
                "单并发".to_owned(),
                format!("{n} 并发"),
                "单位".to_owned(),
                "单核指数".to_owned(),
                format!("{n} 核指数"),
            ],
            None => vec![
                "子项".to_owned(),
                "单并发".to_owned(),
                "单位".to_owned(),
                "单核指数".to_owned(),
            ],
        }
    }

    fn unixbench_item_name(&self, id: &str) -> &'static str {
        match id {
            "dhry2reg" => "Dhrystone 2 整数运算",
            "whetstone-double" => "双精度 Whetstone 浮点",
            "execl" => "Execl 吞吐",
            "fstime" => "文件复制 (1024 B, 2000 块)",
            "fsbuffer" => "文件复制 (256 B, 500 块)",
            "fsdisk" => "文件复制 (4096 B, 8000 块)",
            "pipe" => "管道吞吐",
            "context1" => "管道上下文切换",
            "spawn" => "进程创建",
            "shell1" => "Shell 脚本 (1 并发)",
            "shell8" => "Shell 脚本 (8 并发)",
            "syscall" => "系统调用开销",
            _ => "UnixBench 子项",
        }
    }

    fn unixbench_no_count(&self, item: &str) -> String {
        format!("UnixBench {item} 未输出 COUNT 读数")
    }

    fn unixbench_timed_out(&self, item: &str, seconds: u64) -> String {
        format!("UnixBench {item} 运行超过 {seconds} 秒仍未结束，已终止")
    }

    fn hpl_parameters(&self, version: Option<&str>, n: u32, nb: u32, threads: usize) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("HPL{ver} (OpenBLAS)：稠密双精度 LU 分解，N={n}，NB={nb}，{threads} 线程")
    }

    fn hpl_labels(&self) -> [&'static str; 3] {
        ["浮点算力", "求解耗时", "残差校验"]
    }

    fn hpl_gflops(&self, gflops: f64) -> String {
        format!("{gflops:.2} GFLOPS")
    }

    fn hpl_seconds(&self, seconds: f64) -> String {
        format!("{seconds:.2} s")
    }

    fn hpl_residual_passed(&self, residual: Option<f64>) -> String {
        match residual {
            Some(res) => format!("通过 ({res:.2e})"),
            None => "通过".to_owned(),
        }
    }

    fn hpl_residual_failed(&self) -> String {
        "HPL 缩放残差数值校验未通过".to_owned()
    }

    fn passmark_parameters(&self, version: Option<&str>) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        format!("PassMark PerformanceTest{ver}：Linux CPU 测试套件")
    }

    fn passmark_summary_labels(&self) -> [&'static str; 2] {
        ["CPU Mark 总分", "单线程得分"]
    }

    fn passmark_headers(&self) -> [&'static str; 3] {
        ["细分项目", "得分", "单位"]
    }

    fn passmark_subtests(&self) -> [(&'static str, &'static str); 8] {
        [
            ("整数运算", "MOps/s"),
            ("浮点运算", "MOps/s"),
            ("素数筛查", "MPrimes/s"),
            ("字符串排序", "KStrings/s"),
            ("数据加密", "MB/s"),
            ("数据压缩", "KB/s"),
            ("物理模拟", "Frames/s"),
            ("扩展指令集", "MMatrices/s"),
        ]
    }

    fn passmark_no_scores(&self) -> String {
        "PassMark 未生成 CPU 评分结果".to_owned()
    }

    fn ycruncher_parameters(
        &self,
        version: Option<&str>,
        digits: u64,
        tuning: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let millions = digits / 1_000_000;
        let tune = tuning.map_or_else(String::new, |t| format!(" ({t})"));
        format!("y-cruncher{ver}：圆周率 Pi 计算至 {millions}M 十进制位{tune}")
    }

    fn ycruncher_labels(&self) -> [&'static str; 4] {
        ["纯计算耗时", "总挂钟耗时", "CPU 利用率", "多核并行效率"]
    }

    fn ycruncher_validation_failed(&self) -> String {
        "y-cruncher 圆周率数位抽检校验未通过".to_owned()
    }

    fn prime95_parameters(
        &self,
        version: Option<&str>,
        workers: usize,
        duration_secs: u64,
        fft: Option<&str>,
    ) -> String {
        let ver = version.map_or_else(String::new, |v| format!(" {v}"));
        let kernel = fft.map_or_else(String::new, |f| format!("，{f}"));
        format!(
            "Prime95{ver}：Small-FFT 稳定性烤机，{workers} 工作线程，{duration_secs} 秒{kernel}"
        )
    }

    fn prime95_labels(&self) -> [&'static str; 2] {
        ["稳定性结果", "完成测试项"]
    }

    fn prime95_passed(&self, warnings: u64) -> String {
        format!("通过（0 错误，{warnings} 警告）")
    }

    fn prime95_hardware_error(&self, detail: &str) -> String {
        format!("Prime95 检测到硬件或舍入计算错误：{detail}")
    }

    fn prime95_no_summary(&self) -> String {
        "Prime95 退出时未输出工作线程烤机汇总".to_owned()
    }

    fn geekbench_labels(&self) -> [&'static str; 3] {
        ["单核得分", "多核得分", "结果链接"]
    }

    fn swapped(&self, need: u64, available: &str, swap: u64) -> String {
        format!(
            "可用内存不足（约需 {need} GiB，当前可用 {available} GiB）；\
             已自动挂载 {swap} GiB 临时 Swap，跑分可能偏低"
        )
    }

    fn memory_short(&self, need: u64, available: &str, why: NoSwapWhy<'_>) -> String {
        let why = match why {
            NoSwapWhy::Unused => "当前 vm.swappiness 为 0，内核不会换出内存".to_owned(),
            NoSwapWhy::NoRoom(Some(free)) => {
                format!("运行目录所在磁盘剩余空间仅 {free} GiB")
            }
            NoSwapWhy::NoRoom(None) => "无法读取运行目录所在磁盘剩余空间".to_owned(),
            NoSwapWhy::Denied => "权限不足（容器环境通常禁止挂载 Swap）".to_owned(),
            NoSwapWhy::Unsupported => "运行目录所在文件系统不支持 Swap 文件".to_owned(),
            NoSwapWhy::Error(error) => error.to_owned(),
        };
        format!(
            "可用内存可能不足（约需 {need} GiB，当前可用 {available} GiB）；\
             未启用临时 Swap：{why}"
        )
    }

    fn first_to_go_not_set(&self, error: &str) -> String {
        format!("无法为 Geekbench 设置内存耗尽优先回收策略：{error}")
    }

    fn oom_not_restored(&self, error: &str) -> String {
        format!("无法恢复主程序的 oom_score_adj：{error}")
    }

    fn uploads_retried(&self, count: usize, last: &str) -> String {
        format!("经历 {count} 次上传失败后重新运行成功；上次原因：{last}")
    }

    fn swap_left(&self, error: &str, path: &Path) -> String {
        format!(
            "无法卸载临时 Swap：{error}；请手动执行 swapoff {} 并删除该文件",
            path.display()
        )
    }

    fn upload_failed(&self, reason: &str) -> String {
        format!("成绩上传失败（{reason}）；免费版 Geekbench 需成功上传后才返回分数")
    }

    fn no_scores(&self) -> String {
        "Geekbench 未输出跑分成绩".to_owned()
    }

    fn out_of_memory(&self) -> String {
        "运行期间因系统内存耗尽（OOM）被终止".to_owned()
    }

    fn cpu_no_suites_selected(&self) -> String {
        "未选择任何 CPU 基准测试子套件".to_owned()
    }

    fn cpu_all_skipped(&self) -> String {
        "所选全部 CPU 子套件在当前主机上均不适用而已跳过".to_owned()
    }

    fn cpu_all_failed(&self) -> String {
        "所有可运行的 CPU 基准测试子套件均执行失败".to_owned()
    }
}

impl MemoryWords for SimplifiedChinese {
    fn memory_sysbench(
        &self,
        version: Option<&str>,
        seconds: u64,
        single: &str,
        per_thread: Option<&str>,
    ) -> String {
        let mut text = format!(
            "{}顺序读写，每轮 {seconds} 秒；单线程缓冲区 {single}",
            self::version(version)
        );
        if let Some(per_thread) = per_thread {
            text.push_str(&format!("，多线程每线程 {per_thread}"));
        }
        text
    }

    fn memory_stream(
        &self,
        version: Option<&str>,
        millions: u64,
        array: u64,
        arrays: u64,
        spread: bool,
    ) -> String {
        let spread = if spread {
            "；线程均匀分布于各 NUMA 节点"
        } else {
            ""
        };
        format!(
            "{}单数组 {millions}00 万元素（{array} MiB，三组共 {arrays} MiB），\
             每项取 10 轮最快值{spread}",
            self::version(version)
        )
    }

    fn latency_label(&self) -> &'static str {
        "访问延迟"
    }

    fn latency_method(&self) -> String {
        "multichase：随机指针追踪，每级测试 2.5 秒".to_owned()
    }

    fn numa_method(&self, nodes: usize) -> String {
        format!(
            "{nodes} 个节点；带宽采用 multiload stream-triad（每格取 3 次最快值），\
             延迟测量同上"
        )
    }

    fn results(&self, tool: &str) -> String {
        format!("{tool} 测试结果")
    }

    fn numa_title(&self, probe: Probe) -> &'static str {
        match probe {
            Probe::Bandwidth => "NUMA 内存带宽（MiB/s；行：线程所在节点，列：内存所在节点）",
            Probe::Latency => "NUMA 访问延迟 (ns)",
        }
    }

    fn test_header(&self) -> &'static str {
        "测试项目"
    }

    fn threads_column(&self, threads: usize) -> String {
        if threads > 1 {
            format!("多线程（{threads} 线程）")
        } else {
            "单线程".to_owned()
        }
    }

    fn latency_headers(&self) -> [&'static str; 3] {
        ["层级", "工作集大小", "延迟"]
    }

    fn node(&self, node: usize) -> String {
        format!("节点 {node}")
    }

    fn operation(&self, write: bool) -> &'static str {
        if write { "写入" } else { "读取" }
    }

    fn memory_level(&self) -> &'static str {
        "主内存"
    }

    fn cache_unknown(&self) -> String {
        "未读取到末级缓存容量，按 32 MiB 估算".to_owned()
    }

    fn stream_rule(&self) -> String {
        "受可用内存限制，单数组小于末级缓存 4 倍（未完全满足 STREAM 标准规则）".to_owned()
    }

    fn sysbench_small(&self, four: &str) -> String {
        format!(
            "可用内存较少：测试数据小于末级缓存 4 倍（{four}），\
             部分读写命中缓存，结果可能偏高"
        )
    }

    fn latency_small(&self, four: &str) -> String {
        format!(
            "可用内存较少：主内存测试区小于末级缓存 4 倍（{four}），\
             部分读取命中缓存，延迟可能偏低"
        )
    }

    fn levels_missing(&self, levels: &[&str]) -> String {
        match levels {
            [level] => format!("未获取到 {level} 容量，已略过该层级"),
            [others @ .., last] => {
                format!(
                    "未获取到 {} 与 {last} 容量，已略过上述层级",
                    others.join("、")
                )
            }
            [] => String::new(),
        }
    }

    fn sysbench_run(&self, write: bool, multi: bool) -> String {
        let threads = if multi { "多线程" } else { "单线程" };
        let operation = if write { "写入" } else { "读取" };
        format!("{threads}{operation}")
    }

    fn stream_run(&self, multi: bool) -> String {
        if multi {
            "多线程测试".to_owned()
        } else {
            "单线程测试".to_owned()
        }
    }

    fn latency_run(&self, level: &str) -> String {
        format!("{level} 测试")
    }

    fn numa_run(&self, row: usize, column: usize, probe: Probe) -> String {
        let test = match probe {
            Probe::Bandwidth => "带宽",
            Probe::Latency => "延迟",
        };
        format!("节点 {row} 线程访问节点 {column} 内存的{test}测试")
    }

    fn no_transfer_rate(&self) -> String {
        "sysbench 未输出内存传输速率".to_owned()
    }

    fn stream_invalid(&self, line: &str) -> String {
        format!("STREAM 结果校验未通过：{line}")
    }

    fn no_result(&self, tool: &str) -> String {
        format!("{tool} 未输出测试结果")
    }

    fn no_latency(&self) -> String {
        "multichase 未输出延迟读数".to_owned()
    }

    fn no_bandwidth(&self) -> String {
        "multiload 未输出带宽读数".to_owned()
    }

    fn bind_refused(&self) -> String {
        "当前容器环境禁止将内存绑定至指定 NUMA 节点".to_owned()
    }

    fn no_memory_info(&self) -> String {
        "无法读取系统可用内存（/proc/meminfo）".to_owned()
    }

    fn stream_short(&self, needed: u64, budget: &str) -> String {
        format!(
            "可用内存不足：三组数组至少需要 {needed} MiB，\
             而可用内存的一半仅为 {budget}"
        )
    }

    fn one_node(&self) -> String {
        "需要多个 NUMA 节点".to_owned()
    }
}

impl DiskWords for SimplifiedChinese {
    fn health_headers(&self) -> [&'static str; 8] {
        [
            "设备",
            "健康状态",
            "通电时长",
            "温度",
            "磨损率",
            "累计写入",
            "坏块数",
            "链路错误",
        ]
    }

    fn card_disk(&self, numbers: &str, host: Option<u32>) -> String {
        match host {
            Some(host) => format!("RAID 卡 host{host} 磁盘 {numbers}"),
            None => format!("RAID 卡磁盘 {numbers}"),
        }
    }

    fn verdict(&self, verdict: Verdict) -> &'static str {
        match verdict {
            Verdict::Passed => "正常",
            Verdict::Failed => "异常",
            Verdict::Unknown => "未知",
            Verdict::Unsupported => "不支持",
            Verdict::ReadFailed => "读取失败",
        }
    }

    fn hours(&self, hours: u64) -> String {
        format!("{hours} 小时")
    }

    fn disk_read_failed(&self, disk: &str, reason: &str) -> String {
        format!("{disk}：读取失败：{reason}")
    }

    fn no_smart(&self, disk: &str) -> String {
        format!("{disk} 不支持 SMART")
    }

    fn smart_off(&self, disk: &str) -> String {
        format!("{disk} 未开启 SMART（本程序不会主动修改磁盘设置）")
    }

    fn disk_read_timed_out(&self, seconds: u64) -> String {
        format!("读取超过 {seconds} 秒仍未结束，已终止")
    }

    fn unparsable(&self) -> String {
        "无法解析 smartctl 的输出".to_owned()
    }

    fn unopened(&self) -> String {
        "无法打开磁盘设备或读取其标识信息".to_owned()
    }

    fn arguments_refused(&self) -> String {
        "smartctl 拒绝了命令行参数".to_owned()
    }

    fn node_left(&self, path: &Path, error: &str) -> String {
        format!("无法删除临时设备节点 {}：{error}", path.display())
    }

    fn no_disks(&self) -> String {
        "smartctl 未发现任何物理磁盘".to_owned()
    }

    fn scan_timed_out(&self, seconds: u64) -> String {
        format!("smartctl 扫描磁盘失败：超过 {seconds} 秒仍未结束")
    }

    fn scan_failed(&self, reason: &str) -> String {
        format!("smartctl 扫描磁盘失败：{reason}")
    }

    fn all_failed(&self) -> String {
        "所有磁盘均无法读取 SMART 信息".to_owned()
    }

    fn fio_method(&self, size: &str) -> (&'static str, String) {
        (
            "测试方法",
            format!("对齐 YABS 标准：50% 读 + 50% 写混合随机 I/O，测试文件 {size}"),
        )
    }

    fn zfs_note(&self, needed: u64) -> String {
        format!(
            "测试目录位于 ZFS 且剩余可用空间不足 {needed} GiB，\
             测试成绩可能受压缩/缓存影响（同 YABS 提示）"
        )
    }

    fn block_sizes(&self, first: &str, second: &str) -> String {
        format!("{first} 与 {second}")
    }

    fn block_size_header(&self) -> &'static str {
        "块大小"
    }

    fn fio_row(&self, row: FioRow) -> &'static str {
        match row {
            FioRow::Read => "读取",
            FioRow::Write => "写入",
            FioRow::Total => "合计",
        }
    }

    fn rate(&self, speed: &str, iops: &str) -> String {
        format!("{speed} ({iops})")
    }

    fn fio_timed_out(&self) -> &'static str {
        "超时"
    }

    fn fio_error(&self) -> &'static str {
        "错误"
    }

    fn block_timed_out(&self, block_size: &str, seconds: u64) -> String {
        format!("{block_size} 测试超过 {seconds} 秒仍未结束，已终止")
    }

    fn block_failed(&self, block_size: &str, reason: &str) -> String {
        format!("{block_size} 测试失败：{reason}")
    }

    fn no_space_info(&self) -> String {
        "无法读取测试目录的可用磁盘空间".to_owned()
    }

    fn in_memory(&self) -> String {
        "测试目录位于内存文件系统（如 tmpfs），这会测到内存而非硬盘；\
         请在磁盘目录运行或用 --work-dir 指定挂载路径"
            .to_owned()
    }

    fn no_space(&self, needed: &str, available: &str) -> String {
        format!("测试目录可用空间不足 {needed}（当前仅剩 {available}）")
    }

    fn no_direct_io(&self) -> String {
        "测试目录所在文件系统不支持直接 I/O（O_DIRECT）".to_owned()
    }

    fn setup_failed(&self, reason: &str) -> String {
        format!("无法创建测试文件：{reason}")
    }

    fn no_block_size(&self) -> String {
        "四个块大小均未获得有效测试成绩".to_owned()
    }

    fn no_output(&self, tool: &str) -> String {
        format!("{tool} 未产生任何输出")
    }

    fn fio_reported(&self, error: &str) -> String {
        format!("fio 报告错误码 {error}")
    }

    fn fio_field(&self, field: usize) -> String {
        format!("无法解析 fio 输出的第 {field} 个字段")
    }
}

impl NetworkWords for SimplifiedChinese {
    fn iperf3_method(
        &self,
        version: Option<&str>,
        streams: u8,
        seconds: u16,
        rate: Option<u32>,
    ) -> String {
        let version = version.map_or_else(String::new, |version| format!(" {version}"));
        let rate = rate.map_or_else(String::new, |rate| format!("，单流限速 {rate} Mbit/s"));
        format!("iperf3{version}：{streams} 并发流，单向 {seconds} 秒，TCP{rate}")
    }

    fn family_title(&self, ipv6: bool) -> &'static str {
        if ipv6 { "IPv6" } else { "IPv4" }
    }

    fn network_headers(&self) -> [&'static str; 5] {
        ["节点位置", "服务商", "上传速度", "下载速度", "延迟"]
    }

    fn place(&self, location: &str, country: &str) -> String {
        format!("{location}, {country}")
    }

    fn way_cell(&self, cell: WayCell) -> &'static str {
        match cell {
            WayCell::Busy => "繁忙",
            WayCell::Unreachable => "不可达",
            WayCell::TimedOut => "超时",
            WayCell::Error => "错误",
            WayCell::NotTried => "未尝试",
        }
    }

    fn tries(&self, place: &str, download: bool, tries: u32) -> String {
        let direction = if download { "下载" } else { "上传" };
        format!("{place}：{direction}测速共尝试 {tries} 次")
    }

    fn way_failed(&self, place: &str, download: bool, reason: &str) -> String {
        let direction = if download { "下载" } else { "上传" };
        format!("{place}：{direction}测速失败：{reason}")
    }

    fn nothing_measured(&self) -> String {
        "iperf3 未测得任何有效数据".to_owned()
    }

    fn unknown_servers(&self, names: &[String]) -> String {
        format!("节点列表中不存在以下服务器：{}", names.join(", "))
    }

    fn list_unreadable(&self, reason: &str) -> String {
        format!("无法读取测速节点列表：{reason}")
    }

    fn list_too_large(&self, limit: usize) -> String {
        format!("测速节点列表超过 {limit} 字节上限")
    }

    fn list_format(&self, reason: &str) -> String {
        format!("测速节点列表格式无效：{reason}")
    }

    fn list_newer(&self, version: u64) -> String {
        format!("测速节点列表需要更新版本的 lowendaff-bench（列表格式版本 {version}）")
    }

    fn list_version(&self, version: u64) -> String {
        format!("测速节点列表声明了不存在的格式版本：{version}")
    }

    fn list_too_many(&self, count: usize, most: usize) -> String {
        format!("测速节点列表包含 {count} 个节点，超出上限 {most}")
    }

    fn list_rule(&self, position: usize, rule: ListRule) -> String {
        let rule = match rule {
            ListRule::Id => "id 必须由 1 至 40 个小写字母、数字或连字符组成",
            ListRule::Country => "country 必须为两位大写字母国家代码",
            ListRule::Host => "host 必须为域名或 IP 地址",
            ListRule::Ports => "ports 必须为 1024..=65535 内的单端口或最多 64 个端口范围",
            ListRule::Families => "families 必须为 ipv4、ipv6 或 both",
            ListRule::Text => "location、provider 与 link 必须为 1 至 40 个可打印字符",
        };
        format!("测速节点列表第 {position} 项违反校验规则：{rule}")
    }

    fn list_duplicate(&self, id: &str) -> String {
        format!("测速节点列表中存在重复的节点 ID：{id}")
    }

    fn no_server(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "未选中任何测速节点".to_owned()
        } else {
            format!("节点列表中没有匹配 {} 的测速节点", unknown.join(", "))
        }
    }

    fn no_family(&self) -> String {
        "所选测速节点均不支持当前主机可用的 IP 协议栈".to_owned()
    }

    fn no_speed(&self) -> String {
        "所有测速节点均未返回有效带宽结果".to_owned()
    }
}

impl TraceWords for SimplifiedChinese {
    fn trace_method(&self, version: &str, choice: &TraceChoice) -> String {
        let mut method = format!("NextTrace {version}：ICMP，每跳 3 次探测，最多 30 跳");
        if choice.tcp {
            method.push_str(&format!(
                "；中国大陆目标附加 {} 字节 TCP 大包探测",
                choice.large_packets
            ));
        }
        if !choice.groups.is_empty() {
            method.push_str(&format!("；分组 {}", choice.groups.join(", ")));
        }
        if !choice.targets.is_empty() {
            method.push_str(&format!("；指定目标 {}", choice.targets.join(", ")));
        }
        if choice.list {
            method.push_str("；使用自定义目标列表");
        }
        if !choice.geo {
            method.push_str("；不查询网络与地理位置");
        }
        method
    }

    fn trace_heading(&self, name: &str, host: Option<&str>, outcome: &str) -> String {
        match host {
            Some(host) => format!("{name} ({host})：{outcome}"),
            None => format!("{name}：{outcome}"),
        }
    }

    fn trace_tcp_heading(&self, port: u16, packet_size: u16, outcome: &str) -> String {
        format!("TCP 端口 {port}（{packet_size} 字节探测包）：{outcome}")
    }

    fn youtube(&self) -> &'static str {
        "YouTube"
    }

    fn netflix(&self, place: Option<(&str, &str)>) -> String {
        match place {
            Some((city, country)) => format!("Netflix, {city}, {country}"),
            None => "Netflix".to_owned(),
        }
    }

    fn reached(&self, hops: usize) -> String {
        format!("共 {hops} 跳到达")
    }

    fn not_reached(&self, hops: usize) -> String {
        format!("经 {hops} 跳仍未到达目标")
    }

    fn fragmented(&self, hop: usize, mtu: Option<u16>) -> String {
        match mtu {
            Some(mtu) => format!("在第 {hop} 跳受阻（下一跳路由器最大允许 {mtu} 字节）"),
            None => format!("在第 {hop} 跳受阻（路由器要求分片，无法通过该大包）"),
        }
    }

    fn trace_headers(&self) -> [&'static str; 6] {
        ["跳数", "地址", "ASN", "网络", "位置", "延迟"]
    }

    fn trace_group_title(&self, code: &str) -> &'static str {
        match code {
            "services" => "公共内容节点",
            "china" => "中国大陆线路",
            "international" => "国际骨干与云节点",
            _ => "路由追踪",
        }
    }

    fn trace_md_target_title(&self, name: &str, ipv6: bool) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{name} ({fam})")
    }

    fn trace_md_target_line(&self, host: Option<&str>, outcome: &str) -> String {
        match host {
            Some(host) => format!("目标地址：{host}（{outcome}）"),
            None => outcome.to_owned(),
        }
    }

    fn at_exchange(&self, network: Option<&str>, exchange: &str) -> String {
        match network {
            Some(network) => format!("{network} [{exchange}]"),
            None => format!("[{exchange}]"),
        }
    }

    fn unavailable_lookup(&self, reason: &str) -> String {
        format!("不可用：{reason}")
    }

    fn lookup_problem(&self, problem: LookupProblem) -> String {
        match problem {
            LookupProblem::NoServer => "Google 接口未返回缓存节点",
            LookupProblem::Id => "Google 返回的节点 ID 无法拼装为合法域名",
            LookupProblem::Format => "fast.com 返回内容不符合 API 格式",
            LookupProblem::NoTarget => "fast.com 未返回测速缓存节点",
            LookupProblem::Url => "fast.com 返回的节点地址无法用于路由追踪",
            LookupProblem::Place => "fast.com 返回的节点位置无法展示",
        }
        .to_owned()
    }

    fn answer_unreadable(&self, reason: &str) -> String {
        format!("无法读取响应内容：{reason}")
    }

    fn traced_nothing(&self) -> String {
        "NextTrace 未返回任何路由跳点".to_owned()
    }

    fn unknown_targets(&self, ids: &[String]) -> String {
        format!("路由目标列表中不存在：{}", ids.join(", "))
    }

    fn no_exchanges(&self, reason: &str) -> String {
        format!("未加载 IX 交换中心列表：{reason}")
    }

    fn trace_list_problem(&self, list: TraceList, problem: TraceListProblem<'_>) -> String {
        let (name, entry) = match list {
            TraceList::Targets => ("路由目标列表", "目标"),
            TraceList::Exchanges => ("IX 交换中心端口列表", "端口"),
        };
        match problem {
            TraceListProblem::Unreadable(reason) => format!("无法读取{name}：{reason}"),
            TraceListProblem::TooLarge(limit) => format!("{name}超过 {limit} 字节上限"),
            TraceListProblem::Format(reason) => format!("{name}格式无效：{reason}"),
            TraceListProblem::Newer(version) => {
                format!("{name}需要更新版本的 lowendaff-bench（格式版本 {version}）")
            }
            TraceListProblem::Version(version) => {
                format!("{name}声明了不存在的格式版本：{version}")
            }
            TraceListProblem::TooMany(count, most) => {
                format!("{name}包含 {count} 个{entry}，超出上限 {most}")
            }
            TraceListProblem::Rule(position, rule) => {
                let rule = match rule {
                    TraceRule::Id => {
                        "id 必须为 1 至 40 个小写字母、数字或 -，且不得为 youtube 或 netflix"
                    }
                    TraceRule::Group => "group 必须为 china 或 international",
                    TraceRule::Name => "name 必须为 1 至 40 个可打印字符",
                    TraceRule::Asn => "ASN 必须在 1..=4294967295 范围内",
                    TraceRule::Ipv4 => "ipv4 必须为域名或 IPv4 地址",
                    TraceRule::Ipv6 => "ipv6 必须为域名或 IPv6 地址",
                    TraceRule::Tcp => "tcp 端口仅允许配置在 china 目标上，范围 1..=65535",
                    TraceRule::Address => "address 必须为标准格式的 IPv4 或纯 IPv6 地址",
                    TraceRule::Exchange => "exchange 必须为 1 至 60 个可打印字符",
                };
                format!("{name}第 {position} 个{entry}违反规则：{rule}")
            }
            TraceListProblem::Duplicate(key) => match list {
                TraceList::Targets => format!("{name}存在重复的目标 ID：{key}"),
                TraceList::Exchanges => format!("{name}存在重复的端口地址：{key}"),
            },
        }
    }

    fn no_target(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "未选中任何路由追踪目标".to_owned()
        } else {
            format!("目标列表中不存在：{}", unknown.join(", "))
        }
    }

    fn no_target_family(&self) -> String {
        "所选路由目标均不包含当前主机可用的 IP 协议栈地址".to_owned()
    }

    fn no_target_route(&self) -> String {
        "所有路由目标均未获取到追踪结果".to_owned()
    }
}

impl MediaWords for SimplifiedChinese {
    fn media_method(&self, choice: &MediaChoice) -> String {
        let mut method = if choice.custom_base {
            "自定义探测基准 URL，直连探测（不使用代理）".to_owned()
        } else {
            "HTTPS 直连探测（不使用代理）".to_owned()
        };
        if !choice.groups.is_empty() {
            method.push_str(&format!("；区域 {}", choice.groups.join(", ")));
        }
        if !choice.services.is_empty() {
            method.push_str(&format!("；指定平台 {}", choice.services.join(", ")));
        }
        method
    }

    fn media_group(&self, code: &str) -> &'static str {
        match code {
            "global" => "跨国通用平台",
            "asia" => "亚太地区",
            "europe" => "欧洲地区",
            "americas" => "美洲地区",
            "oceania" => "大洋洲地区",
            _ => "其他地区",
        }
    }

    fn media_headers(&self) -> [&'static str; 3] {
        ["流媒体 / 服务", "IPv4", "IPv6"]
    }

    fn media_yes(&self, region: Option<&str>, unlock: Option<&str>) -> String {
        let mode = match unlock {
            Some("native") => Some("原生"),
            Some("dns") => Some("DNS"),
            _ => None,
        };
        match (region, mode) {
            (Some(reg), Some(how)) => format!("解锁 ({reg}, {how})"),
            (Some(reg), None) => format!("解锁 ({reg})"),
            (None, Some(how)) => format!("解锁 ({how})"),
            (None, None) => "解锁".to_owned(),
        }
    }

    fn media_originals(&self, region: Option<&str>, unlock: Option<&str>) -> String {
        let mode = match unlock {
            Some("native") => Some("原生"),
            Some("dns") => Some("DNS"),
            _ => None,
        };
        match (region, mode) {
            (Some(reg), Some(how)) => format!("仅自制剧 ({reg}, {how})"),
            (Some(reg), None) => format!("仅自制剧 ({reg})"),
            (None, Some(how)) => format!("仅自制剧 ({how})"),
            (None, None) => "仅自制剧".to_owned(),
        }
    }

    fn media_cell(&self, cell: MediaCell, ipv6: bool) -> &'static str {
        match cell {
            MediaCell::No => "未解锁",
            MediaCell::NoChina => "不可用 (CN)",
            MediaCell::ComingSoon => "即将上线",
            MediaCell::ProxyDetected => "未解锁 (识别代理)",
            MediaCell::BlockedWaf => "被拦截 (WAF)",
            MediaCell::RateLimited => "请求限流",
            MediaCell::NoAddress => {
                if ipv6 {
                    "无 IPv6 解析"
                } else {
                    "无 IPv4 解析"
                }
            }
            MediaCell::TimedOut => "超时",
            MediaCell::Error => "探测异常",
        }
    }

    fn media_unexpected(&self, service: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{service} ({family})：{reason}")
    }

    fn unknown_services(&self, ids: &[String]) -> String {
        format!("流媒体目录中不存在以下服务：{}", ids.join(", "))
    }

    fn invalid_base_url(&self, url: &str) -> String {
        format!(
            "base_url 必须以 https:// 或本地回环地址 http://127.0.0.1 / http://[::1] 开头：{url}"
        )
    }

    fn no_service(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "未选择任何流媒体平台".to_owned()
        } else {
            format!("流媒体目录中不存在：{}", unknown.join(", "))
        }
    }

    fn no_media_family(&self) -> String {
        "当前主机既无 IPv4 也无 IPv6 公网连接".to_owned()
    }
}

impl IpWords for SimplifiedChinese {
    fn ip_method(&self, choice: &IpChoice) -> String {
        let mut parts = vec!["9 家情报源"];
        if choice.dnsbl {
            parts.push("DNSBL 黑名单");
        }
        if choice.mail {
            parts.push("25 端口邮局");
        }
        let mut method = format!("直连检测（不使用代理）：{}", parts.join("、"));
        if choice.custom_endpoints {
            method.push_str("；自定义接口地址");
        }
        method
    }

    fn ip_stage(&self, stage: &str) -> &'static str {
        match stage {
            "intelligence" => "威胁情报与数据库",
            "dnsbl" => "DNSBL 黑名单",
            "mail" => "25 端口邮局连通性",
            _ => "IP 质量",
        }
    }

    fn ip_table(&self, table: IpTable) -> &'static str {
        match table {
            IpTable::Summary => "基础画像",
            IpTable::Scores => "风控与欺诈评分",
            IpTable::Factors => "风险标签统计",
            IpTable::BlacklistsAndMail => "黑名单与 25 邮件端口",
        }
    }

    fn ip_headers(&self, table: IpTable) -> [&'static str; 3] {
        let first = match table {
            IpTable::Summary => "属性",
            IpTable::Scores => "数据源",
            IpTable::Factors => "风险标签",
            IpTable::BlacklistsAndMail => "检测项",
        };
        [first, "IPv4", "IPv6"]
    }

    fn ip_field(&self, field: IpField) -> &'static str {
        match field {
            IpField::Address => "公网地址",
            IpField::Hostname => "反向域名",
            IpField::Asn => "ASN",
            IpField::Organization => "所属机构",
            IpField::Registry => "注册局",
            IpField::Origin => "IP 来源",
            IpField::Location => "所在城市",
            IpField::Timezone => "时区",
            IpField::AsnType => "ASN 类型",
            IpField::CompanyType => "机构类型",
            IpField::GeoIp => "地理位置",
        }
    }

    fn ip_native(&self, country: &str) -> String {
        format!("原生 IP ({country})")
    }

    fn ip_broadcast(&self, registered: &str, located: &str) -> String {
        format!("广播 IP ({registered} -> {located})")
    }

    fn ip_usage(&self, code: &str) -> &'static str {
        match code {
            "residential" => "家庭宽带",
            "hosting" => "数据中心 / 机房",
            "business" => "企业专线",
            "mobile" => "蜂窝移动网络",
            "education" => "教育科研网",
            "government" => "政府机构",
            "cdn" => "CDN 节点",
            _ => "未知",
        }
    }

    fn ip_dual_isp(&self, usage: &str) -> String {
        format!("{usage}（双 ISP）")
    }

    fn ip_risk_level(&self, code: &str) -> &'static str {
        match code {
            "low" => "低风险",
            "medium" => "中等风险",
            "high" => "高风险",
            "very_high" => "极高风险",
            _ => "未知",
        }
    }

    fn ip_numeric_score(&self, score: u8, max: u8, level: &str) -> String {
        format!("{score}/{max} ({})", self.ip_risk_level(level))
    }

    fn ip_ratio_score(&self, ratio: &str, level: &str) -> String {
        format!("{ratio} ({})", self.ip_risk_level(level))
    }

    fn ip_greynoise(&self, riot: bool, noise: bool, class: &str) -> &'static str {
        if riot {
            return "已知良性服务 (RIOT)";
        }
        match class {
            "not_observed" if !noise => "正常（未见全网扫描）",
            "benign" => "良性扫描源",
            "malicious" => "恶意扫描源",
            _ if noise => "观测到扫描行为",
            _ => "正常（未见全网扫描）",
        }
    }

    fn ip_shodan(&self, ports: usize, vulns: usize, flagged: bool) -> String {
        let label = if flagged || vulns > 0 {
            "存在标记"
        } else {
            "正常"
        };
        format!("{label}（{ports} 个开放端口，{vulns} 个 CVE）")
    }

    fn ip_factor_name(&self, code: &str) -> &'static str {
        match code {
            "proxy" => "代理 (Proxy)",
            "vpn" => "VPN",
            "tor" => "Tor 节点",
            "datacenter" => "机房 (IDC)",
            "abuser" => "滥用记录",
            "robot" => "自动化爬虫",
            "bogon" => "保留地址 (Bogon)",
            _ => "其他",
        }
    }

    fn ip_factor_cell(&self, checked: usize, flagged: &[&str]) -> String {
        if flagged.is_empty() {
            format!("否 (0/{checked})")
        } else {
            format!("是 ({}/{checked}：{})", flagged.len(), flagged.join(", "))
        }
    }

    fn ip_dnsbl_label(&self) -> &'static str {
        "DNSBL 黑名单"
    }

    fn ip_dnsbl_cell(&self, checked: usize, refused: usize, listed: &[&str]) -> String {
        if listed.is_empty() {
            if refused > 0 {
                format!("正常 (0/{checked}；{refused} 个拒绝查询)")
            } else {
                format!("正常 (0/{checked})")
            }
        } else {
            format!("命中 ({}/{checked}：{})", listed.len(), listed.join(", "))
        }
    }

    fn ip_port25_label(&self) -> &'static str {
        "出站 25 端口"
    }

    fn ip_port25_cell(&self, open: bool) -> &'static str {
        if open { "开放" } else { "封禁" }
    }

    fn ip_mail_yes(&self, code: u16) -> String {
        format!("连通 ({code})")
    }

    fn ip_mail_rejected(&self, code: u16) -> String {
        format!("拒收 ({code})")
    }

    fn ip_mail_blocked(&self) -> &'static str {
        "封禁 / 超时"
    }

    fn ip_unexpected(&self, provider: &str, ipv6: bool, reason: &str) -> String {
        let family = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{provider} ({family})：{reason}")
    }

    fn ip_no_public_address(&self) -> String {
        "未获取到任何可用协议栈的公网 IP 地址".to_owned()
    }

    fn ip_invalid_endpoint(&self, raw: &str) -> String {
        format!("接口覆盖地址必须为 https:// 或本地回环地址 (127.0.0.1 / [::1])：{raw}")
    }
}

impl BgpWords for SimplifiedChinese {
    fn bgp_method(&self, custom_endpoints: bool, upload: bool) -> String {
        let mut line = "RIPEstat RIS 直连查询：BGP 路由宣告、RPKI 与 AS 路径".to_owned();
        if !upload {
            line.push_str("；不上传拓扑图片");
        }
        if custom_endpoints {
            line.push_str("；自定义接口地址");
        }
        line
    }

    fn bgp_summary_title(&self) -> &'static str {
        "BGP 路由概览"
    }

    fn bgp_summary_headers(&self) -> [&'static str; 3] {
        ["字段", "IPv4", "IPv6"]
    }

    fn bgp_field(&self, field: BgpField) -> &'static str {
        match field {
            BgpField::Address => "公网地址",
            BgpField::Prefix => "宣告前缀",
            BgpField::OriginAsn => "起源 ASN",
            BgpField::Rpki => "RPKI 状态",
            BgpField::Visibility => "RIS 可见度",
            BgpField::Upstreams => "直连上游",
        }
    }

    fn bgp_not_announced(&self) -> &'static str {
        "未见 BGP 宣告"
    }

    fn bgp_origin_cell(&self, asn: u32, name: Option<&str>, total_origins: usize) -> String {
        let mut cell = match name.filter(|s| !s.is_empty()) {
            Some(label) => format!("AS{asn} ({label})"),
            None => format!("AS{asn}"),
        };
        if total_origins > 1 {
            cell.push_str(&format!(" (+{} MOAS)", total_origins - 1));
        }
        cell
    }

    fn bgp_rpki_cell(&self, code: &str) -> &'static str {
        match code {
            "valid" => "有效 (Valid)",
            "invalid" => "无效 (Invalid)",
            "not_found" => "未签发 (无 ROA)",
            _ => "未知",
        }
    }

    fn bgp_visibility_cell(&self, peers: u32, distinct_paths: usize) -> String {
        format!("{peers} 个采集节点，{distinct_paths} 条独立路径")
    }

    fn bgp_direct_peering(&self) -> &'static str {
        "与采集器直连对等"
    }

    fn bgp_topology_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 上游拓扑"
        } else {
            "IPv4 上游拓扑"
        }
    }

    fn bgp_paths_title(&self, ipv6: bool) -> &'static str {
        if ipv6 {
            "IPv6 观测路径"
        } else {
            "IPv4 观测路径"
        }
    }

    fn bgp_paths_headers(&self) -> [&'static str; 3] {
        ["观测节点数", "跳数", "AS_PATH"]
    }

    fn bgp_issue(&self, issue: BgpIssueRef<'_>) -> String {
        match issue {
            BgpIssueRef::NoAddress => "API 域名无对应协议栈地址".to_owned(),
            BgpIssueRef::TimedOut => "请求超时".to_owned(),
            BgpIssueRef::TooManyRedirects => "HTTP 重定向次数过多".to_owned(),
            BgpIssueRef::InvalidRedirect(why) => format!("无效重定向：{why}"),
            BgpIssueRef::NotHttps(why) => format!("非 HTTPS 接口：{why}"),
            BgpIssueRef::Transport(why) => why.to_owned(),
            BgpIssueRef::HttpStatus(status) => format!("bgp-state 返回 HTTP {status}"),
            BgpIssueRef::InvalidJson(why) => format!("bgp-state JSON 解析失败：{why}"),
            BgpIssueRef::ApiStatus(status) => format!("bgp-state 状态异常 `{status}`"),
            BgpIssueRef::MissingData => "bgp-state 响应缺少 `data` 字段".to_owned(),
            BgpIssueRef::NoData => "未返回 BGP 数据".to_owned(),
        }
    }

    fn bgp_upload_http_error(&self, status: u16, reason: &str) -> String {
        format!("HTTP {status}：{reason}")
    }

    fn bgp_upload_invalid_json(&self) -> String {
        "图床返回了无效的 JSON".to_owned()
    }

    fn bgp_upload_rejected(&self) -> String {
        "图床拒绝了上传请求".to_owned()
    }

    fn bgp_upload_missing_key(&self) -> String {
        "图床响应缺少有效的 imageKey".to_owned()
    }

    fn bgp_upload_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} 拓扑图上传失败：{reason}")
    }

    fn bgp_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} BGP 查询失败：{reason}")
    }

    fn bgp_query_failed(&self, reason: &str) -> String {
        format!("BGP 路由状态查询失败：{reason}")
    }
}

impl TaierWords for SimplifiedChinese {
    fn taier_method(&self, choice: &TaierChoice) -> String {
        let mut method = format!(
            "中国信通院全球网测（泰尔）：{} 并发流，单向 {} 秒，HTTP/1.1",
            choice.streams, choice.seconds
        );
        if !choice.selections.is_empty() {
            const MAX_SHOWN: usize = 5;
            if choice.selections.len() <= MAX_SHOWN {
                method.push_str(&format!("；目标 {}", choice.selections.join(", ")));
            } else {
                let head = choice.selections[..MAX_SHOWN].join(", ");
                let rest = choice.selections.len() - MAX_SHOWN;
                method.push_str(&format!("；目标 {head}（另有 {rest} 项）"));
            }
        } else {
            if !choice.cities.is_empty() {
                method.push_str(&format!("；城市 {}", choice.cities.join(", ")));
            }
            if !choice.operators.is_empty() {
                method.push_str(&format!("；运营商 {}", choice.operators.join(", ")));
            }
        }
        if choice.custom_list {
            method.push_str("；使用自定义节点列表");
        }
        method
    }

    fn taier_unknown_cities(&self, names: &[String]) -> String {
        format!("泰尔测速节点列表中不存在：{}", names.join(", "))
    }

    fn taier_headers(&self) -> [&'static str; 4] {
        ["节点地区", "运营商", "下载速度", "上传速度"]
    }

    fn taier_place(&self, city: &str, province: &str) -> String {
        if city.eq_ignore_ascii_case(province) {
            city.to_owned()
        } else {
            format!("{city}, {province}")
        }
    }

    fn taier_operator(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "电信",
            "unicom" => "联通",
            "mobile" => "移动",
            "cernet" => "教育网",
            "cbn" => "广电",
            _ => "其他",
        }
    }

    fn taier_cell(&self, cell: TaierCell) -> &'static str {
        match cell {
            TaierCell::Busy => "繁忙",
            TaierCell::Unreachable => "不可达",
            TaierCell::AuthRejected => "鉴权被拒",
            TaierCell::TimedOut => "超时",
            TaierCell::Error => "错误",
        }
    }

    fn taier_fallback(&self, place: &str, operator: &str, tried: u8) -> String {
        format!(
            "{place}（{operator}）：前序节点繁忙或不可达，已切换至第 {tried} 个候选节点完成测速"
        )
    }

    fn taier_node_failed(&self, place: &str, operator: &str, reason: &str) -> String {
        format!("{place}（{operator}）：失败：{reason}")
    }

    fn taier_list_problem(&self, problem: TaierListProblem<'_>) -> String {
        match problem {
            TaierListProblem::Missing => "缺少泰尔测速节点列表".to_owned(),
            TaierListProblem::Unreadable(reason) => {
                format!("无法读取泰尔测速节点列表：{reason}")
            }
            TaierListProblem::TooLarge(limit) => {
                format!("泰尔测速节点列表超过 {limit} 字节上限")
            }
            TaierListProblem::Format(reason) => {
                format!("泰尔测速节点列表格式无效：{reason}")
            }
            TaierListProblem::Newer(version) => {
                format!("泰尔节点列表需要更新版本的 lowendaff-bench（格式版本 {version}）")
            }
            TaierListProblem::Version(version) => {
                format!("泰尔节点列表声明了不存在的格式版本：{version}")
            }
            TaierListProblem::Empty => "泰尔测速节点列表为空".to_owned(),
            TaierListProblem::TooMany(count, most) => {
                format!("泰尔测速节点列表包含 {count} 个地区，超出上限 {most}")
            }
            TaierListProblem::Rule(position, rule) => {
                let detail = match rule {
                    TaierRule::Id => "id 必须为 1 至 40 个小写字母、数字或 -（且不为 all）",
                    TaierRule::Province => "province 必须为 1 至 40 个可打印 ASCII 字符",
                    TaierRule::City => "city 必须为 1 至 40 个可打印 ASCII 字符",
                    TaierRule::Nodes => "nodes 必须包含 1 至 32 个节点条目",
                    TaierRule::Operator => {
                        "operator 必须为 telecom, unicom, mobile, cernet, cbn 或 other"
                    }
                    TaierRule::Ip => "ip 必须为规范格式的 IPv4 地址",
                    TaierRule::Port => "port 必须在 1024..=65535 范围内",
                    TaierRule::DuplicateEndpoint => "同一地区内存在重复的 ip:port 节点",
                };
                format!("泰尔节点列表第 {position} 个地区违反规则：{detail}")
            }
            TaierListProblem::Duplicate(id) => {
                format!("泰尔节点列表存在重复的地区 ID：{id}")
            }
        }
    }

    fn taier_selector_problem(&self, problem: TaierSelectorProblem<'_>) -> String {
        match problem {
            TaierSelectorProblem::Empty => "空的泰尔目标选择器".to_owned(),
            TaierSelectorProblem::Format(raw) => format!(
                "无效的泰尔选择器 `{raw}`：格式应为 <地区>[:<运营商>[+<运营商>...]]\
                 （例如 beijing:cu 或 guangdong/shenzhen:ct+cm）"
            ),
            TaierSelectorProblem::EmptyPlace(raw) => {
                format!("无效的泰尔选择器 `{raw}`：缺少地区名称")
            }
            TaierSelectorProblem::EmptyOperators(raw) => format!(
                "无效的泰尔选择器 `{raw}`：`:` 后缺少运营商代码\
                 （ct, cu, cm, edu, cbn, other 或 all）"
            ),
            TaierSelectorProblem::UnknownOperator { selector, operator } => format!(
                "泰尔选择器 `{selector}` 中存在未知运营商 `{operator}`：可选 \
                 telecom/ct, unicom/cu, mobile/cm, cernet/edu, cbn, other 或 all"
            ),
        }
    }

    fn taier_catalog_header(&self, regions: usize, locations: usize) -> String {
        format!("中国三网测速 (network.taier)：共 {regions} 个省级区域，{locations} 个城市节点")
    }

    fn taier_catalog_carriers_legend(&self) -> &'static str {
        "运营商简写：ct = 电信，cu = 联通，cm = 移动，edu = 教育网，cbn = 广电，other = 其他"
    }

    fn taier_region_kind(&self, code: &str) -> &'static str {
        match code {
            "municipality" => "直辖市",
            "province" => "省份/自治区",
            _ => "境外/其他",
        }
    }

    fn taier_catalog_overview_headers(&self) -> [&'static str; 5] {
        ["区域", "类型", "城市数", "运营商", "--taier 选择器示例"]
    }

    fn taier_catalog_footer_hint(&self) -> &'static str {
        "使用 --taier-list <区域>（如 --taier-list guangdong）可查看该区域下的城市列表。"
    }

    fn taier_catalog_region_heading(&self, name: &str, municipality: bool, count: usize) -> String {
        if municipality {
            format!("{name}（直辖市，{count} 个节点地区）：")
        } else {
            format!("{name}（{count} 个节点地区）：")
        }
    }

    fn taier_catalog_city_headers(&self) -> [&'static str; 3] {
        ["选择器", "城市", "可用运营商（候选节点数）"]
    }

    fn taier_no_node(&self, unknown: &[String]) -> String {
        if unknown.is_empty() {
            "泰尔节点列表中没有匹配所选条件的运营商节点".to_owned()
        } else {
            format!("泰尔节点列表中没有匹配 {} 的节点", unknown.join(", "))
        }
    }

    fn taier_no_result(&self) -> String {
        "所有中国三网测速节点均未返回有效速率".to_owned()
    }
}

impl CloudflareWords for SimplifiedChinese {
    fn cloudflare_method(&self, choice: &CloudflareChoice) -> String {
        let mut method = format!(
            "Cloudflare 测速：{} 并发工作数，单向 {} 秒，HTTPS",
            choice.concurrency, choice.seconds
        );
        if choice.udp {
            method.push_str(&format!(" + STUN UDP（{} 个探测包）", choice.udp_packets));
        } else {
            method.push_str("；跳过 STUN UDP 探测");
        }
        if choice.custom_endpoints {
            method.push_str("；自定义接口地址");
        }
        method
    }

    fn cloudflare_headers(&self) -> [&'static str; 3] {
        ["指标", "IPv4", "IPv6"]
    }

    fn cloudflare_field(&self, field: CloudflareField) -> &'static str {
        match field {
            CloudflareField::Server => "边缘节点",
            CloudflareField::ClientIp => "客户端 IP",
            CloudflareField::ClientAsn => "客户端 ASN",
            CloudflareField::IdleLatency => "空载延迟",
            CloudflareField::Download => "下载吞吐",
            CloudflareField::DownloadLatency => "下载负载延迟",
            CloudflareField::Upload => "上传吞吐",
            CloudflareField::UploadLatency => "上传负载延迟",
            CloudflareField::UdpLoss => "UDP 丢包率",
            CloudflareField::Quality => "链路质量评级",
        }
    }

    fn cloudflare_latency_cell(
        &self,
        median: &str,
        min: &str,
        max: &str,
        jitter: Option<&str>,
        loss_pct: f64,
    ) -> String {
        let mut text = format!("{median} (最小 {min}, 最大 {max}");
        if let Some(jit) = jitter {
            text.push_str(&format!(", 抖动 {jit}"));
        }
        if loss_pct > 0.0 {
            text.push_str(&format!(", 丢包 {loss_pct:.1}%"));
        }
        text.push(')');
        text
    }

    fn cloudflare_speed_cell(&self, speed: &str, p50_mbps: f64, p95_mbps: f64) -> String {
        format!("{speed} (p50 {p50_mbps:.1}, p95 {p95_mbps:.1} Mbit/s)")
    }

    fn cloudflare_udp_quality(&self, code: &str) -> &'static str {
        match code {
            "excellent" => "极佳",
            "good" => "良好",
            "acceptable" => "尚可",
            "poor" => "较差",
            "bad" => "恶劣",
            _ => "未知",
        }
    }

    fn cloudflare_udp_cell(
        &self,
        loss_pct: f64,
        received: u64,
        sent: u64,
        reordered_pct: f64,
        mos: Option<f64>,
        quality: &str,
    ) -> String {
        let mut text = format!("{loss_pct:.1}% ({received}/{sent}");
        if reordered_pct > 0.0 {
            text.push_str(&format!(", 乱序 {reordered_pct:.1}%"));
        }
        if let Some(m) = mos {
            text.push_str(&format!(", MOS {m:.2}"));
        }
        text.push_str(&format!(", {quality})"));
        text
    }

    fn cloudflare_quality_cell(
        &self,
        bloat_grade: &str,
        bloat_ms: Option<f64>,
        stab_grade: &str,
        stab_cv: Option<f64>,
    ) -> String {
        let bloat = match bloat_ms {
            Some(ms) => format!("缓冲膨胀 {bloat_grade} (+{ms:.1} ms)"),
            None => format!("缓冲膨胀 {bloat_grade}"),
        };
        let stab = match stab_cv {
            Some(cv) => format!("稳定性 {stab_grade} (CV {cv:.1}%)"),
            None => format!("稳定性 {stab_grade}"),
        };
        format!("{bloat}，{stab}")
    }

    fn cloudflare_issue(&self, issue: CloudflareIssueRef<'_>) -> String {
        match issue {
            CloudflareIssueRef::EmptyDownload => "下载字节数为 0".to_owned(),
            CloudflareIssueRef::EmptyUpload => "上传字节数为 0".to_owned(),
            CloudflareIssueRef::NoAddress => "无对应协议栈解析地址".to_owned(),
            CloudflareIssueRef::TimedOut => "请求超时".to_owned(),
            CloudflareIssueRef::NoResolver => "无可用 DNS 解析器".to_owned(),
            CloudflareIssueRef::Transport(why) => why.to_owned(),
        }
    }

    fn cloudflare_udp_note(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} STUN UDP 探测不可用：{reason}")
    }

    fn cloudflare_family_failed(&self, ipv6: bool, reason: &str) -> String {
        let fam = if ipv6 { "IPv6" } else { "IPv4" };
        format!("{fam} Cloudflare 测速失败：{reason}")
    }

    fn cloudflare_no_result(&self, reason: &str) -> String {
        if reason.is_empty() {
            "Cloudflare 测速未产生有效结果".to_owned()
        } else {
            format!("Cloudflare 测速失败：{reason}")
        }
    }
}

impl InetspeedWords for SimplifiedChinese {
    fn inetspeed_method(&self, choice: &InetspeedChoice) -> String {
        let rounds = if choice.threads <= 1 {
            "单线程".to_owned()
        } else {
            format!("单线程与 {} 线程", choice.threads)
        };
        let mut method = format!(
            "Apple CDN (iNetSpeed)：{rounds}，上限 {} / {} 秒，{} 次空载采样",
            choice.max, choice.timeout_secs, choice.latency_count
        );
        if choice.forced_endpoint {
            method.push_str("；固定指定节点");
        }
        if choice.custom_endpoints {
            method.push_str("；自定义接口地址");
        }
        method
    }

    fn inetspeed_endpoint_label(&self) -> &'static str {
        "优选边缘节点"
    }

    fn inetspeed_endpoint_value(
        &self,
        host: &str,
        ip: Option<&str>,
        rtt_ms: Option<f64>,
        source: &str,
    ) -> String {
        let src = self.inetspeed_source(source);
        match (ip, rtt_ms) {
            (Some(addr), Some(rtt)) => format!("{addr} ({host}，{rtt:.2} ms，来自 {src})"),
            (Some(addr), None) => format!("{addr} ({host}，来自 {src})"),
            (None, _) if host.is_empty() => format!("未绑定 IP ({src})"),
            (None, _) => format!("{host} (未绑定 IP，{src})"),
        }
    }

    fn inetspeed_source(&self, code: &str) -> &'static str {
        match code {
            "user" => "手动固定",
            "doh" => "双 DoH 优选",
            "system_dns" => "系统 DNS",
            _ => "默认 DNS",
        }
    }

    fn inetspeed_idle_label(&self) -> &'static str {
        "空载延迟"
    }

    fn inetspeed_latency_value(
        &self,
        median_ms: f64,
        min_ms: f64,
        avg_ms: f64,
        max_ms: f64,
        jitter_ms: f64,
    ) -> String {
        format!(
            "中位 {median_ms:.2} ms（最小 {min_ms:.2} / 平均 {avg_ms:.2} / \
             最大 {max_ms:.2} ms，抖动 {jitter_ms:.2} ms）"
        )
    }

    fn inetspeed_total_label(&self) -> &'static str {
        "总传输流量"
    }

    fn inetspeed_headers(&self) -> [&'static str; 5] {
        ["测试轮次", "线程数", "传输速率", "数据量", "负载延迟"]
    }

    fn inetspeed_round_name(&self, download: bool, single: bool) -> &'static str {
        match (download, single) {
            (true, true) => "下载（单线程）",
            (true, false) => "下载（多线程）",
            (false, true) => "上传（单线程）",
            (false, false) => "上传（多线程）",
        }
    }

    fn inetspeed_loaded_cell(&self, median_ms: f64, jitter_ms: f64) -> String {
        format!("{median_ms:.2} ms（抖动 {jitter_ms:.2} ms）")
    }

    fn inetspeed_round_issue(&self, issue: InetspeedRoundIssueRef<'_>) -> String {
        match issue {
            InetspeedRoundIssueRef::ZeroBytes => "传输完成但字节数为 0".to_owned(),
            InetspeedRoundIssueRef::WorkerFaults(count) => {
                format!("{count} 个工作线程发生传输错误")
            }
            InetspeedRoundIssueRef::Transport(fault) => inetspeed_fault_text(fault),
        }
    }

    fn inetspeed_warning(&self, warning: InetspeedWarningRef<'_>) -> String {
        match warning {
            InetspeedWarningRef::HostMismatch => {
                "下载、上传与延迟探测域名不一致，已禁用共享节点 IP 绑定".to_owned()
            }
            InetspeedWarningRef::ForcedProbeFailed(ip) => {
                format!("指定节点 {ip} 预探测失败，继续按该 IP 尝试测速")
            }
            InetspeedWarningRef::DohEmpty => "双 DoH 未返回候选 IP，已回退至系统 DNS".to_owned(),
            InetspeedWarningRef::UnpinnedFallback => {
                "节点发现未获得可用 IP，继续使用非绑定系统 DNS".to_owned()
            }
            InetspeedWarningRef::NoIdleSamples => "未采集到有效的空载延迟样本".to_owned(),
            InetspeedWarningRef::IdleFailed(fault) => {
                format!("空载延迟探测失败：{}", inetspeed_fault_text(fault))
            }
        }
    }

    fn inetspeed_invalid_max(&self, raw: &str) -> String {
        format!("无效的流量上限 {raw:?}：应为如 2G、500M 或 1GiB 的容量格式")
    }

    fn inetspeed_no_result(&self) -> String {
        "Apple CDN 测速未能完成任何传输轮次".to_owned()
    }
}

fn inetspeed_fault_text(fault: InetspeedProbeFaultRef<'_>) -> String {
    match fault {
        InetspeedProbeFaultRef::EmptySample => "探测未返回样本".to_owned(),
        InetspeedProbeFaultRef::NoAddress => "无对应协议栈地址".to_owned(),
        InetspeedProbeFaultRef::TimedOut => "请求超时".to_owned(),
        InetspeedProbeFaultRef::NoResolver => "无可用 DNS 解析器".to_owned(),
        InetspeedProbeFaultRef::Transport(why) => why.to_owned(),
    }
}
