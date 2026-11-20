//! Runner for PassMark PerformanceTest Linux (`-r 1` CPU suite) with bundled `passmark-libs`.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Measurement;
use bench_api::Step;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Fault;
use super::result::Output;

/// Time limit for one PassMark CPU suite run.
pub const RUN_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Builds the PassMark `-r 1` job with `LD_LIBRARY_PATH`, `TERMINFO`, and `TERM=xterm`.
#[must_use]
pub fn attempt(program: &Path, libs_file: &Path, work_dir: &Path) -> Job {
    let libs_dir = libs_file.parent().unwrap_or_else(|| Path::new("."));
    let mut ld_path = OsString::from(libs_dir.as_os_str());
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH")
        && !existing.is_empty()
    {
        ld_path.push(":");
        ld_path.push(existing);
    }
    let terminfo = libs_dir.join("terminfo");
    let mut job = Job::new(program, ["-r", "1"], RUN_LIMIT);
    job.envs = vec![
        ("LD_LIBRARY_PATH".into(), ld_path),
        ("TERMINFO".into(), terminfo.into_os_string()),
        ("TERM".into(), "xterm".into()),
    ];
    job.dir = Some(work_dir.to_path_buf());
    job.group = true;
    job.first_to_go = true;
    job.quotable = false;
    job
}

/// Runs PassMark in `work_dir` and parses `results_cpu.yml`.
pub async fn measure<O, F, Fut>(
    ctx: &Context<O>,
    program: &Path,
    libs_file: &Path,
    work_dir: &Path,
    run: F,
) -> Result<Output, Failure>
where
    O: Measurement,
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    ctx.progress(Step::CpuSuite {
        suite: "passmark",
        stage: None,
        threads: None,
    });
    if let Err(err) = tokio::fs::create_dir_all(work_dir).await {
        return Err(Failure::Run(Fault::Io(err.to_string())));
    }
    let out = match run(attempt(program, libs_file, work_dir)).await {
        Ok(out) => out,
        Err(RunError::Fault(fault)) => return Err(Failure::Run(Fault::Tool(fault))),
        Err(RunError::Cancelled) => return Err(Failure::Cancelled),
    };
    if let Err(fault) = parse::check_exit(&out) {
        return Err(Failure::Run(fault));
    }
    let yml_path = work_dir.join("results_cpu.yml");
    let yml = tokio::fs::read_to_string(&yml_path)
        .await
        .map_err(|_| Failure::Run(Fault::NoScores))?;
    parse::results_yml(&yml).map_err(Failure::Run)
}
