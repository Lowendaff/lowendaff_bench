//! Running a tool once: what it printed and how it ended, its limit, cancellation, its directory
//! and environment, and on Linux its process group and its `oom_score_adj`.

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::typed::Context;
use bench_api::typed::Measurement;
use bench_api::typed::ToolVersion;
use bench_host::profile::HostProfile;
use bench_tool::job::Fault;
use bench_tool::job::Job;
use bench_tool::job::Output;
use bench_tool::job::RunError;
use serde::Serialize;
use tokio::time::Instant;

#[derive(Debug, Clone, Serialize)]
struct Nothing;

impl Measurement for Nothing {
    fn tools(&self) -> Vec<ToolVersion> {
        Vec::new()
    }
}

fn context(cancel: CancellationToken) -> Context<Nothing> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let (ctx, _channels) = Context::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        deadline,
        ".".into(),
        cancel,
    );
    ctx
}

fn shell(script: &str, limit: Duration) -> Job {
    Job::new("/bin/sh", ["-c", script], limit)
}

#[tokio::test]
async fn a_run_gives_its_output_then_its_errors_and_its_exit_code() {
    let ctx = context(CancellationToken::new());
    let mut job = shell(
        "echo \"$LC_ALL $EXTRA\"; echo oops >&2; echo done; exit 3",
        Duration::from_secs(30),
    );
    job.envs = vec![("EXTRA".into(), "given".into())];
    let outcome = job.run(&ctx).await;
    assert_eq!(
        outcome.ended,
        Ok(Output {
            code: Some(3),
            text: "C given\ndone\noops\n".to_owned(),
        })
    );
    assert_eq!(outcome.adjustment, None, "the job did not ask to go first");
}

#[tokio::test]
async fn the_tool_runs_in_its_directory() {
    let ctx = context(CancellationToken::new());
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut job = shell("pwd -P", Duration::from_secs(30));
    job.dir = Some(dir.path().to_path_buf());
    let real = std::fs::canonicalize(dir.path()).expect("the directory's real path");
    let Ok(output) = job.run(&ctx).await.ended else {
        panic!("pwd runs");
    };
    assert_eq!(output.text, format!("{}\n", real.display()));
}

#[tokio::test]
async fn a_run_past_its_limit_is_stopped() {
    let ctx = context(CancellationToken::new());
    let started = std::time::Instant::now();
    let job = Job::new("/bin/sleep", ["30"], Duration::from_secs(1));
    assert_eq!(
        job.run(&ctx).await.ended,
        Err(RunError::Fault(Fault::TimedOut(Duration::from_secs(1))))
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it did not wait for sleep"
    );
}

#[tokio::test]
async fn a_cancelled_run_stops_at_once() {
    let cancel = CancellationToken::new();
    let ctx = context(cancel.clone());
    let started = std::time::Instant::now();
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel.cancel();
    });
    let job = Job::new("/bin/sleep", ["30"], Duration::from_secs(60));
    assert_eq!(job.run(&ctx).await.ended, Err(RunError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it did not wait for sleep"
    );
    canceller.await.expect("the canceller ends");
}

#[tokio::test]
async fn a_run_cancelled_before_it_starts_never_starts() {
    let cancel = CancellationToken::new();
    let ctx = context(cancel.clone());
    cancel.cancel();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let marker = dir.path().join("started");
    let job = shell(
        &format!("touch '{}'", marker.display()),
        Duration::from_secs(30),
    );
    assert_eq!(job.run(&ctx).await.ended, Err(RunError::Cancelled));
    assert!(!marker.exists(), "the tool never started");
}

#[tokio::test]
async fn a_missing_program_cannot_start() {
    let ctx = context(CancellationToken::new());
    let job = Job::new("/nonexistent/tool", [""; 0], Duration::from_secs(5));
    let ended = job.run(&ctx).await.ended;
    let Err(RunError::Fault(Fault::Spawn(reason))) = ended else {
        panic!("the program cannot start: {ended:?}");
    };
    assert!(!reason.is_empty(), "the system's reason is kept");
}

#[test]
fn a_fault_quotes_the_last_line_with_text() {
    let output = |code, text: &str| Output {
        code,
        text: text.to_owned(),
    };
    assert_eq!(output(Some(0), "fine\n").fault(), None);
    assert_eq!(
        output(Some(1), "first\n  FATAL: bad option  \n\n \n").fault(),
        Some(Fault::Exit {
            code: 1,
            last: Some("FATAL: bad option".to_owned()),
        })
    );
    assert_eq!(
        output(None, "").fault(),
        Some(Fault::Signal { last: None }),
        "a signal, and nothing printed"
    );
    assert_eq!(output(Some(0), "a\nb\n").last_line(), Some("b"));
}

/// The state letter of a process from `/proc/<pid>/stat`, or `None` when it is gone.
#[cfg(target_os = "linux")]
fn state(pid: &str) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name, in parentheses, may hold spaces; the state follows it.
    let (_, rest) = stat.rsplit_once(") ")?;
    rest.chars().next()
}

/// Waits up to five seconds for the process to be gone or a zombie.
#[cfg(target_os = "linux")]
async fn ended(pid: &str) -> bool {
    for _ in 0..50 {
        if matches!(state(pid), None | Some('Z')) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// A script that starts a worker, writes its pid into `dir`, and waits for it.
#[cfg(target_os = "linux")]
fn with_worker(dir: &Path, group: bool) -> Job {
    let mut job = shell(
        &format!("sleep 30 & echo $! > '{}/worker'; wait", dir.display()),
        Duration::from_secs(1),
    );
    job.group = group;
    job
}

#[cfg(target_os = "linux")]
fn worker(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("worker"))
        .expect("the script wrote its worker's pid")
        .trim()
        .to_owned()
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_in_a_group_of_its_own_is_stopped_with_its_workers() {
    let ctx = context(CancellationToken::new());
    let dir = tempfile::tempdir().expect("a temporary directory");
    let ended_by = with_worker(dir.path(), true).run(&ctx).await.ended;
    assert_eq!(
        ended_by,
        Err(RunError::Fault(Fault::TimedOut(Duration::from_secs(1))))
    );
    let pid = worker(dir.path());
    assert!(
        ended(&pid).await,
        "the worker {pid} was killed with its group"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn without_a_group_of_its_own_the_workers_outlive_the_tool() {
    let ctx = context(CancellationToken::new());
    let dir = tempfile::tempdir().expect("a temporary directory");
    with_worker(dir.path(), false).run(&ctx).await;
    let pid = worker(dir.path());
    tokio::time::sleep(Duration::from_millis(500)).await;
    let alive = !matches!(state(&pid), None | Some('Z'));
    let number: i32 = pid.parse().expect("a pid");
    let process = rustix::process::Pid::from_raw(number).expect("a positive pid");
    let _ = rustix::process::kill_process(process, rustix::process::Signal::KILL);
    assert!(
        alive,
        "only the shell was stopped, so its worker {pid} lived on"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_that_goes_first_starts_with_oom_score_adj_1000() {
    use bench_tool::job::Adjustment;

    let ctx = context(CancellationToken::new());
    let own = std::fs::read_to_string("/proc/self/oom_score_adj").expect("our own value");
    let mut job = shell(
        "read value < /proc/$$/oom_score_adj; echo $value",
        Duration::from_secs(30),
    );
    job.first_to_go = true;
    let outcome = job.run(&ctx).await;
    assert_eq!(outcome.adjustment, Some(Adjustment::Done));
    let Ok(output) = outcome.ended else {
        panic!("the shell runs");
    };
    assert_eq!(output.text, "1000\n");
    let restored = std::fs::read_to_string("/proc/self/oom_score_adj").expect("our own value");
    assert_eq!(restored, own, "the program's own value is back");
}

#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn without_proc_a_tool_that_goes_first_starts_all_the_same() {
    use bench_tool::job::Adjustment;

    let ctx = context(CancellationToken::new());
    let mut job = shell("echo ran", Duration::from_secs(30));
    job.first_to_go = true;
    let outcome = job.run(&ctx).await;
    assert!(
        matches!(outcome.adjustment, Some(Adjustment::NotSet(_))),
        "{:?}",
        outcome.adjustment
    );
    assert_eq!(
        outcome.ended,
        Ok(Output {
            code: Some(0),
            text: "ran\n".to_owned(),
        })
    );
}
