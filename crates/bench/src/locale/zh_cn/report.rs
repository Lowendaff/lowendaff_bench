//! 简体中文：报告骨架、章节标题、状态汇总与虚拟机 CPU 遥测。

use bench_api::Net;
use bench_tool::job;

use super::SimplifiedChinese;
use crate::locale::ReportWords;
use crate::locale::Totals;
use crate::locale::Unit;
use crate::orchestrator::Fault;
use crate::orchestrator::Unmet;

impl ReportWords for SimplifiedChinese {
    fn label_width(&self) -> usize {
        18
    }

    fn section(&self, title: &str) -> String {
        format!("[{title}]")
    }

    fn system_title(&self) -> String {
        "系统信息".to_owned()
    }

    fn summary_title(&self) -> String {
        "执行汇总".to_owned()
    }

    fn group_title(&self, group: &str) -> Option<&'static str> {
        match group {
            "cpu" => Some("CPU 性能"),
            "memory" => Some("内存性能"),
            "disk" => Some("磁盘体检与性能"),
            "network" => Some("网络测速与路由"),
            "ip" => Some("IP 质量与流媒体"),
            _ => None,
        }
    }

    fn item_name(&self, id: &str) -> Option<&'static str> {
        match id {
            "cpu.bench" => Some("CPU 综合基准"),
            "memory.sysbench" => Some("内存连续读写"),
            "memory.stream" => Some("内存持续带宽"),
            "memory.latency" => Some("缓存与内存延迟"),
            "memory.numa" => Some("多路内存互通"),
            "disk.health" => Some("硬盘健康体检"),
            "disk.fio" => Some("硬盘随机读写"),
            "network.iperf3" => Some("全球带宽测速"),
            "network.taier" => Some("中国三网测速"),
            "network.cloudflare" => Some("Cloudflare 测速"),
            "network.inetspeed" => Some("Apple CDN 测速"),
            "network.trace" => Some("去程路由追踪"),
            "network.bgp" => Some("BGP 路由拓扑"),
            "ip.quality" => Some("IP 质量体检"),
            "ip.media" => Some("流媒体解锁"),
            _ => None,
        }
    }

    fn environment(&self, platform: &str) -> String {
        format!("运行环境：{platform}")
    }

    fn note(&self) -> String {
        "提示".to_owned()
    }

    fn unavailable(&self) -> String {
        "不可用".to_owned()
    }

    fn unavailable_because(&self, why: &str) -> String {
        if why.is_empty() {
            return self.unavailable();
        }
        format!("不可用：{why}")
    }

    fn unknown(&self) -> String {
        "未知".to_owned()
    }

    fn and(&self, items: &[String]) -> String {
        items.join("、")
    }

    fn completed(&self, seconds: f64) -> String {
        format!("已完成（耗时 {seconds:.1} s）")
    }

    fn skipped(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "已跳过".to_owned();
        }
        format!("已跳过：{reason}")
    }

    fn failed(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "失败".to_owned();
        }
        format!("失败：{reason}")
    }

    fn timed_out(&self) -> String {
        "运行超时；已保留超时前完成的测量结果".to_owned()
    }

    fn interrupted(&self) -> String {
        "已中断".to_owned()
    }

    fn unmet(&self, unmet: &Unmet) -> String {
        match unmet {
            Unmet::Glibc {
                needed,
                found: Some(found),
            } => format!("需要 glibc {needed} 或更高版本（当前主机为 {found}）"),
            Unmet::Glibc {
                needed,
                found: None,
            } => format!("需要 glibc {needed} 或更高版本（当前主机未安装 glibc）"),
            Unmet::Network(Net::Ipv4) => "需要 IPv4 网络连接".to_owned(),
            Unmet::Network(Net::Ipv6) => "需要 IPv6 网络连接".to_owned(),
            Unmet::Network(Net::Any | Net::None) => "需要公网连接".to_owned(),
            Unmet::Tool { name, why } if why.is_empty() => format!("外部工具 {name} 不可用"),
            Unmet::Tool { name, why } => format!("外部工具 {name} 不可用：{why}"),
            Unmet::File { name, why } if why.is_empty() => format!("数据文件 {name} 不可用"),
            Unmet::File { name, why } => format!("数据文件 {name} 不可用：{why}"),
        }
    }

    fn fault(&self, fault: &Fault) -> String {
        match fault {
            Fault::Panic(message) if message.trim().is_empty() => {
                format!(
                    "插件内部崩溃：{}",
                    crate::locale::ProgramWords::panic_without_text(self)
                )
            }
            Fault::Panic(message) => format!("插件内部崩溃：{message}"),
            Fault::WorkDir(error) => format!("无法创建工作目录：{error}"),
            Fault::Aborted(error) => format!("任务异常终止：{error}"),
        }
    }

    fn not_applicable(&self, names: &[String]) -> String {
        format!("当前环境不适用：{}", names.join("、"))
    }

    fn totals(&self, totals: &Totals) -> String {
        let Totals {
            items,
            completed,
            skipped,
            failed,
            timed_out,
            interrupted,
            seconds,
        } = totals;
        format!(
            "共 {items} 项：{completed} 完成，{skipped} 跳过，{failed} 失败，\
             {timed_out} 超时，{interrupted} 中断。总耗时 {seconds:.1} s"
        )
    }

    fn tool_fault(&self, tool: &str, fault: &job::Fault, unit: Unit) -> String {
        let quoted = |last: Option<&str>| {
            last.map(|s| crate::report::terminal::strip_terminal_controls(s, false))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "无输出".to_owned())
        };
        match fault {
            job::Fault::Spawn(error) => format!("无法运行 {tool}：{error}"),
            job::Fault::Read(error) => format!("无法读取 {tool} 的输出：{error}"),
            job::Fault::TimedOut(limit) => {
                let secs = limit.as_secs();
                let limit = match unit {
                    Unit::Minutes if secs >= 60 && secs.is_multiple_of(60) => {
                        format!("{} 分钟", secs / 60)
                    }
                    Unit::Seconds | Unit::Minutes => format!("{secs} s"),
                };
                format!("运行满 {limit} 仍未结束，已终止")
            }
            job::Fault::Exit { code, last } => {
                format!("{tool} 退出码 {code}：{}", quoted(last.as_deref()))
            }
            job::Fault::Signal { last } => {
                format!("{tool} 被信号终止：{}", quoted(last.as_deref()))
            }
        }
    }

    fn retried(&self, run: &str, failures: usize, last: &str) -> String {
        format!("{run}：经历 {failures} 次失败后重试成功；上次原因：{last}")
    }

    fn every_attempt_failed(&self, run: &str, last: &str, attempts: usize) -> String {
        format!("{run}：{}", self.attempts(last, attempts))
    }

    fn attempts(&self, last: &str, attempts: usize) -> String {
        format!("{last}（共尝试 {attempts} 次）")
    }

    fn bytes(&self, count: u64) -> String {
        format!("{count} 字节")
    }

    fn md_kv_headers(&self) -> [&'static str; 2] {
        ["项目", "数值"]
    }

    fn vm_cpu_title(&self) -> &'static str {
        "CPU 争抢与软中断"
    }

    fn vm_cpu_method(&self, interval_secs: u64) -> String {
        format!(
            "测试全程每 {interval_secs} 秒采样一次 /proc/stat：\
             软中断（%si）与宿主机 CPU 争抢（%st）"
        )
    }

    fn vm_cpu_summary_title(&self) -> &'static str {
        "统计汇总"
    }

    fn vm_cpu_summary_headers(&self) -> [&'static str; 5] {
        ["指标", "平均值", "P95", "峰值", "峰值出现时段"]
    }

    fn vm_cpu_metric_label(&self, steal: bool) -> &'static str {
        if steal {
            "宿主机争抢 (%st)"
        } else {
            "软中断 (%si)"
        }
    }

    fn vm_cpu_peak_cell(&self, item: &str, time: &str) -> String {
        format!("{item} ({time})")
    }

    fn vm_cpu_samples_title(&self) -> &'static str {
        "采样明细"
    }

    fn vm_cpu_timeline_title(&self) -> &'static str {
        "时间线"
    }

    fn vm_cpu_chart_title(&self) -> &'static str {
        "CPU 争抢 (%st) 与软中断 (%si)"
    }

    fn vm_cpu_y_axis_title(&self) -> &'static str {
        "CPU (%)"
    }

    fn vm_cpu_samples_headers(&self) -> [&'static str; 4] {
        ["时间", "阶段 / 项目", "%si", "%st"]
    }

    fn vm_cpu_setup_phase(&self) -> &'static str {
        "准备阶段"
    }
}
