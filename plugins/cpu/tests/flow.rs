//! The item's flow with scripted attempts instead of sysbench (design cpu, §5.2–§5.4).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Env;
use bench_api::PluginError;
use bench_api::Record;
use plugin_cpu::sysbench::AttemptError;
use plugin_cpu::sysbench::Rate;
use tokio::time::Instant;

struct Outcome {
    result: Result<(), PluginError>,
    records: Vec<Record>,
    /// The threads of each attempt, in order.
    attempts: Vec<usize>,
}

impl Outcome {
    fn rows(&self) -> Vec<Vec<String>> {
        let Some(Record::Table(table)) = self.records.first() else {
            panic!("the first record is the table: {:?}", self.records);
        };
        assert_eq!(table.title(), "sysbench 1.0.20：素数上限 10000，每项 20 秒");
        assert_eq!(table.headers(), ["项目", "线程数", "每秒事件数"]);
        table.rows().to_vec()
    }

    /// The facts after the table, as (id, label, value).
    fn facts(&self) -> Vec<(String, String, String)> {
        let mut facts = Vec::new();
        for record in self.records.iter().skip(1) {
            let Record::Fact { id, label, value } = record else {
                panic!("only facts follow the table: {record:?}");
            };
            facts.push((id.clone(), label.clone(), value.clone()));
        }
        facts
    }

    fn failure(&self) -> &str {
        match &self.result {
            Err(PluginError::Fail(reason)) => reason,
            other => panic!("the item fails: {other:?}"),
        }
    }
}

/// Runs the item with `threads`; its attempts return `script` in order.
async fn measure(threads: usize, script: Vec<Result<Rate, AttemptError>>) -> Outcome {
    let work = tempfile::tempdir().expect("a temporary directory");
    let deadline = Instant::now() + Duration::from_secs(600);
    let (ctx, mut receiver) = Context::new(
        Arc::new(Env::default()),
        deadline,
        work.path().into(),
        CancellationToken::new(),
    );
    let script = RefCell::new(VecDeque::from(script));
    let attempts = RefCell::new(Vec::new());
    let result = plugin_cpu::measure(&ctx, threads, |threads| {
        attempts.borrow_mut().push(threads);
        let next = script
            .borrow_mut()
            .pop_front()
            .expect("every attempt is scripted");
        async move { next }
    })
    .await;
    assert!(script.borrow().is_empty(), "every scripted attempt ran");
    let mut records = Vec::new();
    while let Ok(record) = receiver.try_recv() {
        records.push(record);
    }
    Outcome {
        result,
        records,
        attempts: attempts.into_inner(),
    }
}

fn rate(text: &str) -> Result<Rate, AttemptError> {
    Ok(Rate {
        text: text.to_owned(),
        value: text.parse().expect("a number"),
    })
}

fn failed(reason: &str) -> Result<Rate, AttemptError> {
    Err(AttemptError::Failed(reason.to_owned()))
}

fn row(cells: [&str; 3]) -> Vec<String> {
    cells.map(str::to_owned).to_vec()
}

fn fact(id: &str, label: &str, value: &str) -> (String, String, String) {
    (id.to_owned(), label.to_owned(), value.to_owned())
}

#[tokio::test]
async fn both_runs_show_in_the_table_with_the_ratio() {
    let outcome = measure(4, vec![rate("12842.07"), rate("49847.67")]).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.rows(),
        [
            row(["单线程", "1", "12842.07"]),
            row(["多线程", "4", "49847.67"])
        ]
    );
    assert_eq!(
        outcome.facts(),
        [fact("ratio", "倍数", "多线程是单线程的 3.88 倍")]
    );
    assert_eq!(outcome.attempts, [1, 4]);
}

#[tokio::test]
async fn one_thread_runs_once_without_a_ratio() {
    let outcome = measure(1, vec![rate("1234.56")]).await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(outcome.rows(), [row(["单线程", "1", "1234.56"])]);
    assert!(outcome.facts().is_empty());
    assert_eq!(outcome.attempts, [1]);
}

#[tokio::test]
async fn retries_that_succeeded_get_a_note() {
    let outcome = measure(
        2,
        vec![
            failed("sysbench 退出码 1：x"),
            rate("100.00"),
            failed("超过 60 秒仍未结束，已停止"),
            failed("sysbench 没有输出每秒事件数"),
            rate("150.00"),
        ],
    )
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.facts(),
        [
            fact("ratio", "倍数", "多线程是单线程的 1.50 倍"),
            fact(
                "note-single",
                "说明",
                "单线程测试失败 1 次后成功，最后一次失败的原因：sysbench 退出码 1：x"
            ),
            fact(
                "note-multi",
                "说明",
                "多线程测试失败 2 次后成功，最后一次失败的原因：sysbench 没有输出每秒事件数"
            ),
        ]
    );
    assert_eq!(outcome.attempts, [1, 1, 2, 2, 2]);
}

#[tokio::test]
async fn a_single_thread_run_that_always_fails_fails_the_item_without_a_table() {
    let outcome = measure(4, vec![failed("sysbench 退出码 1：boom"); 4]).await;
    assert_eq!(outcome.failure(), "sysbench 退出码 1：boom（共试了 4 次）");
    assert!(outcome.records.is_empty(), "{:?}", outcome.records);
    assert_eq!(
        outcome.attempts,
        [1, 1, 1, 1],
        "the multi-thread run does not start"
    );
}

#[tokio::test]
async fn a_multi_thread_run_that_always_fails_keeps_the_single_thread_row() {
    let mut script = vec![rate("12842.07")];
    script.extend(vec![failed("超过 60 秒仍未结束，已停止"); 4]);
    let outcome = measure(4, script).await;
    assert_eq!(
        outcome.failure(),
        "多线程测试失败：超过 60 秒仍未结束，已停止（共试了 4 次）"
    );
    assert_eq!(outcome.rows(), [row(["单线程", "1", "12842.07"])]);
    assert!(outcome.facts().is_empty(), "no ratio without two runs");
}

#[tokio::test]
async fn a_cancellation_ends_the_item_without_retries() {
    let outcome = measure(4, vec![Err(AttemptError::Cancelled)]).await;
    assert_eq!(outcome.failure(), "已中断");
    assert_eq!(outcome.attempts, [1]);
}
