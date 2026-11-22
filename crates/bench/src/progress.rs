//! The progress log: one line per event on stderr, stamped with the time since the start, so that
//! a long run shows what it is doing while its report waits for the end.

use std::fmt;
use std::io;
use std::time::Instant;

use bench_api::Step;
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

use crate::locale;
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

/// Wraps a [`MakeWriter`] so every formatted log event strips ANSI/OSC escape sequences and C0/C1
/// control characters before reaching `stderr` or test sinks.
struct SanitizedMakeWriter<W>(W);

struct SanitizedWriter<W>(W);

impl<'a, W> MakeWriter<'a> for SanitizedMakeWriter<W>
where
    W: MakeWriter<'a>,
{
    type Writer = SanitizedWriter<<W as MakeWriter<'a>>::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        SanitizedWriter(self.0.make_writer())
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        SanitizedWriter(self.0.make_writer_for(meta))
    }
}

impl<W: io::Write> io::Write for SanitizedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let clean = report::terminal::strip_terminal_controls(&text, true);
        self.0.write_all(clean.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
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
        .with_writer(SanitizedMakeWriter(writer))
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
    tracing::info!("{}", locale::words().host_line(&platform, &profile.arch));
}

/// Logs each family's reachability and public address. The log masks addresses always: it is
/// shared more readily than the report, which shows them in full on request.
pub fn network(network: &Network) {
    for (name, family) in [("IPv4", &network.ipv4), ("IPv6", &network.ipv6)] {
        let text = report::terminal::family_text(family, name, false);
        tracing::info!("{}", locale::words().network_line(name, &text));
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
        tracing::info!("{}", locale::words().tools_ready(&ready));
    }
    for tool in tools {
        if let Some(reason) = provisioned.failed.get(tool.name) {
            let clean_reason = report::terminal::strip_terminal_controls(reason, false);
            let line = locale::words().tool_failed(tool.name, tool.version, &clean_reason);
            tracing::info!("{line}");
        }
    }
}

/// Logs each plugin's start, its steps, its tools' runs with `-v`, and how it ended, in the
/// report's words.
pub struct Log;

impl Observer<Measured> for Log {
    fn on_start(&mut self, id: &'static str) {
        tracing::info!("{}", locale::words().item_started(id));
    }

    fn on_progress(&mut self, id: &'static str, step: &Step) {
        tracing::info!("{}", locale::words().item_progress(id, step));
    }

    fn on_tool(&mut self, id: &'static str, run: &ToolRun) {
        let command = report::terminal::strip_terminal_controls(&run.command.join(" "), false);
        tracing::debug!("{}", locale::words().tool_run(id, &command, &run.end));
        for line in &run.tail {
            let clean = report::terminal::strip_terminal_controls(line, false);
            tracing::debug!("{}", locale::words().tool_output(id, &clean));
        }
    }

    fn on_finish(&mut self, run: &Run<Measured>) {
        let status =
            report::terminal::strip_terminal_controls(&report::terminal::run_status(run), false);
        tracing::info!("{}", locale::words().item_ended(run.id, &status));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_time_is_written_as_hours_minutes_and_seconds() {
        assert_eq!(super::stamp(0), "[00:00:00]");
        assert_eq!(super::stamp(125), "[00:02:05]");
        assert_eq!(super::stamp(3 * 3600 + 25 * 60 + 7), "[03:25:07]");
        assert_eq!(super::stamp(100 * 3600 + 59 * 60 + 59), "[100:59:59]");
    }

    #[test]
    fn subscriber_strips_escape_sequences_from_log_events() {
        use crate::orchestrator::Observer;
        use bench_api::ToolEnd;
        use bench_api::ToolRun;
        use std::sync::Arc;
        use std::sync::Mutex;

        #[derive(Clone, Default)]
        struct VecSink(Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for VecSink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("lock").extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let sink = VecSink::default();
        let writer_sink = sink.clone();
        let sub = super::subscriber(move || writer_sink.clone(), true);
        tracing::subscriber::with_default(sub, || {
            let mut obs = super::Log;
            obs.on_tool(
                "network.iperf3",
                &ToolRun {
                    command: vec!["iperf3".to_owned(), "-c".to_owned(), "host".to_owned()],
                    end: ToolEnd::Exit(1),
                    tail: vec![
                        "err \x1b]52;c;SGVsbG8=\x07\x1b[31mspoof\x1b[0m\r\nforged".to_owned(),
                    ],
                },
            );
            tracing::info!("peer \r\u{202e}overwrite");
        });
        let logged = String::from_utf8(sink.0.lock().expect("lock").clone()).expect("utf8");
        assert!(!logged.contains('\x1b'));
        assert!(!logged.contains('\x07'));
        assert!(!logged.contains('\r'));
        assert!(!logged.contains('\u{202e}'));
        assert!(logged.contains("err spoof forged"), "{logged}");
        assert!(logged.contains("peer overwrite"), "{logged}");
    }
}
