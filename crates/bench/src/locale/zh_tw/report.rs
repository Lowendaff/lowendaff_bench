//! 繁體中文：報告骨架、章節標題、狀態彙總與虛擬機 CPU 遙測。

use bench_api::Net;
use bench_tool::job;

use super::TraditionalChinese;
use crate::locale::ReportWords;
use crate::locale::Totals;
use crate::locale::Unit;
use crate::orchestrator::Fault;
use crate::orchestrator::Unmet;

impl ReportWords for TraditionalChinese {
    fn label_width(&self) -> usize {
        18
    }

    fn section(&self, title: &str) -> String {
        format!("[{title}]")
    }

    fn system_title(&self) -> String {
        "系統資訊".to_owned()
    }

    fn summary_title(&self) -> String {
        "執行彙總".to_owned()
    }

    fn group_title(&self, group: &str) -> Option<&'static str> {
        match group {
            "cpu" => Some("CPU 效能"),
            "memory" => Some("記憶體效能"),
            "disk" => Some("磁碟檢測與效能"),
            "network" => Some("網路測速與路由"),
            "ip" => Some("IP 品質與串流媒體"),
            _ => None,
        }
    }

    fn item_name(&self, id: &str) -> Option<&'static str> {
        match id {
            "cpu.bench" => Some("CPU 綜合基準"),
            "memory.sysbench" => Some("記憶體連續讀寫"),
            "memory.stream" => Some("記憶體持續頻寬"),
            "memory.latency" => Some("快取與記憶體延遲"),
            "memory.numa" => Some("多路記憶體互通"),
            "disk.health" => Some("硬碟健康檢測"),
            "disk.fio" => Some("硬碟隨機讀寫"),
            "network.iperf3" => Some("全球頻寬測速"),
            "network.taier" => Some("中國三網測速"),
            "network.cloudflare" => Some("Cloudflare 測速"),
            "network.inetspeed" => Some("Apple CDN 測速"),
            "network.tcping" => Some("TCP 握手延遲"),
            "network.trace" => Some("去程路由追蹤"),
            "network.bgp" => Some("BGP 路由拓撲"),
            "ip.quality" => Some("IP 品質檢測"),
            "ip.media" => Some("串流媒體解鎖"),
            _ => None,
        }
    }

    fn environment(&self, platform: &str) -> String {
        format!("執行環境：{platform}")
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
        format!("已完成（耗時 {seconds:.1} s）")
    }

    fn skipped(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "已略過".to_owned();
        }
        format!("已略過：{reason}")
    }

    fn failed(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "失敗".to_owned();
        }
        format!("失敗：{reason}")
    }

    fn timed_out(&self) -> String {
        "執行逾時；已保留逾時前完成的測量結果".to_owned()
    }

    fn interrupted(&self) -> String {
        "已中斷".to_owned()
    }

    fn unmet(&self, unmet: &Unmet) -> String {
        match unmet {
            Unmet::Glibc {
                needed,
                found: Some(found),
            } => format!("需要 glibc {needed} 或更新版本（目前主機為 {found}）"),
            Unmet::Glibc {
                needed,
                found: None,
            } => format!("需要 glibc {needed} 或更新版本（目前主機未安裝 glibc）"),
            Unmet::Network(Net::Ipv4) => "需要 IPv4 網路連線".to_owned(),
            Unmet::Network(Net::Ipv6) => "需要 IPv6 網路連線".to_owned(),
            Unmet::Network(Net::Any | Net::None) => "需要公網連線".to_owned(),
            Unmet::Tool { name, why } if why.is_empty() => format!("外部工具 {name} 不可用"),
            Unmet::Tool { name, why } => format!("外部工具 {name} 不可用：{why}"),
            Unmet::File { name, why } if why.is_empty() => format!("資料檔案 {name} 不可用"),
            Unmet::File { name, why } => format!("資料檔案 {name} 不可用：{why}"),
        }
    }

    fn fault(&self, fault: &Fault) -> String {
        match fault {
            Fault::Panic(message) if message.trim().is_empty() => {
                format!(
                    "外掛內部崩潰：{}",
                    crate::locale::ProgramWords::panic_without_text(self)
                )
            }
            Fault::Panic(message) => format!("外掛內部崩潰：{message}"),
            Fault::WorkDir(error) => format!("無法建立工作目錄：{error}"),
            Fault::Aborted(error) => format!("任務異常終止：{error}"),
        }
    }

    fn not_applicable(&self, names: &[String]) -> String {
        format!("目前環境不適用：{}", names.join("、"))
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
            "共 {items} 項：{completed} 完成，{skipped} 略過，{failed} 失敗，\
             {timed_out} 逾時，{interrupted} 中斷。總耗時 {seconds:.1} s"
        )
    }

    fn tool_fault(&self, tool: &str, fault: &job::Fault, unit: Unit) -> String {
        let quoted = |last: Option<&str>| {
            last.map(|s| crate::report::terminal::strip_terminal_controls(s, false))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "無輸出".to_owned())
        };
        match fault {
            job::Fault::Spawn(error) => format!("無法執行 {tool}：{error}"),
            job::Fault::Read(error) => format!("無法讀取 {tool} 的輸出：{error}"),
            job::Fault::TimedOut(limit) => {
                let secs = limit.as_secs();
                let limit = match unit {
                    Unit::Minutes if secs >= 60 && secs.is_multiple_of(60) => {
                        format!("{} 分鐘", secs / 60)
                    }
                    Unit::Seconds | Unit::Minutes => format!("{secs} s"),
                };
                format!("執行滿 {limit} 仍未結束，已終止")
            }
            job::Fault::Exit { code, last } => {
                format!("{tool} 結束碼 {code}：{}", quoted(last.as_deref()))
            }
            job::Fault::Signal { last } => {
                format!("{tool} 被訊號終止：{}", quoted(last.as_deref()))
            }
        }
    }

    fn retried(&self, run: &str, failures: usize, last: &str) -> String {
        format!("{run}：經歷 {failures} 次失敗後重試成功；上次原因：{last}")
    }

    fn every_attempt_failed(&self, run: &str, last: &str, attempts: usize) -> String {
        format!("{run}：{}", self.attempts(last, attempts))
    }

    fn attempts(&self, last: &str, attempts: usize) -> String {
        format!("{last}（共嘗試 {attempts} 次）")
    }

    fn bytes(&self, count: u64) -> String {
        format!("{count} 位元組")
    }

    fn md_kv_headers(&self) -> [&'static str; 2] {
        ["項目", "數值"]
    }

    fn vm_cpu_title(&self) -> &'static str {
        "CPU 爭用與軟中斷"
    }

    fn vm_cpu_method(&self, interval_secs: u64) -> String {
        format!(
            "測試全程每 {interval_secs} 秒取樣一次 /proc/stat：\
             軟中斷（%si）與宿主機 CPU 爭用（%st）"
        )
    }

    fn vm_cpu_summary_title(&self) -> &'static str {
        "統計彙總"
    }

    fn vm_cpu_summary_headers(&self) -> [&'static str; 5] {
        ["指標", "平均值", "P95", "峰值", "峰值出現時段"]
    }

    fn vm_cpu_metric_label(&self, steal: bool) -> &'static str {
        if steal {
            "宿主機爭用 (%st)"
        } else {
            "軟中斷 (%si)"
        }
    }

    fn vm_cpu_peak_cell(&self, item: &str, time: &str) -> String {
        format!("{item} ({time})")
    }

    fn vm_cpu_samples_title(&self) -> &'static str {
        "取樣明細"
    }

    fn vm_cpu_timeline_title(&self) -> &'static str {
        "時間軸"
    }

    fn vm_cpu_chart_title(&self) -> &'static str {
        "CPU 爭用 (%st) 與軟中斷 (%si)"
    }

    fn vm_cpu_y_axis_title(&self) -> &'static str {
        "CPU (%)"
    }

    fn vm_cpu_samples_headers(&self) -> [&'static str; 4] {
        ["時間", "階段 / 項目", "%si", "%st"]
    }

    fn vm_cpu_setup_phase(&self) -> &'static str {
        "準備階段"
    }
}
