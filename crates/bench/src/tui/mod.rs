//! Interactive `--tui` terminal interface: raw `/dev/tty` session, background benchmark runner,
//! in-memory `tracing` subscriber, live progress dashboard, and scrollable report viewer.

pub mod canvas;
pub mod config;
pub mod input;
pub mod render;
pub mod session;

use std::collections::VecDeque;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::Step;
use bench_api::ToolRun;
use bench_host::profile::HostProfile;
use rustix::termios::OptionalActions;
use rustix::termios::SpecialCodeIndex;
use rustix::termios::Termios;
use tokio::runtime;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::app;
use crate::app::Outcome;
use crate::fetcher::Fetchers;
use crate::locale;
use crate::network;
use crate::orchestrator::Observer;
use crate::orchestrator::Planned;
use crate::orchestrator::Run;
use crate::orchestrator::Status;
use crate::progress;
use crate::progress::Log;
use crate::registry::Entry;
use crate::report;
use crate::results::Measured;

pub use config::GlobalConfig;
pub use config::ItemConfig;
pub use config::ItemParams;
pub use config::TuiConfig;
pub use input::Key;
pub use session::Command;
pub use session::Pane;
pub use session::Screen;
pub use session::Session;

/// Runs the interactive `--tui` session until the user exits.
#[must_use]
pub fn run(registry: Vec<Entry>) -> ExitCode {
    let (tty_read, tty_write, saved) = match open_tty() {
        Ok(handles) => handles,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let workers = thread::available_parallelism().map_or(2, |count| count.get().max(2));
    let runtime = match runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("{}", locale::words().runtime_failed(&err));
            return ExitCode::from(app::EXIT_USAGE);
        }
    };

    let logs = LogSink::new();
    logs.install_global();

    let mut guard = match TerminalGuard::enter(tty_write, saved) {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(app::EXIT_USAGE);
        }
    };

    let stop_reader = Arc::new(AtomicBool::new(false));
    let key_rx = spawn_key_reader(tty_read, Arc::clone(&stop_reader));

    let exit = runtime.block_on(event_loop(&registry, &mut guard, key_rx, &logs));
    stop_reader.store(true, Ordering::Relaxed);
    drop(guard);

    if let Some(report) = exit.printed_report {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(report.as_bytes());
        let _ = stdout.flush();
    }
    ExitCode::from(exit.code)
}

struct LoopExit {
    code: u8,
    printed_report: Option<String>,
}

enum WorkerEvent {
    HostSummary(String),
    ItemStart(&'static str),
    ItemStep(&'static str, String),
    ItemFinish {
        id: &'static str,
        status: String,
        failed: bool,
        skipped: bool,
    },
    Finished {
        result: Result<Outcome, String>,
        saved_note: Option<(session::NoteKind, String)>,
    },
}

struct ActiveRun {
    started: Instant,
    cancel: CancellationToken,
    rx: UnboundedReceiver<WorkerEvent>,
}

async fn event_loop(
    registry: &[Entry],
    guard: &mut TerminalGuard,
    mut key_rx: UnboundedReceiver<Key>,
    logs: &LogSink,
) -> LoopExit {
    let mut session = Session::new(registry);
    let mut active_run: Option<ActiveRun> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    draw_frame(&mut session, guard);

    loop {
        tokio::select! {
            biased;
            Some(key) = key_rx.recv() => {
                match session.handle_key(key) {
                    Command::None => {}
                    Command::StartRun => {
                        if active_run.is_none() {
                            match launch_run(&mut session, registry, logs) {
                                Ok(run) => active_run = Some(run),
                                Err(err) => session.set_note(session::NoteKind::Error, err),
                            }
                        }
                    }
                    Command::CancelRun => {
                        if let Some(run) = &active_run {
                            run.cancel.cancel();
                            tracing::info!("{}", locale::words().interrupt());
                        }
                    }
                    Command::SaveReport { path, contents } => {
                        let res = write_report_file(&path, &contents);
                        session.on_report_saved(&path, res);
                    }
                    Command::Quit(code) => {
                        if let Some(run) = &active_run {
                            run.cancel.cancel();
                        }
                        return LoopExit {
                            code,
                            printed_report: None,
                        };
                    }
                    Command::PrintAndQuit { report, exit_code } => {
                        return LoopExit {
                            code: exit_code,
                            printed_report: Some(report),
                        };
                    }
                }
                logs.drain_into(&mut session);
                draw_frame(&mut session, guard);
            }
            _ = tick.tick() => {
                let mut dirty = logs.drain_into(&mut session);
                if let Some(run) = &mut active_run {
                    session.set_elapsed(run.started.elapsed());
                    dirty = true;
                    let mut finished = None;
                    while let Ok(ev) = run.rx.try_recv() {
                        match ev {
                            WorkerEvent::HostSummary(text) => session.set_host_summary(text),
                            WorkerEvent::ItemStart(id) => session.on_item_start(id),
                            WorkerEvent::ItemStep(id, step) => session.on_item_step(id, step),
                            WorkerEvent::ItemFinish {
                                id,
                                status,
                                failed,
                                skipped,
                            } => session.on_item_finish(id, status, failed, skipped),
                            WorkerEvent::Finished { result, saved_note } => {
                                finished = Some((result, saved_note));
                            }
                        }
                    }
                    if let Some((result, saved_note)) = finished {
                        active_run = None;
                        logs.drain_into(&mut session);
                        let (cols, _) = guard.size();
                        let wrap_w = cols.saturating_sub(28).max(40);
                        match result {
                            Ok(outcome) => {
                                session.finish_run(outcome, Some(wrap_w));
                                if let Some((kind, msg)) = saved_note {
                                    session.set_note(kind, msg);
                                }
                            }
                            Err(err) => {
                                session.screen = Screen::Configure;
                                session.running = None;
                                session.set_note(session::NoteKind::Error, err);
                            }
                        }
                    }
                }
                if dirty || guard.check_resized() {
                    draw_frame(&mut session, guard);
                }
            }
        }
    }
}

fn launch_run(
    session: &mut Session,
    registry: &[Entry],
    logs: &LogSink,
) -> Result<ActiveRun, String> {
    let planned = session
        .config
        .build_plan(registry)
        .map_err(|errs| errs.join("; "))?;

    let output_path = {
        let trimmed = session.config.global.output.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    };
    if let Some(path) = &output_path {
        let _file = app::create_output(path)?;
    }

    logs.clear();
    logs.set_verbose(session.config.global.verbose);

    let selected_meta: Vec<(&'static str, &'static str)> = planned
        .iter()
        .map(|item| (item.id(), item.title().unwrap_or(item.id())))
        .collect();
    session.begin_run(&selected_meta);

    let work_dir = PathBuf::from(session.config.global.work_dir.trim());
    let connections = usize::from(session.config.global.download_connections);
    let save_format = session.config.global.format;
    let save_show_ip = session.config.global.show_ip;
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let (tx, rx) = mpsc::unbounded_channel();
    let started = Instant::now();

    tokio::spawn(async move {
        let outcome = run_worker(planned, work_dir, connections, &worker_cancel, tx.clone()).await;
        let saved_note = match (&outcome, &output_path) {
            (Ok(done), Some(path)) => {
                let text = done.render(save_format, None, save_show_ip);
                match write_report_file(path, &text) {
                    Ok(()) => Some((session::NoteKind::Info, locale::words().tui_saved_to(path))),
                    Err(err) => Some((session::NoteKind::Error, err)),
                }
            }
            _ => None,
        };
        let _ = tx.send(WorkerEvent::Finished {
            result: outcome,
            saved_note,
        });
    });

    Ok(ActiveRun {
        started,
        cancel,
        rx,
    })
}

async fn run_worker(
    planned: Vec<Planned<Measured>>,
    work_dir: PathBuf,
    connections: usize,
    cancel: &CancellationToken,
    tx: UnboundedSender<WorkerEvent>,
) -> Result<Outcome, String> {
    let net_task = tokio::spawn(network::probe());
    let dir_for_host = work_dir.clone();
    let mut host = tokio::task::spawn_blocking(move || HostProfile::collect(&dir_for_host))
        .await
        .map_err(|err| locale::words().host_failed(&err))?;
    progress::host(&host);
    host.network = net_task
        .await
        .map_err(|err| locale::words().network_failed(&err))?;
    progress::network(&host.network);

    let platform = report::terminal::platform_text(host.platform);
    let v4 = report::terminal::family_text(&host.network.ipv4, "IPv4", false);
    let v6 = report::terminal::family_text(&host.network.ipv6, "IPv6", false);
    let summary = format!("{platform}, {}  |  IPv4: {v4}  |  IPv6: {v6}", host.arch);
    let _ = tx.send(WorkerEvent::HostSummary(summary));

    let fetchers = Fetchers::try_new()?;
    let mut observer = TuiObserver { tx, log: Log };
    app::execute_with_observer(
        planned,
        host,
        &work_dir,
        cancel,
        |family, url, range| fetchers.open(family, url, range),
        connections,
        &mut observer,
    )
    .await
}

struct TuiObserver {
    tx: UnboundedSender<WorkerEvent>,
    log: Log,
}

impl Observer<Measured> for TuiObserver {
    fn on_start(&mut self, id: &'static str) {
        self.log.on_start(id);
        let _ = self.tx.send(WorkerEvent::ItemStart(id));
    }

    fn on_progress(&mut self, id: &'static str, step: Step) {
        self.log.on_progress(id, step);
        let full = locale::words().item_progress(id, step);
        let short = full
            .strip_prefix(id)
            .map_or(full.as_str(), str::trim_start)
            .to_owned();
        let _ = self.tx.send(WorkerEvent::ItemStep(id, short));
    }

    fn on_tool(&mut self, id: &'static str, run: &ToolRun) {
        self.log.on_tool(id, run);
    }

    fn on_finish(&mut self, run: &Run<Measured>) {
        self.log.on_finish(run);
        let status = report::terminal::run_status(run);
        let failed = matches!(run.status, Status::Failed(_) | Status::TimedOut);
        let skipped = matches!(run.status, Status::Skipped(_) | Status::Cancelled);
        let _ = self.tx.send(WorkerEvent::ItemFinish {
            id: run.id,
            status,
            failed,
            skipped,
        });
    }
}

fn write_report_file(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = app::create_output(path)?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|err| locale::words().output_unwritable(path, &err))
}

fn draw_frame(session: &mut Session, guard: &mut TerminalGuard) {
    let (cols, rows) = guard.size();
    let canvas = render::render(session, cols, rows);
    let _ = guard.write_frame(&canvas.to_ansi());
}

fn open_tty() -> Result<(File, File, Termios), String> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| locale::words().tui_not_a_tty())?;
    if !rustix::termios::isatty(&tty) {
        return Err(locale::words().tui_not_a_tty());
    }
    let saved = rustix::termios::tcgetattr(&tty).map_err(|_| locale::words().tui_not_a_tty())?;
    let reader = tty
        .try_clone()
        .map_err(|_| locale::words().tui_not_a_tty())?;
    Ok((reader, tty, saved))
}

struct TerminalGuard {
    tty: File,
    saved: Termios,
    last_size: (u16, u16),
}

impl TerminalGuard {
    fn enter(mut tty: File, saved: Termios) -> Result<Self, String> {
        let mut raw = saved.clone();
        raw.make_raw();
        raw.special_codes[SpecialCodeIndex::VMIN] = 0;
        raw.special_codes[SpecialCodeIndex::VTIME] = 1;
        rustix::termios::tcsetattr(&tty, OptionalActions::Now, &raw)
            .map_err(|_| locale::words().tui_not_a_tty())?;
        let _ = tty.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H");
        let _ = tty.flush();
        let size = query_size(&tty);
        Ok(Self {
            tty,
            saved,
            last_size: size,
        })
    }

    fn size(&mut self) -> (u16, u16) {
        let size = query_size(&self.tty);
        self.last_size = size;
        size
    }

    fn check_resized(&mut self) -> bool {
        let now = query_size(&self.tty);
        if now != self.last_size {
            self.last_size = now;
            true
        } else {
            false
        }
    }

    fn write_frame(&mut self, ansi: &str) -> io::Result<()> {
        self.tty.write_all(ansi.as_bytes())?;
        self.tty.flush()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.tty.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = self.tty.flush();
        let _ = rustix::termios::tcsetattr(&self.tty, OptionalActions::Now, &self.saved);
    }
}

fn query_size(tty: &File) -> (u16, u16) {
    rustix::termios::tcgetwinsize(tty)
        .ok()
        .filter(|ws| ws.ws_col > 0 && ws.ws_row > 0)
        .map_or((80, 24), |ws| (ws.ws_col, ws.ws_row))
}

fn spawn_key_reader(mut reader: File, stop: Arc<AtomicBool>) -> UnboundedReceiver<Key> {
    let (tx, rx) = mpsc::unbounded_channel();
    thread::spawn(move || {
        let mut pending = Vec::with_capacity(64);
        let mut scratch = [0_u8; 64];
        while !stop.load(Ordering::Relaxed) {
            match reader.read(&mut scratch) {
                Ok(0) => {
                    if !pending.is_empty() {
                        let (keys, used) = input::parse_keys(&pending, true);
                        pending.drain(..used);
                        for key in keys {
                            if tx.send(key).is_err() {
                                return;
                            }
                        }
                    }
                }
                Ok(n) => {
                    pending.extend_from_slice(&scratch[..n]);
                    let (keys, used) = input::parse_keys(&pending, false);
                    pending.drain(..used);
                    for key in keys {
                        if tx.send(key).is_err() {
                            return;
                        }
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
    rx
}

/// Captures `tracing` events into an in-memory queue instead of writing raw lines to `stderr`
/// while the alternate screen is active.
#[derive(Clone)]
struct LogSink {
    inner: Arc<LogSinkInner>,
}

struct LogSinkInner {
    verbose: AtomicBool,
    lines: Mutex<VecDeque<String>>,
    partial: Mutex<String>,
}

impl LogSink {
    fn new() -> Self {
        Self {
            inner: Arc::new(LogSinkInner {
                verbose: AtomicBool::new(false),
                lines: Mutex::new(VecDeque::with_capacity(256)),
                partial: Mutex::new(String::new()),
            }),
        }
    }

    fn install_global(&self) {
        let sub = progress::subscriber(self.clone(), true);
        let _ = tracing::subscriber::set_global_default(sub);
    }

    fn set_verbose(&self, verbose: bool) {
        self.inner.verbose.store(verbose, Ordering::Relaxed);
    }

    fn clear(&self) {
        if let Ok(mut lines) = self.inner.lines.lock() {
            lines.clear();
        }
        if let Ok(mut partial) = self.inner.partial.lock() {
            partial.clear();
        }
    }

    fn drain_into(&self, session: &mut Session) -> bool {
        let Ok(mut lines) = self.inner.lines.lock() else {
            return false;
        };
        if lines.is_empty() {
            return false;
        }
        while let Some(line) = lines.pop_front() {
            session.push_log(line);
        }
        true
    }
}

struct SinkWriter {
    inner: Option<Arc<LogSinkInner>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
    type Writer = SinkWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SinkWriter {
            inner: Some(Arc::clone(&self.inner)),
        }
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        if *meta.level() > tracing::Level::INFO && !self.inner.verbose.load(Ordering::Relaxed) {
            SinkWriter { inner: None }
        } else {
            self.make_writer()
        }
    }
}

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(inner) = &self.inner else {
            return Ok(buf.len());
        };
        let text = String::from_utf8_lossy(buf);
        if let Ok(mut partial) = inner.partial.lock() {
            partial.push_str(&text);
            while let Some(pos) = partial.find('\n') {
                let line = partial[..pos].trim_end_matches('\r').to_owned();
                partial.drain(..=pos);
                if !line.is_empty()
                    && let Ok(mut lines) = inner.lines.lock()
                {
                    if lines.len() >= 2000 {
                        lines.pop_front();
                    }
                    lines.push_back(line);
                }
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
