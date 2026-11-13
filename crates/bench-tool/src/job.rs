//! Running a tool once, as every test item does: without a shell, in the C locale, with what it
//! prints read in full, within a time limit, and stopped when the plugin is cancelled. A tool that
//! starts workers of its own runs in a process group of its own, which is killed as a whole; a
//! tool that may exhaust memory starts as the kernel's first choice to kill.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::typed::Context;
use bench_api::typed::Measurement;
use rustix::process::Pid;
use rustix::process::Signal;
use serde::Serialize;
use tokio::process::Child;
use tokio::process::Command;

/// The program's own OOM score adjustment.
const OOM_SCORE_ADJ: &str = "/proc/self/oom_score_adj";

/// One run of a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// The program: the path of a provisioned tool.
    pub program: PathBuf,
    /// Its arguments.
    pub args: Vec<OsString>,
    /// Its environment beyond `LC_ALL=C`, which every tool gets.
    pub envs: Vec<(OsString, OsString)>,
    /// Its working directory; `None` leaves the program's own.
    pub dir: Option<PathBuf>,
    /// How long it may run before it is stopped.
    pub limit: Duration,
    /// Whether it runs in a process group of its own, killed as a whole: stopping a tool that
    /// starts workers would otherwise leave them running.
    pub group: bool,
    /// Whether it starts with `oom_score_adj` 1000, so that the kernel kills it, rather than
    /// the program, when memory runs out.
    pub first_to_go: bool,
}

impl Job {
    /// A run of `program` with `args` within `limit`, with no other option.
    #[must_use]
    pub fn new<A>(program: impl Into<PathBuf>, args: A, limit: Duration) -> Self
    where
        A: IntoIterator,
        A::Item: Into<OsString>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            envs: Vec::new(),
            dir: None,
            limit,
            group: false,
            first_to_go: false,
        }
    }

    /// Runs the tool once and reads what it printed. A run past its limit, or cancelled, is
    /// stopped; a run cancelled before it starts never starts.
    pub async fn run<O: Measurement>(&self, ctx: &Context<O>) -> Outcome {
        let cancel = ctx.cancel_token();
        if cancel.is_cancelled() {
            return Outcome {
                ended: Err(RunError::Cancelled),
                adjustment: None,
            };
        }
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .envs(self.envs.iter().map(|(key, value)| (key, value)))
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = &self.dir {
            command.current_dir(dir);
        }
        if self.group {
            command.process_group(0);
        }
        let (spawned, adjustment) = if self.first_to_go {
            let (spawned, adjustment) = spawn_first_to_go(&mut command);
            (spawned, Some(adjustment))
        } else {
            (command.spawn(), None)
        };
        let ended = match spawned {
            Ok(child) => self.wait(child, &cancel).await,
            Err(err) => Err(RunError::Fault(Fault::Spawn(err.to_string()))),
        };
        Outcome { ended, adjustment }
    }

    /// Waits for `child` within the limit, or until `cancel` fires, and kills its group when it
    /// does not end on its own.
    async fn wait(&self, child: Child, cancel: &CancellationToken) -> Result<Output, RunError> {
        // On a timeout or a cancellation the group is killed when this is dropped, at the end:
        // its id is not given to another group while any of its processes lives.
        let group = self.group.then(|| Group::of(&child));
        let ended = tokio::select! {
            finished = tokio::time::timeout(self.limit, child.wait_with_output()) => {
                match finished {
                    Ok(Ok(output)) => {
                        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
                        text.push_str(&String::from_utf8_lossy(&output.stderr));
                        Ok(Output {
                            code: output.status.code(),
                            text,
                        })
                    }
                    Ok(Err(err)) => Err(RunError::Fault(Fault::Read(err.to_string()))),
                    Err(_) => Err(RunError::Fault(Fault::TimedOut(self.limit))),
                }
            }
            () = cancel.cancelled() => Err(RunError::Cancelled),
        };
        // The leader ended on its own and was reaped: once any workers it left end too, the
        // group's id may be another group's.
        if ended.is_ok()
            && let Some(group) = group
        {
            group.disarm();
        }
        ended
    }
}

/// How a run ended, and what became of `oom_score_adj`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// What the tool printed, or why there is nothing to read.
    pub ended: Result<Output, RunError>,
    /// For a job with [`Job::first_to_go`] that started, whether the tool started as the
    /// kernel's first choice; `None` for other jobs.
    pub adjustment: Option<Adjustment>,
}

/// What a finished run printed, and how it exited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code, or `None` when a signal ended the tool.
    pub code: Option<i32>,
    /// Its standard output followed by its standard error.
    pub text: String,
}

impl Output {
    /// The last line with text, trimmed, to quote in a reason.
    #[must_use]
    pub fn last_line(&self) -> Option<&str> {
        self.text
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
    }

    /// The fault of a run that did not exit with 0, quoting its last line; `None` when it did.
    #[must_use]
    pub fn fault(&self) -> Option<Fault> {
        let last = self.last_line().map(str::to_owned);
        match self.code {
            Some(0) => None,
            Some(code) => Some(Fault::Exit { code, last }),
            None => Some(Fault::Signal { last }),
        }
    }
}

/// How a run of a tool went wrong, in the terms that every tool shares. Results keep it, and the
/// reporter words it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Fault {
    /// The tool could not start, for the system's reason.
    Spawn(String),
    /// What it printed could not be read, for the system's reason.
    Read(String),
    /// It ran past this limit and was stopped.
    TimedOut(Duration),
    /// It exited with a code other than 0.
    Exit {
        /// The exit code.
        code: i32,
        /// The last line it printed, if any.
        last: Option<String>,
    },
    /// A signal ended it.
    Signal {
        /// The last line it printed, if any.
        last: Option<String>,
    },
}

/// Why a run left nothing to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// It went wrong.
    Fault(Fault),
    /// The plugin was cancelled, and the tool stopped or never started.
    Cancelled,
}

/// Whether a tool started as the kernel's first choice when memory runs out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Adjustment {
    /// It started with `oom_score_adj` 1000, and the program's own value was restored.
    Done,
    /// The value could not be set, for the system's reason, so the tool started with the
    /// program's own.
    NotSet(String),
    /// The tool started with 1000, but the program's own value could not be restored, for the
    /// system's reason, so the program kept 1000 as well.
    NotRestored(String),
}

/// The process group of a child started with `process_group(0)`. Killing the tool alone would
/// leave its workers running, so the whole group gets SIGKILL when this is dropped, unless it
/// was disarmed.
struct Group {
    leader: Option<Pid>,
}

impl Group {
    /// The group that `child` leads.
    fn of(child: &Child) -> Self {
        let leader = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw);
        Self { leader }
    }

    /// The group ended on its own: dropping it kills nothing.
    fn disarm(mut self) {
        self.leader = None;
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(leader) = self.leader {
            // A group that is gone already has nothing left to kill.
            let _ = rustix::process::kill_process_group(leader, Signal::KILL);
        }
    }
}

/// Starts `command` with `oom_score_adj` 1000, which the tool and the workers it starts inherit,
/// then restores the program's own value.
fn spawn_first_to_go(command: &mut Command) -> (io::Result<Child>, Adjustment) {
    let previous = match std::fs::read_to_string(OOM_SCORE_ADJ) {
        Ok(previous) => previous,
        Err(err) => return (command.spawn(), Adjustment::NotSet(err.to_string())),
    };
    if let Err(err) = std::fs::write(OOM_SCORE_ADJ, "1000") {
        return (command.spawn(), Adjustment::NotSet(err.to_string()));
    }
    let child = command.spawn();
    let adjustment = match std::fs::write(OOM_SCORE_ADJ, previous.trim()) {
        Ok(()) => Adjustment::Done,
        Err(err) => Adjustment::NotRestored(err.to_string()),
    };
    (child, adjustment)
}
