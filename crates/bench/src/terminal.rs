//! Chinese terminal output: one section per plugin, columns aligned by display width so that
//! CJK and ASCII line up.

use std::io::Write;
use std::iter;
use std::time::Duration;

use bench_api::Net;
use bench_api::Record;
use bench_api::Table;
use bench_api::ToolSpec;
use bench_api::typed::Stop;
use bench_host::platform::Platform;

use crate::orchestrator::Fault;
use crate::orchestrator::Run;
use crate::orchestrator::Status;
use crate::orchestrator::Unmet;
use crate::provision::Provisioned;
use crate::results::Measured;

const LABEL_WIDTH: usize = 16;

/// Renders a run to a terminal (or any writer, for tests).
pub struct Terminal<W: Write> {
    out: W,
}

impl<W: Write> Terminal<W> {
    /// Renders into `out`.
    pub fn new(out: W) -> Self {
        Self { out }
    }

    /// Returns the writer, e.g. to inspect the output in tests.
    pub fn into_inner(self) -> W {
        self.out
    }

    /// First line of the output.
    pub fn header(&mut self, version: &str) {
        self.write(&format!("lowendaff-bench {version}"));
    }

    /// The detected platform, below the header.
    pub fn platform(&mut self, platform: Platform) {
        self.write(&format!("运行环境：{platform}"));
    }

    /// Test items that do not apply to this platform in any variant; nothing when empty.
    pub fn not_applicable(&mut self, titles: &[&str]) {
        if !titles.is_empty() {
            self.write(&format!("本机不适用：{}", titles.join("、")));
        }
    }

    /// The 【准备工具】 section: one line per tool with its result.
    pub fn tools(&mut self, tools: &[ToolSpec], provisioned: &Provisioned) {
        self.write("\n【准备工具】");
        for tool in tools {
            let result = match provisioned.failed.get(tool.name) {
                Some(reason) => format!("失败：{reason}"),
                None => "就绪".to_owned(),
            };
            self.field(&format!("{} {}", tool.name, tool.version), &result);
        }
    }

    /// A plugin's section: its title, what it left, and how it ended.
    pub fn run(&mut self, run: &Run<Measured>) {
        self.write(&format!("\n【{}】", run.title.unwrap_or(run.id)));
        let reason = match &run.result {
            Some(Measured::Legacy(report)) => {
                for record in report.output.iter().flatten() {
                    match record {
                        Record::Fact { label, value, .. } => self.field(label, value),
                        Record::Metric { label, result, .. } => {
                            self.field(label, &result.to_string());
                        }
                        Record::Table(table) => self.table(table),
                    }
                }
                match &report.stop {
                    Some(Stop::Skipped(reason) | Stop::Failed(reason)) => Some(reason.as_str()),
                    None => None,
                }
            }
            None => None,
        };
        self.write(&format!("  {}", status_text(run, reason)));
    }

    /// The closing line with a count per status.
    pub fn summary(&mut self, runs: &[Run<Measured>], total: Duration) {
        let count = |pick: fn(&Status) -> bool| runs.iter().filter(|run| pick(&run.status)).count();
        let completed = count(|status| matches!(status, Status::Completed));
        let skipped = count(|status| matches!(status, Status::Skipped(_)));
        let failed = count(|status| matches!(status, Status::Failed(_)));
        let timed_out = count(|status| matches!(status, Status::TimedOut));
        let cancelled = count(|status| matches!(status, Status::Cancelled));
        self.write(&format!(
            "\n共 {} 项：完成 {completed}，跳过 {skipped}，失败 {failed}，超时 {timed_out}，\
             中断 {cancelled}。总用时 {:.1} 秒",
            runs.len(),
            total.as_secs_f64()
        ));
    }

    fn field(&mut self, label: &str, value: &str) {
        self.write(&format!("  {}  {value}", pad(label, LABEL_WIDTH)));
    }

    fn table(&mut self, table: &Table) {
        use unicode_width::UnicodeWidthStr;

        let mut widths: Vec<usize> = table
            .headers()
            .iter()
            .map(|header| header.width())
            .collect();
        for row in table.rows() {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.width());
            }
        }
        self.write(&format!("  {}", table.title()));
        let lines = iter::once(table.headers()).chain(table.rows().iter().map(Vec::as_slice));
        for row in lines {
            let last = row.len().saturating_sub(1);
            let cells: Vec<String> = row
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(index, (cell, width))| {
                    if index == last {
                        cell.clone()
                    } else {
                        pad(cell, *width)
                    }
                })
                .collect();
            self.write(&format!("  {}", cells.join("  ")));
        }
    }

    fn write(&mut self, line: &str) {
        // Output is best effort: a closed stdout must not stop the run.
        let _ = writeln!(self.out, "{line}");
    }
}

/// How a plugin ended, as its section's last line says; `reason` is the plugin's own.
#[must_use]
pub fn status_text<R>(run: &Run<R>, reason: Option<&str>) -> String {
    let reason = reason.unwrap_or_default();
    match &run.status {
        Status::Completed => match run.elapsed {
            Some(elapsed) => format!("完成，用时 {:.1} 秒", elapsed.as_secs_f64()),
            None => "完成".to_owned(),
        },
        Status::Skipped(Some(unmet)) => format!("跳过：{}", unmet_text(unmet)),
        Status::Skipped(None) => format!("跳过：{reason}"),
        Status::Failed(Some(fault)) => format!("失败：{}", fault_text(fault)),
        Status::Failed(None) => format!("失败：{reason}"),
        Status::TimedOut => "超时，已保留超时前的结果".to_owned(),
        Status::Cancelled => "已中断".to_owned(),
    }
}

/// A requirement that the host does not meet, in Chinese.
#[must_use]
pub fn unmet_text(unmet: &Unmet) -> String {
    match unmet {
        Unmet::Glibc {
            needed,
            found: Some(found),
        } => format!("需要 glibc {needed} 或更新，本机是 {found}"),
        Unmet::Glibc {
            needed,
            found: None,
        } => format!("需要 glibc {needed} 或更新，本机没有 glibc"),
        Unmet::Network(Net::Ipv4) => "需要 IPv4 网络连接".to_owned(),
        Unmet::Network(Net::Ipv6) => "需要 IPv6 网络连接".to_owned(),
        Unmet::Network(Net::Any | Net::None) => "需要网络连接".to_owned(),
        Unmet::Tool { name, .. } => format!("工具 {name} 不可用"),
    }
}

/// A fault around a plugin, in Chinese.
#[must_use]
pub fn fault_text(fault: &Fault) -> String {
    match fault {
        Fault::Panic(message) => format!("插件崩溃：{message}"),
        Fault::WorkDir(error) => format!("无法创建工作目录：{error}"),
        Fault::Aborted(error) => format!("任务异常结束：{error}"),
    }
}

/// Pads to a display width; CJK characters count as two columns.
fn pad(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let fill = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(fill))
}
