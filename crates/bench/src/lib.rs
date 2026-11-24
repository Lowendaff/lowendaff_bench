//! lowendaff-bench orchestrator: plans plugins, provisions their tools, runs them in
//! order and prints the results.

#![warn(missing_docs)]

pub mod app;
mod bgp_upload;
mod cloudflare_probe;
pub mod fetcher;
mod inetspeed_probe;
pub mod locale;
mod netprobe;
pub mod network;
pub mod orchestrator;
mod prober;
pub mod progress;
pub mod provision;
pub mod registry;
pub mod report;
pub mod report_path;
pub mod results;
mod taier_probe;
pub mod tui;
pub mod vm_cpu;
