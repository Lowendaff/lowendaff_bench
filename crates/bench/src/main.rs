//! `lowendaff-bench`: argument parsing, root check, runtime, Ctrl-C and exit code.

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use bench::app;
use bench::app::Cli;
use bench::app::ReportOptions;
use bench::fetcher::Fetchers;
use bench::locale;
use bench::network;
use bench::progress;
use bench::registry;
use bench::report::Format;
use bench::report::terminal::strip_terminal_controls;
use bench::report_path;
use bench::tui;
use bench_api::CancellationToken;
use bench_host::profile::HostProfile;
use tokio::runtime;
use tokio::signal;

enum OutputSink {
    Stdout(io::Stdout),
    File {
        file: std::fs::File,
        error: Arc<Mutex<Option<io::Error>>>,
    },
}

impl io::Write for OutputSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(out) => out.write(buf),
            Self::File { file, error } => file.write(buf).inspect_err(|err| {
                if let Ok(mut slot) = error.lock()
                    && slot.is_none()
                {
                    *slot = Some(io::Error::new(err.kind(), err.to_string()));
                }
            }),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(out) => out.flush(),
            Self::File { file, error } => file.flush().inspect_err(|err| {
                if let Ok(mut slot) = error.lock()
                    && slot.is_none()
                {
                    *slot = Some(io::Error::new(err.kind(), err.to_string()));
                }
            }),
        }
    }
}

fn print_stderr(message: &str) {
    let clean = strip_terminal_controls(message, false);
    eprintln!("{clean}");
}

fn main() -> ExitCode {
    let default_tui = app::should_enter_tui(std::env::args_os().len());
    let cli = Cli::from_env();
    let registry = registry::all();
    if cli.list {
        app::list(&registry, &mut io::stdout());
        return ExitCode::SUCCESS;
    }
    if let Some(filter) = &cli.taier_list {
        return run_taier_list(&cli, filter);
    }
    if !rustix::process::geteuid().is_root() {
        print_stderr(&locale::words().not_root());
        return ExitCode::from(app::EXIT_USAGE);
    }
    if default_tui {
        return tui::run(&registry);
    }
    progress::init(cli.verbose);
    let (planned, proxy) = match app::prepare_with_proxy(&cli, &registry) {
        Ok(ready) => ready,
        Err(errors) => {
            for error in errors {
                print_stderr(&error);
            }
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let file_write_err: Option<(PathBuf, Arc<Mutex<Option<io::Error>>>)>;
    let output: OutputSink = match &cli.output {
        Some(path) => match app::create_output(path) {
            Ok(file) => {
                let err_slot = Arc::new(Mutex::new(None));
                file_write_err = Some((path.clone(), Arc::clone(&err_slot)));
                OutputSink::File {
                    file,
                    error: err_slot,
                }
            }
            Err(error) => {
                print_stderr(&error);
                return ExitCode::from(app::EXIT_USAGE);
            }
        },
        None => {
            file_write_err = None;
            OutputSink::Stdout(io::stdout())
        }
    };
    let report_width = if cli.output.is_none() && cli.format == Format::Text {
        width()
    } else {
        None
    };
    // At least two workers, so a plugin that blocks a thread cannot stall the timeouts.
    let workers = thread::available_parallelism().map_or(2, |count| count.get().max(2));
    let runtime = match runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            print_stderr(&locale::words().runtime_failed(&err));
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let cancel = CancellationToken::new();
    runtime.spawn(watch_interrupt(cancel.clone()));
    let result = runtime.block_on(async {
        // The network takes longer than the files, so both start at once.
        let net_cancel = cancel.clone();
        let network = tokio::spawn(async move { network::probe_until(&net_cancel).await });
        let work_dir = cli.work_dir.clone();
        let mut host = tokio::task::spawn_blocking(move || HostProfile::collect(&work_dir))
            .await
            .map_err(|err| locale::words().host_failed(&err))?;
        progress::host(&host);
        host.network = network
            .await
            .map_err(|err| locale::words().network_failed(&err))?;
        if !cancel.is_cancelled() {
            progress::network(&host.network);
        }
        let fetchers = Fetchers::try_new_with_proxy(&proxy)?;
        let auto_copy = cli
            .output
            .is_none()
            .then(|| report_path::default_path(cli.format));
        let report = ReportOptions {
            out: output,
            format: cli.format,
            width: report_width,
            show_ip: cli.show_ip,
        };
        app::execute_with_copy(
            planned,
            host,
            &cli.work_dir,
            &cancel,
            |family, url, range| {
                let fetchers = &fetchers;
                async move { fetchers.open(family, &url, range).await }
            },
            usize::from(cli.download_connections),
            report,
            auto_copy.as_deref(),
        )
        .await
    });
    runtime.shutdown_background();
    match result {
        Ok(code) => {
            if let Some((path, err_slot)) = file_write_err
                && let Ok(mut guard) = err_slot.lock()
                && let Some(err) = guard.take()
            {
                print_stderr(&locale::words().output_unwritable(&path, &err));
                return ExitCode::from(app::EXIT_USAGE);
            }
            ExitCode::from(code)
        }
        Err(error) => {
            print_stderr(&error);
            ExitCode::from(app::EXIT_USAGE)
        }
    }
}

fn run_taier_list(cli: &Cli, filter: &str) -> ExitCode {
    progress::init(cli.verbose);
    let proxy = match app::download_proxy(cli) {
        Ok(proxy) => proxy,
        Err(errors) => {
            for error in errors {
                print_stderr(&error);
            }
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            print_stderr(&locale::words().runtime_failed(&err));
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let locations = runtime.block_on(app::fetch_taier_locations(&proxy));
    runtime.shutdown_background();
    let locations = match locations {
        Ok(locations) => locations,
        Err(err) => {
            print_stderr(&err);
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    match app::format_taier_list(&locations, filter, &mut io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            print_stderr(&err);
            ExitCode::from(app::EXIT_USAGE)
        }
    }
}

/// The terminal's width when stdout is one; the report's tables wrap to it.
fn width() -> Option<u16> {
    let stdout = io::stdout();
    if !rustix::termios::isatty(&stdout) {
        return None;
    }
    rustix::termios::tcgetwinsize(&stdout)
        .ok()
        .map(|size| size.ws_col)
        .filter(|columns| *columns > 0)
}

#[cfg(unix)]
async fn recv_any_signal(
    int: &mut Option<signal::unix::Signal>,
    term: &mut Option<signal::unix::Signal>,
    hup: &mut Option<signal::unix::Signal>,
) -> Option<()> {
    let f_int = async {
        match int.as_mut() {
            Some(s) => s.recv().await,
            None => std::future::pending().await,
        }
    };
    let f_term = async {
        match term.as_mut() {
            Some(s) => s.recv().await,
            None => std::future::pending().await,
        }
    };
    let f_hup = async {
        match hup.as_mut() {
            Some(s) => s.recv().await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        r = f_int => r,
        r = f_term => r,
        r = f_hup => r,
    }
}

/// First termination signal (`SIGINT`, `SIGTERM`, `SIGHUP`) stops the current test and skips the
/// rest; a second one exits at once (the run directory is then left behind).
#[cfg(unix)]
async fn watch_interrupt(cancel: CancellationToken) {
    use tokio::signal::unix::SignalKind;

    let mut sigint = signal::unix::signal(SignalKind::interrupt()).ok();
    let mut sigterm = signal::unix::signal(SignalKind::terminate()).ok();
    let mut sighup = signal::unix::signal(SignalKind::hangup()).ok();
    if sigint.is_none() && sigterm.is_none() && sighup.is_none() {
        return;
    }

    if recv_any_signal(&mut sigint, &mut sigterm, &mut sighup)
        .await
        .is_none()
    {
        return;
    }
    cancel.cancel();
    tracing::info!("{}", locale::words().interrupt());
    if recv_any_signal(&mut sigint, &mut sigterm, &mut sighup)
        .await
        .is_some()
    {
        std::process::exit(i32::from(app::EXIT_INTERRUPTED));
    }
}

#[cfg(not(unix))]
async fn watch_interrupt(cancel: CancellationToken) {
    if signal::ctrl_c().await.is_err() {
        return;
    }
    cancel.cancel();
    tracing::info!("{}", locale::words().interrupt());
    if signal::ctrl_c().await.is_ok() {
        std::process::exit(i32::from(app::EXIT_INTERRUPTED));
    }
}
