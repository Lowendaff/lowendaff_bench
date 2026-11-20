//! The terminal report, written once when every plugin is done: the host profile, a section per
//! group of plugins, in an order of its own, then the summary. Its words come from the catalog
//! (`crate::locale`). Columns line up by display width, so that wide characters line up too, and
//! tables wrap to the terminal's width when there is one.

mod cpu;
mod disk;
mod host;
mod media;
mod memory;
mod network;
mod summary;
mod tool;

use std::fmt::Write;
use std::net::IpAddr;
use std::time::Duration;

use bench_host::network::Address;
use bench_host::profile::HostProfile;
use comfy_table::CellAlignment;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use comfy_table::presets;

use crate::locale;
use crate::orchestrator::Run;
use crate::results::Measured;

pub use disk::iops;
pub use disk::speed;
pub use host::family_text;
pub use host::kind_text;
pub use host::platform_text;
pub use host::why_text;
pub use network::bits;
pub use network::latency;
pub use summary::fault_text;
pub use summary::run_status;
pub use summary::unmet_text;

/// The sections of the plugins, in the report's order, after the host's: the groups' ids.
const SECTIONS: [&str; 5] = ["cpu", "memory", "disk", "network", "media"];

/// The reporter's name for a plugin, by its id.
#[must_use]
pub fn name(id: &str) -> Option<&'static str> {
    locale::words().item_name(id)
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
    let words = locale::words();
    out.line(&format!("lowendaff-bench {}", page.version));
    let platform = host::platform_text(page.host.platform);
    out.line(&words.environment(&platform));
    host::section(&mut out, page.host, page.show_ip);
    let groups: Vec<&str> = page.runs.iter().map(|run| run.group).collect();
    for (group, title) in sections(&groups) {
        let runs: Vec<&Run<Measured>> = page.runs.iter().filter(|run| run.group == group).collect();
        out.line(&format!("\n{}", words.section(title)));
        let several = runs.len() > 1;
        for run in runs {
            let indent = if several {
                out.line(&format!("  {}", run.title.unwrap_or(run.id)));
                "    "
            } else {
                "  "
            };
            plugin(&mut out, run, indent, page);
        }
    }
    summary::section(&mut out, page);
    out.text
}

/// The sections of these groups, in the report's order: the groups it knows, in an order of
/// their own, then any other group, in the order given; each with its title, or its name when the
/// catalog has no title for it.
#[must_use]
pub fn sections<'a>(groups: &[&'a str]) -> Vec<(&'a str, &'a str)> {
    let words = locale::words();
    let title = |group: &'a str| words.group_title(group).unwrap_or(group);
    let mut sections: Vec<(&str, &str)> = SECTIONS
        .into_iter()
        .filter(|group| groups.contains(group))
        .map(|group| (group, title(group)))
        .collect();
    for group in groups {
        if !sections.iter().any(|(known, _)| known == group) {
            sections.push((group, title(group)));
        }
    }
    sections
}

/// One plugin's block: what it left, then how it ended.
fn plugin(out: &mut Out, run: &Run<Measured>, indent: &str, page: &Page) {
    match &run.result {
        Some(Measured::CpuSysbench(report)) => cpu::sysbench(out, report, indent),
        Some(Measured::CpuGeekbench(report)) => cpu::geekbench(out, report, indent),
        Some(Measured::MemorySysbench(report)) => memory::sysbench(out, report, indent),
        Some(Measured::MemoryStream(report)) => memory::stream(out, report, indent),
        Some(Measured::MemoryLatency(report)) => memory::latency(out, report, indent),
        Some(Measured::MemoryNuma(report)) => memory::numa(out, report, indent),
        Some(Measured::DiskHealth(report)) => disk::health(out, report, indent),
        Some(Measured::DiskFio(report)) => disk::fio(out, report, indent),
        Some(Measured::NetworkIperf3(report)) => network::iperf3(out, report, indent),
        Some(Measured::NetworkTrace(report)) => {
            let own = own_addresses(page.host);
            network::trace(out, report, indent, &own, page.show_ip);
        }
        Some(Measured::MediaUnlock(report)) => media::unlock(out, report, indent),
        None => {}
    }
    out.line(&format!("{indent}{}", run_status(run)));
}

/// The host's own public addresses, which the report masks unless asked to show them.
fn own_addresses(host: &HostProfile) -> Vec<IpAddr> {
    [&host.network.ipv4, &host.network.ipv6]
        .into_iter()
        .filter_map(|family| match family.address {
            Address::Found { address, .. } => Some(address),
            _ => None,
        })
        .collect()
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

    /// A label padded to the catalog's label width, and its value.
    fn field(&mut self, indent: &str, label: &str, value: &str) {
        let label = pad(label, locale::words().label_width());
        self.line(&format!("{indent}{label}  {value}"));
    }

    /// A table without borders: columns two spaces apart, those holding only numbers aligned to
    /// the right, wrapped to the terminal's width when there is one.
    fn table(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>]) {
        self.draw(indent, headers, rows, true);
    }

    /// A table as [`Out::table`] draws it, with every column aligned to the left: for a table
    /// whose columns hold plain numbers on some hosts and units or words on others, so that its
    /// layout does not change with the host, nor a lone count stand far from its row.
    fn table_left(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>]) {
        self.draw(indent, headers, rows, false);
    }

    /// Draws a table; with `numbers`, a column that holds only numbers is aligned to the right.
    fn draw(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>], numbers: bool) {
        let mut table = Table::new();
        table.load_style(presets::NOTHING);
        table.set_header(headers);
        for row in rows {
            table.add_row(row);
        }
        let last = headers.len().saturating_sub(1);
        for (index, column) in table.column_iter_mut().enumerate() {
            column.set_padding((0, if index == last { 0 } else { 2 }));
            let numeric = numbers
                && !rows.is_empty()
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

/// Pads to a display width; wide characters, as in CJK, count as two columns.
fn pad(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let fill = width.saturating_sub(text.width());
    format!("{text}{}", " ".repeat(fill))
}

#[cfg(test)]
mod tests {
    use super::Out;

    #[test]
    fn a_table_without_rows_has_no_column_of_numbers() {
        // Its headers stay on the left, which shows once they wrap.
        let mut out = Out {
            text: String::new(),
            width: Some(14),
        };
        out.table("  ", &["Headline abcd".to_owned(), "x".to_owned()], &[]);
        assert_eq!(out.text, "  Headline   x\n  abcd\n");
    }

    #[test]
    fn a_wide_character_takes_two_columns() {
        // As an emoji or a CJK character does in a terminal.
        assert_eq!(super::pad("\u{1F600}x", 5), "\u{1F600}x  ");
    }
}
