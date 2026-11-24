//! The report's frame: its headings, the names of the test items, how each item ended, the
//! summary, and the words that every section shares.

use bench_tool::job;

use crate::orchestrator::Fault;
use crate::orchestrator::Unmet;

/// The unit in which a tool's time limit is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Seconds, for the tools that run for seconds.
    Seconds,
    /// Minutes, for Geekbench.
    Minutes,
}

/// How many test items ended in each way, and the run's total time, for the summary's last line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Totals {
    /// Every test item of the run.
    pub items: usize,
    /// Those that completed.
    pub completed: usize,
    /// Those that were skipped.
    pub skipped: usize,
    /// Those that failed.
    pub failed: usize,
    /// Those that timed out.
    pub timed_out: usize,
    /// Those that were interrupted.
    pub interrupted: usize,
    /// The whole run's time, in seconds.
    pub seconds: f64,
}

/// The report's frame and shared words.
pub trait ReportWords {
    /// The width that the labels of the report's fields pad to, in columns.
    fn label_width(&self) -> usize;
    /// A section's heading, such as `[System]`.
    fn section(&self, title: &str) -> String;
    /// The first section's title: the host.
    fn system_title(&self) -> String;
    /// The last section's title.
    fn summary_title(&self) -> String;
    /// A group's section title; `None` for a group that the catalog does not know, which shows
    /// its name instead.
    fn group_title(&self, group: &str) -> Option<&'static str>;
    /// A test item's name, by its id; `None` for an id that the catalog does not know.
    fn item_name(&self, id: &str) -> Option<&'static str>;
    /// The line after the version: where the program runs.
    fn environment(&self, platform: &str) -> String;
    /// The label of a note under a table or a field.
    fn note(&self) -> String;
    /// A value that could not be read.
    fn unavailable(&self) -> String;
    /// A value that could not be read, and why.
    fn unavailable_because(&self, why: &str) -> String;
    /// A value that its source does not give.
    fn unknown(&self) -> String;
    /// Items written as one list in running text, such as `sda, sdb`.
    fn and(&self, items: &[String]) -> String;
    /// A test item that completed, in `seconds`.
    fn completed(&self, seconds: f64) -> String;
    /// A test item that was skipped, and why.
    fn skipped(&self, reason: &str) -> String;
    /// A test item that failed, and why.
    fn failed(&self, reason: &str) -> String;
    /// A test item that ran out of time; what it published stays.
    fn timed_out(&self) -> String;
    /// A test item that Ctrl-C stopped, or that never started after it.
    fn interrupted(&self) -> String;
    /// A requirement that the host does not meet.
    fn unmet(&self, unmet: &Unmet) -> String;
    /// A fault around a test item, outside its own reasons.
    fn fault(&self, fault: &Fault) -> String;
    /// The test items made for other platforms, by name.
    fn not_applicable(&self, names: &[String]) -> String;
    /// The summary's last line.
    fn totals(&self, totals: &Totals) -> String;
    /// How a run of `tool` went wrong; `unit` tells its time limit.
    fn tool_fault(&self, tool: &str, fault: &job::Fault, unit: Unit) -> String;
    /// A run that worked after failed attempts: its name, the attempts that failed, and the
    /// last one's reason.
    fn retried(&self, run: &str, failures: usize, last: &str) -> String;
    /// A run whose every attempt failed: its name, the last attempt's reason, and the attempts.
    fn every_attempt_failed(&self, run: &str, last: &str, attempts: usize) -> String;
    /// The last attempt's reason, and how many attempts there were.
    fn attempts(&self, last: &str, attempts: usize) -> String;
    /// A size of a few bytes, below a KiB.
    fn bytes(&self, count: u64) -> String;
    /// Two-column Markdown table headers for key-value blocks (`Item`, `Value`).
    fn md_kv_headers(&self) -> [&'static str; 2];
    /// Section title for 5-second VM `%si` and `%st` CPU telemetry.
    fn vm_cpu_title(&self) -> &'static str;
    /// Method line explaining `/proc/stat` `%si` and `%st` sampling on virtual machines.
    fn vm_cpu_method(&self, interval_secs: u64) -> String;
    /// Subheading for aggregated `%si` / `%st` summary table.
    fn vm_cpu_summary_title(&self) -> &'static str;
    /// Headers for the VM CPU summary table.
    fn vm_cpu_summary_headers(&self) -> [&'static str; 5];
    /// Row label in the VM CPU summary table (`SoftIRQ (%si)` or `Steal (%st)`).
    fn vm_cpu_metric_label(&self, steal: bool) -> &'static str;
    /// Formats the peak item and timestamp cell in the VM CPU summary table.
    fn vm_cpu_peak_cell(&self, item: &str, time: &str) -> String;
    /// Subheading for the full 5-second `%si` / `%st` sample table in plain-text reports.
    fn vm_cpu_samples_title(&self) -> &'static str;
    /// Subheading for the Mermaid `%si` / `%st` timeline chart in Markdown reports.
    fn vm_cpu_timeline_title(&self) -> &'static str;
    /// Diagram title inside the Mermaid `xychart-beta` `%si` / `%st` chart.
    fn vm_cpu_chart_title(&self) -> &'static str;
    /// Y-axis title inside the Mermaid `xychart-beta` `%si` / `%st` chart.
    fn vm_cpu_y_axis_title(&self) -> &'static str;
    /// Headers for the 5-second `%si` / `%st` sample table.
    fn vm_cpu_samples_headers(&self) -> [&'static str; 4];
    /// Phase label shown for samples collected before the first plugin starts.
    fn vm_cpu_setup_phase(&self) -> &'static str;
}
