//! Geekbench's processes (design cpu, §6.4, §7.3): a process group of their own that is killed
//! as a whole, the kernel's first choice when memory runs out, and the kernel's count of such
//! kills.

use std::io;

use bench_host::Root;
use rustix::process::Pid;
use rustix::process::Signal;
use tokio::process::Child;
use tokio::process::Command;

/// The program's own OOM score adjustment.
const OOM_SCORE_ADJ: &str = "/proc/self/oom_score_adj";

/// The process group of a child started with `process_group(0)`. Killing the launcher alone
/// would leave Geekbench's worker running (design cpu, §3.2), so the whole group gets SIGKILL
/// when this is dropped, unless it ended on its own.
#[derive(Debug)]
pub struct Group {
    leader: Option<Pid>,
}

impl Group {
    /// The group that `child` leads.
    #[must_use]
    pub fn of(child: &Child) -> Self {
        let leader = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw);
        Self { leader }
    }

    /// Sends SIGKILL to every process in the group.
    pub fn kill(&self) {
        if let Some(leader) = self.leader {
            // A group that is gone already has nothing left to kill.
            let _ = rustix::process::kill_process_group(leader, Signal::KILL);
        }
    }

    /// The group ended on its own: dropping it kills nothing.
    pub fn disarm(mut self) {
        self.leader = None;
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Whether the program could make Geekbench the kernel's first choice.
#[derive(Debug)]
pub enum Adjustment {
    /// Geekbench started with `oom_score_adj` 1000, and the program's own value was restored.
    Done,
    /// The value could not be set, so Geekbench started with the program's own.
    NotSet(io::Error),
    /// Geekbench started with 1000, but the program kept 1000 as well.
    NotRestored(io::Error),
}

/// Starts `command` with `oom_score_adj` 1000, which Geekbench and the worker it starts
/// inherit, then restores the program's own value (design cpu, §7.3).
pub fn spawn_first_to_go(command: &mut Command) -> (io::Result<Child>, Adjustment) {
    let previous = match std::fs::read_to_string(OOM_SCORE_ADJ) {
        Ok(previous) => previous,
        Err(err) => return (command.spawn(), Adjustment::NotSet(err)),
    };
    if let Err(err) = std::fs::write(OOM_SCORE_ADJ, "1000") {
        return (command.spawn(), Adjustment::NotSet(err));
    }
    let child = command.spawn();
    let adjustment = match std::fs::write(OOM_SCORE_ADJ, previous.trim()) {
        Ok(()) => Adjustment::Done,
        Err(err) => Adjustment::NotRestored(err),
    };
    (child, adjustment)
}

/// The kernel's count of processes killed for lack of memory, from `/proc/vmstat`.
#[must_use]
pub fn oom_kills(root: &Root) -> Option<u64> {
    root.read("/proc/vmstat")?
        .lines()
        .find_map(|line| line.strip_prefix("oom_kill "))?
        .trim()
        .parse()
        .ok()
}
