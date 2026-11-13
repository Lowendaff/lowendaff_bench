//! Chinese terminal output: one live section per plugin, columns aligned by
//! display width so CJK and ASCII line up.

use std::io::Write;
use std::iter;
use std::time::Duration;
use std::time::Instant;

use bench_api::Descriptor;
use bench_api::Platform;
use bench_api::Record;
use bench_api::Table;
use bench_api::ToolSpec;

use crate::orchestrator::Observer;
use crate::orchestrator::Outcome;
use crate::orchestrator::Run;
use crate::provision::Provisioned;

const LABEL_WIDTH: usize = 16;

/// Renders a run to a terminal (or any writer, for tests).
pub struct Terminal<W: Write> {
    out: W,
    started: Option<Instant>,
}

impl<W: Write> Terminal<W> {
    /// Renders into `out`.
    pub fn new(out: W) -> Self {
        Self { out, started: None }
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

    /// The closing line with a count per outcome.
    pub fn summary(&mut self, runs: &[Run], total: Duration) {
        let count =
            |pick: fn(&Outcome) -> bool| runs.iter().filter(|run| pick(&run.outcome)).count();
        let completed = count(|outcome| matches!(outcome, Outcome::Completed));
        let skipped = count(|outcome| matches!(outcome, Outcome::Skipped { .. }));
        let failed = count(|outcome| matches!(outcome, Outcome::Failed { .. }));
        let timed_out = count(|outcome| matches!(outcome, Outcome::TimedOut));
        let cancelled = count(|outcome| matches!(outcome, Outcome::Cancelled));
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

impl<W: Write> Observer for Terminal<W> {
    fn on_start(&mut self, plugin: &Descriptor) {
        self.started = Some(Instant::now());
        self.write(&format!("\n【{}】", plugin.title));
    }

    fn on_record(&mut self, _plugin: &Descriptor, record: &Record) {
        match record {
            Record::Fact { label, value, .. } => self.field(label, value),
            Record::Metric { label, result, .. } => self.field(label, &result.to_string()),
            Record::Table(table) => self.table(table),
        }
    }

    fn on_finish(&mut self, plugin: &Descriptor, outcome: &Outcome) {
        let Some(started) = self.started.take() else {
            // Skipped or never started: the section still appears, so nothing goes missing.
            self.write(&format!("\n【{}】", plugin.title));
            self.write(&format!("  {}", outcome_text(outcome, None)));
            return;
        };
        self.write(&format!(
            "  {}",
            outcome_text(outcome, Some(started.elapsed()))
        ));
    }
}

fn outcome_text(outcome: &Outcome, elapsed: Option<Duration>) -> String {
    match outcome {
        Outcome::Completed => match elapsed {
            Some(elapsed) => format!("完成，用时 {:.1} 秒", elapsed.as_secs_f64()),
            None => "完成".to_owned(),
        },
        Outcome::Skipped { reason } => format!("跳过：{reason}"),
        Outcome::Failed { error } => format!("失败：{error}"),
        Outcome::TimedOut => "超时，已保留超时前的结果".to_owned(),
        Outcome::Cancelled => "已中断".to_owned(),
    }
}

/// Pads to a display width; CJK characters count as two columns.
fn pad(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let fill = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(fill))
}
