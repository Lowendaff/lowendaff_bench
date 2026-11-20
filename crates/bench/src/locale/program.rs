//! The command line's help, `--list`, and the errors of the command line and the run itself.

use std::fmt::Display;
use std::path::Path;

/// The command line's help, `--list`, and the errors around the run.
pub trait ProgramWords {
    /// What the program is, at the top of its help.
    fn about(&self) -> String;
    /// The help of `--all` / `-a`.
    fn help_all(&self) -> String;
    /// The help of `--only`.
    fn help_only(&self) -> String;
    /// The help of `--skip`.
    fn help_skip(&self) -> String;
    /// The help of `-o` / `--output`.
    fn help_output(&self) -> String;
    /// The help of `-f` / `--format`.
    fn help_format(&self) -> String;
    /// The help of `--work-dir`.
    fn help_work_dir(&self) -> String;
    /// The help of `--list`.
    fn help_list(&self) -> String;
    /// The help of `-v`.
    fn help_verbose(&self) -> String;
    /// The help of `--show-ip`.
    fn help_show_ip(&self) -> String;
    /// The help of `--download-connections`.
    fn help_download_connections(&self) -> String;
    /// The help of `--cpu-bench`.
    fn help_cpu_bench(&self) -> String;
    /// The help of `--countries`.
    fn help_countries(&self) -> String;
    /// The help of `--trace-groups`.
    fn help_trace_groups(&self) -> String;
    /// The help of `--media-groups`.
    fn help_media_groups(&self) -> String;
    /// The help of `--no-dnsbl`.
    fn help_no_dnsbl(&self) -> String;
    /// The help of `--no-mail`.
    fn help_no_mail(&self) -> String;
    /// The help of `--no-bgp-upload`.
    fn help_no_bgp_upload(&self) -> String;
    /// Interactive mode started without an interactive terminal.
    fn tui_not_a_tty(&self) -> String;
    /// Items written as one list, as in "sysbench 1.0.20, fio 3.43".
    fn list(&self, items: &[String]) -> String;
    /// The first line of `--list`: how many test items there are.
    fn item_count(&self, count: usize) -> String;
    /// A group's line in `--list`: its title, and the name that `--only` and `--skip` take.
    fn group_line(&self, title: &str, name: &str) -> String;
    /// An item's line in `--list`: its id and name, the platforms that it applies to (none when
    /// it applies to every one), whether it needs several NUMA nodes, and the tools that it runs
    /// with their versions.
    fn item_line(
        &self,
        id: &str,
        name: &str,
        platforms: &[String],
        numa: bool,
        tools: &[String],
    ) -> String;
    /// A name for `--only` or `--skip` that is no test item or group.
    fn unknown_selection(&self, name: &str) -> String;
    /// An `--output` file that cannot be created for writing.
    fn output_unwritable(&self, path: &Path, error: &dyn Display) -> String;
    /// Invalid JSON passed via `--internal-overrides`.
    fn overrides_invalid(&self, error: &dyn Display) -> String;
    /// A settings table for a test item that does not exist.
    fn config_unknown_item(&self, id: &str) -> String;
    /// A test item's settings that it does not accept.
    fn config_invalid(&self, id: &str, error: &dyn Display) -> String;
    /// A test item that the selection names but the registry lacks.
    fn unknown_item(&self, id: &str) -> String;
    /// The program, run by another user than root.
    fn not_root(&self) -> String;
    /// The async runtime, which could not start.
    fn runtime_failed(&self, error: &dyn Display) -> String;
    /// The host profile, which could not be read.
    fn host_failed(&self, error: &dyn Display) -> String;
    /// The network probe, which could not finish.
    fn network_failed(&self, error: &dyn Display) -> String;
    /// The run directory, which could not be created under `dir`.
    fn run_dir_failed(&self, dir: &Path, error: &dyn Display) -> String;
    /// The tools' directory, which could not be created.
    fn tools_dir_failed(&self, error: &dyn Display) -> String;
    /// The HTTP client, which could not be set up.
    fn http_client_failed(&self, error: &dyn Display) -> String;
    /// The TLS configuration, which was refused.
    fn tls_failed(&self, error: &dyn Display) -> String;
    /// A plugin's panic whose message is no text.
    fn panic_without_text(&self) -> String;
}
