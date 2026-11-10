//! Running a tool once (design memory, §5.8, §7): what it printed, its time limit,
//! cancellation, and on Linux the CPUs it runs on.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Env;
use plugin_memory::output::AttemptError;
use plugin_memory::output::Output;
use plugin_memory::process;
use plugin_memory::process::Job;
use tokio::time::Instant;

fn context(cancel: CancellationToken) -> Context {
    let deadline = Instant::now() + Duration::from_secs(60);
    let (ctx, _records) = Context::new(Arc::new(Env::default()), deadline, ".".into(), cancel);
    ctx
}

fn job(program: &str, args: &[&str], limit: Duration) -> Job {
    Job {
        tool: "测试工具",
        program: PathBuf::from(program),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        envs: Vec::new(),
        cpus: None,
        limit,
    }
}

#[tokio::test]
async fn a_run_gives_its_output_then_its_errors_and_its_exit_code() {
    let ctx = context(CancellationToken::new());
    let script = "echo \"$LC_ALL $EXTRA\"; echo oops >&2; echo done; exit 3";
    let mut job = job("/bin/sh", &["-c", script], Duration::from_secs(30));
    job.envs = vec![("EXTRA", "given".to_owned())];
    assert_eq!(
        process::run(&ctx, job).await,
        Ok(Output {
            code: Some(3),
            text: "C given\ndone\noops\n".to_owned(),
        })
    );
}

#[tokio::test]
async fn a_run_past_its_limit_is_stopped() {
    let ctx = context(CancellationToken::new());
    let started = std::time::Instant::now();
    let output = process::run(&ctx, job("/bin/sleep", &["30"], Duration::from_secs(1))).await;
    assert_eq!(
        output,
        Err(AttemptError::Failed("超过 1 秒仍未结束，已停止".to_owned()))
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
    cancel.cancel();
    let output = process::run(&ctx, job("/bin/sleep", &["30"], Duration::from_secs(60))).await;
    assert_eq!(output, Err(AttemptError::Cancelled));
}

#[tokio::test]
async fn a_missing_program_cannot_run() {
    let ctx = context(CancellationToken::new());
    let output = process::run(
        &ctx,
        job("/nonexistent/stream-2m", &[], Duration::from_secs(5)),
    )
    .await;
    let Err(AttemptError::Failed(reason)) = output else {
        panic!("the run fails: {output:?}");
    };
    assert!(reason.starts_with("无法运行 测试工具："), "{reason}");
}

/// The last CPU of `Cpus_allowed_list` in a status file.
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
    let ctx = context(CancellationToken::new());
    let mut job = job("/bin/cat", &["/proc/self/status"], Duration::from_secs(30));
    job.cpus = Some(vec![cpu]);
    let output = process::run(&ctx, job).await.expect("cat runs");
    assert_eq!(output.code, Some(0), "{}", output.text);
    assert_eq!(allowed(&output.text).0, cpu.to_string(), "the child's CPUs");
    assert_eq!(
        allowed(&own()).0,
        before,
        "the test's thread keeps its CPUs"
    );
}
