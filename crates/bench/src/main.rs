//! `lowendaff-bench`: argument parsing, root check, runtime, Ctrl-C and exit code.

use std::io;
use std::process::ExitCode;
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
use bench::report_path;
use bench::tui;
use bench_api::CancellationToken;
use bench_host::profile::HostProfile;
use tokio::runtime;
use tokio::signal;

fn main() -> ExitCode {
    let default_tui = app::should_enter_tui(std::env::args_os().len());
    let cli = Cli::from_env();
    let registry = registry::all();
    if cli.list {
        app::list(&registry, &mut io::stdout());
        return ExitCode::SUCCESS;
    }
    if !rustix::process::geteuid().is_root() {
        eprintln!("{}", locale::words().not_root());
        return ExitCode::from(app::EXIT_USAGE);
    }
    if default_tui {
        return tui::run(registry);
    }
    progress::init(cli.verbose);
    let planned = match app::prepare(&cli, &registry) {
        Ok(planned) => planned,
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let proxy = match app::download_proxy(&cli) {
        Ok(proxy) => proxy,
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let output: Box<dyn io::Write + Send> = match &cli.output {
        Some(path) => match app::create_output(path) {
            Ok(file) => Box::new(file),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(app::EXIT_USAGE);
            }
        },
        None => Box::new(io::stdout()),
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
            eprintln!("{}", locale::words().runtime_failed(&err));
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let cancel = CancellationToken::new();
    runtime.spawn(watch_interrupt(cancel.clone()));
    let result = runtime.block_on(async {
        // The network takes longer than the files, so both start at once.
        let network = tokio::spawn(network::probe());
        let work_dir = cli.work_dir.clone();
        let mut host = tokio::task::spawn_blocking(move || HostProfile::collect(&work_dir))
            .await
            .map_err(|err| locale::words().host_failed(&err))?;
        progress::host(&host);
        host.network = network
            .await
            .map_err(|err| locale::words().network_failed(&err))?;
        progress::network(&host.network);
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
            |family, url, range| fetchers.open(family, url, range),
            usize::from(cli.download_connections),
            report,
            auto_copy.as_deref(),
        )
        .await
    });
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
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

/// First Ctrl-C stops the current test and skips the rest; a second one exits at once
/// (the run directory is then left behind).
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
