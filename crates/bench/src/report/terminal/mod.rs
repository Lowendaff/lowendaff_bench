//! The terminal report (and shared layout primitives for Markdown), written once when every
//! plugin is done: the host profile, a section per group of plugins in Terminal mode (or a
//! `lowendaff.com`-compatible section per test item in Markdown mode), and on virtual machines the
//! 5-second `/proc/stat` `%si` / `%st` telemetry section.

mod cpu;
mod disk;
mod host;
mod ip;
mod memory;
mod network;
mod summary;
mod tool;

use std::fmt::Write;
use std::net::IpAddr;
use std::time::Duration;

use bench_host::network::Address;
use bench_host::platform::PlatformKind;
use bench_host::profile::HostProfile;
use comfy_table::CellAlignment;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use comfy_table::presets;

use crate::locale;
use crate::orchestrator::Run;
use crate::orchestrator::Status;
use crate::results::Measured;
use crate::vm_cpu;
use crate::vm_cpu::MetricStats;
use crate::vm_cpu::VmCpuSample;

pub use disk::iops;
pub use disk::speed;
pub use host::family_text;
pub use host::kind_text;
pub use host::platform_text;
pub use host::why_text;
pub(super) use ip::redact_ip_in_note;
pub use network::bits;
pub use network::latency;
pub use summary::fault_text;
pub use summary::run_status;
pub(super) use summary::stop_reason;
pub(super) use summary::totals;
pub use summary::unmet_text;

/// Strips ANSI/VT100 escape sequences (CSI, OSC, DCS, APC, PM, SOS, two-byte ESC), C0/C1 control
/// characters, DEL (`\x7f`), and Unicode bidirectional override code points from `text`. Preserves
/// `\n` when `keep_newlines` is `true` and replaces `\n` and `\t` with `' '` otherwise.
#[must_use]
pub fn strip_terminal_controls(text: &str, keep_newlines: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    consume_csi(&mut chars);
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    chars.next();
                    consume_st_sequence(&mut chars, keep_newlines, &mut out);
                }
                Some(next) if ('\x20'..='\x2f').contains(&next) => {
                    chars.next();
                    while let Some(&mid) = chars.peek() {
                        if ('\x20'..='\x2f').contains(&mid) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Some(&fin) = chars.peek()
                        && ('\x30'..='\x7e').contains(&fin)
                    {
                        chars.next();
                    }
                }
                Some(fin) if ('\x30'..='\x7e').contains(&fin) => {
                    chars.next();
                }
                _ => {}
            },
            '\u{009b}' => consume_csi(&mut chars),
            '\u{0090}' | '\u{0098}' | '\u{009d}' | '\u{009e}' | '\u{009f}' => {
                consume_st_sequence(&mut chars, keep_newlines, &mut out);
            }
            '\n' => {
                if keep_newlines {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
            }
            '\t' => out.push(' '),
            c if c.is_control() || is_bidi_control(c) => {}
            other => out.push(other),
        }
    }
    out
}

fn consume_csi<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    while let Some(&ch) = chars.peek() {
        if ('\x20'..='\x3f').contains(&ch) {
            chars.next();
        } else {
            break;
        }
    }
    if let Some(&fin) = chars.peek()
        && ('\x40'..='\x7e').contains(&fin)
    {
        chars.next();
    }
}

fn consume_st_sequence<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    keep_newlines: bool,
    out: &mut String,
) {
    while let Some(ch) = chars.next() {
        match ch {
            '\x07' | '\u{009c}' => break,
            '\x1b' => {
                if chars.peek() == Some(&'\\') {
                    chars.next();
                }
                break;
            }
            '\n' => {
                if keep_newlines {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                break;
            }
            _ => {}
        }
    }
}

fn is_bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// The sections of the plugins, in the report's order, after the host's: the groups' ids.
const SECTIONS: [&str; 5] = ["cpu", "memory", "disk", "network", "ip"];

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
    render_with_vm_cpu(page, &[], width)
}

/// The report as text, including virtual-machine `%si` and `%st` telemetry when present.
#[must_use]
pub fn render_with_vm_cpu(
    page: &Page,
    vm_cpu_samples: &[VmCpuSample],
    width: Option<u16>,
) -> String {
    let mut out = Out::terminal(width);
    write_page(&mut out, page, vm_cpu_samples);
    out.finish()
}

/// The report as GitHub-Flavored Markdown (`lowendaff.com` review-markdown compatible).
pub(super) fn render_markdown_with_vm_cpu(page: &Page, vm_cpu_samples: &[VmCpuSample]) -> String {
    let mut out = Out::markdown();
    write_page(&mut out, page, vm_cpu_samples);
    out.finish()
}

fn write_page(out: &mut Out, page: &Page, vm_cpu_samples: &[VmCpuSample]) {
    let words = locale::words();
    out.title(&format!("lowendaff-bench {}", page.version));
    let platform = host::platform_text(page.host.platform);
    out.line(&words.environment(&platform));
    host::section(out, page.host, page.show_ip);
    let item_title = |run: &Run<Measured>| -> &'static str {
        words.item_name(run.id).or(run.title).unwrap_or(run.id)
    };
    let groups: Vec<&str> = page.runs.iter().map(|run| run.group).collect();
    if out.is_markdown() {
        for (group, _) in sections(&groups) {
            let runs: Vec<&Run<Measured>> =
                page.runs.iter().filter(|run| run.group == group).collect();
            for run in runs {
                out.section_unique(item_title(run));
                plugin(out, run, "  ", page);
            }
        }
    } else {
        for (group, title) in sections(&groups) {
            let runs: Vec<&Run<Measured>> =
                page.runs.iter().filter(|run| run.group == group).collect();
            out.section(title);
            let several = runs.len() > 1;
            for run in runs {
                let indent = if several {
                    out.subsection(item_title(run));
                    "    "
                } else {
                    "  "
                };
                plugin(out, run, indent, page);
            }
        }
    }
    if page.host.platform.kind() == PlatformKind::Vm && !vm_cpu_samples.is_empty() {
        vm_cpu_section(out, vm_cpu_samples);
    }
}

fn vm_cpu_section(out: &mut Out, samples: &[VmCpuSample]) {
    let Some(summary) = vm_cpu::summarize(samples) else {
        return;
    };
    let words = locale::words();
    out.section(words.vm_cpu_title());
    out.line(&format!(
        "  {}",
        words.vm_cpu_method(vm_cpu::SAMPLE_INTERVAL.as_secs())
    ));

    let item_label = |item: Option<&'static str>| -> &'static str {
        match item {
            Some(id) => words.item_name(id).unwrap_or(id),
            None => words.vm_cpu_setup_phase(),
        }
    };
    let summary_row = |steal: bool, stats: &MetricStats| -> Vec<String> {
        let peak_time = vm_cpu::format_elapsed(stats.peak_elapsed);
        let peak_where = words.vm_cpu_peak_cell(item_label(stats.peak_item), &peak_time);
        vec![
            words.vm_cpu_metric_label(steal).to_owned(),
            format!("{:.2}%", stats.avg),
            format!("{:.2}%", stats.p95),
            format!("{:.2}%", stats.max),
            peak_where,
        ]
    };

    out.heading("  ", words.vm_cpu_summary_title());
    let sum_headers = words.vm_cpu_summary_headers().map(str::to_owned);
    let sum_rows = vec![
        summary_row(false, &summary.si),
        summary_row(true, &summary.st),
    ];
    out.table_left("    ", &sum_headers, &sum_rows);

    if out.is_markdown() {
        if let Some(chart) = vm_cpu::mermaid_xychart(
            samples,
            words.vm_cpu_chart_title(),
            words.vm_cpu_y_axis_title(),
            words.vm_cpu_metric_label(true),
            words.vm_cpu_metric_label(false),
        ) {
            out.heading("  ", words.vm_cpu_timeline_title());
            out.code_block("mermaid", &chart);
        }
    } else {
        out.heading("  ", words.vm_cpu_samples_title());
        let sample_headers = words.vm_cpu_samples_headers().map(str::to_owned);
        let sample_rows: Vec<Vec<String>> = samples
            .iter()
            .map(|s| {
                vec![
                    vm_cpu::format_elapsed(s.elapsed),
                    item_label(s.item).to_owned(),
                    format!("{:.2}%", s.si_pct),
                    format!("{:.2}%", s.st_pct),
                ]
            })
            .collect();
        out.table_left("    ", &sample_headers, &sample_rows);
    }
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

/// One plugin's block: what it left, followed by any non-completed stop status (`skipped`,
/// `failed`, `timed out`, or `interrupted`). Completed items omit trailing elapsed-time lines.
fn plugin(out: &mut Out, run: &Run<Measured>, indent: &str, page: &Page) {
    match &run.result {
        Some(Measured::CpuBench(report)) => cpu::bench(out, report, indent),
        Some(Measured::MemorySysbench(report)) => memory::sysbench(out, report, indent),
        Some(Measured::MemoryStream(report)) => memory::stream(out, report, indent),
        Some(Measured::MemoryLatency(report)) => memory::latency(out, report, indent),
        Some(Measured::MemoryNuma(report)) => memory::numa(out, report, indent),
        Some(Measured::DiskHealth(report)) => disk::health(out, report, indent),
        Some(Measured::DiskFio(report)) => disk::fio(out, report, indent),
        Some(Measured::NetworkIperf3(report)) => network::iperf3(out, report, indent),
        Some(Measured::NetworkTaier(report)) => network::taier(out, report, indent),
        Some(Measured::NetworkCloudflare(report)) => {
            network::cloudflare(out, report, indent, page.show_ip);
        }
        Some(Measured::NetworkInetspeed(report)) => network::inetspeed(out, report, indent),
        Some(Measured::NetworkTcping(report)) => network::tcping(out, report, indent),
        Some(Measured::NetworkTrace(report)) => {
            let own = collect_own_addresses(page.host, page.runs);
            network::trace(out, report, indent, &own, page.show_ip);
        }
        Some(Measured::NetworkBgp(report)) => network::bgp(out, report, indent, page.show_ip),
        Some(Measured::IpQuality(report)) => {
            let own = collect_own_addresses(page.host, page.runs);
            ip::quality(out, report, indent, &own, page.show_ip);
        }
        Some(Measured::IpMedia(report)) => ip::media(out, report, indent),
        None => {}
    }
    if !matches!(run.status, Status::Completed) {
        let status = run_status(run);
        if out.is_markdown() {
            out.quote(&status);
        } else {
            out.line(&format!("{indent}{status}"));
        }
    }
}

/// The host's own public addresses, which the report masks unless asked to show them.
pub(super) fn own_addresses(host: &HostProfile) -> Vec<IpAddr> {
    [&host.network.ipv4, &host.network.ipv6]
        .into_iter()
        .filter_map(|family| match family.address {
            Address::Found { address, .. } => Some(address),
            _ => None,
        })
        .collect()
}

/// Collects all public/routable host addresses observed in `host` and across `runs` so multi-egress
/// or echo-fallback addresses (e.g. `network.cloudflare` `client_ip`) are masked consistently in
/// `network.trace` when `show_ip` is `false`.
pub(super) fn collect_own_addresses(host: &HostProfile, runs: &[Run<Measured>]) -> Vec<IpAddr> {
    let mut own = own_addresses(host);
    let mut push = |ip: IpAddr| {
        if bench_api::is_routable_endpoint_ip(ip) && !own.contains(&ip) {
            own.push(ip);
        }
    };
    for run in runs {
        match &run.result {
            Some(Measured::NetworkCloudflare(r)) => {
                if let Some(out) = &r.output {
                    if let Some(ip) = out.ipv4.as_ref().and_then(|f| f.client_ip) {
                        push(ip);
                    }
                    if let Some(ip) = out.ipv6.as_ref().and_then(|f| f.client_ip) {
                        push(ip);
                    }
                }
            }
            Some(Measured::NetworkBgp(r)) => {
                if let Some(out) = &r.output {
                    if let Some(rep) = &out.ipv4 {
                        push(rep.address);
                    }
                    if let Some(rep) = &out.ipv6 {
                        push(rep.address);
                    }
                }
            }
            Some(Measured::IpQuality(r)) => {
                if let Some(out) = &r.output {
                    if let Some(rep) = &out.ipv4 {
                        push(rep.address);
                    }
                    if let Some(rep) = &out.ipv6 {
                        push(rep.address);
                    }
                }
            }
            _ => {}
        }
    }
    own
}

/// Previous block emitted in Markdown mode, for spacing paragraphs, quotes, and tables cleanly.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Empty,
    Heading,
    Line,
    Quote,
    Table,
}

/// Target syntax for [`Out`].
enum Mode {
    Terminal {
        width: Option<u16>,
    },
    Markdown {
        depth: usize,
        last: Block,
        pending_fields: Vec<(String, String)>,
        seen_sections: Vec<String>,
    },
}

/// The report being written.
struct Out {
    text: String,
    mode: Mode,
}

impl Out {
    fn terminal(width: Option<u16>) -> Self {
        Self {
            text: String::new(),
            mode: Mode::Terminal { width },
        }
    }

    fn markdown() -> Self {
        Self {
            text: String::new(),
            mode: Mode::Markdown {
                depth: 1,
                last: Block::Empty,
                pending_fields: Vec::new(),
                seen_sections: Vec::new(),
            },
        }
    }

    fn finish(mut self) -> String {
        self.flush_md_fields();
        self.text
    }

    fn flush_md_fields(&mut self) {
        let Mode::Markdown {
            last,
            pending_fields,
            ..
        } = &mut self.mode
        else {
            return;
        };
        if pending_fields.is_empty() {
            return;
        }
        let rows: Vec<[String; 2]> = pending_fields
            .drain(..)
            .map(|(label, value)| [label, value])
            .collect();
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        let headers = locale::words().md_kv_headers();
        let h_cells = [headers[0].to_owned(), headers[1].to_owned()];
        write_md_row(&mut self.text, &h_cells);
        self.text.push_str("| --- | --- |\n");
        for row in &rows {
            write_md_row(&mut self.text, row);
        }
        *last = Block::Table;
    }

    /// Top-level document title (`lowendaff-bench <version>`).
    fn title(&mut self, text: &str) {
        self.flush_md_fields();
        match &mut self.mode {
            Mode::Terminal { .. } => self.line(text),
            Mode::Markdown {
                depth,
                last,
                seen_sections,
                ..
            } => {
                let clean = strip_terminal_controls(text, false);
                seen_sections.push(clean);
                let _ = writeln!(self.text, "# {}", escape_md_text(text));
                *depth = 1;
                *last = Block::Heading;
            }
        }
    }

    /// Major report section (`[System]`, `[CPU performance]`).
    fn section(&mut self, title: &str) {
        self.flush_md_fields();
        match &mut self.mode {
            Mode::Terminal { .. } => {
                let words = locale::words();
                self.text.push('\n');
                self.line(&words.section(title));
            }
            Mode::Markdown {
                depth,
                last,
                seen_sections,
                ..
            } => {
                let clean = strip_terminal_controls(title, false);
                seen_sections.push(clean);
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let _ = writeln!(self.text, "## {}", escape_md_text(title));
                *depth = 2;
                *last = Block::Heading;
            }
        }
    }

    /// Emits `## <title>` in Markdown mode unless an `##` section with the same title was already
    /// emitted, falling back to `### <title>` so duplicate top-level headings never occur.
    fn section_unique(&mut self, title: &str) {
        self.flush_md_fields();
        if let Mode::Markdown { seen_sections, .. } = &self.mode {
            let clean = strip_terminal_controls(title, false);
            if seen_sections.iter().any(|prev| prev == &clean) {
                self.subsection(title);
                return;
            }
        }
        self.section(title);
    }

    /// Plugin or group subheading (`###` in Markdown mode).
    fn subsection(&mut self, title: &str) {
        self.flush_md_fields();
        match &mut self.mode {
            Mode::Terminal { .. } => self.line(&format!("  {title}")),
            Mode::Markdown { depth, last, .. } => {
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let _ = writeln!(self.text, "### {}", escape_md_text(title));
                *depth = 3;
                *last = Block::Heading;
            }
        }
    }

    /// Table or target heading inside the current section (`###` directly under `##`, or `####`
    /// under `###` — never jumping levels or exceeding `####`).
    fn heading(&mut self, indent: &str, title: &str) {
        self.flush_md_fields();
        match &mut self.mode {
            Mode::Terminal { .. } => self.line(&format!("{indent}{title}")),
            Mode::Markdown { depth, last, .. } => {
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let hashes = if *depth >= 3 { "####" } else { "###" };
                let _ = writeln!(self.text, "{hashes} {}", escape_md_text(title));
                *last = Block::Heading;
            }
        }
    }

    fn line(&mut self, line: &str) {
        self.flush_md_fields();
        match &mut self.mode {
            Mode::Terminal { .. } => {
                let clean = strip_terminal_controls(line, false);
                let _ = writeln!(self.text, "{clean}");
            }
            Mode::Markdown { last, .. } => {
                let clean = strip_terminal_controls(line, false);
                let trimmed = clean.trim();
                if trimmed.is_empty() {
                    return;
                }
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let escaped = escape_md_line(trimmed);
                let _ = writeln!(self.text, "{escaped}");
                *last = Block::Line;
            }
        }
    }

    /// Single-paragraph blockquote (`> ...`) in Markdown mode, for notes and stop reasons.
    fn quote(&mut self, text: &str) {
        self.flush_md_fields();
        if let Mode::Markdown { last, .. } = &mut self.mode {
            let clean = strip_terminal_controls(text, false);
            let trimmed = clean.trim();
            if trimmed.is_empty() {
                return;
            }
            if !self.text.is_empty() {
                self.text.push('\n');
            }
            let _ = writeln!(self.text, "> {}", escape_md_text(trimmed));
            *last = Block::Quote;
        }
    }

    /// A label padded to the catalog's label width, and its value.
    fn field(&mut self, indent: &str, label: &str, value: &str) {
        self.field_width(indent, locale::words().label_width(), label, value);
    }

    /// A label padded to `width` columns in terminal mode, or accumulated into a two-column
    /// `Item | Value` table in Markdown mode (with `Note` entries rendered as `> **Note:** ...`
    /// blockquotes).
    fn field_width(&mut self, indent: &str, width: usize, label: &str, value: &str) {
        let clean_label = strip_terminal_controls(label, false);
        let clean_value = strip_terminal_controls(value, false);
        match &mut self.mode {
            Mode::Terminal { .. } => {
                let padded = pad(&clean_label, width);
                self.line(&format!("{indent}{padded}  {clean_value}"));
            }
            Mode::Markdown { pending_fields, .. } => {
                if clean_label == locale::words().note() {
                    self.flush_md_fields();
                    if let Mode::Markdown { last, .. } = &mut self.mode {
                        if !self.text.is_empty() {
                            self.text.push('\n');
                        }
                        let _ = writeln!(
                            self.text,
                            "> **{}:** {}",
                            escape_md_text(&clean_label),
                            escape_md_text(&clean_value)
                        );
                        *last = Block::Quote;
                    }
                } else {
                    pending_fields.push((clean_label, clean_value));
                }
            }
        }
    }

    /// A table without borders: columns two spaces apart, those holding only numbers aligned to
    /// the right, wrapped to the terminal's width when there is one.
    fn table(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>]) {
        self.draw(indent, headers, rows, true);
    }

    /// A table as [`Out::table`] draws it, with every column aligned to the left.
    fn table_left(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>]) {
        self.draw(indent, headers, rows, false);
    }

    /// Draws a table; with `numbers`, a column that holds only numbers is aligned to the right.
    fn draw(&mut self, indent: &str, headers: &[String], rows: &[Vec<String>], numbers: bool) {
        self.flush_md_fields();
        let clean_headers: Vec<String> = headers
            .iter()
            .map(|h| strip_terminal_controls(h, false))
            .collect();
        let clean_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| strip_terminal_controls(cell, false))
                    .collect()
            })
            .collect();
        match &mut self.mode {
            Mode::Terminal { width } => {
                let width = *width;
                let mut table = Table::new();
                table.load_style(presets::NOTHING);
                table.set_header(&clean_headers);
                for row in &clean_rows {
                    table.add_row(row);
                }
                let last = clean_headers.len().saturating_sub(1);
                for (index, column) in table.column_iter_mut().enumerate() {
                    column.set_padding((0, if index == last { 0 } else { 2 }));
                    if numeric_column(&clean_rows, index, numbers) {
                        column.set_cell_alignment(CellAlignment::Right);
                    }
                }
                if let Some(width) = width {
                    let indent_width = u16::try_from(indent.len()).unwrap_or(u16::MAX);
                    table.set_content_arrangement(ContentArrangement::Dynamic);
                    table.set_width(width.saturating_sub(indent_width));
                }
                for line in table.lines() {
                    self.line(&format!("{indent}{}", line.trim_end()));
                }
            }
            Mode::Markdown { last, .. } => {
                if clean_headers.is_empty() {
                    return;
                }
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                write_md_row(&mut self.text, &clean_headers);
                self.text.push('|');
                for index in 0..clean_headers.len() {
                    if numeric_column(&clean_rows, index, numbers) {
                        self.text.push_str(" ---: |");
                    } else {
                        self.text.push_str(" --- |");
                    }
                }
                self.text.push('\n');
                for row in &clean_rows {
                    write_md_row(&mut self.text, row);
                }
                *last = Block::Table;
            }
        }
    }

    /// Whether the target syntax is Markdown rather than plain terminal text.
    fn is_markdown(&self) -> bool {
        matches!(self.mode, Mode::Markdown { .. })
    }

    /// Fenced code block (` ```<lang> `) in Markdown mode, unescaped so diagram languages such as
    /// `mermaid` preserve their operators and line breaks.
    fn code_block(&mut self, lang: &str, content: &str) {
        self.flush_md_fields();
        if let Mode::Markdown { last, .. } = &mut self.mode {
            if !self.text.is_empty() {
                self.text.push('\n');
            }
            let clean_lang = strip_terminal_controls(lang, false);
            let clean_content = strip_terminal_controls(content, true);
            let _ = writeln!(self.text, "```{clean_lang}");
            let _ = writeln!(self.text, "{clean_content}");
            let _ = writeln!(self.text, "```");
            *last = Block::Table;
        }
    }
}

fn numeric_column(rows: &[Vec<String>], index: usize, numbers: bool) -> bool {
    numbers
        && !rows.is_empty()
        && rows
            .iter()
            .all(|row| row.get(index).is_some_and(|cell| is_number(cell)))
}

fn write_md_row(buf: &mut String, cells: &[String]) {
    buf.push('|');
    for cell in cells {
        buf.push(' ');
        buf.push_str(&escape_md_cell(cell));
        buf.push_str(" |");
    }
    buf.push('\n');
}

/// Context-aware inline CommonMark / GFM escaping: escapes metacharacters only when they would
/// form actual Markdown emphasis, strikethrough, code spans, links/images, HTML/autolinks, or
/// backslash escapes, keeping literal tokens such as NextTrace's `*`, `[Any2West]`, `< 1 ms`, and
/// `~10 ms` unescaped.
fn escape_md_text(text: &str) -> String {
    let clean = strip_terminal_controls(text, false);
    escape_md_inline(&clean, false)
}

/// Escapes a standalone Markdown paragraph line, neutralizing leading CommonMark block markers
/// (ATX headings, blockquotes, thematic breaks, bullet/ordered list markers, code fences, and link
/// reference definitions) only when they actually trigger block parsing.
fn escape_md_line(trimmed: &str) -> String {
    let clean = strip_terminal_controls(trimmed, false);
    let text = clean.trim();
    let mut escaped = escape_md_inline(text, false);

    if let Some(digits) = ordered_list_digits(text) {
        if let Some(pos) = find_nth_ascii_digit_end(&escaped, digits) {
            escaped.insert(pos, '\\');
        }
        return escaped;
    }

    if needs_leading_block_escape(text) && !escaped.starts_with('\\') {
        escaped.insert(0, '\\');
    }
    escaped
}

fn ordered_list_digits(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && matches!(bytes.get(digits + 1), None | Some(b' ' | b'\t'))
    {
        Some(digits)
    } else {
        None
    }
}

fn find_nth_ascii_digit_end(text: &str, digits: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    (bytes.len() >= digits && bytes[..digits].iter().all(u8::is_ascii_digit)).then_some(digits)
}

fn needs_leading_block_escape(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    if is_thematic_break(text) {
        return true;
    }
    match first {
        b'>' => true,
        b'#' => {
            let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
            hashes <= 6 && matches!(bytes.get(hashes), None | Some(b' ' | b'\t'))
        }
        b'-' | b'+' | b'*' => matches!(bytes.get(1), None | Some(b' ' | b'\t')),
        b'`' => text.starts_with("```"),
        b'~' => text.starts_with("~~~"),
        b'[' => text.contains("]:"),
        _ => false,
    }
}

fn is_thematic_break(text: &str) -> bool {
    let mut marker: Option<char> = None;
    let mut count = 0_usize;
    for ch in text.chars() {
        if ch == ' ' || ch == '\t' {
            continue;
        }
        if !matches!(ch, '-' | '*' | '_') {
            return false;
        }
        match marker {
            None => {
                marker = Some(ch);
                count = 1;
            }
            Some(prev) if prev == ch => count += 1,
            Some(_) => return false,
        }
    }
    count >= 3
}

/// Escapes pipe-table cell characters (`|` and context-sensitive inline Markdown metacharacters)
/// after normalizing control characters and replacing newlines with spaces.
fn escape_md_cell(cell: &str) -> String {
    let clean = strip_terminal_controls(cell, false);
    escape_md_inline(&clean, true)
}

#[derive(Clone, Copy)]
struct DelimRun {
    start: usize,
    end: usize,
    can_open: bool,
    can_close: bool,
}

fn escape_md_inline(clean: &str, in_cell: bool) -> String {
    let chars: Vec<char> = clean.chars().collect();
    let n = chars.len();
    if n == 0 {
        return String::new();
    }

    let in_url = find_url_spans(&chars);
    let esc_star = paired_delim_mask(&chars, &in_url, '*', false);
    let esc_under = paired_delim_mask(&chars, &in_url, '_', true);
    let esc_tilde = paired_tilde_mask(&chars, &in_url);
    let esc_tick = paired_backtick_mask(&chars);
    let esc_bracket = link_bracket_mask(&chars);

    let mut out = String::with_capacity(clean.len());
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '\\' if i + 1 < n && chars[i + 1].is_ascii_punctuation() => out.push_str("\\\\"),
            '|' if in_cell => out.push_str("\\|"),
            '*' if esc_star[i] => out.push_str("\\*"),
            '_' if esc_under[i] => out.push_str("\\_"),
            '~' if esc_tilde[i] => out.push_str("\\~"),
            '`' if esc_tick[i] => out.push_str("\\`"),
            '[' if esc_bracket[i] => out.push_str("\\["),
            ']' if esc_bracket[i] => out.push_str("\\]"),
            '<' if i + 1 < n
                && (chars[i + 1].is_ascii_alphabetic()
                    || matches!(chars[i + 1], '/' | '!' | '?')) =>
            {
                out.push_str("\\<");
            }
            other => out.push(other),
        }
    }
    out
}

fn find_url_spans(chars: &[char]) -> Vec<bool> {
    let n = chars.len();
    let mut in_url = vec![false; n];
    let mut idx = 0;
    while idx < n {
        let prefix: String = chars[idx..].iter().take(8).collect();
        if prefix.starts_with("https://") || prefix.starts_with("http://") {
            let paren_link = idx >= 2 && chars[idx - 2] == ']' && chars[idx - 1] == '(';
            while idx < n && !chars[idx].is_whitespace() && !(paren_link && chars[idx] == ')') {
                in_url[idx] = true;
                idx += 1;
            }
        } else {
            idx += 1;
        }
    }
    in_url
}

fn paired_delim_mask(
    chars: &[char],
    in_url: &[bool],
    target: char,
    intraword_safe: bool,
) -> Vec<bool> {
    let n = chars.len();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i] == target && !in_url[i] {
            let start = i;
            while i < n && chars[i] == target && !in_url[i] {
                i += 1;
            }
            let end = i;
            let prev_alnum = start > 0 && chars[start - 1].is_alphanumeric();
            let next_alnum = end < n && chars[end].is_alphanumeric();
            let intraword = intraword_safe && prev_alnum && next_alnum;
            let can_open = !intraword && end < n && !chars[end].is_whitespace();
            let can_close = !intraword && start > 0 && !chars[start - 1].is_whitespace();
            runs.push(DelimRun {
                start,
                end,
                can_open,
                can_close,
            });
        } else {
            i += 1;
        }
    }

    let mut mask = vec![false; n];
    let first_open = runs.iter().position(|r| r.can_open);
    let last_close = runs.iter().rposition(|r| r.can_close);
    if let (Some(open_idx), Some(close_idx)) = (first_open, last_close)
        && open_idx < close_idx
    {
        for run in &runs {
            if run.can_open || run.can_close {
                for slot in &mut mask[run.start..run.end] {
                    *slot = true;
                }
            }
        }
    }
    mask
}

fn paired_tilde_mask(chars: &[char], in_url: &[bool]) -> Vec<bool> {
    let n = chars.len();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i] == '~' && !in_url[i] {
            let start = i;
            while i < n && chars[i] == '~' && !in_url[i] {
                i += 1;
            }
            let end = i;
            if end - start <= 2 {
                let can_open = end < n && !chars[end].is_whitespace();
                let can_close = start > 0 && !chars[start - 1].is_whitespace();
                runs.push(DelimRun {
                    start,
                    end,
                    can_open,
                    can_close,
                });
            }
        } else {
            i += 1;
        }
    }

    let mut mask = vec![false; n];
    for (idx, opener) in runs.iter().enumerate() {
        if !opener.can_open {
            continue;
        }
        let len = opener.end - opener.start;
        for closer in &runs[idx + 1..] {
            if closer.can_close && (closer.end - closer.start) == len {
                for slot in &mut mask[opener.start..opener.end] {
                    *slot = true;
                }
                for slot in &mut mask[closer.start..closer.end] {
                    *slot = true;
                }
            }
        }
    }
    mask
}

fn paired_backtick_mask(chars: &[char]) -> Vec<bool> {
    let n = chars.len();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i] == '`' {
            let start = i;
            while i < n && chars[i] == '`' {
                i += 1;
            }
            runs.push((start, i));
        } else {
            i += 1;
        }
    }

    let mut mask = vec![false; n];
    for (idx, &(s1, e1)) in runs.iter().enumerate() {
        let len = e1 - s1;
        let has_pair = runs
            .iter()
            .enumerate()
            .any(|(j, &(s2, e2))| j != idx && (e2 - s2) == len);
        if has_pair {
            for slot in &mut mask[s1..e1] {
                *slot = true;
            }
        }
    }
    mask
}

fn link_bracket_mask(chars: &[char]) -> Vec<bool> {
    let n = chars.len();
    let mut mask = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '[' => stack.push(i),
            ']' => {
                if let Some(open_idx) = stack.pop()
                    && i + 1 < n
                    && matches!(chars[i + 1], '(' | '[')
                {
                    mask[open_idx] = true;
                    mask[i] = true;
                }
            }
            _ => {}
        }
    }
    mask
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
    use super::escape_md_cell;
    use super::escape_md_line;
    use super::escape_md_text;

    #[test]
    fn a_table_without_rows_has_no_column_of_numbers() {
        let mut out = Out::terminal(Some(14));
        out.table("  ", &["Headline abcd".to_owned(), "x".to_owned()], &[]);
        assert_eq!(out.finish(), "  Headline   x\n  abcd\n");
    }

    #[test]
    fn a_wide_character_takes_two_columns() {
        assert_eq!(super::pad("\u{1F600}x", 5), "\u{1F600}x  ");
    }

    #[test]
    fn context_aware_markdown_escaping_preserves_lone_tokens_and_neutralizes_syntax() {
        // Lone NextTrace timeout `*`, space-bounded `* * *`, IPv6 `2001:db8:*`, `[Any2West]`,
        // `< 1 ms`, `~10 ms`, `+13.0 ms`, and `-3 dBm` stay clean without backslashes.
        assert_eq!(escape_md_cell("*"), "*");
        assert_eq!(escape_md_cell("* * *"), "* * *");
        assert_eq!(escape_md_cell("2001:db8:*"), "2001:db8:*");
        assert_eq!(escape_md_cell("Guangzhou*"), "Guangzhou*");
        assert_eq!(
            escape_md_cell("Netflix [Any2West] < 1 ms ~0.4 ms"),
            "Netflix [Any2West] < 1 ms ~0.4 ms"
        );
        assert_eq!(escape_md_line("+13.0 ms"), "+13.0 ms");
        assert_eq!(escape_md_line("-3 dBm"), "-3 dBm");

        // Paired `*...*` (including masked IPv4 `203.0.*.*`), paired `_..._`, paired `~~...~~`,
        // paired backticks, `[text](url)` links, `<tag>` HTML, and pipe `|` in cells are escaped.
        assert_eq!(escape_md_cell("203.0.*.*"), "203.0.\\*.\\*");
        assert_eq!(escape_md_cell("a | b"), "a \\| b");
        assert_eq!(escape_md_cell("~~strike~~"), "\\~\\~strike\\~\\~");
        assert_eq!(escape_md_cell("`cmd` and lone"), "\\`cmd\\` and lone");
        assert_eq!(escape_md_text("`lone"), "`lone");
        assert_eq!(escape_md_line("* * *"), "\\* * *");
    }

    #[test]
    fn terminal_and_markdown_outputs_strip_ansi_osc_c1_and_bidi_controls() {
        let payload = "Evil\x1b[31mRed\x1b[0m\x1b]52;c;SGVsbG8=\x07\x1b]0;Title\x1b\\\
                       \u{009b}2J\rOverwrite\nSpoof: 9999\u{202e}Org";
        assert_eq!(
            super::strip_terminal_controls(payload, false),
            "EvilRedOverwrite Spoof: 9999Org"
        );

        let mut term = Out::terminal(None);
        term.field("  ", "Organization", payload);
        term.table(
            "  ",
            &["Host".to_owned()],
            &[vec!["ptr\x1b[2J.example\r\nfake".to_owned()]],
        );
        let term_text = term.finish();
        assert!(!term_text.contains('\x1b'));
        assert!(!term_text.contains('\x07'));
        assert!(!term_text.contains('\r'));
        assert!(!term_text.contains('\u{202e}'));
        assert!(term_text.contains("EvilRedOverwrite Spoof: 9999Org"));
        assert!(term_text.contains("ptr.example fake"));

        let mut md = Out::markdown();
        md.section("System");
        md.field("  ", "Organization", payload);
        md.field("  ", "DMI", "<OUT OF SPEC> ![img](https://evil.test)");
        md.field("  ", "Note", "degraded PCIe [x4]");
        md.code_block("mermaid\x1b[31m", "graph LR\n  A[\"ok\x1b]0;x\x07\"] --> B");
        md.line("# Fake heading");
        md.line("> Fake quote");
        md.line("1. Fake list");
        md.line("1.1.1.1 (Cloudflare)");
        let md_text = md.finish();
        assert!(!md_text.contains('\x1b'));
        assert!(!md_text.contains('\x07'));
        assert!(
            md_text.contains("| Organization | EvilRedOverwrite Spoof: 9999Org |\n"),
            "{md_text}"
        );
        assert!(
            md_text.contains("| DMI | \\<OUT OF SPEC> !\\[img\\](https://evil.test) |\n"),
            "{md_text}"
        );
        assert!(
            md_text.contains("> **Note:** degraded PCIe [x4]\n"),
            "{md_text}"
        );
        assert!(
            md_text.contains("```mermaid\ngraph LR\n  A[\"ok\"] --> B\n```"),
            "{md_text}"
        );
        assert!(md_text.contains("\n\\# Fake heading\n"), "{md_text}");
        assert!(md_text.contains("\n\\> Fake quote\n"), "{md_text}");
        assert!(md_text.contains("\n1\\. Fake list\n"), "{md_text}");
        assert!(md_text.contains("\n1.1.1.1 (Cloudflare)\n"), "{md_text}");
    }
}
