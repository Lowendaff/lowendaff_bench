//! English: the command line's help, `--list`, and the errors around the run.

use std::fmt::Display;
use std::path::Path;

use super::English;
use crate::locale::ProgramWords;

impl ProgramWords for English {
    fn about(&self) -> String {
        "Comprehensive test of a Linux server; run it as root (starts interactive TUI when given \
         no options)."
            .to_owned()
    }

    fn help_all(&self) -> String {
        "Run every test item in non-interactive CLI mode".to_owned()
    }

    fn help_only(&self) -> String {
        "Run only these test items: comma-separated groups (cpu,memory,disk,network,ip), ids \
         (such as cpu.bench,disk.fio), or all"
            .to_owned()
    }

    fn help_skip(&self) -> String {
        "Skip these test items: comma-separated groups, ids, or all (such as cpu.bench,ip.media)"
            .to_owned()
    }

    fn help_output(&self) -> String {
        "Write the report to this file instead of standard output (otherwise prints to stdout \
         and saves a copy in /tmp)"
            .to_owned()
    }

    fn help_format(&self) -> String {
        "Report format: text, markdown (or md), or json".to_owned()
    }

    fn help_lang(&self) -> String {
        "Interface and report language: en (default), zh-cn, zh-tw, or ja (JSON stays English)"
            .to_owned()
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

    fn help_socks5_proxy(&self) -> String {
        "SOCKS5 proxy for tool and list downloads only (socks5:// or socks5h://; never used for \
         benchmarks)"
            .to_owned()
    }

    fn help_github_proxy(&self) -> String {
        "GitHub mirror prefix URL for GitHub tool and list downloads (e.g. https://ghfast.top/)"
            .to_owned()
    }

    fn invalid_socks5_proxy(&self) -> String {
        "invalid --socks5-proxy: expected socks5://[user:pass@]host[:port] or \
         socks5h://[user:pass@]host[:port]"
            .to_owned()
    }

    fn invalid_github_proxy(&self) -> String {
        "invalid --github-proxy: expected an http:// or https:// base URL without credentials, \
         such as https://ghfast.top/"
            .to_owned()
    }

    fn help_cpu_bench(&self) -> String {
        "Sub-suites for cpu.bench: comma-separated sysbench, 7zip, unixbench, hpl, passmark, \
         ycruncher, prime95, geekbench, or all"
            .to_owned()
    }

    fn help_countries(&self) -> String {
        "The countries whose servers network.iperf3 uses, by ISO 3166 code (e.g. US,JP), or all"
            .to_owned()
    }

    fn help_taier(&self) -> String {
        "Target selectors for network.taier as <place>[:<op>[+<op>...]] (e.g. \
         beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct,all:cm)"
            .to_owned()
    }

    fn help_taier_list(&self) -> String {
        "List available network.taier regions (or cities in PROVINCE), then exit".to_owned()
    }

    fn help_taier_cities(&self) -> String {
        "Cities, provinces, or location IDs for network.taier (e.g. Beijing,Shanghai,Jiangsu), \
         or all"
            .to_owned()
    }

    fn help_taier_operators(&self) -> String {
        "Carriers for network.taier: comma-separated telecom, unicom, mobile, cernet, cbn, \
         other, or all"
            .to_owned()
    }

    fn help_no_cloudflare_udp(&self) -> String {
        "Skip the RFC 5389 STUN UDP packet-loss probe in network.cloudflare".to_owned()
    }

    fn help_inetspeed_threads(&self) -> String {
        "Concurrent worker threads for multi-thread rounds in network.inetspeed (1 to 64)"
            .to_owned()
    }

    fn help_inetspeed_endpoint(&self) -> String {
        "Pin a specific Apple CDN IPv4 or IPv6 endpoint IP in network.inetspeed, skipping DoH"
            .to_owned()
    }

    fn invalid_inetspeed_endpoint(&self, raw: &str) -> String {
        format!("invalid --inetspeed-endpoint `{raw}`: expected an IPv4 or IPv6 address")
    }

    fn help_tcping_groups(&self) -> String {
        "Target groups for network.tcping: comma-separated china, china-premium, global, or all"
            .to_owned()
    }

    fn help_tcping_all(&self) -> String {
        "Expand network.tcping to all 31 mainland China provinces and secondary global hubs"
            .to_owned()
    }

    fn help_tcping_count(&self) -> String {
        "TCP handshake probes per target in network.tcping (5 to 50, default 10)".to_owned()
    }

    fn help_trace_groups(&self) -> String {
        "Target groups for network.trace: comma-separated services, china, international, or all"
            .to_owned()
    }

    fn help_no_trace_tcp(&self) -> String {
        "Skip TCP SYN large-packet route traces to mainland China targets in network.trace"
            .to_owned()
    }

    fn help_trace_packet_size(&self) -> String {
        "TCP probe packet size in bytes for mainland China targets in network.trace (64 to 1500, \
         default 1024)"
            .to_owned()
    }

    fn help_media_groups(&self) -> String {
        "Regional groups for ip.media: comma-separated global, asia, europe, americas, oceania, \
         or all"
            .to_owned()
    }

    fn help_no_dnsbl(&self) -> String {
        "Skip DNSBL blacklist lookups in ip.quality".to_owned()
    }

    fn help_no_mail(&self) -> String {
        "Skip outbound TCP port-25 mail checks in ip.quality".to_owned()
    }

    fn help_no_bgp_upload(&self) -> String {
        "Skip uploading BGP topology SVG diagrams to imageupload.app in text mode".to_owned()
    }

    fn tui_not_a_tty(&self) -> String {
        "interactive mode requires a terminal (TTY); pass --all or --only <ITEM> for CLI mode"
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

    fn output_unwritable(&self, path: &Path, error: &dyn Display) -> String {
        format!("cannot write the report to {}: {error}", path.display())
    }

    fn overrides_invalid(&self, error: &dyn Display) -> String {
        format!("invalid plugin overrides: {error}")
    }

    fn config_unknown_item(&self, id: &str) -> String {
        format!("settings given for unknown test item `{id}`")
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
