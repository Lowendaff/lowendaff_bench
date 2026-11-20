//! User-editable TUI configuration for all 12 benchmark plugins and global run/report options.

use std::collections::BTreeMap;

use bench_api::Net;
use bench_api::Requirements;
use plugin_cpu::bench::DEFAULT_SUITES;
use plugin_cpu::bench::SUITE_CODES;
use plugin_network::iperf3::result::Settings as IperfSettings;

use crate::app;
use crate::locale;
use crate::orchestrator;
use crate::orchestrator::Planned;
use crate::registry::Entry;
use crate::report;
use crate::report::Format;
use crate::results::Measured;

/// CPU sub-suite codes in UI display and execution order.
pub const CPU_SUITES: [&str; 8] = SUITE_CODES;

/// Route tracing group codes in UI display order.
pub const TRACE_GROUPS: [&str; 4] = ["services", "china", "international", "all"];

/// Streaming media region codes in UI display order.
pub const MEDIA_GROUPS: [&str; 6] = ["global", "asia", "europe", "americas", "oceania", "all"];

/// Per-plugin configurable parameters, initialized to the plugin's standard defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemParams {
    /// Plugin uses fixed standardized parameters for cross-server comparability.
    Fixed,
    /// `cpu.bench` sub-suite toggles (`sysbench`, `7zip`, `unixbench`, `hpl`, `passmark`,
    /// `ycruncher`, `prime95`, `geekbench`).
    CpuBench {
        /// Enabled sub-suites in [`CPU_SUITES`] order.
        suites: [bool; 8],
    },
    /// `network.iperf3` speed test parameters.
    Iperf3 {
        /// Comma-separated ISO 3166 country codes or `all`; empty uses default locations.
        countries: String,
        /// Comma-separated server IDs; overrides `countries` when non-empty.
        servers: String,
        /// Test duration per direction in seconds (`1..=3600`).
        seconds: u16,
        /// Parallel TCP streams (`1..=128`).
        streams: u8,
        /// Optional per-stream bandwidth cap in Mbit/s (`1..=1_000_000`).
        rate: Option<u32>,
    },
    /// `network.trace` route tracing parameters.
    Trace {
        /// Selected target groups (`services`, `china`, `international`, `all`).
        groups: [bool; 4],
        /// Comma-separated target IDs; overrides `groups` when non-empty.
        targets: String,
        /// Whether NextTrace resolves hop ASNs and geolocation.
        geo: bool,
    },
    /// `network.bgp` routing topology parameters.
    Bgp {
        /// Whether to upload rendered topology SVGs to `imageupload.app` in `text` mode.
        upload: bool,
    },
    /// `ip.quality` reputation check parameters.
    IpQuality {
        /// Whether to query 25 DNSBL spam/abuse zones.
        dnsbl: bool,
        /// Whether to probe outbound TCP port 25 against mail providers.
        mail: bool,
    },
    /// `ip.media` streaming unlock parameters.
    IpMedia {
        /// Selected regional groups (`global`, `asia`, `europe`, `americas`, `oceania`, `all`).
        groups: [bool; 6],
        /// Comma-separated platform IDs; overrides `groups` when non-empty.
        services: String,
    },
}

impl ItemParams {
    /// Default parameter set for plugin `id`.
    #[must_use]
    pub fn default_for(id: &str) -> Self {
        match id {
            "cpu.bench" => Self::CpuBench {
                suites: DEFAULT_SUITES,
            },
            "network.iperf3" => Self::Iperf3 {
                countries: String::new(),
                servers: String::new(),
                seconds: IperfSettings::YABS.seconds,
                streams: IperfSettings::YABS.streams,
                rate: None,
            },
            "network.trace" => Self::Trace {
                groups: [false; 4],
                targets: String::new(),
                geo: true,
            },
            "network.bgp" => Self::Bgp { upload: true },
            "ip.quality" => Self::IpQuality {
                dnsbl: true,
                mail: true,
            },
            "ip.media" => Self::IpMedia {
                groups: [false; 6],
                services: String::new(),
            },
            _ => Self::Fixed,
        }
    }

    /// Whether every parameter of this item matches its program default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        match self {
            Self::Fixed => true,
            Self::CpuBench { suites } => *suites == DEFAULT_SUITES,
            Self::Iperf3 {
                countries,
                servers,
                seconds,
                streams,
                rate,
            } => {
                split_csv(countries).is_empty()
                    && split_csv(servers).is_empty()
                    && *seconds == IperfSettings::YABS.seconds
                    && *streams == IperfSettings::YABS.streams
                    && rate.is_none()
            }
            Self::Trace {
                groups,
                targets,
                geo,
            } => !groups.iter().any(|on| *on) && split_csv(targets).is_empty() && *geo,
            Self::Bgp { upload } => *upload,
            Self::IpQuality { dnsbl, mail } => *dnsbl && *mail,
            Self::IpMedia { groups, services } => {
                !groups.iter().any(|on| *on) && split_csv(services).is_empty()
            }
        }
    }

    fn to_json(&self) -> Option<serde_json::Value> {
        let mut map = serde_json::Map::new();
        match self {
            Self::Fixed => {}
            Self::CpuBench { suites } => {
                if *suites != DEFAULT_SUITES {
                    let chosen: Vec<String> = CPU_SUITES
                        .iter()
                        .zip(suites.iter())
                        .filter(|(_, on)| **on)
                        .map(|(code, _)| (*code).to_owned())
                        .collect();
                    map.insert("suites".to_owned(), string_array(&chosen));
                }
            }
            Self::Iperf3 {
                countries,
                servers,
                seconds,
                streams,
                rate,
            } => {
                let countries = split_csv(countries);
                if !countries.is_empty() {
                    map.insert("countries".to_owned(), string_array(&countries));
                }
                let servers = split_csv(servers);
                if !servers.is_empty() {
                    map.insert("servers".to_owned(), string_array(&servers));
                }
                if *seconds != IperfSettings::YABS.seconds {
                    map.insert("seconds".to_owned(), serde_json::Value::from(*seconds));
                }
                if *streams != IperfSettings::YABS.streams {
                    map.insert("streams".to_owned(), serde_json::Value::from(*streams));
                }
                if let Some(rate) = rate {
                    map.insert("rate".to_owned(), serde_json::Value::from(*rate));
                }
            }
            Self::Trace {
                groups,
                targets,
                geo,
            } => {
                let chosen: Vec<String> = TRACE_GROUPS
                    .iter()
                    .zip(groups.iter())
                    .filter(|(_, on)| **on)
                    .map(|(code, _)| (*code).to_owned())
                    .collect();
                if !chosen.is_empty() {
                    map.insert("groups".to_owned(), string_array(&chosen));
                }
                let targets = split_csv(targets);
                if !targets.is_empty() {
                    map.insert("targets".to_owned(), string_array(&targets));
                }
                if !geo {
                    map.insert("geo".to_owned(), serde_json::Value::Bool(false));
                }
            }
            Self::Bgp { upload } => {
                if !upload {
                    map.insert("upload".to_owned(), serde_json::Value::Bool(false));
                }
            }
            Self::IpQuality { dnsbl, mail } => {
                if !dnsbl {
                    map.insert("dnsbl".to_owned(), serde_json::Value::Bool(false));
                }
                if !mail {
                    map.insert("mail".to_owned(), serde_json::Value::Bool(false));
                }
            }
            Self::IpMedia { groups, services } => {
                let chosen: Vec<String> = MEDIA_GROUPS
                    .iter()
                    .zip(groups.iter())
                    .filter(|(_, on)| **on)
                    .map(|(code, _)| (*code).to_owned())
                    .collect();
                if !chosen.is_empty() {
                    map.insert("groups".to_owned(), string_array(&chosen));
                }
                let services = split_csv(services);
                if !services.is_empty() {
                    map.insert("services".to_owned(), string_array(&services));
                }
            }
        }
        (!map.is_empty()).then_some(serde_json::Value::Object(map))
    }
}

/// Configuration of one registered benchmark item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemConfig {
    /// Plugin identifier (`<group>.<name>`).
    pub id: &'static str,
    /// Human-readable item title from the catalog.
    pub title: &'static str,
    /// Group segment (`cpu`, `memory`, `disk`, `network`, `ip`).
    pub group: &'static str,
    /// Group display title from the catalog.
    pub group_title: &'static str,
    /// Worst-case timeout in whole minutes (at least 1).
    pub timeout_mins: u64,
    /// Formatted host requirement summary.
    pub requires_text: String,
    /// Whether this item will run.
    pub enabled: bool,
    /// Per-plugin configurable settings.
    pub params: ItemParams,
}

impl ItemConfig {
    /// Resets `enabled` and `params` to defaults.
    pub fn reset(&mut self) {
        self.enabled = true;
        self.params = ItemParams::default_for(self.id);
    }
}

/// Global run and report options configured inside `--tui`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalConfig {
    /// Report format (`text`, `markdown`, or `json`).
    pub format: Format,
    /// Optional file path where the report is also saved when the run finishes.
    pub output: String,
    /// Directory under which the temporary run directory is created.
    pub work_dir: String,
    /// Whether public IP addresses and rDNS names appear unmasked (`false` by default).
    pub show_ip: bool,
    /// Maximum parallel connections per tool download (`32` by default, `1..=256`).
    pub download_connections: u16,
    /// Whether verbose debug events (tool commands, HTTP/SMTP probes) are logged (`false`).
    pub verbose: bool,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            format: Format::Text,
            output: String::new(),
            work_dir: ".".to_owned(),
            show_ip: false,
            download_connections: app::DOWNLOAD_CONNECTIONS,
            verbose: false,
        }
    }
}

impl GlobalConfig {
    /// Whether every global option still holds its default value.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One row displayed in the right-hand parameter inspector pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRow {
    /// Parameter name.
    pub label: String,
    /// Formatted current value.
    pub value: String,
    /// Whether the value differs from its default.
    pub customized: bool,
    /// Raw text for inline editing when `Enter` is pressed (`None` for pure toggles/choices).
    pub edit_raw: Option<String>,
}

/// Complete user configuration managed by the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiConfig {
    /// Per-item toggle and parameters, in registry execution order.
    pub items: Vec<ItemConfig>,
    /// Global execution and report options.
    pub global: GlobalConfig,
}

impl TuiConfig {
    /// Initializes configuration from `registry` with every item enabled and default parameters.
    #[must_use]
    pub fn from_registry(registry: &[Entry]) -> Self {
        let words = locale::words();
        let items = registry
            .iter()
            .map(|entry| {
                let group = group_static(entry.id);
                let group_title = words.group_title(group).unwrap_or(group);
                let timeout_mins = entry.timeout.as_secs().div_ceil(60).max(1);
                ItemConfig {
                    id: entry.id,
                    title: entry.title.unwrap_or(entry.id),
                    group,
                    group_title,
                    timeout_mins,
                    requires_text: describe_requirements(&entry.requires),
                    enabled: true,
                    params: ItemParams::default_for(entry.id),
                }
            })
            .collect();
        Self {
            items,
            global: GlobalConfig::default(),
        }
    }

    /// Number of enabled test items.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.items.iter().filter(|item| item.enabled).count()
    }

    /// Number of items with customized parameters.
    #[must_use]
    pub fn customized_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| !item.params.is_default())
            .count()
    }

    /// IDs of enabled items, in registry execution order.
    #[must_use]
    pub fn selected_ids(&self) -> Vec<&'static str> {
        self.items
            .iter()
            .filter(|item| item.enabled)
            .map(|item| item.id)
            .collect()
    }

    /// Serializes per-plugin parameter overrides for `orchestrator::plan`.
    #[must_use]
    pub fn plugin_settings(&self) -> BTreeMap<String, serde_json::Value> {
        let mut out = BTreeMap::new();
        for item in &self.items {
            if let Some(value) = item.params.to_json() {
                out.insert(item.id.to_owned(), value);
            }
        }
        out
    }

    /// Validates the selection and builds the runnable `Planned` list.
    pub fn build_plan(&self, registry: &[Entry]) -> Result<Vec<Planned<Measured>>, Vec<String>> {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return Err(vec![locale::words().tui_no_items_selected()]);
        }
        if self.global.work_dir.trim().is_empty() {
            return Err(vec![locale::words().tui_empty_work_dir()]);
        }
        for item in &self.items {
            if item.enabled
                && let ItemParams::CpuBench { suites } = &item.params
                && !suites.iter().any(|on| *on)
            {
                return Err(vec![locale::words().tui_no_cpu_suites_selected()]);
            }
        }
        let settings = self.plugin_settings();
        orchestrator::plan(registry, &ids, &settings)
    }

    /// Toggles all items: disables all if all are enabled; enables all otherwise.
    pub fn toggle_all(&mut self) {
        let target = self.enabled_count() < self.items.len();
        for item in &mut self.items {
            item.enabled = target;
        }
    }

    /// Toggles every item belonging to `group`.
    pub fn toggle_group(&mut self, group: &str) {
        let all_on = self
            .items
            .iter()
            .filter(|item| item.group == group)
            .all(|item| item.enabled);
        let target = !all_on;
        for item in &mut self.items {
            if item.group == group {
                item.enabled = target;
            }
        }
    }

    /// Inspector rows for `items[item_idx]`.
    #[must_use]
    pub fn fields_for_item(&self, item_idx: usize) -> Vec<FieldRow> {
        let words = locale::words();
        let Some(item) = self.items.get(item_idx) else {
            return Vec::new();
        };
        let mut rows = vec![bool_row(words.tui_field_enabled(), item.enabled, true)];
        match &item.params {
            ItemParams::Fixed => {}
            ItemParams::CpuBench { suites } => {
                for ((code, on), def_on) in CPU_SUITES
                    .iter()
                    .zip(suites.iter())
                    .zip(DEFAULT_SUITES.iter())
                {
                    rows.push(bool_row(&words.tui_field_cpu_suite(code), *on, *def_on));
                }
            }
            ItemParams::Iperf3 {
                countries,
                servers,
                seconds,
                streams,
                rate,
            } => {
                rows.push(text_row(
                    words.tui_field_countries(),
                    countries,
                    words.tui_default_locations(),
                ));
                rows.push(text_row(
                    words.tui_field_servers(),
                    servers,
                    words.tui_default_none(),
                ));
                rows.push(FieldRow {
                    label: words.tui_field_seconds().to_owned(),
                    value: seconds.to_string(),
                    customized: *seconds != IperfSettings::YABS.seconds,
                    edit_raw: Some(seconds.to_string()),
                });
                rows.push(FieldRow {
                    label: words.tui_field_streams().to_owned(),
                    value: streams.to_string(),
                    customized: *streams != IperfSettings::YABS.streams,
                    edit_raw: Some(streams.to_string()),
                });
                let rate_val = rate.map_or_else(
                    || words.tui_unlimited().to_owned(),
                    |val| format!("{val} Mbit/s"),
                );
                rows.push(FieldRow {
                    label: words.tui_field_rate().to_owned(),
                    value: rate_val,
                    customized: rate.is_some(),
                    edit_raw: Some(rate.map_or_else(String::new, |val| val.to_string())),
                });
            }
            ItemParams::Trace {
                groups,
                targets,
                geo,
            } => {
                for (code, on) in TRACE_GROUPS.iter().zip(groups.iter()) {
                    rows.push(bool_row(&words.tui_field_trace_group(code), *on, false));
                }
                rows.push(text_row(
                    words.tui_field_targets(),
                    targets,
                    words.tui_default_none(),
                ));
                rows.push(bool_row(words.tui_field_geo(), *geo, true));
            }
            ItemParams::Bgp { upload } => {
                rows.push(bool_row(words.tui_field_bgp_upload(), *upload, true));
            }
            ItemParams::IpQuality { dnsbl, mail } => {
                rows.push(bool_row(words.tui_field_dnsbl(), *dnsbl, true));
                rows.push(bool_row(words.tui_field_mail(), *mail, true));
            }
            ItemParams::IpMedia { groups, services } => {
                for (code, on) in MEDIA_GROUPS.iter().zip(groups.iter()) {
                    rows.push(bool_row(&words.tui_field_media_group(code), *on, false));
                }
                rows.push(text_row(
                    words.tui_field_services(),
                    services,
                    words.tui_default_none(),
                ));
            }
        }
        rows
    }

    /// Inspector rows for [`GlobalConfig`].
    #[must_use]
    pub fn fields_for_global(&self) -> Vec<FieldRow> {
        let words = locale::words();
        let g = &self.global;
        vec![
            FieldRow {
                label: words.tui_field_format().to_owned(),
                value: format_name(g.format).to_owned(),
                customized: g.format != Format::Text,
                edit_raw: None,
            },
            text_row(words.tui_field_output(), &g.output, words.tui_viewer_only()),
            FieldRow {
                label: words.tui_field_work_dir().to_owned(),
                value: g.work_dir.clone(),
                customized: g.work_dir != ".",
                edit_raw: Some(g.work_dir.clone()),
            },
            FieldRow {
                label: words.tui_field_show_ip().to_owned(),
                value: if g.show_ip {
                    words.tui_unmasked().to_owned()
                } else {
                    words.tui_masked().to_owned()
                },
                customized: g.show_ip,
                edit_raw: None,
            },
            FieldRow {
                label: words.tui_field_connections().to_owned(),
                value: g.download_connections.to_string(),
                customized: g.download_connections != app::DOWNLOAD_CONNECTIONS,
                edit_raw: Some(g.download_connections.to_string()),
            },
            bool_row(words.tui_field_verbose(), g.verbose, false),
        ]
    }

    /// Activates `field_idx` on `items[item_idx]` (`Space` or `Enter`). Toggles booleans in place
    /// and returns `Some((label, initial_text))` when an inline text/number editor should open.
    pub fn activate_item_field(
        &mut self,
        item_idx: usize,
        field_idx: usize,
    ) -> Option<(String, String)> {
        let rows = self.fields_for_item(item_idx);
        let row = rows.get(field_idx)?;
        if let Some(raw) = &row.edit_raw {
            return Some((row.label.clone(), raw.clone()));
        }
        let item = self.items.get_mut(item_idx)?;
        if field_idx == 0 {
            item.enabled = !item.enabled;
            return None;
        }
        match &mut item.params {
            ItemParams::Fixed | ItemParams::Iperf3 { .. } => {}
            ItemParams::CpuBench { suites } => {
                if (1..=8).contains(&field_idx) {
                    suites[field_idx - 1] = !suites[field_idx - 1];
                }
            }
            ItemParams::Trace { groups, geo, .. } => match field_idx {
                1..=4 => groups[field_idx - 1] = !groups[field_idx - 1],
                6 => *geo = !*geo,
                _ => {}
            },
            ItemParams::Bgp { upload } => {
                if field_idx == 1 {
                    *upload = !*upload;
                }
            }
            ItemParams::IpQuality { dnsbl, mail } => match field_idx {
                1 => *dnsbl = !*dnsbl,
                2 => *mail = !*mail,
                _ => {}
            },
            ItemParams::IpMedia { groups, .. } => {
                if (1..=6).contains(&field_idx) {
                    groups[field_idx - 1] = !groups[field_idx - 1];
                }
            }
        }
        None
    }

    /// Adjusts `field_idx` on `items[item_idx]` via `Left` (`-1`) or `Right` (`+1`).
    pub fn step_item_field(&mut self, item_idx: usize, field_idx: usize, delta: i32) {
        let Some(item) = self.items.get_mut(item_idx) else {
            return;
        };
        if field_idx == 0 {
            item.enabled = !item.enabled;
            return;
        }
        match &mut item.params {
            ItemParams::Fixed => {}
            ItemParams::CpuBench { suites } => {
                if (1..=8).contains(&field_idx) {
                    suites[field_idx - 1] = !suites[field_idx - 1];
                }
            }
            ItemParams::Iperf3 {
                seconds,
                streams,
                rate,
                ..
            } => match field_idx {
                3 => *seconds = step_u16(*seconds, delta, 1, 3600),
                4 => *streams = step_u8(*streams, delta, 1, 128),
                5 => {
                    let next = i64::from(rate.unwrap_or(0)) + i64::from(delta) * 50;
                    *rate = (next > 0).then(|| u32::try_from(next.min(1_000_000)).unwrap_or(1));
                }
                _ => {}
            },
            ItemParams::Trace { groups, geo, .. } => match field_idx {
                1..=4 => groups[field_idx - 1] = !groups[field_idx - 1],
                6 => *geo = !*geo,
                _ => {}
            },
            ItemParams::Bgp { upload } => {
                if field_idx == 1 {
                    *upload = !*upload;
                }
            }
            ItemParams::IpQuality { dnsbl, mail } => match field_idx {
                1 => *dnsbl = !*dnsbl,
                2 => *mail = !*mail,
                _ => {}
            },
            ItemParams::IpMedia { groups, .. } => {
                if (1..=6).contains(&field_idx) {
                    groups[field_idx - 1] = !groups[field_idx - 1];
                }
            }
        }
    }

    /// Applies edited text to `field_idx` on `items[item_idx]`.
    pub fn apply_item_edit(
        &mut self,
        item_idx: usize,
        field_idx: usize,
        input: &str,
    ) -> Result<(), String> {
        let words = locale::words();
        let Some(item) = self.items.get_mut(item_idx) else {
            return Ok(());
        };
        let trimmed = input.trim();
        match &mut item.params {
            ItemParams::Fixed
            | ItemParams::CpuBench { .. }
            | ItemParams::Bgp { .. }
            | ItemParams::IpQuality { .. } => {}
            ItemParams::Iperf3 {
                countries,
                servers,
                seconds,
                streams,
                rate,
            } => match field_idx {
                1 => *countries = normalize_csv(trimmed),
                2 => *servers = normalize_csv(trimmed),
                3 => {
                    if trimmed.is_empty() {
                        *seconds = IperfSettings::YABS.seconds;
                    } else {
                        let val = parse_range(trimmed, 1, 3600).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_seconds(), 1, 3600)
                        })?;
                        *seconds = u16::try_from(val).unwrap_or(IperfSettings::YABS.seconds);
                    }
                }
                4 => {
                    if trimmed.is_empty() {
                        *streams = IperfSettings::YABS.streams;
                    } else {
                        let val = parse_range(trimmed, 1, 128).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_streams(), 1, 128)
                        })?;
                        *streams = u8::try_from(val).unwrap_or(IperfSettings::YABS.streams);
                    }
                }
                5 => {
                    if trimmed.is_empty() {
                        *rate = None;
                    } else {
                        let val = parse_range(trimmed, 1, 1_000_000).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_rate(), 1, 1_000_000)
                        })?;
                        *rate = Some(val);
                    }
                }
                _ => {}
            },
            ItemParams::Trace { targets, .. } => {
                if field_idx == 5 {
                    *targets = normalize_csv(trimmed);
                }
            }
            ItemParams::IpMedia { services, .. } => {
                if field_idx == 7 {
                    *services = normalize_csv(trimmed);
                }
            }
        }
        Ok(())
    }

    /// Activates `field_idx` on [`GlobalConfig`].
    pub fn activate_global_field(&mut self, field_idx: usize) -> Option<(String, String)> {
        let rows = self.fields_for_global();
        let row = rows.get(field_idx)?;
        if let Some(raw) = &row.edit_raw {
            return Some((row.label.clone(), raw.clone()));
        }
        match field_idx {
            0 => self.global.format = cycle_format(self.global.format, 1),
            3 => self.global.show_ip = !self.global.show_ip,
            5 => self.global.verbose = !self.global.verbose,
            _ => {}
        }
        None
    }

    /// Adjusts `field_idx` on [`GlobalConfig`] via `Left` (`-1`) or `Right` (`+1`).
    pub fn step_global_field(&mut self, field_idx: usize, delta: i32) {
        match field_idx {
            0 => self.global.format = cycle_format(self.global.format, delta),
            3 => self.global.show_ip = !self.global.show_ip,
            4 => {
                self.global.download_connections =
                    step_u16(self.global.download_connections, delta, 1, 256);
            }
            5 => self.global.verbose = !self.global.verbose,
            _ => {}
        }
    }

    /// Applies edited text to `field_idx` on [`GlobalConfig`].
    pub fn apply_global_edit(&mut self, field_idx: usize, input: &str) -> Result<(), String> {
        let words = locale::words();
        let trimmed = input.trim();
        match field_idx {
            1 => self.global.output = trimmed.to_owned(),
            2 => {
                if trimmed.is_empty() {
                    self.global.work_dir = ".".to_owned();
                } else {
                    self.global.work_dir = trimmed.to_owned();
                }
            }
            4 => {
                if trimmed.is_empty() {
                    self.global.download_connections = app::DOWNLOAD_CONNECTIONS;
                } else {
                    let val = parse_range(trimmed, 1, 256).ok_or_else(|| {
                        words.tui_invalid_number(words.tui_field_connections(), 1, 256)
                    })?;
                    self.global.download_connections =
                        u16::try_from(val).unwrap_or(app::DOWNLOAD_CONNECTIONS);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Canonical lowercase name of a [`Format`].
#[must_use]
pub const fn format_name(format: Format) -> &'static str {
    match format {
        Format::Text => "text",
        Format::Markdown => "markdown",
        Format::Json => "json",
    }
}

/// Cycles report [`Format`] forward or backward.
#[must_use]
pub const fn cycle_format(format: Format, delta: i32) -> Format {
    if delta >= 0 {
        match format {
            Format::Text => Format::Markdown,
            Format::Markdown => Format::Json,
            Format::Json => Format::Text,
        }
    } else {
        match format {
            Format::Text => Format::Json,
            Format::Markdown => Format::Text,
            Format::Json => Format::Markdown,
        }
    }
}

fn group_static(id: &'static str) -> &'static str {
    id.split('.').next().unwrap_or(id)
}

fn describe_requirements(req: &Requirements) -> String {
    let words = locale::words();
    let mut parts = Vec::new();
    for kind in req.platforms {
        parts.push(report::terminal::kind_text(*kind).to_owned());
    }
    if req.numa {
        parts.push(words.tui_req_numa().to_owned());
    }
    match req.net {
        Net::None => {}
        Net::Any => parts.push(words.tui_req_internet().to_owned()),
        Net::Ipv4 => parts.push(words.tui_req_ipv4().to_owned()),
        Net::Ipv6 => parts.push(words.tui_req_ipv6().to_owned()),
    }
    if let Some(glibc) = req.glibc {
        parts.push(words.tui_req_glibc(glibc.major, glibc.minor));
    }
    for tool in req.tools {
        parts.push(format!("{} {}", tool.name, tool.version));
    }
    if parts.is_empty() {
        words.tui_any_host().to_owned()
    } else {
        parts.join(", ")
    }
}

fn bool_row(label: &str, value: bool, default_val: bool) -> FieldRow {
    let words = locale::words();
    let mark = if value { "[x]" } else { "[ ]" };
    let text = if value {
        words.tui_yes()
    } else {
        words.tui_no()
    };
    FieldRow {
        label: label.to_owned(),
        value: format!("{mark} {text}"),
        customized: value != default_val,
        edit_raw: None,
    }
}

fn text_row(label: &str, raw: &str, placeholder: &str) -> FieldRow {
    let trimmed = raw.trim();
    FieldRow {
        label: label.to_owned(),
        value: if trimmed.is_empty() {
            format!("({placeholder})")
        } else {
            trimmed.to_owned()
        },
        customized: !trimmed.is_empty(),
        edit_raw: Some(trimmed.to_owned()),
    }
}

fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_csv(raw: &str) -> String {
    split_csv(raw).join(",")
}

fn string_array(items: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|item| serde_json::Value::String(item.clone()))
            .collect(),
    )
}

fn parse_range(raw: &str, min: u32, max: u32) -> Option<u32> {
    let parsed: u32 = raw.parse().ok()?;
    (min..=max).contains(&parsed).then_some(parsed)
}

fn step_u16(current: u16, delta: i32, min: u16, max: u16) -> u16 {
    let next = i32::from(current) + delta;
    u16::try_from(next.clamp(i32::from(min), i32::from(max))).unwrap_or(current)
}

fn step_u8(current: u8, delta: i32, min: u8, max: u8) -> u8 {
    let next = i32::from(current) + delta;
    u8::try_from(next.clamp(i32::from(min), i32::from(max))).unwrap_or(current)
}
