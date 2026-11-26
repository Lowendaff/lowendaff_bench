//! 日本語：レポート枠組み・セクション見出し・共通語句・VM CPU テレメトリ。

use bench_api::Net;
use bench_tool::job;

use super::Japanese;
use crate::locale::ReportWords;
use crate::locale::Totals;
use crate::locale::Unit;
use crate::orchestrator::Fault;
use crate::orchestrator::Unmet;

impl ReportWords for Japanese {
    fn label_width(&self) -> usize {
        20
    }

    fn section(&self, title: &str) -> String {
        format!("[{title}]")
    }

    fn system_title(&self) -> String {
        "システム概要".to_owned()
    }

    fn summary_title(&self) -> String {
        "実行サマリー".to_owned()
    }

    fn group_title(&self, group: &str) -> Option<&'static str> {
        match group {
            "cpu" => Some("CPU 性能"),
            "memory" => Some("メモリ性能"),
            "disk" => Some("ディスク診断・速度"),
            "network" => Some("ネットワーク・経路"),
            "ip" => Some("IP 品質・動画配信"),
            _ => None,
        }
    }

    fn item_name(&self, id: &str) -> Option<&'static str> {
        match id {
            "cpu.bench" => Some("CPU 統合ベンチマーク"),
            "memory.sysbench" => Some("メモリ連続読み書き"),
            "memory.stream" => Some("メモリ実効帯域幅"),
            "memory.latency" => Some("キャッシュ・メモリ遅延"),
            "memory.numa" => Some("マルチ CPU メモリ速度"),
            "disk.health" => Some("ディスク健康診断"),
            "disk.fio" => Some("ディスク読み書き速度"),
            "network.iperf3" => Some("グローバル回線速度"),
            "network.taier" => Some("中国キャリア回線速度"),
            "network.cloudflare" => Some("Cloudflare 回線速度"),
            "network.inetspeed" => Some("Apple CDN 回線速度"),
            "network.tcping" => Some("TCP レイテンシ"),
            "network.trace" => Some("ネットワーク経路追跡"),
            "network.bgp" => Some("BGP 経路トポロジー"),
            "ip.quality" => Some("IP 品質・リスク診断"),
            "ip.media" => Some("動画配信サービス判定"),
            _ => None,
        }
    }

    fn environment(&self, platform: &str) -> String {
        format!("実行環境：{platform}")
    }

    fn note(&self) -> String {
        "注記".to_owned()
    }

    fn unavailable(&self) -> String {
        "取得不可".to_owned()
    }

    fn unavailable_because(&self, why: &str) -> String {
        if why.is_empty() {
            return self.unavailable();
        }
        format!("取得不可：{why}")
    }

    fn unknown(&self) -> String {
        "不明".to_owned()
    }

    fn and(&self, items: &[String]) -> String {
        items.join("、")
    }

    fn completed(&self, seconds: f64) -> String {
        format!("完了（{seconds:.1} s）")
    }

    fn skipped(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "スキップ".to_owned();
        }
        format!("スキップ：{reason}")
    }

    fn failed(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "失敗".to_owned();
        }
        format!("失敗：{reason}")
    }

    fn timed_out(&self) -> String {
        "タイムアウト；そこまでの測定結果を保持しています".to_owned()
    }

    fn interrupted(&self) -> String {
        "中断".to_owned()
    }

    fn unmet(&self, unmet: &Unmet) -> String {
        match unmet {
            Unmet::Glibc {
                needed,
                found: Some(found),
            } => format!("glibc {needed} 以上が必要（このホストは {found}）"),
            Unmet::Glibc {
                needed,
                found: None,
            } => format!("glibc {needed} 以上が必要（このホストには glibc がありません）"),
            Unmet::Network(Net::Ipv4) => "IPv4 接続が必要".to_owned(),
            Unmet::Network(Net::Ipv6) => "IPv6 接続が必要".to_owned(),
            Unmet::Network(Net::Any | Net::None) => "ネットワーク接続が必要".to_owned(),
            Unmet::Tool { name, why } if why.is_empty() => format!("ツール {name} が利用不可"),
            Unmet::Tool { name, why } => format!("ツール {name} が利用不可：{why}"),
            Unmet::File { name, why } if why.is_empty() => format!("ファイル {name} が利用不可"),
            Unmet::File { name, why } => format!("ファイル {name} が利用不可：{why}"),
        }
    }

    fn fault(&self, fault: &Fault) -> String {
        match fault {
            Fault::Panic(message) if message.trim().is_empty() => {
                format!(
                    "プラグインがクラッシュしました：{}",
                    crate::locale::ProgramWords::panic_without_text(self)
                )
            }
            Fault::Panic(message) => format!("プラグインがクラッシュしました：{message}"),
            Fault::WorkDir(error) => format!("作業ディレクトリを作成できません：{error}"),
            Fault::Aborted(error) => format!("タスクが異常終了しました：{error}"),
        }
    }

    fn not_applicable(&self, names: &[String]) -> String {
        format!("このホストでは対象外：{}", names.join("、"))
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
            "計 {items} 項目：{completed} 完了、{skipped} スキップ、{failed} 失敗、\
             {timed_out} タイムアウト、{interrupted} 中断。合計時間 {seconds:.1} s"
        )
    }

    fn tool_fault(&self, tool: &str, fault: &job::Fault, unit: Unit) -> String {
        let quoted = |last: Option<&str>| {
            last.map(|s| crate::report::terminal::strip_terminal_controls(s, false))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "出力なし".to_owned())
        };
        match fault {
            job::Fault::Spawn(error) => format!("{tool} を起動できません：{error}"),
            job::Fault::Read(error) => format!("{tool} の出力を読み取れません：{error}"),
            job::Fault::TimedOut(limit) => {
                let secs = limit.as_secs();
                let limit = match unit {
                    Unit::Minutes if secs >= 60 && secs.is_multiple_of(60) => {
                        format!("{} 分", secs / 60)
                    }
                    Unit::Seconds | Unit::Minutes => format!("{secs} s"),
                };
                format!("{limit} 経過しても終了しないため停止")
            }
            job::Fault::Exit { code, last } => {
                format!("{tool} 終了コード {code}：{}", quoted(last.as_deref()))
            }
            job::Fault::Signal { last } => {
                format!("{tool} がシグナルにより終了：{}", quoted(last.as_deref()))
            }
        }
    }

    fn retried(&self, run: &str, failures: usize, last: &str) -> String {
        format!("{run}：{failures} 回の失敗後に成功；直前の理由：{last}")
    }

    fn every_attempt_failed(&self, run: &str, last: &str, attempts: usize) -> String {
        format!("{run}：{}", self.attempts(last, attempts))
    }

    fn attempts(&self, last: &str, attempts: usize) -> String {
        format!("{last}（計 {attempts} 回試行）")
    }

    fn bytes(&self, count: u64) -> String {
        format!("{count} バイト")
    }

    fn md_kv_headers(&self) -> [&'static str; 2] {
        ["項目", "値"]
    }

    fn vm_cpu_title(&self) -> &'static str {
        "CPU スティール・ソフト割り込み"
    }

    fn vm_cpu_method(&self, interval_secs: u64) -> String {
        format!(
            "開始から終了まで {interval_secs} 秒ごとに /proc/stat を記録：\
             ソフト割り込み（%si）とハイパーバイザースティール（%st）"
        )
    }

    fn vm_cpu_summary_title(&self) -> &'static str {
        "集計"
    }

    fn vm_cpu_summary_headers(&self) -> [&'static str; 5] {
        ["指標", "平均", "P95", "最大", "最大発生タイミング"]
    }

    fn vm_cpu_metric_label(&self, steal: bool) -> &'static str {
        if steal {
            "スティール (%st)"
        } else {
            "ソフト割込 (%si)"
        }
    }

    fn vm_cpu_peak_cell(&self, item: &str, time: &str) -> String {
        format!("{item} ({time})")
    }

    fn vm_cpu_samples_title(&self) -> &'static str {
        "サンプル一覧"
    }

    fn vm_cpu_timeline_title(&self) -> &'static str {
        "タイムライン"
    }

    fn vm_cpu_chart_title(&self) -> &'static str {
        "CPU スティール (%st) とソフト割り込み (%si)"
    }

    fn vm_cpu_y_axis_title(&self) -> &'static str {
        "CPU (%)"
    }

    fn vm_cpu_samples_headers(&self) -> [&'static str; 4] {
        ["経過時間", "実行項目", "%si", "%st"]
    }

    fn vm_cpu_setup_phase(&self) -> &'static str {
        "準備フェーズ"
    }
}
