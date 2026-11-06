//! Chinese terminal output (design D-15): one live section per plugin, columns aligned by
//! display width so CJK and ASCII line up.

use std::io::Write;
use std::time::Duration;
use std::time::Instant;

use bench_api::Descriptor;
use bench_api::Measurement;
use bench_api::Record;
use bench_api::Table;
use bench_api::ToolSpec;
use unicode_width::UnicodeWidthStr;

use crate::orchestrator::Observer;
use crate::orchestrator::Outcome;
use crate::orchestrator::Run;
use crate::provision::Provisioned;

const LABEL_WIDTH: usize = 16;

pub struct Terminal<W: Write> {
    out: W,
    started: Option<Instant>,
}

impl<W: Write> Terminal<W> {
    pub fn new(out: W) -> Self {
        Self { out, started: None }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn header(&mut self, version: &str) {
        self.write(&format!("lowendaff-bench {version}"));
    }

    pub fn tools(&mut self, tools: &[ToolSpec], provisioned: &Provisioned) {
        self.write("\n【准备工具】");
        for tool in tools {
            let line = match provisioned.failed.get(tool.name) {
                Some(reason) => format!("失败：{reason}"),
                None => "就绪".to_owned(),
            };
            self.field(&format!("{} {}", tool.name, tool.version), &line);
        }
    }

    pub fn summary(&mut self, runs: &[Run], total: Duration) {
        let count =
            |pick: fn(&Outcome) -> bool| runs.iter().filter(|run| pick(&run.outcome)).count();
        let completed = count(|o| matches!(o, Outcome::Completed));
        let skipped = count(|o| matches!(o, Outcome::Skipped { .. }));
        let failed = count(|o| matches!(o, Outcome::Failed { .. }));
        let timed_out = count(|o| matches!(o, Outcome::TimedOut));
        let cancelled = count(|o| matches!(o, Outcome::Cancelled));
        self.write(&format!(
            "\n共 {} 项：完成 {completed}，跳过 {skipped}，失败 {failed}，超时 {timed_out}，中断 {cancelled}。总用时 {:.1} 秒",
            runs.len(),
            total.as_secs_f64()
        ));
    }

    fn field(&mut self, label: &str, value: &str) {
        self.write(&format!("  {}  {value}", pad(label, LABEL_WIDTH)));
    }

    fn table(&mut self, table: &Table) {
        let mut widths: Vec<usize> = table.headers().iter().map(|h| h.width()).collect();
        for row in table.rows() {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.width());
            }
        }
        self.write(&format!("  {}", table.title()));
        for row in std::iter::once(table.headers()).chain(table.rows().iter().map(Vec::as_slice)) {
            let last = row.len().saturating_sub(1);
            let cells: Vec<String> = row
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(i, (cell, width))| {
                    if i == last {
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
            Record::Metric { label, result, .. } => self.field(label, &measurement(result)),
            Record::Table(table) => self.table(table),
            _ => {}
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

/// `171.01 MB/s`, `940 Mbit/s`, or the reason the value is missing.
pub fn measurement(result: &Measurement) -> String {
    match result.value() {
        Ok((value, unit)) if value.fract() == 0.0 => format!("{value:.0} {unit}"),
        Ok((value, unit)) => format!("{value:.2} {unit}"),
        Err(missing) => missing.label().to_owned(),
    }
}

/// Pads to a display width; CJK characters count as two columns.
fn pad(text: &str, width: usize) -> String {
    let fill = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(fill))
}
