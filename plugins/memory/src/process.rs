//! Running a tool once: without a shell, in the C locale, within a
//! time limit, killed on cancellation, and for the NUMA matrix on the CPUs of one node.

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use bench_api::Context;
use tokio::process::Child;
use tokio::process::Command;

use crate::output::AttemptError;
use crate::output::Output;

/// One run of a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// The tool's name in reasons, such as `STREAM`.
    pub tool: &'static str,
    /// The program.
    pub program: PathBuf,
    /// Its arguments.
    pub args: Vec<String>,
    /// Its environment beyond the C locale.
    pub envs: Vec<(&'static str, String)>,
    /// The CPUs it runs on, or `None` to leave it to the scheduler.
    pub cpus: Option<Vec<usize>>,
    /// How long it may run.
    pub limit: Duration,
}

/// Runs `job` once and reads what it printed. A run past its limit, or cancelled, is killed.
pub async fn run(ctx: &Context, job: Job) -> Result<Output, AttemptError> {
    let mut command = Command::new(&job.program);
    command
        .args(&job.args)
        .envs(job.envs.iter().map(|(key, value)| (key, value)))
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let spawned = match job.cpus {
        Some(cpus) => spawn_on(command, cpus).await,
        None => command.spawn(),
    };
    let child =
        spawned.map_err(|err| AttemptError::Failed(format!("无法运行 {}：{err}", job.tool)))?;
    let cancel = ctx.cancel_token();
    tokio::select! {
        finished = tokio::time::timeout(job.limit, child.wait_with_output()) => match finished {
            Err(_) => Err(AttemptError::Failed(format!(
                "超过 {} 秒仍未结束，已停止",
                job.limit.as_secs()
            ))),
            Ok(Err(err)) => Err(AttemptError::Failed(format!(
                "无法读取 {} 的输出：{err}",
                job.tool
            ))),
            Ok(Ok(output)) => {
                let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                text.push_str(&String::from_utf8_lossy(&output.stderr));
                Ok(Output {
                    code: output.status.code(),
                    text,
                })
            }
        },
        () = cancel.cancelled() => Err(AttemptError::Cancelled),
    }
}

/// Starts `command` from a new thread that first moves itself to `cpus`, so that the child
/// inherits them while the runtime's own threads keep theirs.
#[cfg(target_os = "linux")]
async fn spawn_on(mut command: Command, cpus: Vec<usize>) -> io::Result<Child> {
    let runtime = tokio::runtime::Handle::current();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("memory-placement".to_owned())
        .spawn(move || {
            let _runtime = runtime.enter();
            let spawned = place(&cpus).and_then(|()| command.spawn());
            // Without a receiver the child is dropped here, and killed.
            let _ = sender.send(spawned);
        })?;
    receiver
        .await
        .map_err(|_| io::Error::other("the thread starting the tool ended early"))?
}

/// Moves the calling thread to `cpus`.
#[cfg(target_os = "linux")]
fn place(cpus: &[usize]) -> io::Result<()> {
    use rustix::thread::CpuSet;

    let mut set = CpuSet::new();
    for &cpu in cpus {
        if cpu >= CpuSet::MAX_CPU {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("CPU {cpu} is beyond an affinity mask"),
            ));
        }
        set.set(cpu);
    }
    rustix::thread::sched_setaffinity(None, &set).map_err(io::Error::from)
}

/// The tools run on Linux only; elsewhere, as in the tests on other systems, the scheduler
/// places them.
#[cfg(not(target_os = "linux"))]
async fn spawn_on(mut command: Command, _cpus: Vec<usize>) -> io::Result<Child> {
    command.spawn()
}
