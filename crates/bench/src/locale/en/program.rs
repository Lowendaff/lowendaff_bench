//! English: the command line's help, `--list`, and the errors around the run.

use std::fmt::Display;
use std::path::Path;

use super::English;
use crate::locale::ProgramWords;

impl ProgramWords for English {
    fn about(&self) -> String {
        "Comprehensive test of a Linux server; run it as root.".to_owned()
    }

    fn help_only(&self) -> String {
        "Run only these test items: comma-separated ids or groups, such as cpu,disk.fio".to_owned()
    }

    fn help_skip(&self) -> String {
        "Skip these test items: comma-separated ids or groups, such as cpu.geekbench,ip.media"
            .to_owned()
    }

    fn help_output(&self) -> String {
        "Write the report to this file instead of standard output".to_owned()
    }

    fn help_format(&self) -> String {
        "Report format: text, markdown (or md), or json".to_owned()
    }

    fn help_config(&self) -> String {
        "Config file (TOML), with a table for each test item: [plugins.\"<id>\"]".to_owned()
    }

    fn help_work_dir(&self) -> String {
        "Create the run directory under this directory, and remove it at the end (avoid /tmp)"
            .to_owned()
    }

    fn help_list(&self) -> String {
        "List every test item, then exit".to_owned()
    }

    fn help_verbose(&self) -> String {
        "Log more detail: each download's address and retries, and each tool's command and exit \
         code"
            .to_owned()
    }

    fn help_show_ip(&self) -> String {
        "Show the public addresses in full in the report (only their first part by default)"
            .to_owned()
    }

    fn help_download_connections(&self) -> String {
        "The most connections that the download of one file uses at once (1 to 256)".to_owned()
    }

    fn help_countries(&self) -> String {
        "The countries whose servers the network speed test uses, by ISO 3166 code, or all; \
         the list's default servers otherwise"
            .to_owned()
    }

    fn list(&self, items: &[String]) -> String {
        items.join(", ")
    }

    fn item_count(&self, count: usize) -> String {
        super::count(count, "test item", "test items")
    }

    fn group_line(&self, title: &str, name: &str) -> String {
        format!("[{title}] (group: {name})")
    }

    fn item_line(
        &self,
        id: &str,
        name: &str,
        platforms: &[String],
        numa: bool,
        tools: &[String],
    ) -> String {
        let mut line = format!("  {id}  {name}");
        if !platforms.is_empty() {
            line.push_str(&format!(" (applies to: {})", self.list(platforms)));
        }
        if numa {
            line.push_str(" (applies to: several NUMA nodes)");
        }
        if !tools.is_empty() {
            line.push_str(&format!(" (tools: {})", self.list(tools)));
        }
        line
    }

    fn unknown_selection(&self, name: &str) -> String {
        format!("unknown test item or group `{name}`")
    }

    fn config_unreadable(&self, path: &Path, error: &dyn Display) -> String {
        format!("cannot read the config file {}: {error}", path.display())
    }

    fn output_unwritable(&self, path: &Path, error: &dyn Display) -> String {
        format!("cannot write the report to {}: {error}", path.display())
    }

    fn config_unparsable(&self, path: &Path, error: &dyn Display) -> String {
        format!("cannot parse the config file {}: {error}", path.display())
    }

    fn config_unknown_key(&self, key: &str) -> String {
        format!(
            "unknown key `{key}` in the config file; only [plugins.\"<id>\"] tables are supported"
        )
    }

    fn config_plugins_not_table(&self) -> String {
        "plugins in the config file must be a table".to_owned()
    }

    fn config_unknown_item(&self, id: &str) -> String {
        format!("the config file has a table for the unknown test item `{id}`")
    }

    fn config_invalid(&self, id: &str, error: &dyn Display) -> String {
        format!("invalid settings for test item {id}: {error}")
    }

    fn unknown_item(&self, id: &str) -> String {
        format!("unknown test item `{id}`")
    }

    fn not_root(&self) -> String {
        "lowendaff-bench must run as root".to_owned()
    }

    fn runtime_failed(&self, error: &dyn Display) -> String {
        format!("cannot start the runtime: {error}")
    }

    fn host_failed(&self, error: &dyn Display) -> String {
        format!("cannot read the host profile: {error}")
    }

    fn network_failed(&self, error: &dyn Display) -> String {
        format!("cannot probe the network: {error}")
    }

    fn run_dir_failed(&self, dir: &Path, error: &dyn Display) -> String {
        format!(
            "cannot create a run directory in {}: {error}",
            dir.display()
        )
    }

    fn tools_dir_failed(&self, error: &dyn Display) -> String {
        format!("cannot create the tools directory: {error}")
    }

    fn http_client_failed(&self, error: &dyn Display) -> String {
        format!("cannot set up the HTTP client: {error}")
    }

    fn tls_failed(&self, error: &dyn Display) -> String {
        format!("cannot set up TLS: {error}")
    }

    fn panic_without_text(&self) -> String {
        "(a panic without a text message)".to_owned()
    }
}
