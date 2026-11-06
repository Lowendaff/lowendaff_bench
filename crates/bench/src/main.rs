//! `lowendaff-bench`: argument parsing, root check, runtime, Ctrl-C and exit code.

use std::process::ExitCode;

use bench::app::Cli;
use bench::app::EXIT_INTERRUPTED;
use bench::app::EXIT_USAGE;
use bench::app::execute;
use bench::app::list;
use bench::app::prepare;
use bench::probe::probe;
use bench::registry::REGISTRY;
use bench_api::CancellationToken;

fn main() -> ExitCode {
    let cli = Cli::from_env();
    if cli.list {
        list(REGISTRY, &mut std::io::stdout());
        return ExitCode::SUCCESS;
    }
    if !rustix::process::geteuid().is_root() {
        eprintln!("lowendaff-bench 必须以 root 身份运行");
        return ExitCode::from(EXIT_USAGE);
    }
    let planned = match prepare(&cli, REGISTRY) {
        Ok(planned) => planned,
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            return ExitCode::from(EXIT_USAGE);
        }
    };
    // At least two workers, so a plugin that blocks a thread cannot stall the timeouts.
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().max(2));
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("无法启动运行时：{err}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let cancel = CancellationToken::new();
    runtime.spawn(watch_interrupt(cancel.clone()));
    let result = runtime.block_on(async {
        let env = probe().await;
        execute(planned, env, &cli.work_dir, &cancel, std::io::stdout()).await
    });
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// First Ctrl-C stops the current test and skips the rest; a second one exits at once
/// (the run directory is then left behind).
async fn watch_interrupt(cancel: CancellationToken) {
    if tokio::signal::ctrl_c().await.is_err() {
        return;
    }
    cancel.cancel();
    eprintln!("\n收到中断信号，正在停止当前测试；再按一次 Ctrl-C 立即退出");
    if tokio::signal::ctrl_c().await.is_ok() {
        std::process::exit(i32::from(EXIT_INTERRUPTED));
    }
}
