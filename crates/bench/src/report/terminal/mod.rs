//! The terminal report, written once when every plugin is done: the host profile, a section per
//! group of plugins, in an order of its own, then the summary. Columns line up by display width,
//! so CJK and ASCII line up, and tables wrap to the terminal's width when there is one.

mod host;
mod legacy;
mod summary;

use std::fmt::Write;
use std::time::Duration;

use bench_host::profile::HostProfile;
use comfy_table::CellAlignment;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use comfy_table::presets;

use crate::orchestrator::Run;
use crate::results::Measured;

pub use host::family_text;
pub use host::kind_text;
pub use host::platform_text;
pub use host::why_text;
pub use summary::fault_text;
pub use summary::run_status;
use summary::status_text;
pub use summary::unmet_text;

/// Labels pad to this display width, so that values line up.
const LABEL_WIDTH: usize = 16;

/// The sections of the plugins, in the report's order, after the host's: a group's id and its
/// title.
const SECTIONS: [(&str, &str); 3] = [
    ("cpu", "CPU 性能"),
    ("memory", "内存性能"),
    ("disk", "硬盘"),
];

/// The names of the plugins of the typed contract, by id; the plugins of the first contract
/// bring their titles.
const NAMES: [(&str, &str); 0] = [];

/// The reporter's name for a plugin of the typed contract, by its id.
#[must_use]
pub fn name(id: &str) -> Option<&'static str> {
    NAMES
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, name)| *name)
}

/// Everything the report shows.
pub struct Page<'a> {
    /// The program's version, on the first line.
    pub version: &'a str,
    /// The host, as the run read it.
    pub host: &'a HostProfile,
    /// Whether to show the public addresses in full.
    pub show_ip: bool,
    /// The titles of the test items made for other platforms only.
    pub not_applicable: &'a [&'a str],
    /// Every plugin's run, in the run order.
    pub runs: &'a [Run<Measured>],
    /// The whole run's time.
    pub total: Duration,
}

/// The report as text. With a `width`, tables wrap to it; without, they never wrap, as in a file.
#[must_use]
pub fn render(page: &Page, width: Option<u16>) -> String {
    let mut out = Out {
        text: String::new(),
        width,
    };
    out.line(&format!("lowendaff-bench {}", page.version));
    out.line(&format!(
        "运行环境：{}",
        host::platform_text(page.host.platform)
    ));
    host::section(&mut out, page.host, page.show_ip);
    let groups: Vec<&str> = page.runs.iter().map(|run| run.group).collect();
    for (group, title) in sections(&groups) {
        let runs: Vec<&Run<Measured>> = page.runs.iter().filter(|run| run.group == group).collect();
        out.line(&format!("\n【{title}】"));
        let several = runs.len() > 1;
        for run in runs {
            let indent = if several {
                out.line(&format!("  {}", run.title.unwrap_or(run.id)));
                "    "
            } else {
                "  "
            };
            plugin(&mut out, run, indent);
        }
    }
    summary::section(&mut out, page);
    out.text
}

/// The sections of these groups, in the report's order: the groups it knows, in an order of
/// their own, then any other group, in the order given and titled by its name.
#[must_use]
pub fn sections<'a>(groups: &[&'a str]) -> Vec<(&'a str, &'a str)> {
    let mut sections: Vec<(&str, &str)> = SECTIONS
        .into_iter()
        .filter(|(group, _)| groups.contains(group))
        .collect();
    for group in groups {
        if !sections.iter().any(|(known, _)| known == group) {
            sections.push((group, group));
        }
    }
    sections
}

/// One plugin's block: what it left, then how it ended.
fn plugin(out: &mut Out, run: &Run<Measured>, indent: &str) {
    let reason = match &run.result {
        Some(Measured::Legacy(report)) => legacy::block(out, report, indent),
        None => None,
    };
    out.line(&format!("{indent}{}", status_text(run, reason)));
}

/// The report being written.
struct Out {
    text: String,
    width: Option<u16>,
}

impl Out {
    fn line(&mut self, line: &str) {
        let _ = writeln!(self.text, "{line}");
    }

    /// A label padded to [`LABEL_WIDTH`] and its value.
    fn field(&mut self, indent: &str, label: &str, value: &str) {
        self.line(&format!("{indent}{}  {value}", pad(label, LABEL_WIDTH)));
    }

    /// A table without borders: columns two spaces apart, those holding only numbers aligned to
    /// the right, wrapped to the terminal's width when there is one.
    fn table(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>]) {
        let mut table = Table::new();
        table.load_style(presets::NOTHING);
        table.set_header(headers);
        for row in rows {
            table.add_row(row);
        }
        let last = headers.len().saturating_sub(1);
        for (index, column) in table.column_iter_mut().enumerate() {
            column.set_padding((0, if index == last { 0 } else { 2 }));
            let numeric = !rows.is_empty()
                && rows
                    .iter()
                    .all(|row| row.get(index).is_some_and(|cell| is_number(cell)));
            if numeric {
                column.set_cell_alignment(CellAlignment::Right);
            }
        }
        if let Some(width) = self.width {
            let indent_width = u16::try_from(indent.len()).unwrap_or(u16::MAX);
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_width(width.saturating_sub(indent_width));
        }
        for line in table.lines() {
            self.line(&format!("{indent}{}", line.trim_end()));
        }
    }
}

/// Whether a cell holds a plain number, such as `859.00` or `-3`.
fn is_number(cell: &str) -> bool {
    cell.parse::<f64>().is_ok()
}

/// Pads to a display width; CJK characters count as two columns.
fn pad(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let fill = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(fill))
}
