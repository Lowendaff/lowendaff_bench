//! The terminal report (and shared layout primitives for Markdown), written once when every
//! plugin is done: the host profile, a section per group of plugins, in an order of its own, then
//! the summary. Its words come from the catalog (`crate::locale`). Columns line up by display
//! width, so that wide characters line up too, and tables wrap to the terminal's width when there
//! is one.

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
    let mut out = Out::terminal(width);
    write_page(&mut out, page);
    out.text
}

/// The report as GitHub-Flavored Markdown.
pub(super) fn render_markdown(page: &Page) -> String {
    let mut out = Out::markdown();
    write_page(&mut out, page);
    out.text
}

fn write_page(out: &mut Out, page: &Page) {
    let words = locale::words();
    out.title(&format!("lowendaff-bench {}", page.version));
    let platform = host::platform_text(page.host.platform);
    out.line(&words.environment(&platform));
    host::section(out, page.host, page.show_ip);
    let groups: Vec<&str> = page.runs.iter().map(|run| run.group).collect();
    for (group, title) in sections(&groups) {
        let runs: Vec<&Run<Measured>> = page.runs.iter().filter(|run| run.group == group).collect();
        out.section(title);
        let several = runs.len() > 1;
        for run in runs {
            let indent = if several {
                out.subsection(run.title.unwrap_or(run.id));
                "    "
            } else {
                "  "
            };
            plugin(out, run, indent, page);
        }
    }
    summary::section(out, page);
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
        Some(Measured::NetworkTrace(report)) => {
            let own = collect_own_addresses(page.host, page.runs);
            network::trace(out, report, indent, &own, page.show_ip);
        }
        Some(Measured::NetworkBgp(report)) => network::bgp(out, report, indent, page.show_ip),
        Some(Measured::IpQuality(report)) => ip::quality(out, report, indent, page.show_ip),
        Some(Measured::IpMedia(report)) => ip::media(out, report, indent),
        None => {}
    }
    out.line(&format!("{indent}{}", run_status(run)));
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

/// Previous block emitted in Markdown mode, for spacing list items and paragraphs cleanly.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Empty,
    Heading,
    Line,
    Field,
    Table,
}

/// Target syntax for [`Out`].
enum Mode {
    Terminal { width: Option<u16> },
    Markdown { depth: usize, last: Block },
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
            },
        }
    }

    /// Top-level document title (`lowendaff-bench <version>`).
    fn title(&mut self, text: &str) {
        match &mut self.mode {
            Mode::Terminal { .. } => self.line(text),
            Mode::Markdown { depth, last } => {
                let _ = writeln!(self.text, "# {}", escape_md_text(text));
                *depth = 1;
                *last = Block::Heading;
            }
        }
    }

    /// Major report section (`[System]`, `[CPU performance]`, `[Summary]`).
    fn section(&mut self, title: &str) {
        match &mut self.mode {
            Mode::Terminal { .. } => {
                let words = locale::words();
                self.text.push('\n');
                self.line(&words.section(title));
            }
            Mode::Markdown { depth, last } => {
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let _ = writeln!(self.text, "## {}", escape_md_text(title));
                *depth = 2;
                *last = Block::Heading;
            }
        }
    }

    /// Emits a blank separator line in terminal mode (Markdown blocks manage their own spacing).
    fn gap(&mut self) {
        if matches!(self.mode, Mode::Terminal { .. }) {
            self.text.push('\n');
        }
    }

    /// Plugin subheading inside a multi-plugin section.
    fn subsection(&mut self, title: &str) {
        match &mut self.mode {
            Mode::Terminal { .. } => self.line(&format!("  {title}")),
            Mode::Markdown { depth, last } => {
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let _ = writeln!(self.text, "### {}", escape_md_text(title));
                *depth = 3;
                *last = Block::Heading;
            }
        }
    }

    /// Table or sub-group heading inside the current section or plugin block.
    fn heading(&mut self, indent: &str, title: &str) {
        match &mut self.mode {
            Mode::Terminal { .. } => self.line(&format!("{indent}{title}")),
            Mode::Markdown { depth, last } => {
                if !self.text.is_empty() {
                    self.text.push('\n');
                }
                let hashes = "#".repeat(*depth + 1);
                let _ = writeln!(self.text, "{hashes} {}", escape_md_text(title));
                *last = Block::Heading;
            }
        }
    }

    fn line(&mut self, line: &str) {
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

    /// A label padded to the catalog's label width, and its value.
    fn field(&mut self, indent: &str, label: &str, value: &str) {
        self.field_width(indent, locale::words().label_width(), label, value);
    }

    /// A label padded to `width` columns in terminal mode, and its value.
    fn field_width(&mut self, indent: &str, width: usize, label: &str, value: &str) {
        match &mut self.mode {
            Mode::Terminal { .. } => {
                let clean_label = strip_terminal_controls(label, false);
                let clean_value = strip_terminal_controls(value, false);
                let padded = pad(&clean_label, width);
                self.line(&format!("{indent}{padded}  {clean_value}"));
            }
            Mode::Markdown { last, .. } => {
                if *last != Block::Field && !self.text.is_empty() {
                    self.text.push('\n');
                }
                let _ = writeln!(
                    self.text,
                    "- **{}:** {}",
                    escape_md_text(label),
                    escape_md_text(value)
                );
                *last = Block::Field;
            }
        }
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

/// Escapes inline Markdown / GFM metacharacters (`\`, `*`, `<`, `[`, `]`, `` ` ``, plus `_` and
/// `~` outside bare `http(s)://` URLs and intraword identifiers) after stripping terminal escape
/// and control sequences.
fn escape_md_text(text: &str) -> String {
    let clean = strip_terminal_controls(text, false);
    escape_md_inline(&clean, false)
}

/// Escapes a standalone Markdown paragraph line, neutralizing leading CommonMark block markers
/// (`#`, `>`, `-`, `+`, or ordered-list `1.`/`1)`) in addition to inline metacharacters.
fn escape_md_line(trimmed: &str) -> String {
    let escaped = escape_md_text(trimmed);
    let needs_prefix = match escaped.as_bytes().first().copied() {
        Some(b'#' | b'>' | b'-' | b'+') => true,
        Some(b'0'..=b'9') => {
            let digits = escaped.bytes().take_while(u8::is_ascii_digit).count();
            matches!(escaped.as_bytes().get(digits), Some(b'.' | b')'))
        }
        _ => false,
    };
    if needs_prefix {
        format!("\\{escaped}")
    } else {
        escaped
    }
}

/// Escapes pipe-table cell characters (`\`, `|`, inline Markdown metacharacters, and newlines)
/// after stripping terminal escape and control sequences.
fn escape_md_cell(cell: &str) -> String {
    let clean = strip_terminal_controls(cell, true);
    escape_md_inline(&clean, true)
}

fn escape_md_inline(clean: &str, in_cell: bool) -> String {
    let chars: Vec<char> = clean.chars().collect();
    let mut in_url = vec![false; chars.len()];
    let mut idx = 0;
    while idx < chars.len() {
        let rest: String = chars[idx..].iter().take(8).collect();
        if rest.starts_with("https://") || rest.starts_with("http://") {
            while idx < chars.len() && !chars[idx].is_whitespace() {
                in_url[idx] = true;
                idx += 1;
            }
        } else {
            idx += 1;
        }
    }

    let mut out = String::with_capacity(clean.len());
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '|' if in_cell => out.push_str("\\|"),
            '\n' if in_cell => out.push_str("<br>"),
            '*' => out.push_str("\\*"),
            '<' => out.push_str("\\<"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '`' => out.push_str("\\`"),
            '~' if !in_url[i] => out.push_str("\\~"),
            '_' if !in_url[i] => {
                let prev_alnum = i > 0 && chars[i - 1].is_alphanumeric();
                let next_alnum = i + 1 < chars.len() && chars[i + 1].is_alphanumeric();
                if prev_alnum && next_alnum {
                    out.push('_');
                } else {
                    out.push_str("\\_");
                }
            }
            other => out.push(other),
        }
    }
    out
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
        let mut out = Out::terminal(Some(14));
        out.table("  ", &["Headline abcd".to_owned(), "x".to_owned()], &[]);
        assert_eq!(out.text, "  Headline   x\n  abcd\n");
    }

    #[test]
    fn a_wide_character_takes_two_columns() {
        // As an emoji or a CJK character does in a terminal.
        assert_eq!(super::pad("\u{1F600}x", 5), "\u{1F600}x  ");
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
        assert!(!term.text.contains('\x1b'));
        assert!(!term.text.contains('\x07'));
        assert!(!term.text.contains('\r'));
        assert!(!term.text.contains('\u{202e}'));
        assert!(term.text.contains("EvilRedOverwrite Spoof: 9999Org"));
        assert!(term.text.contains("ptr.example fake"));

        let mut md = Out::markdown();
        md.field("  ", "Organization", payload);
        md.code_block("mermaid\x1b[31m", "graph LR\n  A[\"ok\x1b]0;x\x07\"] --> B");
        assert!(!md.text.contains('\x1b'));
        assert!(!md.text.contains('\x07'));
        assert!(
            md.text
                .contains("- **Organization:** EvilRedOverwrite Spoof: 9999Org")
        );
        assert!(
            md.text
                .contains("```mermaid\ngraph LR\n  A[\"ok\"] --> B\n```")
        );

        md.field("  ", "DMI", "<OUT OF SPEC> ![img](https://evil.test)");
        md.line("# Fake heading");
        md.line("> Fake quote");
        assert!(
            md.text
                .contains("- **DMI:** \\<OUT OF SPEC> !\\[img\\](https://evil.test)\n"),
            "{}",
            md.text
        );
        assert!(md.text.contains("\n\\# Fake heading\n"), "{}", md.text);
        assert!(md.text.contains("\n\\> Fake quote\n"), "{}", md.text);
    }
}
