//! User-editable TUI configuration for all 15 benchmark plugins and global run/report options.

use std::collections::BTreeMap;
use std::net::IpAddr;

use bench_api::Net;
use bench_api::Requirements;
use plugin_cpu::bench::DEFAULT_SUITES;
use plugin_cpu::bench::SUITE_CODES;
use plugin_network::cloudflare::Settings as CloudflareSettings;
use plugin_network::inetspeed::Settings as InetspeedSettings;
use plugin_network::inetspeed::parse as inetspeed_parse;
use plugin_network::iperf3::result::Settings as IperfSettings;
use plugin_network::taier::Selector as TaierSelector;
use plugin_network::taier::Settings as TaierSettings;
use plugin_network::trace;

use crate::app;
use crate::fetcher::GithubProxy;
use crate::fetcher::Socks5Proxy;
use crate::locale;
use crate::orchestrator;
use crate::orchestrator::Planned;
use crate::registry::Entry;
use crate::report;
use crate::report::Format;
use crate::report_path;
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
        /// Test duration per direction in seconds (`1..=60`).
        seconds: u16,
        /// Parallel TCP streams (`1..=128`).
        streams: u8,
        /// Optional per-stream bandwidth cap in Mbit/s (`1..=1_000_000`).
        rate: Option<u32>,
    },
    /// `network.taier` China carrier speed test parameters.
    Taier {
        /// Comma-separated `<place>[:<op>[+<op>...]]` selectors; empty uses defaults (`Beijing`,
        /// `Shanghai`, `Guangzhou` across `Telecom`, `Unicom`, `Mobile`).
        selections: String,
        /// Duration per direction in seconds (`1..=60`).
        seconds: u16,
        /// Parallel TCP streams (`1..=32`).
        streams: u8,
    },
    /// `network.cloudflare` speed test parameters.
    Cloudflare {
        /// Duration per throughput direction in seconds (`1..=60`).
        seconds: u16,
        /// Concurrent HTTP workers (`1..=32`).
        concurrency: u8,
        /// Whether to run the RFC 5389 STUN UDP packet-loss probe.
        udp: bool,
    },
    /// `network.inetspeed` Apple CDN speed test parameters.
    Inetspeed {
        /// Multi-thread worker count (`1..=64`).
        threads: u8,
        /// Per-thread timeout in seconds (`1..=120`).
        timeout: u16,
        /// Per-thread byte cap string (`2G`, `500M`, `1GiB`).
        max: String,
        /// Idle latency sample count (`1..=100`).
        latency_count: u8,
        /// Optional pinned endpoint IP literal; empty uses Dual DoH discovery.
        endpoint: String,
    },
    /// `network.trace` route tracing parameters.
    Trace {
        /// Selected target groups (`services`, `china`, `international`, `all`).
        groups: [bool; 4],
        /// Comma-separated target IDs; overrides `groups` when non-empty.
        targets: String,
        /// Whether NextTrace resolves hop ASNs and geolocation.
        geo: bool,
        /// Whether mainland China targets also run a TCP SYN large-packet trace.
        tcp: bool,
        /// Total TCP probe packet size in bytes (`64..=1500`).
        large_packets: u16,
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
    /// Default parameter set when plugin `id` is enabled.
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
            "network.taier" => Self::Taier {
                selections: String::new(),
                seconds: TaierSettings::DEFAULT_SECONDS,
                streams: TaierSettings::DEFAULT_STREAMS,
            },
            "network.cloudflare" => Self::Cloudflare {
                seconds: CloudflareSettings::DEFAULT_SECONDS,
                concurrency: CloudflareSettings::DEFAULT_CONCURRENCY,
                udp: true,
            },
            "network.inetspeed" => Self::Inetspeed {
                threads: InetspeedSettings::DEFAULT_THREADS,
                timeout: InetspeedSettings::DEFAULT_TIMEOUT_SECS,
                max: InetspeedSettings::DEFAULT_MAX.to_owned(),
                latency_count: InetspeedSettings::DEFAULT_LATENCY_COUNT,
                endpoint: String::new(),
            },
            "network.trace" => Self::Trace {
                groups: [false; 4],
                targets: String::new(),
                geo: true,
                tcp: true,
                large_packets: trace::DEFAULT_LARGE_PACKETS,
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

    /// Cleared parameter set shown when plugin `id` is disabled.
    #[must_use]
    pub fn cleared_for(id: &str) -> Self {
        match id {
            "cpu.bench" => Self::CpuBench { suites: [false; 8] },
            "network.iperf3" => Self::Iperf3 {
                countries: String::new(),
                servers: String::new(),
                seconds: IperfSettings::YABS.seconds,
                streams: IperfSettings::YABS.streams,
                rate: None,
            },
            "network.taier" => Self::Taier {
                selections: String::new(),
                seconds: TaierSettings::DEFAULT_SECONDS,
                streams: TaierSettings::DEFAULT_STREAMS,
            },
            "network.cloudflare" => Self::Cloudflare {
                seconds: CloudflareSettings::DEFAULT_SECONDS,
                concurrency: CloudflareSettings::DEFAULT_CONCURRENCY,
                udp: false,
            },
            "network.inetspeed" => Self::Inetspeed {
                threads: InetspeedSettings::DEFAULT_THREADS,
                timeout: InetspeedSettings::DEFAULT_TIMEOUT_SECS,
                max: InetspeedSettings::DEFAULT_MAX.to_owned(),
                latency_count: InetspeedSettings::DEFAULT_LATENCY_COUNT,
                endpoint: String::new(),
            },
            "network.trace" => Self::Trace {
                groups: [false; 4],
                targets: String::new(),
                geo: false,
                tcp: false,
                large_packets: trace::DEFAULT_LARGE_PACKETS,
            },
            "network.bgp" => Self::Bgp { upload: false },
            "ip.quality" => Self::IpQuality {
                dnsbl: false,
                mail: false,
            },
            "ip.media" => Self::IpMedia {
                groups: [false; 6],
                services: String::new(),
            },
            _ => Self::Fixed,
        }
    }

    /// Whether every parameter of this item matches its enabled program default.
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
            Self::Taier {
                selections,
                seconds,
                streams,
            } => {
                split_csv(selections).is_empty()
                    && *seconds == TaierSettings::DEFAULT_SECONDS
                    && *streams == TaierSettings::DEFAULT_STREAMS
            }
            Self::Cloudflare {
                seconds,
                concurrency,
                udp,
            } => {
                *seconds == CloudflareSettings::DEFAULT_SECONDS
                    && *concurrency == CloudflareSettings::DEFAULT_CONCURRENCY
                    && *udp
            }
            Self::Inetspeed {
                threads,
                timeout,
                max,
                latency_count,
                endpoint,
            } => {
                *threads == InetspeedSettings::DEFAULT_THREADS
                    && *timeout == InetspeedSettings::DEFAULT_TIMEOUT_SECS
                    && max
                        .trim()
                        .eq_ignore_ascii_case(InetspeedSettings::DEFAULT_MAX)
                    && *latency_count == InetspeedSettings::DEFAULT_LATENCY_COUNT
                    && endpoint.trim().is_empty()
            }
            Self::Trace {
                groups,
                targets,
                geo,
                tcp,
                large_packets,
            } => {
                !groups.iter().any(|on| *on)
                    && split_csv(targets).is_empty()
                    && *geo
                    && *tcp
                    && *large_packets == trace::DEFAULT_LARGE_PACKETS
            }
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
            Self::Taier {
                selections,
                seconds,
                streams,
            } => {
                let selections = split_csv(selections);
                if !selections.is_empty() {
                    map.insert("selections".to_owned(), string_array(&selections));
                }
                if *seconds != TaierSettings::DEFAULT_SECONDS {
                    map.insert("seconds".to_owned(), serde_json::Value::from(*seconds));
                }
                if *streams != TaierSettings::DEFAULT_STREAMS {
                    map.insert("streams".to_owned(), serde_json::Value::from(*streams));
                }
            }
            Self::Cloudflare {
                seconds,
                concurrency,
                udp,
            } => {
                if *seconds != CloudflareSettings::DEFAULT_SECONDS {
                    map.insert("seconds".to_owned(), serde_json::Value::from(*seconds));
                }
                if *concurrency != CloudflareSettings::DEFAULT_CONCURRENCY {
                    map.insert(
                        "concurrency".to_owned(),
                        serde_json::Value::from(*concurrency),
                    );
                }
                if !udp {
                    map.insert("udp".to_owned(), serde_json::Value::Bool(false));
                }
            }
            Self::Inetspeed {
                threads,
                timeout,
                max,
                latency_count,
                endpoint,
            } => {
                if *threads != InetspeedSettings::DEFAULT_THREADS {
                    map.insert("threads".to_owned(), serde_json::Value::from(*threads));
                }
                if *timeout != InetspeedSettings::DEFAULT_TIMEOUT_SECS {
                    map.insert("timeout".to_owned(), serde_json::Value::from(*timeout));
                }
                let max_trimmed = max.trim();
                if !max_trimmed.is_empty()
                    && !max_trimmed.eq_ignore_ascii_case(InetspeedSettings::DEFAULT_MAX)
                {
                    map.insert(
                        "max".to_owned(),
                        serde_json::Value::String(max_trimmed.to_owned()),
                    );
                }
                if *latency_count != InetspeedSettings::DEFAULT_LATENCY_COUNT {
                    map.insert(
                        "latency_count".to_owned(),
                        serde_json::Value::from(*latency_count),
                    );
                }
                let ep_trimmed = endpoint.trim();
                if !ep_trimmed.is_empty() {
                    map.insert(
                        "endpoint".to_owned(),
                        serde_json::Value::String(ep_trimmed.to_owned()),
                    );
                }
            }
            Self::Trace {
                groups,
                targets,
                geo,
                tcp,
                large_packets,
            } => {
                let chosen: Vec<String> = if groups[3] {
                    vec!["all".to_owned()]
                } else {
                    TRACE_GROUPS[..3]
                        .iter()
                        .zip(groups[..3].iter())
                        .filter(|(_, on)| **on)
                        .map(|(code, _)| (*code).to_owned())
                        .collect()
                };
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
                if !tcp {
                    map.insert("tcp".to_owned(), serde_json::Value::Bool(false));
                }
                if *large_packets != trace::DEFAULT_LARGE_PACKETS {
                    map.insert(
                        "large_packets".to_owned(),
                        serde_json::Value::from(*large_packets),
                    );
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
                let chosen: Vec<String> = if groups[5] {
                    vec!["all".to_owned()]
                } else {
                    MEDIA_GROUPS[..5]
                        .iter()
                        .zip(groups[..5].iter())
                        .filter(|(_, on)| **on)
                        .map(|(code, _)| (*code).to_owned())
                        .collect()
                };
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
#[derive(Debug, Clone)]
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
    /// Static host requirements descriptor, used to refresh `requires_text` on language switch.
    pub requires: Requirements,
    /// Formatted host requirement summary.
    pub requires_text: String,
    /// Whether this item will run.
    pub enabled: bool,
    /// Per-plugin configurable settings.
    pub params: ItemParams,
}

impl ItemConfig {
    /// Sets `enabled`, clearing sub-selections when disabled and restoring defaults when enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.params = if enabled {
            ItemParams::default_for(self.id)
        } else {
            ItemParams::cleared_for(self.id)
        };
    }

    /// Flips `enabled`, clearing sub-selections on disable and restoring defaults on enable.
    pub fn toggle_enabled(&mut self) {
        let next = !self.enabled;
        self.set_enabled(next);
    }

    /// Whether this item is enabled and carries non-default parameters.
    #[must_use]
    pub fn is_customized(&self) -> bool {
        self.enabled && !self.params.is_default()
    }

    /// Resets `enabled` and `params` to defaults.
    pub fn reset(&mut self) {
        self.enabled = true;
        self.params = ItemParams::default_for(self.id);
    }
}

/// Global run and report options configured inside the interactive TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalConfig {
    /// Report format (`text`, `markdown`, or `json`).
    pub format: Format,
    /// Custom file path where the report is saved; empty archives to `/tmp` automatically.
    pub output: String,
    /// Directory under which the temporary run directory is created.
    pub work_dir: String,
    /// Whether public IP addresses and rDNS names appear unmasked (`false` by default).
    pub show_ip: bool,
    /// Maximum parallel connections per tool download (`32` by default, `1..=256`).
    pub download_connections: u16,
    /// Optional SOCKS5 proxy URL (`socks5://` or `socks5h://`) for tool and list downloads.
    pub socks5_proxy: String,
    /// Optional GitHub mirror URL prefix (e.g. `https://ghfast.top/`) for GitHub downloads.
    pub github_proxy: String,
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
            socks5_proxy: String::new(),
            github_proxy: String::new(),
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
    /// Whether the row belongs to a currently disabled parent item.
    pub dimmed: bool,
    /// Raw text for inline editing when `Enter` is pressed (`None` for pure toggles/choices).
    pub edit_raw: Option<String>,
}

/// Complete user configuration managed by the TUI.
#[derive(Debug, Clone)]
pub struct TuiConfig {
    /// Per-item toggle and parameters, in registry execution order.
    pub items: Vec<ItemConfig>,
    /// Global execution and report options.
    pub global: GlobalConfig,
}

impl TuiConfig {
    /// Initializes configuration from `registry` with every item enabled and default parameters.
    #[must_use]
    pub fn new(registry: &[Entry]) -> Self {
        let words = locale::words();
        let items = registry
            .iter()
            .map(|entry| {
                let group = group_static(entry.id);
                let group_title = words.group_title(group).unwrap_or(group);
                let timeout_mins = entry.timeout.as_secs().div_ceil(60).max(1);
                let title = words
                    .item_name(entry.id)
                    .or(entry.title)
                    .unwrap_or(entry.id);
                ItemConfig {
                    id: entry.id,
                    title,
                    group,
                    group_title,
                    timeout_mins,
                    requires: entry.requires,
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

    /// Re-populates localized item titles, group titles, and host requirement summaries from the
    /// currently active [`locale::words`] catalog.
    pub fn refresh_locale(&mut self) {
        let words = locale::words();
        for item in &mut self.items {
            item.title = words.item_name(item.id).unwrap_or(item.id);
            item.group_title = words.group_title(item.group).unwrap_or(item.group);
            item.requires_text = describe_requirements(&item.requires);
        }
    }

    /// Number of enabled test items.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.items.iter().filter(|item| item.enabled).count()
    }

    /// Number of enabled items with customized parameters.
    #[must_use]
    pub fn customized_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.is_customized())
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

    /// Serializes per-plugin parameter overrides for enabled items for `orchestrator::plan`.
    #[must_use]
    pub fn plugin_settings(&self) -> BTreeMap<String, serde_json::Value> {
        let mut out = BTreeMap::new();
        for item in &self.items {
            if item.enabled
                && let Some(value) = item.params.to_json()
            {
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

    /// Toggles all items: disables all (clearing sub-options) if all are enabled; enables all
    /// with defaults otherwise.
    pub fn toggle_all(&mut self) {
        let target = self.enabled_count() < self.items.len();
        for item in &mut self.items {
            item.set_enabled(target);
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
                item.set_enabled(target);
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
        let inactive = !item.enabled;
        match &item.params {
            ItemParams::Fixed => {}
            ItemParams::CpuBench { suites } => {
                let suite_rows = CPU_SUITES
                    .iter()
                    .zip(suites.iter())
                    .zip(DEFAULT_SUITES.iter());
                for ((code, on), def_on) in suite_rows {
                    rows.push(sub_row(
                        bool_row(&words.tui_field_cpu_suite(code), *on, *def_on),
                        inactive,
                    ));
                }
            }
            ItemParams::Iperf3 {
                countries,
                servers,
                seconds,
                streams,
                rate,
            } => {
                rows.push(sub_row(
                    text_row(
                        words.tui_field_countries(),
                        countries,
                        words.tui_default_locations(),
                    ),
                    inactive,
                ));
                rows.push(sub_row(
                    text_row(words.tui_field_servers(), servers, words.tui_default_none()),
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_seconds().to_owned(),
                        value: seconds.to_string(),
                        customized: *seconds != IperfSettings::YABS.seconds,
                        dimmed: false,
                        edit_raw: Some(seconds.to_string()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_streams().to_owned(),
                        value: streams.to_string(),
                        customized: *streams != IperfSettings::YABS.streams,
                        dimmed: false,
                        edit_raw: Some(streams.to_string()),
                    },
                    inactive,
                ));
                let rate_val = rate.map_or_else(
                    || words.tui_unlimited().to_owned(),
                    |val| format!("{val} Mbit/s"),
                );
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_rate().to_owned(),
                        value: rate_val,
                        customized: rate.is_some(),
                        dimmed: false,
                        edit_raw: Some(rate.map_or_else(String::new, |val| val.to_string())),
                    },
                    inactive,
                ));
            }
            ItemParams::Taier {
                selections,
                seconds,
                streams,
            } => {
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_taier_browse().to_owned(),
                        value: words.tui_taier_browse_value(selections),
                        customized: !selections.trim().is_empty(),
                        dimmed: false,
                        edit_raw: None,
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    text_row(
                        words.tui_field_taier_targets(),
                        selections,
                        words.tui_default_taier_cities(),
                    ),
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_seconds().to_owned(),
                        value: seconds.to_string(),
                        customized: *seconds != TaierSettings::DEFAULT_SECONDS,
                        dimmed: false,
                        edit_raw: Some(seconds.to_string()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_streams().to_owned(),
                        value: streams.to_string(),
                        customized: *streams != TaierSettings::DEFAULT_STREAMS,
                        dimmed: false,
                        edit_raw: Some(streams.to_string()),
                    },
                    inactive,
                ));
            }
            ItemParams::Cloudflare {
                seconds,
                concurrency,
                udp,
            } => {
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_seconds().to_owned(),
                        value: seconds.to_string(),
                        customized: *seconds != CloudflareSettings::DEFAULT_SECONDS,
                        dimmed: false,
                        edit_raw: Some(seconds.to_string()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_cloudflare_concurrency().to_owned(),
                        value: concurrency.to_string(),
                        customized: *concurrency != CloudflareSettings::DEFAULT_CONCURRENCY,
                        dimmed: false,
                        edit_raw: Some(concurrency.to_string()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    bool_row(words.tui_field_cloudflare_udp(), *udp, true),
                    inactive,
                ));
            }
            ItemParams::Inetspeed {
                threads,
                timeout,
                max,
                latency_count,
                endpoint,
            } => {
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_inetspeed_threads().to_owned(),
                        value: threads.to_string(),
                        customized: *threads != InetspeedSettings::DEFAULT_THREADS,
                        dimmed: false,
                        edit_raw: Some(threads.to_string()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_inetspeed_timeout().to_owned(),
                        value: timeout.to_string(),
                        customized: *timeout != InetspeedSettings::DEFAULT_TIMEOUT_SECS,
                        dimmed: false,
                        edit_raw: Some(timeout.to_string()),
                    },
                    inactive,
                ));
                let max_trimmed = max.trim();
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_inetspeed_max().to_owned(),
                        value: max_trimmed.to_owned(),
                        customized: !max_trimmed
                            .eq_ignore_ascii_case(InetspeedSettings::DEFAULT_MAX),
                        dimmed: false,
                        edit_raw: Some(max_trimmed.to_owned()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_inetspeed_latency_count().to_owned(),
                        value: latency_count.to_string(),
                        customized: *latency_count != InetspeedSettings::DEFAULT_LATENCY_COUNT,
                        dimmed: false,
                        edit_raw: Some(latency_count.to_string()),
                    },
                    inactive,
                ));
                rows.push(sub_row(
                    text_row(
                        words.tui_field_inetspeed_endpoint(),
                        endpoint,
                        words.tui_default_inetspeed_endpoint(),
                    ),
                    inactive,
                ));
            }
            ItemParams::Trace {
                groups,
                targets,
                geo,
                tcp,
                large_packets,
            } => {
                let group_rows = TRACE_GROUPS.iter().zip(groups.iter());
                for (code, on) in group_rows {
                    rows.push(sub_row(
                        bool_row(&words.tui_field_trace_group(code), *on, false),
                        inactive,
                    ));
                }
                rows.push(sub_row(
                    text_row(words.tui_field_targets(), targets, words.tui_default_none()),
                    inactive,
                ));
                rows.push(sub_row(
                    bool_row(words.tui_field_geo(), *geo, true),
                    inactive,
                ));
                rows.push(sub_row(
                    bool_row(words.tui_field_trace_tcp(), *tcp, true),
                    inactive,
                ));
                rows.push(sub_row(
                    FieldRow {
                        label: words.tui_field_trace_packet_size().to_owned(),
                        value: large_packets.to_string(),
                        customized: *large_packets != trace::DEFAULT_LARGE_PACKETS,
                        dimmed: !*tcp,
                        edit_raw: Some(large_packets.to_string()),
                    },
                    inactive,
                ));
            }
            ItemParams::Bgp { upload } => {
                rows.push(sub_row(
                    bool_row(words.tui_field_bgp_upload(), *upload, true),
                    inactive,
                ));
            }
            ItemParams::IpQuality { dnsbl, mail } => {
                rows.push(sub_row(
                    bool_row(words.tui_field_dnsbl(), *dnsbl, true),
                    inactive,
                ));
                rows.push(sub_row(
                    bool_row(words.tui_field_mail(), *mail, true),
                    inactive,
                ));
            }
            ItemParams::IpMedia { groups, services } => {
                let region_rows = MEDIA_GROUPS.iter().zip(groups.iter());
                for (code, on) in region_rows {
                    rows.push(sub_row(
                        bool_row(&words.tui_field_media_group(code), *on, false),
                        inactive,
                    ));
                }
                rows.push(sub_row(
                    text_row(
                        words.tui_field_services(),
                        services,
                        words.tui_default_none(),
                    ),
                    inactive,
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
        let out_trimmed = g.output.trim();
        let output_row = if out_trimmed.is_empty() {
            FieldRow {
                label: words.tui_field_output().to_owned(),
                value: report_path::auto_label(g.format),
                customized: false,
                dimmed: false,
                edit_raw: Some(String::new()),
            }
        } else {
            FieldRow {
                label: words.tui_field_output().to_owned(),
                value: out_trimmed.to_owned(),
                customized: true,
                dimmed: false,
                edit_raw: Some(out_trimmed.to_owned()),
            }
        };
        let active_lang = locale::current_language();
        vec![
            FieldRow {
                label: words.tui_field_language().to_owned(),
                value: format!("{} ({})", active_lang.endonym(), active_lang.code()),
                customized: active_lang != locale::Language::En,
                dimmed: false,
                edit_raw: None,
            },
            FieldRow {
                label: words.tui_field_format().to_owned(),
                value: format_name(g.format).to_owned(),
                customized: g.format != Format::Text,
                dimmed: false,
                edit_raw: None,
            },
            output_row,
            FieldRow {
                label: words.tui_field_work_dir().to_owned(),
                value: g.work_dir.clone(),
                customized: g.work_dir != ".",
                dimmed: false,
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
                dimmed: false,
                edit_raw: None,
            },
            FieldRow {
                label: words.tui_field_connections().to_owned(),
                value: g.download_connections.to_string(),
                customized: g.download_connections != app::DOWNLOAD_CONNECTIONS,
                dimmed: false,
                edit_raw: Some(g.download_connections.to_string()),
            },
            FieldRow {
                label: words.tui_field_socks5_proxy().to_owned(),
                value: if g.socks5_proxy.trim().is_empty() {
                    format!("({})", words.tui_default_none())
                } else {
                    redact_url_password(&g.socks5_proxy)
                },
                customized: !g.socks5_proxy.trim().is_empty(),
                dimmed: false,
                edit_raw: Some(g.socks5_proxy.trim().to_owned()),
            },
            text_row(
                words.tui_field_github_proxy(),
                &g.github_proxy,
                words.tui_default_none(),
            ),
            bool_row(words.tui_field_verbose(), g.verbose, false),
        ]
    }

    /// Activates `field_idx` on `items[item_idx]` (`Space` or `Enter`). Toggles booleans in place
    /// (with bi-directional master-switch linkage) and returns `Some((label, initial_text))` when
    /// an inline text/number editor should open.
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
        self.toggle_item_boolean(item_idx, field_idx);
        None
    }

    /// Adjusts `field_idx` on `items[item_idx]` via `Left` (`-1`) or `Right` (`+1`).
    pub fn step_item_field(&mut self, item_idx: usize, field_idx: usize, delta: i32) {
        let Some(item) = self.items.get_mut(item_idx) else {
            return;
        };
        if let ItemParams::Iperf3 {
            seconds,
            streams,
            rate,
            ..
        } = &mut item.params
        {
            match field_idx {
                3 => {
                    *seconds = step_u16(*seconds, delta, 1, 60);
                    item.enabled = true;
                    return;
                }
                4 => {
                    *streams = step_u8(*streams, delta, 1, 128);
                    item.enabled = true;
                    return;
                }
                5 => {
                    let next = i64::from(rate.unwrap_or(0)) + i64::from(delta) * 50;
                    *rate = (next > 0).then(|| u32::try_from(next.min(1_000_000)).unwrap_or(1));
                    if rate.is_some() {
                        item.enabled = true;
                    }
                    return;
                }
                _ => {}
            }
        }
        if let ItemParams::Taier {
            seconds, streams, ..
        } = &mut item.params
        {
            match field_idx {
                3 => {
                    *seconds = step_u16(*seconds, delta, 1, 60);
                    item.enabled = true;
                    return;
                }
                4 => {
                    *streams = step_u8(*streams, delta, 1, 32);
                    item.enabled = true;
                    return;
                }
                _ => {}
            }
        }
        if let ItemParams::Cloudflare {
            seconds,
            concurrency,
            udp,
        } = &mut item.params
        {
            match field_idx {
                1 => {
                    *seconds = step_u16(*seconds, delta, 1, 60);
                    if !item.enabled {
                        item.enabled = true;
                        *udp = true;
                    }
                    return;
                }
                2 => {
                    *concurrency = step_u8(*concurrency, delta, 1, 32);
                    if !item.enabled {
                        item.enabled = true;
                        *udp = true;
                    }
                    return;
                }
                _ => {}
            }
        }
        if let ItemParams::Inetspeed {
            threads,
            timeout,
            latency_count,
            ..
        } = &mut item.params
        {
            match field_idx {
                1 => {
                    *threads = step_u8(*threads, delta, 1, 64);
                    item.enabled = true;
                    return;
                }
                2 => {
                    *timeout = step_u16(*timeout, delta, 1, 120);
                    item.enabled = true;
                    return;
                }
                4 => {
                    *latency_count = step_u8(*latency_count, delta, 1, 100);
                    item.enabled = true;
                    return;
                }
                _ => {}
            }
        }
        if let ItemParams::Trace {
            geo,
            tcp,
            large_packets,
            ..
        } = &mut item.params
            && field_idx == 8
        {
            *large_packets = step_u16(
                *large_packets,
                delta.saturating_mul(32),
                trace::MIN_LARGE_PACKETS,
                trace::MAX_LARGE_PACKETS,
            );
            if !item.enabled {
                item.enabled = true;
                *geo = true;
                *tcp = true;
            }
            return;
        }
        self.toggle_item_boolean(item_idx, field_idx);
    }

    fn toggle_item_boolean(&mut self, item_idx: usize, field_idx: usize) {
        let Some(item) = self.items.get_mut(item_idx) else {
            return;
        };
        if field_idx == 0 {
            item.toggle_enabled();
            return;
        }
        let was_disabled = !item.enabled;
        match &mut item.params {
            ItemParams::Fixed
            | ItemParams::Iperf3 { .. }
            | ItemParams::Taier { .. }
            | ItemParams::Inetspeed { .. } => {}
            ItemParams::Cloudflare { udp, .. } => {
                if field_idx == 3 {
                    if was_disabled {
                        item.enabled = true;
                        *udp = true;
                    } else {
                        *udp = !*udp;
                    }
                }
            }
            ItemParams::CpuBench { suites } => {
                if (1..=8).contains(&field_idx) {
                    suites[field_idx - 1] = !suites[field_idx - 1];
                    item.enabled = suites.iter().any(|on| *on);
                }
            }
            ItemParams::Trace {
                groups,
                targets,
                geo,
                tcp,
                ..
            } => match field_idx {
                1..=4 => {
                    if was_disabled {
                        item.enabled = true;
                        *geo = true;
                        *tcp = true;
                    }
                    toggle_trace_group(groups, field_idx - 1);
                    if groups.iter().any(|on| *on) {
                        targets.clear();
                    }
                }
                6 => {
                    if was_disabled {
                        item.enabled = true;
                        *geo = true;
                        *tcp = true;
                    } else {
                        *geo = !*geo;
                    }
                }
                7 => {
                    if was_disabled {
                        item.enabled = true;
                        *geo = true;
                        *tcp = true;
                    } else {
                        *tcp = !*tcp;
                    }
                }
                _ => {}
            },
            ItemParams::Bgp { upload } => {
                if field_idx == 1 {
                    if was_disabled {
                        item.enabled = true;
                        *upload = true;
                    } else {
                        *upload = !*upload;
                    }
                }
            }
            ItemParams::IpQuality { dnsbl, mail } => match field_idx {
                1 => {
                    if was_disabled {
                        item.enabled = true;
                        *dnsbl = true;
                    } else {
                        *dnsbl = !*dnsbl;
                    }
                }
                2 => {
                    if was_disabled {
                        item.enabled = true;
                        *mail = true;
                    } else {
                        *mail = !*mail;
                    }
                }
                _ => {}
            },
            ItemParams::IpMedia { groups, services } => {
                if (1..=6).contains(&field_idx) {
                    item.enabled = true;
                    toggle_media_group(groups, field_idx - 1);
                    if groups.iter().any(|on| *on) {
                        services.clear();
                    }
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
        let was_disabled = !item.enabled;
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
                1 => {
                    let norm = normalize_csv(trimmed);
                    if !norm.is_empty() {
                        item.enabled = true;
                        servers.clear();
                    }
                    *countries = norm;
                }
                2 => {
                    let norm = normalize_csv(trimmed);
                    if !norm.is_empty() {
                        item.enabled = true;
                        countries.clear();
                    }
                    *servers = norm;
                }
                3 => {
                    if trimmed.is_empty() {
                        *seconds = IperfSettings::YABS.seconds;
                    } else {
                        let val = parse_range(trimmed, 1, 60).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_seconds(), 1, 60)
                        })?;
                        *seconds = u16::try_from(val).unwrap_or(IperfSettings::YABS.seconds);
                        item.enabled = true;
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
                        item.enabled = true;
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
                        item.enabled = true;
                    }
                }
                _ => {}
            },
            ItemParams::Taier {
                selections,
                seconds,
                streams,
            } => match field_idx {
                2 => {
                    let parts = split_csv(trimmed);
                    let mut normalized = Vec::with_capacity(parts.len());
                    for part in &parts {
                        let parsed = TaierSelector::try_new(part)
                            .map_err(|err| app::format_taier_selector_error(&err))?;
                        normalized.push(parsed.to_string());
                    }
                    let joined = normalized.join(",");
                    if !joined.is_empty() {
                        item.enabled = true;
                    }
                    *selections = joined;
                }
                3 => {
                    if trimmed.is_empty() {
                        *seconds = TaierSettings::DEFAULT_SECONDS;
                    } else {
                        let val = parse_range(trimmed, 1, 60).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_seconds(), 1, 60)
                        })?;
                        *seconds = u16::try_from(val).unwrap_or(TaierSettings::DEFAULT_SECONDS);
                        if was_disabled {
                            item.enabled = true;
                        }
                    }
                }
                4 => {
                    if trimmed.is_empty() {
                        *streams = TaierSettings::DEFAULT_STREAMS;
                    } else {
                        let val = parse_range(trimmed, 1, 32).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_streams(), 1, 32)
                        })?;
                        *streams = u8::try_from(val).unwrap_or(TaierSettings::DEFAULT_STREAMS);
                        if was_disabled {
                            item.enabled = true;
                        }
                    }
                }
                _ => {}
            },
            ItemParams::Cloudflare {
                seconds,
                concurrency,
                udp,
            } => match field_idx {
                1 => {
                    if trimmed.is_empty() {
                        *seconds = CloudflareSettings::DEFAULT_SECONDS;
                    } else {
                        let val = parse_range(trimmed, 1, 60).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_seconds(), 1, 60)
                        })?;
                        *seconds =
                            u16::try_from(val).unwrap_or(CloudflareSettings::DEFAULT_SECONDS);
                        if was_disabled {
                            item.enabled = true;
                            *udp = true;
                        }
                    }
                }
                2 => {
                    if trimmed.is_empty() {
                        *concurrency = CloudflareSettings::DEFAULT_CONCURRENCY;
                    } else {
                        let val = parse_range(trimmed, 1, 32).ok_or_else(|| {
                            words.tui_invalid_number(
                                words.tui_field_cloudflare_concurrency(),
                                1,
                                32,
                            )
                        })?;
                        *concurrency =
                            u8::try_from(val).unwrap_or(CloudflareSettings::DEFAULT_CONCURRENCY);
                        if was_disabled {
                            item.enabled = true;
                            *udp = true;
                        }
                    }
                }
                _ => {}
            },
            ItemParams::Inetspeed {
                threads,
                timeout,
                max,
                latency_count,
                endpoint,
            } => match field_idx {
                1 => {
                    if trimmed.is_empty() {
                        *threads = InetspeedSettings::DEFAULT_THREADS;
                    } else {
                        let val = parse_range(trimmed, 1, 64).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_inetspeed_threads(), 1, 64)
                        })?;
                        *threads = u8::try_from(val).unwrap_or(InetspeedSettings::DEFAULT_THREADS);
                        item.enabled = true;
                    }
                }
                2 => {
                    if trimmed.is_empty() {
                        *timeout = InetspeedSettings::DEFAULT_TIMEOUT_SECS;
                    } else {
                        let val = parse_range(trimmed, 1, 120).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_inetspeed_timeout(), 1, 120)
                        })?;
                        *timeout =
                            u16::try_from(val).unwrap_or(InetspeedSettings::DEFAULT_TIMEOUT_SECS);
                        item.enabled = true;
                    }
                }
                3 => {
                    if trimmed.is_empty() {
                        *max = InetspeedSettings::DEFAULT_MAX.to_owned();
                    } else {
                        let _ = inetspeed_parse::parse_size(trimmed)
                            .map_err(|_| words.inetspeed_invalid_max(trimmed))?;
                        *max = trimmed.to_owned();
                        item.enabled = true;
                    }
                }
                4 => {
                    if trimmed.is_empty() {
                        *latency_count = InetspeedSettings::DEFAULT_LATENCY_COUNT;
                    } else {
                        let val = parse_range(trimmed, 1, 100).ok_or_else(|| {
                            words.tui_invalid_number(
                                words.tui_field_inetspeed_latency_count(),
                                1,
                                100,
                            )
                        })?;
                        *latency_count =
                            u8::try_from(val).unwrap_or(InetspeedSettings::DEFAULT_LATENCY_COUNT);
                        item.enabled = true;
                    }
                }
                5 => {
                    if trimmed.is_empty() {
                        endpoint.clear();
                    } else {
                        let ip: IpAddr = trimmed
                            .parse()
                            .ok()
                            .filter(|&ip| bench_api::is_routable_endpoint_ip(ip))
                            .ok_or_else(|| words.invalid_inetspeed_endpoint(trimmed))?;
                        *endpoint = ip.to_string();
                        item.enabled = true;
                    }
                }
                _ => {}
            },
            ItemParams::Trace {
                groups,
                targets,
                geo,
                tcp,
                large_packets,
            } => match field_idx {
                5 => {
                    let norm = normalize_csv(trimmed);
                    if !norm.is_empty() {
                        if was_disabled {
                            item.enabled = true;
                            *geo = true;
                            *tcp = true;
                        }
                        *groups = [false; 4];
                    }
                    *targets = norm;
                }
                8 => {
                    if trimmed.is_empty() {
                        *large_packets = trace::DEFAULT_LARGE_PACKETS;
                    } else {
                        let min = u32::from(trace::MIN_LARGE_PACKETS);
                        let max = u32::from(trace::MAX_LARGE_PACKETS);
                        let val = parse_range(trimmed, min, max).ok_or_else(|| {
                            words.tui_invalid_number(words.tui_field_trace_packet_size(), min, max)
                        })?;
                        *large_packets = u16::try_from(val).unwrap_or(trace::DEFAULT_LARGE_PACKETS);
                        if was_disabled {
                            item.enabled = true;
                            *geo = true;
                            *tcp = true;
                        }
                    }
                }
                _ => {}
            },
            ItemParams::IpMedia { groups, services } => {
                if field_idx == 7 {
                    let norm = normalize_csv(trimmed);
                    if !norm.is_empty() {
                        item.enabled = true;
                        *groups = [false; 6];
                    }
                    *services = norm;
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
            0 => {
                let next = cycle_language(locale::current_language(), 1);
                locale::set_language(next);
                self.refresh_locale();
            }
            1 => self.global.format = cycle_format(self.global.format, 1),
            4 => self.global.show_ip = !self.global.show_ip,
            8 => self.global.verbose = !self.global.verbose,
            _ => {}
        }
        None
    }

    /// Adjusts `field_idx` on [`GlobalConfig`] via `Left` (`-1`) or `Right` (`+1`).
    pub fn step_global_field(&mut self, field_idx: usize, delta: i32) {
        if delta == 0 {
            return;
        }
        match field_idx {
            0 => {
                let next = cycle_language(locale::current_language(), delta);
                locale::set_language(next);
                self.refresh_locale();
            }
            1 => self.global.format = cycle_format(self.global.format, delta),
            4 => self.global.show_ip = !self.global.show_ip,
            5 => {
                self.global.download_connections =
                    step_u16(self.global.download_connections, delta, 1, 256);
            }
            8 => self.global.verbose = !self.global.verbose,
            _ => {}
        }
    }

    /// Applies edited text to `field_idx` on [`GlobalConfig`].
    pub fn apply_global_edit(&mut self, field_idx: usize, input: &str) -> Result<(), String> {
        let words = locale::words();
        let trimmed = input.trim();
        match field_idx {
            2 => self.global.output = trimmed.to_owned(),
            3 => {
                if trimmed.is_empty() {
                    self.global.work_dir = ".".to_owned();
                } else {
                    self.global.work_dir = trimmed.to_owned();
                }
            }
            5 => {
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
            6 => {
                if trimmed.is_empty() {
                    self.global.socks5_proxy.clear();
                } else {
                    let _ = Socks5Proxy::try_new(trimmed)?;
                    self.global.socks5_proxy = trimmed.to_owned();
                }
            }
            7 => {
                if trimmed.is_empty() {
                    self.global.github_proxy.clear();
                } else {
                    let _ = GithubProxy::try_new(trimmed)?;
                    self.global.github_proxy = trimmed.to_owned();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Applies the serialized selector CSV returned by the interactive `TaierPicker` modal and
    /// enables `network.taier`.
    pub fn apply_taier_picker_selection(&mut self, selections_csv: String) {
        for item in &mut self.items {
            if item.id == "network.taier"
                && let ItemParams::Taier { selections, .. } = &mut item.params
            {
                *selections = selections_csv;
                item.enabled = true;
                break;
            }
        }
    }
}

fn toggle_trace_group(groups: &mut [bool; 4], idx: usize) {
    if idx == 3 {
        let next = !groups[3];
        *groups = [next; 4];
    } else if idx < 3 {
        groups[idx] = !groups[idx];
        groups[3] = groups[0] && groups[1] && groups[2];
    }
}

fn toggle_media_group(groups: &mut [bool; 6], idx: usize) {
    if idx == 5 {
        let next = !groups[5];
        *groups = [next; 6];
    } else if idx < 5 {
        groups[idx] = !groups[idx];
        groups[5] = groups[..5].iter().all(|on| *on);
    }
}

fn sub_row(mut row: FieldRow, inactive: bool) -> FieldRow {
    if inactive {
        row.customized = false;
        row.dimmed = true;
    }
    row
}

/// Cycles interface [`locale::Language`] forward or backward.
#[must_use]
pub fn cycle_language(lang: locale::Language, delta: i32) -> locale::Language {
    if delta == 0 {
        return lang;
    }
    let all = locale::Language::ALL;
    let idx = all.iter().position(|item| *item == lang).unwrap_or(0);
    let len = all.len();
    let next = if delta > 0 {
        (idx + 1) % len
    } else {
        (idx + len - 1) % len
    };
    all[next]
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
    if delta == 0 {
        return format;
    }
    if delta > 0 {
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
        dimmed: false,
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
        dimmed: false,
        edit_raw: Some(trimmed.to_owned()),
    }
}

fn redact_url_password(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return trimmed.to_owned();
    };
    let boundary = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, suffix) = rest.split_at(boundary);
    let Some((userinfo, host_part)) = authority.rsplit_once('@') else {
        return trimmed.to_owned();
    };
    match userinfo.split_once(':') {
        Some((user, _)) => format!("{scheme}://{user}:***@{host_part}{suffix}"),
        None if !userinfo.is_empty() => format!("{scheme}://***@{host_part}{suffix}"),
        None => trimmed.to_owned(),
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
    let next = i32::from(current).saturating_add(delta);
    u16::try_from(next.clamp(i32::from(min), i32::from(max))).unwrap_or(current)
}

fn step_u8(current: u8, delta: i32, min: u8, max: u8) -> u8 {
    let next = i32::from(current).saturating_add(delta);
    u8::try_from(next.clamp(i32::from(min), i32::from(max))).unwrap_or(current)
}
