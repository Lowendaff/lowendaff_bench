//! Every text shown by the interactive terminal interface.

use std::path::Path;

/// Texts of the interactive terminal interface: configuration, live progress, and report viewer.
pub trait TuiWords {
    /// Header title in configuration mode.
    fn tui_config_title(&self, version: &str) -> String;
    /// Header title while the benchmark is running.
    fn tui_running_title(&self, version: &str, elapsed: &str) -> String;
    /// Header title in the finished report viewer.
    fn tui_results_title(&self, version: &str) -> String;
    /// Badge showing how many items are enabled out of the total.
    fn tui_enabled_badge(&self, enabled: usize, total: usize) -> String;
    /// Notice shown when the terminal window is smaller than the minimum layout size.
    fn tui_terminal_too_small(&self, cols: u16, rows: u16) -> String;
    /// Left pane title in configuration mode.
    fn tui_items_pane(&self) -> &'static str;
    /// Action row at the top of the left list that starts the benchmark.
    fn tui_start_action(&self, enabled: usize) -> String;
    /// Left list row for global run and report options.
    fn tui_global_row(&self) -> &'static str;
    /// Right pane title when the start action is highlighted.
    fn tui_overview_pane(&self) -> &'static str;
    /// Right pane title when global options are highlighted.
    fn tui_global_pane(&self) -> &'static str;
    /// Right pane title for a highlighted test item.
    fn tui_params_pane(&self, id: &str) -> String;
    /// Summary lines shown in the right pane when the start action is highlighted.
    fn tui_overview_lines(
        &self,
        enabled: usize,
        total: usize,
        customized: usize,
        format: &str,
        output: &str,
        work_dir: &str,
    ) -> Vec<String>;
    /// Metadata line naming a test item's category and time limit.
    fn tui_item_meta(&self, group: &str, timeout_mins: u64) -> String;
    /// Metadata line naming a test item's host requirements, or all hosts when empty.
    fn tui_item_requires(&self, requirements: &str) -> String;
    /// Text when a test item runs on every Linux host without special requirements.
    fn tui_any_host(&self) -> &'static str;
    /// Requirement label for IPv4 connectivity.
    fn tui_req_ipv4(&self) -> &'static str;
    /// Requirement label for IPv6 connectivity.
    fn tui_req_ipv6(&self) -> &'static str;
    /// Requirement label for any internet connectivity.
    fn tui_req_internet(&self) -> &'static str;
    /// Requirement label for several NUMA nodes.
    fn tui_req_numa(&self) -> &'static str;
    /// Requirement label for minimum glibc version.
    fn tui_req_glibc(&self, major: u32, minor: u32) -> String;
    /// Note shown for plugins whose measurement parameters are fixed for comparability.
    fn tui_fixed_params_note(&self) -> &'static str;
    /// Boolean value shown as enabled/yes.
    fn tui_yes(&self) -> &'static str;
    /// Boolean value shown as disabled/no.
    fn tui_no(&self) -> &'static str;
    /// Field label: whether a test item is enabled.
    fn tui_field_enabled(&self) -> &'static str;
    /// `cpu.bench` field: sub-suite toggle.
    fn tui_field_cpu_suite(&self, suite: &str) -> String;
    /// `network.iperf3` field: ISO country codes or `all`.
    fn tui_field_countries(&self) -> &'static str;
    /// `network.iperf3` field: specific server IDs.
    fn tui_field_servers(&self) -> &'static str;
    /// `network.iperf3` field: test duration per direction in seconds.
    fn tui_field_seconds(&self) -> &'static str;
    /// `network.iperf3` field: parallel TCP streams.
    fn tui_field_streams(&self) -> &'static str;
    /// `network.iperf3` field: per-stream bandwidth cap in Mbit/s.
    fn tui_field_rate(&self) -> &'static str;
    /// `network.taier` field: carrier toggle.
    fn tui_field_taier_operator(&self, operator: &str) -> String;
    /// `network.taier` field: city/province filter or `all`.
    fn tui_field_taier_cities(&self) -> &'static str;
    /// `network.cloudflare` field: concurrent HTTP workers.
    fn tui_field_cf_concurrency(&self) -> &'static str;
    /// `network.cloudflare` field: STUN UDP packet-loss probe toggle.
    fn tui_field_cf_udp(&self) -> &'static str;
    /// `network.inetspeed` field: multi-thread worker count.
    fn tui_field_inet_threads(&self) -> &'static str;
    /// `network.inetspeed` field: per-thread timeout in seconds.
    fn tui_field_inet_timeout(&self) -> &'static str;
    /// `network.inetspeed` field: per-thread byte cap (`2G`, `500M`, `1GiB`).
    fn tui_field_inet_max(&self) -> &'static str;
    /// `network.inetspeed` field: idle latency sample count.
    fn tui_field_inet_latency_count(&self) -> &'static str;
    /// `network.inetspeed` field: pinned endpoint IP override.
    fn tui_field_inet_endpoint(&self) -> &'static str;
    /// `network.trace` field: target group toggle.
    fn tui_field_trace_group(&self, group: &str) -> String;
    /// `network.trace` field: specific target IDs.
    fn tui_field_targets(&self) -> &'static str;
    /// `network.trace` field: hop ASN and geolocation lookup.
    fn tui_field_geo(&self) -> &'static str;
    /// `network.bgp` field: upload topology SVG to `imageupload.app` in `text` mode.
    fn tui_field_bgp_upload(&self) -> &'static str;
    /// `ip.quality` field: 25 DNSBL blacklist lookups.
    fn tui_field_dnsbl(&self) -> &'static str;
    /// `ip.quality` field: outbound TCP port-25 mail checks.
    fn tui_field_mail(&self) -> &'static str;
    /// `ip.media` field: regional group toggle.
    fn tui_field_media_group(&self, group: &str) -> String;
    /// `ip.media` field: specific service IDs.
    fn tui_field_services(&self) -> &'static str;
    /// Global field: report format.
    fn tui_field_format(&self) -> &'static str;
    /// Global field: optional output file path.
    fn tui_field_output(&self) -> &'static str;
    /// Global field: temporary run directory parent.
    fn tui_field_work_dir(&self) -> &'static str;
    /// Global field: unmask public IP addresses in the report.
    fn tui_field_show_ip(&self) -> &'static str;
    /// Global field: max parallel connections per tool download.
    fn tui_field_connections(&self) -> &'static str;
    /// Global field: SOCKS5 proxy for tool and list downloads.
    fn tui_field_socks5_proxy(&self) -> &'static str;
    /// Global field: GitHub mirror URL prefix for GitHub downloads.
    fn tui_field_github_proxy(&self) -> &'static str;
    /// Global field: verbose tool and probe debug logging.
    fn tui_field_verbose(&self) -> &'static str;
    /// Placeholder value for default curated iperf3 locations.
    fn tui_default_locations(&self) -> &'static str;
    /// Placeholder value for default Taier cities.
    fn tui_default_taier_cities(&self) -> &'static str;
    /// Placeholder value for automatic Dual DoH Apple CDN discovery.
    fn tui_default_inet_endpoint(&self) -> &'static str;
    /// Placeholder value when an override list is unset.
    fn tui_default_none(&self) -> &'static str;
    /// Placeholder value when rate limit is unset.
    fn tui_unlimited(&self) -> &'static str;
    /// Note explaining `cpu.bench` default and opt-in sub-suites.
    fn tui_default_cpu_note(&self) -> &'static str;
    /// Note explaining `network.taier` default cities and carriers.
    fn tui_default_taier_note(&self) -> &'static str;
    /// Note explaining `network.cloudflare` test phases.
    fn tui_default_cloudflare_note(&self) -> &'static str;
    /// Note explaining `network.inetspeed` Dual DoH discovery and 1T/NT rounds.
    fn tui_default_inetspeed_note(&self) -> &'static str;
    /// Placeholder value when trace groups use default cache + carrier list.
    fn tui_default_trace_note(&self) -> &'static str;
    /// Note explaining `network.bgp` topology SVG upload in `text` mode.
    fn tui_default_bgp_note(&self) -> &'static str;
    /// Placeholder value when media groups test all regions by default.
    fn tui_default_media_note(&self) -> &'static str;
    /// Value text when public IPs are masked.
    fn tui_masked(&self) -> &'static str;
    /// Value text when public IPs are shown in full.
    fn tui_unmasked(&self) -> &'static str;
    /// Title of the inline edit prompt.
    fn tui_edit_title(&self, field: &str) -> String;
    /// Hint inside the inline edit prompt.
    fn tui_edit_hint(&self) -> &'static str;
    /// Validation error when no test item is enabled.
    fn tui_no_items_selected(&self) -> String;
    /// Validation error when `cpu.bench` is enabled with all sub-suites unchecked.
    fn tui_no_cpu_suites_selected(&self) -> String;
    /// Validation error when `network.taier` is enabled with all carriers unchecked.
    fn tui_no_taier_operators_selected(&self) -> String;
    /// Validation error for an integer field outside `min..=max`.
    fn tui_invalid_number(&self, field: &str, min: u32, max: u32) -> String;
    /// Validation error when `work_dir` is empty.
    fn tui_empty_work_dir(&self) -> String;
    /// Status message when an item or global options were reset to defaults.
    fn tui_reset_done(&self, target: &str) -> String;
    /// Keybar help on the configuration screen (left list focused).
    fn tui_keys_config_list(&self) -> &'static str;
    /// Keybar help on the configuration screen (right parameter pane focused).
    fn tui_keys_config_params(&self) -> &'static str;
    /// Left pane title while running.
    fn tui_progress_pane(&self, done: usize, total: usize) -> String;
    /// Right pane title for the live event log.
    fn tui_log_pane(&self, following: bool) -> &'static str;
    /// Host probe status line before the host profile finishes loading.
    fn tui_collecting_host(&self) -> &'static str;
    /// Status badge for a queued item.
    fn tui_state_queued(&self) -> &'static str;
    /// Status badge for a currently running item.
    fn tui_state_running(&self) -> &'static str;
    /// Banner shown after the first Ctrl-C while stopping the active test.
    fn tui_stopping_banner(&self) -> &'static str;
    /// Keybar help while running.
    fn tui_keys_running(&self) -> &'static str;
    /// Left sidebar title in the results viewer.
    fn tui_sections_pane(&self) -> &'static str;
    /// Entry in the results sidebar that jumps to the top of the report.
    fn tui_section_top(&self) -> &'static str;
    /// Right pane title in the results viewer showing active view settings.
    fn tui_report_pane(&self, format: &str, wrapped: bool, show_ip: bool) -> String;
    /// Title of the save-report prompt in the results viewer.
    fn tui_save_prompt_title(&self) -> &'static str;
    /// Confirmation message after writing the report to `path`.
    fn tui_saved_to(&self, path: &Path) -> String;
    /// Keybar help in the results viewer.
    fn tui_keys_results(&self) -> &'static str;
}
