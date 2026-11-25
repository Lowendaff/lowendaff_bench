//! English texts for the interactive terminal interface.

use std::path::Path;

use super::English;
use crate::locale::TuiWords;

impl TuiWords for English {
    fn tui_config_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  Interactive Setup")
    }

    fn tui_running_title(&self, version: &str, elapsed: &str) -> String {
        format!("lowendaff-bench {version}  Running {elapsed}")
    }

    fn tui_results_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  Report Viewer")
    }

    fn tui_enabled_badge(&self, enabled: usize, total: usize) -> String {
        format!("{enabled}/{total} enabled")
    }

    fn tui_terminal_too_small(&self, cols: u16, rows: u16) -> String {
        format!("Terminal window ({cols}x{rows}) is too small; resize to at least 60x16 or press q")
    }

    fn tui_items_pane(&self) -> &'static str {
        "Test Items & Options"
    }

    fn tui_start_action(&self, enabled: usize) -> String {
        format!("Start benchmark ({enabled} selected)")
    }

    fn tui_global_row(&self) -> &'static str {
        "Run & report options"
    }

    fn tui_overview_pane(&self) -> &'static str {
        "Run Overview"
    }

    fn tui_global_pane(&self) -> &'static str {
        "Global Run & Report Options"
    }

    fn tui_params_pane(&self, id: &str) -> String {
        format!("Item Settings ({id})")
    }

    fn tui_overview_lines(
        &self,
        enabled: usize,
        total: usize,
        customized: usize,
        format: &str,
        output: &str,
        work_dir: &str,
    ) -> Vec<String> {
        vec![
            format!("Selected items:   {enabled} of {total}"),
            format!("Customized items: {customized}"),
            format!("Report format:    {format}"),
            format!("Output file:      {output}"),
            format!("Work directory:   {work_dir}"),
            String::new(),
            "Press Enter or r to run the selected items in the standard order.".to_owned(),
            "Move Down to enable/disable items or adjust per-item and global parameters."
                .to_owned(),
            "Shortcut g toggles an entire category; a toggles all items; d resets defaults."
                .to_owned(),
        ]
    }

    fn tui_item_meta(&self, group: &str, timeout_mins: u64) -> String {
        format!("Group: {group}  |  Timeout: {timeout_mins} min")
    }

    fn tui_item_requires(&self, requirements: &str) -> String {
        format!("Requires: {requirements}")
    }

    fn tui_any_host(&self) -> &'static str {
        "all Linux hosts"
    }

    fn tui_req_ipv4(&self) -> &'static str {
        "IPv4"
    }

    fn tui_req_ipv6(&self) -> &'static str {
        "IPv6"
    }

    fn tui_req_internet(&self) -> &'static str {
        "internet"
    }

    fn tui_req_numa(&self) -> &'static str {
        "several NUMA nodes"
    }

    fn tui_req_glibc(&self, major: u32, minor: u32) -> String {
        format!("glibc >= {major}.{minor}")
    }

    fn tui_fixed_params_note(&self) -> &'static str {
        "Uses standardized parameters so scores stay comparable across servers."
    }

    fn tui_yes(&self) -> &'static str {
        "Yes"
    }

    fn tui_no(&self) -> &'static str {
        "No"
    }

    fn tui_field_enabled(&self) -> &'static str {
        "Enabled"
    }

    fn tui_field_cpu_suite(&self, suite: &str) -> String {
        let name = match suite {
            "sysbench" => "sysbench (prime sieve)",
            "7zip" => "7-Zip (LZMA2 MIPS)",
            "unixbench" => "UnixBench (12-item index)",
            "hpl" => "HPL (OpenBLAS GFLOPS)",
            "passmark" => "PassMark (CPU Mark)",
            "ycruncher" => "y-cruncher (Pi, x86_64)",
            "prime95" => "Prime95 (Small FFT, x86_64)",
            "geekbench" => "Geekbench 7",
            other => other,
        };
        format!("Suite: {name}")
    }

    fn tui_field_countries(&self) -> &'static str {
        "Countries (ISO / all)"
    }

    fn tui_field_servers(&self) -> &'static str {
        "Server IDs (override)"
    }

    fn tui_field_seconds(&self) -> &'static str {
        "Duration per way (s)"
    }

    fn tui_field_streams(&self) -> &'static str {
        "Parallel streams"
    }

    fn tui_field_rate(&self) -> &'static str {
        "Stream cap (Mbit/s)"
    }

    fn tui_field_taier_browse(&self) -> &'static str {
        "Select nodes (tree)"
    }

    fn tui_field_taier_targets(&self) -> &'static str {
        "Target selectors"
    }

    fn tui_taier_browse_value(&self, selections: &str) -> String {
        let count = selections
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .count();
        if count == 0 {
            "[Enter] Browse provinces, cities & carriers (default)".to_owned()
        } else if count == 1 {
            "[Enter] Browse provinces, cities & carriers (1 selector)".to_owned()
        } else {
            format!("[Enter] Browse provinces, cities & carriers ({count} selectors)")
        }
    }

    fn tui_taier_picker_title(&self) -> &'static str {
        "China Carrier Node Picker (network.taier)"
    }

    fn tui_taier_picker_root(&self) -> &'static str {
        "All Regions"
    }

    fn tui_taier_picker_loading(&self) -> &'static str {
        "Fetching CAICT GlobalSpeed node list from bench_data... (Esc to cancel)"
    }

    fn tui_taier_picker_error_hint(&self) -> &'static str {
        "Enter / r: retry download  |  Esc: close"
    }

    fn tui_taier_picker_tip(&self) -> &'static str {
        "Municipalities open carriers directly; provinces drill into cities. Press / to search."
    }

    fn tui_taier_picker_keys(&self) -> &'static str {
        "Space: toggle  Enter/Right: drill  Left: up  /: search  a: all  d: default  c: clear  \
         s/Esc: apply  q: cancel"
    }

    fn tui_taier_operator_label(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "China Telecom (ct)",
            "unicom" => "China Unicom (cu)",
            "mobile" => "China Mobile (cm)",
            "cernet" => "CERNET Education (edu)",
            "cbn" => "China Broadnet (cbn)",
            "other" => "Other / Overseas (other)",
            _ => "Carrier",
        }
    }

    fn tui_taier_picker_empty_err(&self) -> String {
        "Select at least one carrier node (press d for defaults or q to cancel)".to_owned()
    }

    fn tui_field_cloudflare_concurrency(&self) -> &'static str {
        "Concurrent workers"
    }

    fn tui_field_cloudflare_udp(&self) -> &'static str {
        "STUN UDP loss probe"
    }

    fn tui_field_inetspeed_threads(&self) -> &'static str {
        "Multi-thread workers"
    }

    fn tui_field_inetspeed_timeout(&self) -> &'static str {
        "Timeout per thread (s)"
    }

    fn tui_field_inetspeed_max(&self) -> &'static str {
        "Cap per thread"
    }

    fn tui_field_inetspeed_latency_count(&self) -> &'static str {
        "Idle latency probes"
    }

    fn tui_field_inetspeed_endpoint(&self) -> &'static str {
        "Pinned endpoint IP"
    }

    fn tui_field_trace_group(&self, group: &str) -> String {
        format!("Group: {group}")
    }

    fn tui_field_targets(&self) -> &'static str {
        "Target IDs (override)"
    }

    fn tui_field_geo(&self) -> &'static str {
        "Hop ASN & geolocation"
    }

    fn tui_field_trace_tcp(&self) -> &'static str {
        "TCP large-packet (China)"
    }

    fn tui_field_trace_packet_size(&self) -> &'static str {
        "TCP packet size (bytes)"
    }

    fn tui_field_bgp_upload(&self) -> &'static str {
        "Upload topology image"
    }

    fn tui_field_dnsbl(&self) -> &'static str {
        "DNSBL blacklists (25)"
    }

    fn tui_field_mail(&self) -> &'static str {
        "Port-25 mail checks"
    }

    fn tui_field_media_group(&self, group: &str) -> String {
        format!("Region: {group}")
    }

    fn tui_field_services(&self) -> &'static str {
        "Service IDs (override)"
    }

    fn tui_field_format(&self) -> &'static str {
        "Report format"
    }

    fn tui_field_output(&self) -> &'static str {
        "Save report to file"
    }

    fn tui_field_work_dir(&self) -> &'static str {
        "Work directory"
    }

    fn tui_field_show_ip(&self) -> &'static str {
        "Public IP display"
    }

    fn tui_field_connections(&self) -> &'static str {
        "Download connections"
    }

    fn tui_field_socks5_proxy(&self) -> &'static str {
        "SOCKS5 proxy (downloads)"
    }

    fn tui_field_github_proxy(&self) -> &'static str {
        "GitHub proxy URL"
    }

    fn tui_field_verbose(&self) -> &'static str {
        "Verbose debug log"
    }

    fn tui_default_locations(&self) -> &'static str {
        "default (5 locations)"
    }

    fn tui_default_taier_cities(&self) -> &'static str {
        "default (Beijing, Shanghai, Guangzhou)"
    }

    fn tui_default_inetspeed_endpoint(&self) -> &'static str {
        "auto (Dual DoH + RTT)"
    }

    fn tui_default_none(&self) -> &'static str {
        "none"
    }

    fn tui_unlimited(&self) -> &'static str {
        "unlimited"
    }

    fn tui_default_cpu_note(&self) -> &'static str {
        "y-cruncher & Prime95 are opt-in (x86_64 only, auto-skipped on ARM64)."
    }

    fn tui_default_taier_note(&self) -> &'static str {
        "Defaults to Beijing, Shanghai & Guangzhou across Telecom, Unicom & Mobile."
    }

    fn tui_default_cloudflare_note(&self) -> &'static str {
        "Measures speed.cloudflare.com HTTPS throughput, bufferbloat & STUN UDP loss."
    }

    fn tui_default_inetspeed_note(&self) -> &'static str {
        "Discovers optimal mensura.cdn-apple.com IP via Dual DoH and runs 1T & NT rounds."
    }

    fn tui_default_trace_note(&self) -> &'static str {
        "Unchecked groups = caches + default carrier routes."
    }

    fn tui_default_bgp_note(&self) -> &'static str {
        "Uploads anonymized AS-path SVG to imageupload.app (unlisted) for text links."
    }

    fn tui_default_media_note(&self) -> &'static str {
        "Unchecked regions = all 5 world regions."
    }

    fn tui_masked(&self) -> &'static str {
        "No (masked)"
    }

    fn tui_unmasked(&self) -> &'static str {
        "Yes (full IP)"
    }

    fn tui_edit_title(&self, field: &str) -> String {
        format!("Edit {field}")
    }

    fn tui_edit_hint(&self) -> &'static str {
        "Enter: apply  |  Ctrl-U: clear (use default)  |  Esc: cancel"
    }

    fn tui_no_items_selected(&self) -> String {
        "Select at least one test item before starting".to_owned()
    }

    fn tui_no_cpu_suites_selected(&self) -> String {
        "Select at least one CPU sub-suite in cpu.bench (or disable cpu.bench)".to_owned()
    }

    fn tui_invalid_number(&self, field: &str, min: u32, max: u32) -> String {
        format!("{field} must be a whole number from {min} to {max}")
    }

    fn tui_empty_work_dir(&self) -> String {
        "Work directory cannot be empty (use . for the current directory)".to_owned()
    }

    fn tui_all_items_and_options(&self) -> &'static str {
        "all items and options"
    }

    fn tui_reset_done(&self, target: &str) -> String {
        format!("Reset {target} to defaults")
    }

    fn tui_gauge_completed(&self, done: usize, total: usize) -> String {
        format!("{done} / {total} completed")
    }

    fn tui_taier_picker_badge(&self, selected: usize, est_secs: u64) -> String {
        let est_text = if est_secs >= 60 {
            format!("~{}m {:02}s", est_secs / 60, est_secs % 60)
        } else {
            format!("~{est_secs}s")
        };
        format!("{} ({est_text})", super::count(selected, "slot", "slots"))
    }

    fn tui_taier_filter_prefix(&self) -> &'static str {
        "Filter: "
    }

    fn tui_taier_filter_active(&self, filter: &str) -> String {
        format!("Filter: \"{filter}\"  (/: edit, Esc: clear)")
    }

    fn tui_taier_no_matches(&self) -> &'static str {
        "No matching locations (press Esc to clear filter)"
    }

    fn tui_taier_targets_preview(&self, targets: Option<&str>) -> String {
        match targets {
            Some(text) => format!("Targets: {text}"),
            None => "Targets: (none selected)".to_owned(),
        }
    }

    fn tui_taier_region_meta(&self, municipality: bool, overseas: bool, cities: usize) -> String {
        if municipality {
            "  (municipality)".to_owned()
        } else if overseas {
            format!("  ({cities} overseas)")
        } else {
            format!("  ({})", super::count(cities, "city", "cities"))
        }
    }

    fn tui_taier_node_count(&self, count: usize) -> String {
        if count == 1 {
            "1 node".to_owned()
        } else {
            format!("{count} fallback nodes")
        }
    }

    fn tui_keys_config_list(&self) -> &'static str {
        "Up/Down: move  Space: toggle  Right/Tab: edit params  g: group  a: all  d: reset  \
         r: run  q: quit"
    }

    fn tui_keys_config_params(&self) -> &'static str {
        "Up/Down: field  Space/Enter: toggle/edit  Left/Right: adjust  Esc/Tab: back  \
         d: reset  r: run"
    }

    fn tui_progress_pane(&self, done: usize, total: usize) -> String {
        format!("Progress ({done}/{total})")
    }

    fn tui_log_pane(&self, following: bool) -> &'static str {
        if following {
            "Live Event Log (following)"
        } else {
            "Live Event Log (paused - End to follow)"
        }
    }

    fn tui_collecting_host(&self) -> &'static str {
        "Collecting host profile and probing IPv4/IPv6 connectivity..."
    }

    fn tui_state_queued(&self) -> &'static str {
        "queued"
    }

    fn tui_state_running(&self) -> &'static str {
        "running"
    }

    fn tui_state_not_applicable(&self) -> &'static str {
        "not applicable here"
    }

    fn tui_stopping_banner(&self) -> &'static str {
        "Stopping current test for partial report... Press Ctrl-C again to exit immediately"
    }

    fn tui_keys_running(&self) -> &'static str {
        "Up/Down/PgUp/PgDn: scroll log  End: follow latest  Ctrl-C: stop & view partial report"
    }

    fn tui_sections_pane(&self) -> &'static str {
        "Sections"
    }

    fn tui_section_top(&self) -> &'static str {
        "Top / Overview"
    }

    fn tui_report_pane(&self, format: &str, wrapped: bool, show_ip: bool) -> String {
        let wrap = if format == "text" {
            if wrapped { ", wrap: on" } else { ", wrap: off" }
        } else {
            ""
        };
        let ip = if show_ip { "full IP" } else { "masked IP" };
        format!("Report ({format}{wrap}, {ip})")
    }

    fn tui_save_prompt_title(&self) -> &'static str {
        "Save Report to File"
    }

    fn tui_saved_to(&self, path: &Path) -> String {
        format!("Saved report to {}", path.display())
    }

    fn tui_keys_results(&self) -> &'static str {
        "Arrows/PgUp/PgDn: scroll  [/]: section  b: sidebar  f: format  w: wrap  i: IP  \
         s: save  p: print & quit  n: new  q: quit"
    }
}
