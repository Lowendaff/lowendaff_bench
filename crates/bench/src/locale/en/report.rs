//! English: the report's frame and shared words.

use bench_api::Net;
use bench_tool::job;

use super::English;
use crate::locale::ReportWords;
use crate::locale::Totals;
use crate::locale::Unit;
use crate::orchestrator::Fault;
use crate::orchestrator::Unmet;

impl ReportWords for English {
    fn label_width(&self) -> usize {
        18
    }

    fn section(&self, title: &str) -> String {
        format!("[{title}]")
    }

    fn system_title(&self) -> String {
        "System".to_owned()
    }

    fn summary_title(&self) -> String {
        "Summary".to_owned()
    }

    fn group_title(&self, group: &str) -> Option<&'static str> {
        match group {
            "cpu" => Some("CPU performance"),
            "memory" => Some("Memory performance"),
            "disk" => Some("Disks"),
            "network" => Some("Network"),
            "ip" => Some("IP"),
            _ => None,
        }
    }

    fn item_name(&self, id: &str) -> Option<&'static str> {
        match id {
            "cpu.bench" => Some("CPU benchmark"),
            "memory.sysbench" => Some("Memory sysbench"),
            "memory.stream" => Some("Memory STREAM"),
            "memory.latency" => Some("Memory latency"),
            "memory.numa" => Some("Memory NUMA"),
            "disk.health" => Some("Disk health"),
            "disk.fio" => Some("Disk performance"),
            "network.iperf3" => Some("Network speed"),
            "network.taier" => Some("China carrier speed"),
            "network.cloudflare" => Some("Cloudflare speed"),
            "network.inetspeed" => Some("Apple CDN speed"),
            "network.trace" => Some("Route tracing"),
            "network.bgp" => Some("BGP paths"),
            "ip.quality" => Some("IP reputation"),
            "ip.media" => Some("Media unlock"),
            _ => None,
        }
    }

    fn environment(&self, platform: &str) -> String {
        format!("Environment: {platform}")
    }

    fn note(&self) -> String {
        "Note".to_owned()
    }

    fn unavailable(&self) -> String {
        "unavailable".to_owned()
    }

    fn unavailable_because(&self, why: &str) -> String {
        if why.is_empty() {
            return self.unavailable();
        }
        format!("unavailable: {why}")
    }

    fn unknown(&self) -> String {
        "unknown".to_owned()
    }

    fn and(&self, items: &[String]) -> String {
        items.join(", ")
    }

    fn completed(&self, seconds: f64) -> String {
        format!("done in {seconds:.1} s")
    }

    fn skipped(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "skipped".to_owned();
        }
        format!("skipped: {reason}")
    }

    fn failed(&self, reason: &str) -> String {
        if reason.is_empty() {
            return "failed".to_owned();
        }
        format!("failed: {reason}")
    }

    fn timed_out(&self) -> String {
        "timed out; the results until then are kept".to_owned()
    }

    fn interrupted(&self) -> String {
        "interrupted".to_owned()
    }

    fn unmet(&self, unmet: &Unmet) -> String {
        match unmet {
            Unmet::Glibc {
                needed,
                found: Some(found),
            } => format!("needs glibc {needed} or newer; this host has {found}"),
            Unmet::Glibc {
                needed,
                found: None,
            } => format!("needs glibc {needed} or newer; this host has no glibc"),
            Unmet::Network(Net::Ipv4) => "needs an IPv4 connection".to_owned(),
            Unmet::Network(Net::Ipv6) => "needs an IPv6 connection".to_owned(),
            Unmet::Network(Net::Any | Net::None) => "needs a network connection".to_owned(),
            Unmet::Tool { name, why } if why.is_empty() => format!("tool {name} unavailable"),
            Unmet::Tool { name, why } => format!("tool {name} unavailable: {why}"),
            Unmet::File { name, why } if why.is_empty() => format!("file {name} unavailable"),
            Unmet::File { name, why } => format!("file {name} unavailable: {why}"),
        }
    }

    fn fault(&self, fault: &Fault) -> String {
        match fault {
            Fault::Panic(message) if message.trim().is_empty() => {
                format!(
                    "the plugin crashed: {}",
                    crate::locale::ProgramWords::panic_without_text(self)
                )
            }
            Fault::Panic(message) => format!("the plugin crashed: {message}"),
            Fault::WorkDir(error) => format!("cannot create the work directory: {error}"),
            Fault::Aborted(error) => format!("the task ended abnormally: {error}"),
        }
    }

    fn not_applicable(&self, names: &[String]) -> String {
        format!("Not applicable here: {}", names.join(", "))
    }

    fn totals(&self, totals: &Totals) -> String {
        let Totals {
            items,
            completed,
            skipped,
            failed,
            timed_out,
            interrupted,
            seconds,
        } = totals;
        format!(
            "{}: {completed} done, {skipped} skipped, {failed} failed, {timed_out} timed out, \
             {interrupted} interrupted. Total time {seconds:.1} s",
            super::count(*items, "item", "items")
        )
    }

    fn tool_fault(&self, tool: &str, fault: &job::Fault, unit: Unit) -> String {
        let quoted = |last: Option<&str>| {
            last.map(|s| crate::report::terminal::strip_terminal_controls(s, false))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "no output".to_owned())
        };
        match fault {
            job::Fault::Spawn(error) => format!("cannot run {tool}: {error}"),
            job::Fault::Read(error) => format!("cannot read the output of {tool}: {error}"),
            job::Fault::TimedOut(limit) => {
                let secs = limit.as_secs();
                let limit = match unit {
                    Unit::Minutes if secs >= 60 && secs.is_multiple_of(60) => super::count(
                        usize::try_from(secs / 60).unwrap_or(usize::MAX),
                        "minute",
                        "minutes",
                    ),
                    Unit::Seconds | Unit::Minutes => format!("{secs} s"),
                };
                format!("still running after {limit}, stopped")
            }
            job::Fault::Exit { code, last } => {
                format!("{tool} exit code {code}: {}", quoted(last.as_deref()))
            }
            job::Fault::Signal { last } => {
                format!("{tool} ended by a signal: {}", quoted(last.as_deref()))
            }
        }
    }

    fn retried(&self, run: &str, failures: usize, last: &str) -> String {
        format!(
            "{run}: worked after {}; the last reason: {last}",
            super::count(failures, "failure", "failures")
        )
    }

    fn every_attempt_failed(&self, run: &str, last: &str, attempts: usize) -> String {
        format!("{run}: {}", self.attempts(last, attempts))
    }

    fn attempts(&self, last: &str, attempts: usize) -> String {
        format!("{last} ({})", super::count(attempts, "attempt", "attempts"))
    }

    fn bytes(&self, count: u64) -> String {
        if count == 1 {
            "1 byte".to_owned()
        } else {
            format!("{count} bytes")
        }
    }

    fn md_kv_headers(&self) -> [&'static str; 2] {
        ["Item", "Value"]
    }

    fn vm_cpu_title(&self) -> &'static str {
        "CPU steal & softirq"
    }

    fn vm_cpu_method(&self, interval_secs: u64) -> String {
        format!(
            "/proc/stat sampled every {interval_secs} s from start to finish: \
             softirq (%si) and hypervisor steal (%st)"
        )
    }

    fn vm_cpu_summary_title(&self) -> &'static str {
        "Summary"
    }

    fn vm_cpu_summary_headers(&self) -> [&'static str; 5] {
        ["Metric", "Average", "P95", "Peak", "Peak during"]
    }

    fn vm_cpu_metric_label(&self, steal: bool) -> &'static str {
        if steal {
            "Steal (%st)"
        } else {
            "SoftIRQ (%si)"
        }
    }

    fn vm_cpu_peak_cell(&self, item: &str, time: &str) -> String {
        format!("{item} ({time})")
    }

    fn vm_cpu_samples_title(&self) -> &'static str {
        "Samples"
    }

    fn vm_cpu_timeline_title(&self) -> &'static str {
        "Timeline"
    }

    fn vm_cpu_chart_title(&self) -> &'static str {
        "CPU Steal (%st) & SoftIRQ (%si)"
    }

    fn vm_cpu_y_axis_title(&self) -> &'static str {
        "CPU (%)"
    }

    fn vm_cpu_samples_headers(&self) -> [&'static str; 4] {
        ["Time", "Item", "%si", "%st"]
    }

    fn vm_cpu_setup_phase(&self) -> &'static str {
        "Setup"
    }
}
