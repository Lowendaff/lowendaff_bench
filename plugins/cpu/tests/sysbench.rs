//! sysbench's command, the parser against sysbench 1.0.20's real output
//! (`tests/fixtures/README.md`), and the retries (design cpu, §5.2, §5.4).

use std::cell::Cell;
use std::path::Path;

use plugin_cpu::sysbench;
use plugin_cpu::sysbench::AttemptError;
use plugin_cpu::sysbench::Rate;
use plugin_cpu::sysbench::RunError;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sysbench")
        .join(name);
    std::fs::read_to_string(&path).expect("fixtures are committed next to the tests")
}

fn rate(text: &str) -> Rate {
    Rate {
        text: text.to_owned(),
        value: text.parse().expect("a number"),
    }
}

#[test]
fn the_command_is_linux_benchs_cpu_test() {
    let args: Vec<String> = sysbench::args(4)
        .into_iter()
        .map(|arg| arg.into_string().expect("ASCII"))
        .collect();
    assert_eq!(
        args,
        [
            "cpu",
            "--cpu-max-prime=10000",
            "--threads=4",
            "--time=20",
            "run"
        ]
    );
}

#[test]
fn the_rate_is_read_from_sysbenchs_output() {
    assert_eq!(
        sysbench::outcome(Some(0), &fixture("aarch64-1-thread.txt")),
        Ok(rate("12802.94"))
    );
    assert_eq!(
        sysbench::outcome(Some(0), &fixture("aarch64-4-threads.txt")),
        Ok(rate("45915.29"))
    );
}

#[test]
fn failures_quote_sysbenchs_last_line() {
    assert_eq!(
        sysbench::outcome(Some(1), &fixture("aarch64-bad-option.txt")),
        Err("sysbench 退出码 1：Invalid value for the 'cpu_max_prime' option: 'abc'".to_owned())
    );
    assert_eq!(
        sysbench::outcome(None, ""),
        Err("sysbench 被信号终止：没有输出".to_owned())
    );
    assert_eq!(
        sysbench::outcome(Some(0), "sysbench 1.0.20\n\nCPU speed:\n"),
        Err("sysbench 没有输出每秒事件数".to_owned())
    );
    assert_eq!(
        sysbench::outcome(Some(0), "    events per second: nan\n"),
        Err("sysbench 没有输出每秒事件数".to_owned()),
        "not a usable number"
    );
    assert_eq!(
        sysbench::outcome(Some(0), "    events per second: 0.00\n"),
        Err("sysbench 没有输出每秒事件数".to_owned()),
        "no events means no result"
    );
}

#[tokio::test]
async fn a_run_is_retried_until_it_succeeds_at_most_four_times() {
    let calls = Cell::new(0);
    let flaky = || {
        calls.set(calls.get() + 1);
        let call = calls.get();
        async move {
            if call < 3 {
                Err(AttemptError::Failed(format!("第 {call} 次失败")))
            } else {
                Ok(rate("100.00"))
            }
        }
    };
    let success = sysbench::retried(flaky)
        .await
        .expect("the third attempt works");
    assert_eq!(success.rate, rate("100.00"));
    assert_eq!(success.failures, ["第 1 次失败", "第 2 次失败"]);
    assert_eq!(calls.get(), 3);

    let calls = Cell::new(0);
    let broken = || {
        calls.set(calls.get() + 1);
        let call = calls.get();
        async move { Err(AttemptError::Failed(format!("第 {call} 次失败"))) }
    };
    assert_eq!(
        sysbench::retried(broken).await,
        Err(RunError::Failed(
            ["第 1 次失败", "第 2 次失败", "第 3 次失败", "第 4 次失败"]
                .map(str::to_owned)
                .to_vec()
        ))
    );
    assert_eq!(calls.get(), 4, "the first attempt and 3 retries");
}

#[tokio::test]
async fn a_cancelled_run_is_not_retried() {
    let calls = Cell::new(0);
    let cancelled = || {
        calls.set(calls.get() + 1);
        async { Err(AttemptError::Cancelled) }
    };
    assert_eq!(sysbench::retried(cancelled).await, Err(RunError::Cancelled));
    assert_eq!(calls.get(), 1);
}
