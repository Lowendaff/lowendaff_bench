//! Interactive terminal interface powered by `ratatui` and `crossterm`: alternate-screen
//! lifecycle with panic restoration, background benchmark runner, in-memory `tracing` subscriber,
//! live progress dashboard, and 2D-scrollable multi-format report viewer.

pub mod config;
pub mod session;
pub mod taier_picker;
pub mod ui;

use std::collections::VecDeque;
use std::io;
use std::io::IsTerminal;
use std::io::Stdout;
use std::io::Write;
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
use crossterm::cursor::Show;
use crossterm::event;
use crossterm::event::DisableBracketedPaste;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::Event;
use crossterm::event::KeyEvent;
use crossterm::terminal;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::runtime;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::MissedTickBehavior;

use crate::app;
use crate::app::Outcome;
use crate::bgp_upload;
use crate::fetcher::DownloadProxy;
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
use crate::report_path;
use crate::results::Measured;

pub use config::GlobalConfig;
pub use config::ItemConfig;
pub use config::ItemParams;
pub use config::TuiConfig;
pub use session::Command;
pub use session::Editor;
pub use session::Pane;
pub use session::Screen;
pub use session::Session;
pub use taier_picker::TaierPicker;
pub use taier_picker::TaierPickerModal;

/// Runs the interactive TUI session until the user exits.
#[must_use]
pub fn run(registry: Vec<Entry>) -> ExitCode {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!("{}", locale::words().tui_not_a_tty());
        return ExitCode::from(app::EXIT_USAGE);
    }

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

    install_panic_hook();
    let mut term = match TerminalGuard::enter() {
        Ok(term) => term,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(app::EXIT_USAGE);
        }
    };

    let stop_reader = Arc::new(AtomicBool::new(false));
    let ev_rx = spawn_event_reader(Arc::clone(&stop_reader));

    let exit = runtime.block_on(event_loop(&registry, &mut term.terminal, ev_rx, &logs));
    stop_reader.store(true, Ordering::Relaxed);
    drop(term);

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

enum TermEvent {
    Key(KeyEvent),
    Paste(String),
    Resize,
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
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut ev_rx: UnboundedReceiver<TermEvent>,
    logs: &LogSink,
) -> LoopExit {
    let mut session = Session::new(registry);
    let mut active_run: Option<ActiveRun> = None;
    let (taier_tx, mut taier_rx) = mpsc::unbounded_channel();
    let mut taier_loading = false;
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let _ = terminal.draw(|frame| ui::draw(frame, &mut session));

    loop {
        tokio::select! {
            biased;
            Some(ev) = ev_rx.recv() => {
                let cmd = match ev {
                    TermEvent::Key(key) => session.handle_key(key),
                    TermEvent::Paste(text) => {
                        session.handle_paste(&text);
                        Command::None
                    }
                    TermEvent::Resize => Command::None,
                };
                match cmd {
                    Command::None => {}
                    Command::StartRun => {
                        if active_run.is_none() {
                            match launch_run(&mut session, registry, logs) {
                                Ok(run) => active_run = Some(run),
                                Err(err) => session.set_note(session::NoteKind::Error, err),
                            }
                        }
                    }
                    Command::LoadTaierNodes => {
                        if !taier_loading {
                            match DownloadProxy::try_new(
                                Some(&session.config.global.socks5_proxy),
                                Some(&session.config.global.github_proxy),
                            ) {
                                Ok(proxy) => {
                                    taier_loading = true;
                                    let tx = taier_tx.clone();
                                    tokio::spawn(async move {
                                        let res = app::fetch_taier_locations(&proxy).await;
                                        let _ = tx.send(res);
                                    });
                                }
                                Err(err) => session.on_taier_catalog_loaded(Err(err)),
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
                        let res = report_path::save_copy(&path, &contents);
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
                let _ = terminal.draw(|frame| ui::draw(frame, &mut session));
            }
            Some(res) = taier_rx.recv() => {
                taier_loading = false;
                session.on_taier_catalog_loaded(res);
                logs.drain_into(&mut session);
                let _ = terminal.draw(|frame| ui::draw(frame, &mut session));
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
                        let cols = terminal.size().map_or(80, |sz| sz.width);
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
                if dirty {
                    let _ = terminal.draw(|frame| ui::draw(frame, &mut session));
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
    let proxy = DownloadProxy::try_new(
        Some(&session.config.global.socks5_proxy),
        Some(&session.config.global.github_proxy),
    )?;

    let custom_output = {
        let trimmed = session.config.global.output.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    };
    if let Some(path) = &custom_output {
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
        let outcome = run_worker(
            planned,
            work_dir,
            connections,
            proxy,
            &worker_cancel,
            tx.clone(),
        )
        .await;
        let saved_note = match &outcome {
            Ok(done) => {
                let target_path = custom_output
                    .clone()
                    .unwrap_or_else(|| report_path::default_path(save_format));
                let text = done.render(save_format, None, save_show_ip);
                match report_path::save_copy(&target_path, &text) {
                    Ok(()) => Some((
                        session::NoteKind::Info,
                        locale::words().tui_saved_to(&target_path),
                    )),
                    Err(err) => Some((session::NoteKind::Error, err)),
                }
            }
            Err(_) => None,
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
    proxy: DownloadProxy,
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

    let fetchers = Fetchers::try_new_with_proxy(&proxy)?;
    let mut observer = TuiObserver { tx, log: Log };
    let mut outcome = app::execute_with_observer(
        planned,
        host,
        &work_dir,
        cancel,
        |family, url, range| fetchers.open(family, url, range),
        connections,
        &mut observer,
    )
    .await?;
    let upload_started = Instant::now();
    bgp_upload::populate_runs(&mut outcome.runs, cancel).await;
    outcome.total += upload_started.elapsed();
    if cancel.is_cancelled() {
        outcome.exit_code = app::EXIT_INTERRUPTED;
    }
    Ok(outcome)
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

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        prev(info);
    }));
}

fn restore_terminal() {
    let _ = terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        io::stdout(),
        DisableBracketedPaste,
        LeaveAlternateScreen,
        Show
    );
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        terminal::enable_raw_mode().map_err(|_| locale::words().tui_not_a_tty())?;
        let mut stdout = io::stdout();
        if crossterm::execute!(stdout, EnterAlternateScreen, EnableBracketedPaste).is_err() {
            let _ = terminal::disable_raw_mode();
            return Err(locale::words().tui_not_a_tty());
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|_| {
            restore_terminal();
            locale::words().tui_not_a_tty()
        })?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        restore_terminal();
    }
}

fn spawn_event_reader(stop: Arc<AtomicBool>) -> UnboundedReceiver<TermEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match event::poll(Duration::from_millis(80)) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) => {
                        if tx.send(TermEvent::Key(key)).is_err() {
                            return;
                        }
                    }
                    Ok(Event::Paste(text)) => {
                        if tx.send(TermEvent::Paste(text)).is_err() {
                            return;
                        }
                    }
                    Ok(Event::Resize(_, _)) => {
                        if tx.send(TermEvent::Resize).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
    rx
}

/// Captures `tracing` events into an in-memory queue instead of writing raw lines to `stderr`
/// while the `ratatui` alternate screen is active.
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
