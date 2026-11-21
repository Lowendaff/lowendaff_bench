//! lowendaff-bench orchestrator: plans plugins, provisions their tools, runs them in
//! order and prints the results.

pub mod app;
pub mod bgp_upload;
pub mod cloudflare_probe;
pub mod fetcher;
pub mod inetspeed_probe;
pub mod locale;
pub mod netprobe;
pub mod network;
pub mod orchestrator;
pub mod prober;
pub mod progress;
pub mod provision;
pub mod registry;
pub mod report;
pub mod report_path;
pub mod results;
pub mod taier_probe;
pub mod tui;
