//! Running a tool once: what it printed and how it ended, its limit, cancellation, its directory
//! and environment, its report for the log, and on Linux its process group, its `oom_score_adj`,
//! its CPUs and its user.

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
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
use bench_tool::job::STREAM_LIMIT;
use bench_tool::job::User;
use serde::Serialize;
#[cfg(target_os = "linux")]
use tempfile::TempDir;
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
async fn runaway_stdout_and_stderr_are_capped_at_stream_limit_without_blocking_child() {
    let (ctx, _channels) = context(CancellationToken::new());
    // Write 5 MiB to stdout and 5 MiB to stderr (> 4 MiB STREAM_LIMIT each), then exit 0.
    let script = "dd if=/dev/zero bs=1048576 count=5 2>/dev/null; \
                  dd if=/dev/zero bs=1048576 count=5 >&2 2>/dev/null";
    let outcome = shell(script, Duration::from_secs(30)).run(&ctx).await;
    let Ok(output) = outcome.ended else {
        panic!(
            "oversized output run should still complete cleanly: {:?}",
            outcome.ended
        );
    };
    assert_eq!(output.code, Some(0));
    assert_eq!(output.text.len(), STREAM_LIMIT * 2);
}

#[tokio::test]
async fn exited_child_with_inherited_background_pipe_returns_after_drain_grace() {
    let (ctx, _channels) = context(CancellationToken::new());
    let dir = tempfile::tempdir().expect("temp dir");
    let pid_file = dir.path().join("bg.pid");
    // Background sleep inherits stdout/stderr without closing them while the direct shell exits 0.
    let script = format!(
        "echo ready; sleep 30 & echo $! > '{}'; exit 0",
        pid_file.display()
    );
    let started = std::time::Instant::now();
    let outcome = shell(&script, Duration::from_secs(15)).run(&ctx).await;
    let elapsed = started.elapsed();

    if let Ok(raw_pid) = std::fs::read_to_string(&pid_file)
        && let Ok(num) = raw_pid.trim().parse::<i32>()
        && let Some(pid) = rustix::process::Pid::from_raw(num)
    {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    }

    assert_eq!(
        outcome.ended,
        Ok(Output {
            code: Some(0),
            text: "ready\n".to_owned(),
        })
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "must not block until the 15s limit when a grandchild keeps pipes open (took {elapsed:?})"
    );
}

#[tokio::test]
async fn interrupt_after_sends_sigint_before_limit() {
    let (ctx, _channels) = context(CancellationToken::new());
    let script = "trap 'kill $bg 2>/dev/null; echo flushed; exit 0' INT; \
                  sleep 30 & bg=$!; wait $bg";
    let mut job = shell(script, Duration::from_secs(15));
    job.group = true;
    job.interrupt_after = Some(Duration::from_millis(150));
    let started = std::time::Instant::now();
    let outcome = job.run(&ctx).await;
    assert_eq!(
        outcome.ended,
        Ok(Output {
            code: Some(0),
            text: "flushed\n".to_owned(),
        })
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "interrupt_after stopped the run cleanly well before its hard limit"
    );
}

#[tokio::test]
async fn interrupt_after_never_signals_after_child_is_reaped() {
    if let Some(pid_path) = std::env::var_os("BENCH_JOB_LATE_INT_SUBTEST") {
        let child = std::process::Command::new("/bin/sleep")
            .arg("10")
            .spawn()
            .expect("spawn background sleep");
        std::fs::write(pid_path, child.id().to_string()).expect("write sleep pid");
        println!("reaped-soon");
        std::process::exit(0);
    }

    let (ctx, _channels) = context(CancellationToken::new());
    let dir = tempfile::tempdir().expect("temp dir");
    let pid_file = dir.path().join("sleep.pid");
    let exe = std::env::current_exe().expect("test binary path");
    let mut job = Job::new(
        exe,
        [
            "--exact",
            "interrupt_after_never_signals_after_child_is_reaped",
            "--nocapture",
        ],
        Duration::from_secs(10),
    );
    job.envs = vec![("BENCH_JOB_LATE_INT_SUBTEST".into(), pid_file.clone().into())];
    job.group = true;
    // Direct child exits immediately while `/bin/sleep 10` holds inherited stdout open across
    // PIPE_DRAIN_GRACE (200ms); 80ms falls inside that post-reap drain window.
    job.interrupt_after = Some(Duration::from_millis(80));
    let outcome = job.run(&ctx).await;
    let Ok(output) = outcome.ended else {
        panic!("helper child should exit cleanly: {:?}", outcome.ended);
    };
    assert_eq!(output.code, Some(0));
    assert!(
        output.text.contains("reaped-soon"),
        "unexpected output: {}",
        output.text
    );

    let raw_pid = std::fs::read_to_string(&pid_file).expect("sleep pid recorded");
    let pid_num: i32 = raw_pid.trim().parse().expect("numeric pid");
    let pid = rustix::process::Pid::from_raw(pid_num).expect("valid pid");
    #[cfg(target_os = "linux")]
    let alive_proc = !matches!(state(raw_pid.trim()), None | Some('Z'));
    #[cfg(not(target_os = "linux"))]
    let alive_proc = true;
    let kill_res = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    assert!(
        alive_proc && kill_res.is_ok(),
        "grandchild in group must not receive late SIGINT after leader is reaped: {kill_res:?}"
    );
}

#[tokio::test]
async fn inherited_proxy_variables_are_removed_from_tool_environment() {
    let vars = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];
    if std::env::var_os("BENCH_JOB_PROXY_SUBTEST").is_none() {
        let exe = std::env::current_exe().expect("test binary path");
        let mut cmd = std::process::Command::new(exe);
        cmd.args([
            "--exact",
            "inherited_proxy_variables_are_removed_from_tool_environment",
            "--nocapture",
        ])
        .env("BENCH_JOB_PROXY_SUBTEST", "1");
        for var in vars {
            cmd.env(var, "http://127.0.0.1:9");
        }
        let status = cmd.status().expect("child test process runs");
        assert!(status.success(), "child test exited with {status}");
        return;
    }
    for var in vars {
        assert_eq!(
            std::env::var(var).as_deref(),
            Ok("http://127.0.0.1:9"),
            "parent environment holds {var}"
        );
    }
    let (ctx, _channels) = context(CancellationToken::new());
    let script = "printf '%s|%s|%s|%s|%s|%s\n' \"$HTTP_PROXY\" \"$http_proxy\" \"$HTTPS_PROXY\" \
         \"$https_proxy\" \"$ALL_PROXY\" \"$all_proxy\"";
    let outcome = shell(script, Duration::from_secs(30)).run(&ctx).await;
    assert_eq!(
        outcome.ended,
        Ok(Output {
            code: Some(0),
            text: "|||||\n".to_owned(),
        })
    );
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

/// Only root may run a tool as another user: without root, the tool cannot start, for the
/// system's reason, and the log names the program by its file name.
#[tokio::test]
async fn without_root_a_tool_cannot_run_as_another_user() {
    if rustix::process::geteuid().is_root() {
        // Root may: the next test.
        return;
    }
    let (ctx, mut channels) = context(CancellationToken::new());
    let mut job = shell("echo ran", Duration::from_secs(30));
    job.user = Some(User::NOBODY);
    let ended = job.run(&ctx).await.ended;
    let Err(RunError::Fault(Fault::Spawn(reason))) = &ended else {
        panic!("the tool cannot start: {ended:?}");
    };
    assert!(reason.contains("not permitted"), "{reason}");
    assert_eq!(
        reported(&mut channels)[0].command,
        words(&["sh", "-c", "echo ran"])
    );
}

/// As root, a tool runs as its user and group, without root's other groups, under its own name,
/// also where a directory above it is closed to the user, as /root is above a run directory.
/// Root only, as in the Linux check: the tests run as an ordinary user elsewhere.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_runs_as_its_user_under_its_name_below_a_directory_closed_to_that_user() {
    if !rustix::process::geteuid().is_root() {
        return;
    }
    let (ctx, mut channels) = context(CancellationToken::new());
    let (_closed, tool) = tool_below_a_closed_directory(0o711);
    // The shell's process name and arguments, then its user, group and other groups.
    let script = "cat /proc/$$/comm; tr '\\0' ' ' < /proc/$$/cmdline; echo; \
                  grep -E '^(Uid|Gid|Groups):' /proc/self/status";
    let mut job = Job::new(&tool, ["-c", script], Duration::from_secs(30));
    job.user = Some(User::NOBODY);
    job.dir = Some("/".into());
    let ended = job.run(&ctx).await.ended;
    let Ok(output) = &ended else {
        panic!("the tool runs: {ended:?}");
    };
    let fields: Vec<Vec<&str>> = output
        .text
        .lines()
        .map(|line| line.split_whitespace().collect())
        .collect();
    assert_eq!(
        fields.get(2..),
        Some(
            [
                vec!["Uid:", "65534", "65534", "65534", "65534"],
                vec!["Gid:", "65534", "65534", "65534", "65534"],
                vec!["Groups:"],
            ]
            .as_slice()
        ),
        "{}",
        output.text
    );
    assert_eq!(
        fields[0],
        ["sh"],
        "the process takes the tool's name, not its descriptor's number: {}",
        output.text
    );
    assert_eq!(
        fields[1].first(),
        Some(&"sh"),
        "the tool starts with its own name as its first argument: {}",
        output.text
    );
    assert_eq!(
        reported(&mut channels)[0].command,
        words(&["sh", "-c", script])
    );
}

/// As root, a tool whose own directory the user may not search does not start, with the
/// system's reason.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_tool_in_a_directory_closed_to_its_user_does_not_start() {
    if !rustix::process::geteuid().is_root() {
        return;
    }
    let (ctx, _channels) = context(CancellationToken::new());
    let (_closed, tool) = tool_below_a_closed_directory(0o700);
    let mut job = Job::new(&tool, ["-c", "true"], Duration::from_secs(30));
    job.user = Some(User::NOBODY);
    job.dir = Some("/".into());
    let ended = job.run(&ctx).await.ended;
    let Err(RunError::Fault(Fault::Spawn(reason))) = &ended else {
        panic!("the tool does not start: {ended:?}");
    };
    assert!(reason.contains("Permission denied"), "{reason}");
}

/// A copy of the shell as `<closed>/tools/sh`, where `<closed>` is closed to others and `tools`
/// has `mode`. The copy keeps the shell's own name: busybox picks its program by that name.
#[cfg(target_os = "linux")]
fn tool_below_a_closed_directory(mode: u32) -> (TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let closed = tempfile::tempdir().expect("a temporary directory");
    std::fs::set_permissions(closed.path(), std::fs::Permissions::from_mode(0o700))
        .expect("the directory is closed to others");
    let tools = closed.path().join("tools");
    std::fs::create_dir(&tools).expect("a directory for the tool");
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(mode))
        .expect("the tools directory's mode");
    let tool = tools.join("sh");
    std::fs::copy("/bin/sh", &tool).expect("a copy of the shell");
    (closed, tool)
}
