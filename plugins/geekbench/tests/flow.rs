//! The item's runs and records with scripted runs instead of Geekbench, and its notes.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Env;
use bench_api::PluginError;
use bench_api::Record;
use plugin_geekbench::AttemptError;
use plugin_geekbench::memory::GIB;
use plugin_geekbench::output::Scores;
use plugin_geekbench::process::Adjustment;
use plugin_geekbench::report;
use tokio::time::Instant;

struct Outcome {
    result: Result<(), PluginError>,
    records: Vec<Record>,
    runs: usize,
}

impl Outcome {
    /// The facts, as (id, label, value).
    fn facts(&self) -> Vec<(String, String, String)> {
        self.records.iter().map(fact_of).collect()
    }

    fn failure(&self) -> &str {
        match &self.result {
            Err(PluginError::Fail(reason)) => reason,
            other => panic!("the item fails: {other:?}"),
        }
    }
}

fn fact_of(record: &Record) -> (String, String, String) {
    let Record::Fact { id, label, value } = record else {
        panic!("only facts: {record:?}");
    };
    (id.clone(), label.clone(), value.clone())
}

fn fact(id: &str, label: &str, value: &str) -> (String, String, String) {
    (id.to_owned(), label.to_owned(), value.to_owned())
}

/// Runs the item's runs against `script`; `cancel` is cancelled after the first run.
async fn measure(script: Vec<Result<Scores, AttemptError>>, cancel_after_first: bool) -> Outcome {
    let work = tempfile::tempdir().expect("a temporary directory");
    let cancel = CancellationToken::new();
    let (ctx, mut receiver) = Context::new(
        Arc::new(Env::default()),
        Instant::now() + Duration::from_secs(4 * 3600),
        work.path().into(),
        cancel.clone(),
    );
    let script = RefCell::new(VecDeque::from(script));
    let runs = Cell::new(0);
    let result = plugin_geekbench::measure(&ctx, || {
        runs.set(runs.get() + 1);
        if cancel_after_first {
            cancel.cancel();
        }
        let next = script
            .borrow_mut()
            .pop_front()
            .expect("every run is scripted");
        async move { next }
    })
    .await;
    assert!(script.borrow().is_empty(), "every scripted run happened");
    let mut records = Vec::new();
    while let Ok(record) = receiver.try_recv() {
        records.push(record);
    }
    Outcome {
        result,
        records,
        runs: runs.get(),
    }
}

fn scores() -> Result<Scores, AttemptError> {
    Ok(Scores {
        single: 1771,
        multi: 4347,
        url: Some("https://browser.geekbench.com/v7/cpu/840201".to_owned()),
    })
}

fn upload_failed() -> Result<Scores, AttemptError> {
    Err(AttemptError::UploadFailed(
        "unable to resolve host browser.geekbench.com".to_owned(),
    ))
}

#[tokio::test(start_paused = true)]
async fn the_scores_and_the_link_are_shown() {
    let outcome = measure(vec![scores()], false).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.facts(),
        [
            fact("single", "单核", "1771"),
            fact("multi", "多核", "4347"),
            fact(
                "url",
                "结果链接",
                "https://browser.geekbench.com/v7/cpu/840201"
            ),
        ]
    );
    assert_eq!(outcome.runs, 1);
}

#[tokio::test(start_paused = true)]
async fn failed_uploads_are_run_again_30_seconds_later() {
    let started = Instant::now();
    let outcome = measure(vec![upload_failed(), upload_failed(), scores()], false).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(outcome.runs, 3);
    assert_eq!(
        started.elapsed(),
        Duration::from_secs(60),
        "two waits of 30 seconds"
    );
    assert_eq!(
        outcome.facts().last(),
        Some(&fact(
            "note-retries",
            "说明",
            "上传失败 2 次后重跑成功，失败原因：unable to resolve host browser.geekbench.com"
        ))
    );
}

#[tokio::test(start_paused = true)]
async fn four_failed_uploads_fail_the_item() {
    let outcome = measure(vec![upload_failed(); 4], false).await;
    assert_eq!(
        outcome.failure(),
        "上传失败（unable to resolve host browser.geekbench.com），免费版只有上传成功才给出分数"
    );
    assert_eq!(outcome.runs, 4, "the first run and 3 more");
    assert!(outcome.records.is_empty());
}

#[tokio::test(start_paused = true)]
async fn other_failures_are_not_run_again() {
    let cases = [
        (
            AttemptError::Failed("Geekbench 退出码 1：x".to_owned()),
            "Geekbench 退出码 1：x",
        ),
        (AttemptError::OutOfMemory, "运行中因内存不足被系统终止"),
        (AttemptError::TimedOut, "超过 60 分钟仍未结束，已停止"),
        (AttemptError::Cancelled, "已中断"),
    ];
    for (error, reason) in cases {
        let outcome = measure(vec![Err(error)], false).await;
        assert_eq!(outcome.failure(), reason);
        assert_eq!(outcome.runs, 1, "{reason}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_cancellation_during_the_wait_ends_the_item() {
    let outcome = measure(vec![upload_failed()], true).await;
    assert_eq!(outcome.failure(), "已中断");
    assert_eq!(outcome.runs, 1);
}

#[test]
fn a_result_without_a_link_says_so() {
    let records = report::scores(&Scores {
        single: 1,
        multi: 2,
        url: None,
    });
    assert_eq!(fact_of(&records[2]), fact("url", "结果链接", "不可用"));
}

#[test]
fn memory_notes_give_the_estimate_and_what_is_available() {
    let available = 1730 << 20;
    assert_eq!(
        fact_of(&report::swap_added(5 * GIB, available, 4)),
        fact(
            "note-memory",
            "说明",
            "可用内存不足（估计需要 5 GiB，可用 1.6 GiB），临时加了 4 GiB swap，分数可能偏低"
        )
    );
    let reason = report::swap_error(&io::Error::from(io::ErrorKind::InvalidInput));
    assert_eq!(
        fact_of(&report::swap_impossible(5 * GIB, available, &reason)),
        fact(
            "note-memory",
            "说明",
            "可用内存可能不足（估计需要 5 GiB，可用 1.6 GiB），\
             无法临时加 swap：运行目录所在的文件系统不支持 swap 文件"
        )
    );
    assert_eq!(
        fact_of(&report::swap_impossible(
            2 * GIB,
            600 << 20,
            report::SWAP_UNUSED
        )),
        fact(
            "note-memory",
            "说明",
            "可用内存可能不足（估计需要 2 GiB，可用 0.5 GiB），\
             无法临时加 swap：本机 vm.swappiness 为 0，内核不会把内存换出到 swap"
        ),
        "the owner's VM"
    );
    assert_eq!(
        report::swap_error(&io::Error::from(io::ErrorKind::PermissionDenied)),
        "没有权限（容器里通常不允许）"
    );
    assert_eq!(
        report::no_room(Some(GIB + GIB / 2)),
        "运行目录所在的磁盘只剩 1.5 GiB"
    );
    assert_eq!(report::no_room(None), "无法读取运行目录所在磁盘的剩余空间");
}

#[test]
fn a_swap_left_on_and_the_oom_adjustment_get_notes() {
    let err = io::Error::from_raw_os_error(12);
    let note = fact_of(&report::swap_left_on(Path::new("/root/run/swapfile"), &err));
    assert_eq!((note.0.as_str(), note.1.as_str()), ("note-swap", "说明"));
    assert!(note.2.starts_with("无法关闭临时 swap："), "{}", note.2);
    assert!(
        note.2
            .ends_with("；请手动执行 swapoff /root/run/swapfile，再删除这个文件"),
        "{}",
        note.2
    );

    assert!(report::oom(&Adjustment::Done).is_none());
    let denied = || io::Error::from(io::ErrorKind::PermissionDenied);
    let set = report::oom(&Adjustment::NotSet(denied())).expect("a note");
    assert!(
        fact_of(&set)
            .2
            .starts_with("无法让 Geekbench 在内存耗尽时优先被结束："),
        "{set:?}"
    );
    let kept = report::oom(&Adjustment::NotRestored(denied())).expect("a note");
    assert!(
        fact_of(&kept)
            .2
            .starts_with("无法恢复本程序的 oom_score_adj："),
        "{kept:?}"
    );
}
