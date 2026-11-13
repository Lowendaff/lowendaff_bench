//! `lowendaff-bench`: argument parsing, root check, runtime, Ctrl-C and exit code.

use std::io;
use std::process::ExitCode;
use std::thread;

use bench::app;
use bench::app::Cli;
use bench::probe;
use bench::registry::REGISTRY;
use bench_api::CancellationToken;
use tokio::runtime;
use tokio::signal;

fn main() -> ExitCode {
    let cli = Cli::from_env();
    if cli.list {
        app::list(REGISTRY, &mut io::stdout());
        return ExitCode::SUCCESS;
    }
    if !rustix::process::geteuid().is_root() {
        eprintln!("lowendaff-bench 必须以 root 身份运行");
        return ExitCode::from(app::EXIT_USAGE);
    }
    let planned = match app::prepare(&cli, REGISTRY) {
        Ok(planned) => planned,
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            return ExitCode::from(app::EXIT_USAGE);
        }
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
            eprintln!("无法启动运行时：{err}");
            return ExitCode::from(app::EXIT_USAGE);
        }
    };
    let cancel = CancellationToken::new();
    runtime.spawn(watch_interrupt(cancel.clone()));
    let result = runtime.block_on(async {
        let env = probe::probe().await;
        app::execute(planned, env, &cli.work_dir, &cancel, io::stdout(), width()).await
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
    eprintln!("\n收到中断信号，正在停止当前测试；再按一次 Ctrl-C 立即退出");
    if signal::ctrl_c().await.is_ok() {
        std::process::exit(i32::from(app::EXIT_INTERRUPTED));
    }
}
