//! Running a tool once: what it printed and how it ended, its limit, cancellation, its directory
//! and environment, its report for the log, and on Linux its process group, its `oom_score_adj`
//! and its CPUs.

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Channels;
use bench_api::Context;
use bench_api::Measurement;
use bench_api::Progress;
use bench_api::ToolEnd;
use bench_api::ToolRun;
use bench_api::ToolVersion;
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

fn context(cancel: CancellationToken) -> (Context<Nothing>, Channels<Nothing>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    Context::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        deadline,
        ".".into(),
        cancel,
    )
}

/// The runs reported so far.
fn reported(channels: &mut Channels<Nothing>) -> Vec<ToolRun> {
    let mut runs = Vec::new();
    while let Ok(progress) = channels.progress.try_recv() {
        if let Progress::Tool(run) = progress {
            runs.push(run);
        }
    }
    runs
}

fn words(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

fn shell(script: &str, limit: Duration) -> Job {
    Job::new("/bin/sh", ["-c", script], limit)
}

/// Held by the tests that change `oom_score_adj`, which is the process's: under `cargo test`,
/// tests run as threads of one process, and would see one another's value.
#[cfg(target_os = "linux")]
static OOM_SCORE_ADJ: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn a_run_gives_its_output_then_its_errors_and_its_exit_code() {
    let (ctx, _channels) = context(CancellationToken::new());
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
    let (ctx, _channels) = context(CancellationToken::new());
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
    let (ctx, _channels) = context(CancellationToken::new());
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
    let (ctx, mut channels) = context(cancel.clone());
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
    assert_eq!(
        reported(&mut channels),
        [ToolRun {
            command: words(&["sleep", "30"]),
            end: ToolEnd::Cancelled,
            tail: Vec::new(),
        }]
    );
}

#[tokio::test]
async fn a_run_cancelled_before_it_starts_never_starts() {
    let cancel = CancellationToken::new();
    let (ctx, _channels) = context(cancel.clone());
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
    let (ctx, _channels) = context(CancellationToken::new());
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

#[tokio::test]
async fn each_run_is_reported_with_its_command_its_end_and_its_last_lines() {
    let (ctx, mut channels) = context(CancellationToken::new());
    let script = "echo one; echo two; echo; echo three >&2; echo four; exit 3";
    shell(script, Duration::from_secs(30)).run(&ctx).await;
    assert_eq!(
        reported(&mut channels),
        [ToolRun {
            command: words(&["sh", "-c", script]),
            end: ToolEnd::Exit(3),
            tail: words(&["two", "four", "three"]),
        }]
    );

    shell("echo fine", Duration::from_secs(30)).run(&ctx).await;
    let mut quiet = shell("echo the claim link; exit 1", Duration::from_secs(30));
    quiet.quotable = false;
    quiet.run(&ctx).await;
    let ends: Vec<(ToolEnd, Vec<String>)> = reported(&mut channels)
        .into_iter()
        .map(|run| (run.end, run.tail))
        .collect();
    assert_eq!(
        ends,
        [
            (ToolEnd::Exit(0), Vec::new()),
            (ToolEnd::Exit(1), Vec::new())
        ],
        "no lines after a success, nor from a tool whose output may not be quoted"
    );
}

#[tokio::test]
async fn runs_that_do_not_finish_are_reported_too() {
    let (ctx, mut channels) = context(CancellationToken::new());
    Job::new("/bin/sleep", ["30"], Duration::from_secs(1))
        .run(&ctx)
        .await;
    Job::new("/nonexistent/tool", ["--fast"], Duration::from_secs(5))
        .run(&ctx)
        .await;
    let runs = reported(&mut channels);
    assert_eq!(
        runs.first(),
        Some(&ToolRun {
            command: words(&["sleep", "30"]),
            end: ToolEnd::TimedOut(Duration::from_secs(1)),
            tail: Vec::new(),
        })
    );
    let Some(ToolRun {
        command,
        end: ToolEnd::Spawn(reason),
        ..
    }) = runs.get(1)
    else {
        panic!("the missing program is reported: {runs:?}");
    };
    assert_eq!(command, &words(&["tool", "--fast"]));
    assert!(!reason.is_empty(), "the system's reason");
    assert_eq!(runs.len(), 2);

    let cancel = CancellationToken::new();
    let (ctx, mut channels) = context(cancel.clone());
    cancel.cancel();
    shell("echo never", Duration::from_secs(30)).run(&ctx).await;
    assert!(
        reported(&mut channels).is_empty(),
        "a run that never started is not reported"
    );
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
    let (ctx, _channels) = context(CancellationToken::new());
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
async fn a_group_whose_leader_ends_on_its_own_is_left_alone() {
    let (ctx, _channels) = context(CancellationToken::new());
    let dir = tempfile::tempdir().expect("a temporary directory");
    // The worker writes nowhere, so the leader's end closes the output and the run finishes.
    let script = format!(
        "sleep 30 > /dev/null 2>&1 & echo $! > '{}/worker'",
        dir.path().display()
    );
    let mut job = shell(&script, Duration::from_secs(10));
    job.group = true;
    let ended = job.run(&ctx).await.ended;
    let pid = worker(dir.path());
    tokio::time::sleep(Duration::from_millis(500)).await;
    let alive = !matches!(state(&pid), None | Some('Z'));
    let number: i32 = pid.parse().expect("a pid");
    let process = rustix::process::Pid::from_raw(number).expect("a positive pid");
    let _ = rustix::process::kill_process(process, rustix::process::Signal::KILL);
    assert!(ended.is_ok(), "{ended:?}");
    assert!(
        alive,
        "the leader was reaped, so its group's id may be another's: nothing is killed"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn without_a_group_of_its_own_the_workers_outlive_the_tool() {
    let (ctx, _channels) = context(CancellationToken::new());
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

    let _alone = OOM_SCORE_ADJ.lock().await;
    let (ctx, _channels) = context(CancellationToken::new());
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

    let (ctx, _channels) = context(CancellationToken::new());
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

/// The last CPU of `Cpus_allowed_list` in a status file, and the whole list.
#[cfg(target_os = "linux")]
fn allowed(status: &str) -> (String, usize) {
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .expect("a CPU list")
        .trim()
        .to_owned();
    let last = list
        .rsplit([',', '-'])
        .next()
        .and_then(|cpu| cpu.parse().ok())
        .expect("a CPU number");
    (list, last)
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_placed_tool_runs_on_its_cpus_while_the_test_keeps_its_own() {
    let own = || std::fs::read_to_string("/proc/thread-self/status").expect("the thread's status");
    let (before, cpu) = allowed(&own());
    let (ctx, _channels) = context(CancellationToken::new());
    let mut job = Job::new("/bin/cat", ["/proc/self/status"], Duration::from_secs(30));
    job.cpus = Some(vec![cpu]);
    let Ok(output) = job.run(&ctx).await.ended else {
        panic!("cat runs");
    };
    assert_eq!(output.code, Some(0), "{}", output.text);
    assert_eq!(allowed(&output.text).0, cpu.to_string(), "the child's CPUs");
    assert_eq!(
        allowed(&own()).0,
        before,
        "the test's thread keeps its CPUs"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_that_goes_first_can_be_placed_too() {
    use bench_tool::job::Adjustment;

    let _alone = OOM_SCORE_ADJ.lock().await;
    let own = || std::fs::read_to_string("/proc/thread-self/status").expect("the thread's status");
    let (_, cpu) = allowed(&own());
    let (ctx, _channels) = context(CancellationToken::new());
    let mut job = shell(
        "grep Cpus_allowed_list /proc/$$/status; cat /proc/$$/oom_score_adj",
        Duration::from_secs(30),
    );
    job.first_to_go = true;
    job.cpus = Some(vec![cpu]);
    let outcome = job.run(&ctx).await;
    assert_eq!(outcome.adjustment, Some(Adjustment::Done));
    let Ok(output) = outcome.ended else {
        panic!("the shell runs");
    };
    assert_eq!(allowed(&output.text).0, cpu.to_string(), "{}", output.text);
    assert!(output.text.ends_with("\n1000\n"), "{}", output.text);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_cpu_beyond_an_affinity_mask_cannot_be_used() {
    let (ctx, _channels) = context(CancellationToken::new());
    let mut job = Job::new("/bin/true", Vec::<String>::new(), Duration::from_secs(30));
    job.cpus = Some(vec![0, 1 << 20]);
    let ended = job.run(&ctx).await.ended;
    assert_eq!(
        ended,
        Err(RunError::Fault(Fault::Spawn(
            "CPU 1048576 is beyond an affinity mask".to_owned()
        )))
    );
}

#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn elsewhere_the_scheduler_places_a_tool() {
    let (ctx, _channels) = context(CancellationToken::new());
    let mut job = shell("echo ran", Duration::from_secs(30));
    job.cpus = Some(vec![1 << 20]);
    assert_eq!(
        job.run(&ctx).await.ended,
        Ok(Output {
            code: Some(0),
            text: "ran\n".to_owned(),
        })
    );
}
