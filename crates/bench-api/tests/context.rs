//! A plugin's context: what the plugin publishes and reports reaches the orchestrator's end,
//! the last publish wins, and a plugin never fails because nobody listens any more.

use std::collections::BTreeMap;
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
use serde::Serialize;
use tokio::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Output {
    runs: u32,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        vec![ToolVersion {
            name: "probe",
            version: "1.0".to_owned(),
        }]
    }
}

fn context() -> (Context<Output>, Channels<Output>, CancellationToken) {
    let tools = BTreeMap::from([("probe".to_owned(), PathBuf::from("/run/tools/probe"))]);
    let host = HostProfile {
        arch: "riscv64".to_owned(),
        ..HostProfile::default()
    };
    let cancel = CancellationToken::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let (ctx, channels) = Context::new(
        Arc::new(host),
        Arc::new(tools),
        deadline,
        PathBuf::from("/run/work"),
        cancel.clone(),
    );
    (ctx, channels, cancel)
}

#[test]
fn the_last_publish_wins_and_nothing_is_published_at_first() {
    let (ctx, channels, _) = context();
    assert_eq!(*channels.published.borrow(), None);
    ctx.publish(&Output { runs: 1 });
    ctx.publish(&Output { runs: 2 });
    assert_eq!(*channels.published.borrow(), Some(Output { runs: 2 }));
}

#[test]
fn progress_lines_and_tool_runs_arrive_in_order() {
    let (ctx, mut channels, _) = context();
    let run = ToolRun {
        command: vec!["probe".to_owned(), "--fast".to_owned()],
        end: ToolEnd::Exit(1),
        tail: vec!["no disk".to_owned()],
    };
    ctx.progress("单线程测试");
    ctx.ran(run.clone());
    ctx.progress(String::from("多线程测试"));
    let line = |text: &str| Ok(Progress::Line(text.to_owned()));
    assert_eq!(channels.progress.try_recv(), line("单线程测试"));
    assert_eq!(channels.progress.try_recv(), Ok(Progress::Tool(run)));
    assert_eq!(channels.progress.try_recv(), line("多线程测试"));
    assert!(
        channels.progress.try_recv().is_err(),
        "nothing else was sent"
    );
}

#[test]
fn a_plugin_keeps_running_when_nobody_listens() {
    let (ctx, channels, _) = context();
    drop(channels);
    ctx.progress("after the orchestrator stopped listening");
    ctx.ran(ToolRun {
        command: vec!["probe".to_owned()],
        end: ToolEnd::Cancelled,
        tail: Vec::new(),
    });
    ctx.publish(&Output { runs: 3 });
}

#[test]
fn the_context_hands_out_host_tools_directory_deadline_and_token() {
    let (ctx, _channels, cancel) = context();
    assert_eq!(ctx.host().arch, "riscv64");
    assert_eq!(
        ctx.tool("probe"),
        Some(PathBuf::from("/run/tools/probe").as_path())
    );
    assert_eq!(ctx.tool("undeclared"), None);
    assert_eq!(ctx.work_dir(), PathBuf::from("/run/work").as_path());
    assert!(ctx.deadline() > Instant::now());
    assert!(!ctx.cancel_token().is_cancelled());
    cancel.cancel();
    assert!(
        ctx.cancel_token().is_cancelled(),
        "the plugin sees the orchestrator's cancel"
    );
    assert_eq!(Output { runs: 0 }.tools()[0].version, "1.0");
}
