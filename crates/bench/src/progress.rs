//! The progress log: one line per event on stderr, stamped with the time since the start, so that
//! a long run shows what it is doing while its report waits for the end.

use std::fmt;
use std::io;
use std::time::Instant;

use bench_api::ToolEnd;
use bench_api::ToolRun;
use bench_api::ToolSpec;
use bench_host::network::Network;
use bench_host::profile::HostProfile;
use tracing::Subscriber;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

use crate::orchestrator::Observer;
use crate::orchestrator::Run;
use crate::provision::Provisioned;
use crate::report;
use crate::results::Measured;

/// The time since the start, as `[hh:mm:ss]`: a static binary cannot read the local time zone
/// reliably once threads run, and in a long run the elapsed time says more.
struct Elapsed(Instant);

impl FormatTime for Elapsed {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        write!(w, "{}", stamp(self.0.elapsed().as_secs()))
    }
}

/// `[hh:mm:ss]` for a number of seconds.
fn stamp(seconds: u64) -> String {
    format!(
        "[{:02}:{:02}:{:02}]",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// A subscriber that writes this program's events to `writer`, one line each; `verbose` adds
/// the debug events, such as the address of each download.
#[must_use]
pub fn subscriber<W>(writer: W, verbose: bool) -> impl Subscriber + Send + Sync
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    use tracing_subscriber::layer::SubscriberExt;

    let level = if verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    // Only the program's own crates and binary: the libraries' events are in English, and name
    // the servers that the program connects to.
    let ours = Targets::new()
        .with_target("bench", level)
        .with_target("lowendaff_bench", level);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_timer(Elapsed(Instant::now()))
        .with_level(false)
        .with_target(false)
        .with_max_level(level)
        .finish()
        .with(ours)
}

/// Sends the program's log to stderr.
pub fn init(verbose: bool) {
    // Only `main` installs it, once.
    let _ = tracing::subscriber::set_global_default(subscriber(io::stderr, verbose));
}

/// Logs where the program runs, once the host profile is read.
pub fn host(profile: &HostProfile) {
    let platform = report::terminal::platform_text(profile.platform);
    tracing::info!("主机概况：{platform}，{}", profile.arch);
}

/// Logs each family's reachability and public address. The log masks addresses always: it is
/// shared more readily than the report, which shows them in full on request.
pub fn network(network: &Network) {
    for (name, family) in [("IPv4", &network.ipv4), ("IPv6", &network.ipv6)] {
        let text = report::terminal::family_text(family, name, false);
        tracing::info!("网络：{name} {text}");
    }
}

/// Logs how the tools' downloads went: the ready ones on one line, then each failure with its
/// reason.
pub fn tools(tools: &[ToolSpec], provisioned: &Provisioned) {
    let ready: Vec<String> = tools
        .iter()
        .filter(|tool| provisioned.ready.contains_key(tool.name))
        .map(|tool| format!("{} {}", tool.name, tool.version))
        .collect();
    if !ready.is_empty() {
        tracing::info!("准备工具：{} 就绪", ready.join("、"));
    }
    for tool in tools {
        if let Some(reason) = provisioned.failed.get(tool.name) {
            tracing::info!("准备工具：{} {} 失败：{reason}", tool.name, tool.version);
        }
    }
}

/// Logs each plugin's start, its progress lines, its tools' runs with `-v`, and how it ended, in
/// the report's words.
pub struct Log;

impl Observer<Measured> for Log {
    fn on_start(&mut self, id: &'static str) {
        tracing::info!("{id} 开始");
    }

    fn on_progress(&mut self, id: &'static str, line: &str) {
        tracing::info!("{id} {line}");
    }

    fn on_tool(&mut self, id: &'static str, run: &ToolRun) {
        tracing::debug!(
            "{id} 运行 {}：{}",
            run.command.join(" "),
            end_text(&run.end)
        );
        for line in &run.tail {
            tracing::debug!("{id} 输出：{line}");
        }
    }

    fn on_finish(&mut self, run: &Run<Measured>) {
        tracing::info!("{} {}", run.id, report::terminal::run_status(run));
    }
}

/// How a tool's run ended, in the log's words.
fn end_text(end: &ToolEnd) -> String {
    match end {
        ToolEnd::Exit(code) => format!("退出码 {code}"),
        ToolEnd::Signal => "被信号终止".to_owned(),
        ToolEnd::TimedOut(limit) => format!("超过 {} 秒仍未结束，已停止", limit.as_secs()),
        ToolEnd::Cancelled => "已中断".to_owned(),
        ToolEnd::Spawn(error) => format!("无法运行：{error}"),
        ToolEnd::Read(error) => format!("无法读取输出：{error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::stamp;

    #[test]
    fn the_time_is_written_as_hours_minutes_and_seconds() {
        assert_eq!(stamp(0), "[00:00:00]");
        assert_eq!(stamp(125), "[00:02:05]");
        assert_eq!(stamp(3 * 3600 + 25 * 60 + 7), "[03:25:07]");
        assert_eq!(stamp(100 * 3600 + 59 * 60 + 59), "[100:59:59]");
    }
}
