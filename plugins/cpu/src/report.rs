//! What CPU 性能 shows (design cpu, §5.3, §5.4): the runs in a table, the ratio between them,
//! and notes on retries.

use bench_api::Record;
use bench_api::RecordError;
use bench_api::Table;

use crate::sysbench::MAX_PRIME;
use crate::sysbench::Rate;
use crate::sysbench::SECONDS;

/// The table of the single-thread run and, with more threads, the multi-thread run. Its title
/// names the tool and the parameters, as the review templates ask.
pub fn table(
    version: &str,
    single: &Rate,
    multi: Option<(usize, &Rate)>,
) -> Result<Record, RecordError> {
    let mut rows = vec![vec![
        "单线程".to_owned(),
        "1".to_owned(),
        single.text.clone(),
    ]];
    if let Some((threads, rate)) = multi {
        rows.push(vec![
            "多线程".to_owned(),
            threads.to_string(),
            rate.text.clone(),
        ]);
    }
    let table = Table::try_new(
        "results",
        &format!("sysbench {version}：素数上限 {MAX_PRIME}，每项 {SECONDS} 秒"),
        vec!["项目".into(), "线程数".into(), "每秒事件数".into()],
        rows,
    )?;
    Ok(Record::Table(table))
}

/// How many times the multi-thread rate is the single-thread one, with two decimals.
#[must_use]
pub fn ratio(single: &Rate, multi: &Rate) -> Record {
    Record::fact(
        "ratio",
        "倍数",
        format!("多线程是单线程的 {:.2} 倍", multi.value / single.value),
    )
}

/// A note on a run that succeeded after failed attempts, or `None` when the first attempt
/// succeeded. `run` names the run, such as `多线程测试`.
#[must_use]
pub fn retries(id: &str, run: &str, failures: &[String]) -> Option<Record> {
    let last = failures.last()?;
    Some(Record::fact(
        id,
        "说明",
        format!(
            "{run}失败 {} 次后成功，最后一次失败的原因：{last}",
            failures.len()
        ),
    ))
}

/// The reason of a run whose every attempt failed: the last reason, and the number of
/// attempts.
#[must_use]
pub fn failure(failures: &[String]) -> String {
    let last = failures.last().map_or("", String::as_str);
    format!("{last}（共试了 {} 次）", failures.len())
}
