//! Reading what the tools printed (design memory, §5.3–§5.6, §5.8), from their real output on
//! both architectures (tests/fixtures/README.md), and from damaged output.

use std::path::Path;

use plugin_memory::output;
use plugin_memory::output::AttemptError;
use plugin_memory::output::Output;
use plugin_memory::output::Value;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn exited(code: i32, text: &str) -> Output {
    Output {
        code: Some(code),
        text: text.to_owned(),
    }
}

fn value(text: &str) -> Value {
    Value {
        text: text.to_owned(),
        value: text.parse().expect("a number"),
    }
}

fn failed(reason: &str) -> AttemptError {
    AttemptError::Failed(reason.to_owned())
}

#[test]
fn sysbench_gives_its_transfer_rate() {
    for (name, rate) in [
        ("aarch64-sysbench-read-1-thread", "42552.53"),
        ("aarch64-sysbench-write-4-threads", "39311.82"),
        ("x86_64-sysbench-read-1-thread", "21266.25"),
    ] {
        let output = exited(0, &fixture(name));
        assert_eq!(output::sysbench(&output), Ok(value(rate)), "{name}");
    }
    assert_eq!(
        output::sysbench(&exited(1, &fixture("aarch64-sysbench-bad-option"))),
        Err(failed(
            "sysbench 退出码 1：FATAL: Invalid value for memory-block-size: abc"
        ))
    );
    let rate = "1.00 MiB transferred (0.00 MiB/sec)\n";
    assert_eq!(
        output::sysbench(&exited(0, rate)),
        Err(failed("sysbench 没有输出传输速度")),
        "a rate of zero is no result"
    );
    let signalled = Output {
        code: None,
        text: "Threads started!\n".to_owned(),
    };
    assert_eq!(
        output::sysbench(&signalled),
        Err(failed("sysbench 被信号终止：Threads started!"))
    );
}

#[test]
fn stream_gives_the_best_rate_of_each_kernel_when_its_results_validate() {
    for (name, rates) in [
        (
            "aarch64-stream-1-thread",
            ["65600.1", "61398.8", "68269.4", "71748.6"],
        ),
        (
            "aarch64-stream-4-threads",
            ["80805.4", "73705.5", "81508.7", "81940.0"],
        ),
        (
            "x86_64-stream-1-thread",
            ["36995.0", "40160.9", "40820.5", "45201.3"],
        ),
    ] {
        let output = exited(0, &fixture(name));
        assert_eq!(output::stream(&output), Ok(rates.map(value)), "{name}");
    }
    let invalid = exited(0, &fixture("handwritten-stream-failed-validation"));
    assert_eq!(
        output::stream(&invalid),
        Err(failed(
            "STREAM 的结果没有通过校验：Failed Validation on array a[], \
             AvgRelAbsErr > epsilon (1.000000e-13)"
        ))
    );
    let whole = fixture("aarch64-stream-1-thread");
    let cut = &whole[..whole.find("Function").expect("the table")];
    assert_eq!(
        output::stream(&exited(0, cut)),
        Err(failed("STREAM 没有输出结果"))
    );
    let without_add = whole.replace("Add:", "Plus:");
    assert_eq!(
        output::stream(&exited(0, &without_add)),
        Err(failed("STREAM 没有输出结果")),
        "every kernel has a rate"
    );
    assert_eq!(
        output::stream(&exited(1, "boom\n")),
        Err(failed("STREAM 退出码 1：boom"))
    );
}

#[test]
fn multichase_gives_its_one_number() {
    for (name, ns) in [
        ("aarch64-multichase", "0.712"),
        ("x86_64-multichase", "1.035"),
    ] {
        let output = exited(0, &fixture(name));
        assert_eq!(output::multichase(&output), Ok(value(ns)), "{name}");
    }
    let refused = exited(1, &fixture("aarch64-multichase-mbind-refused"));
    assert_eq!(output::multichase(&refused), Err(AttemptError::Refused));
    assert_eq!(
        output::multichase(&exited(1, "mbind: Invalid argument\n")),
        Err(failed("multichase 退出码 1：mbind: Invalid argument")),
        "only a refusal is not worth retrying"
    );
    assert_eq!(
        output::multichase(&exited(0, "")),
        Err(failed("multichase 没有输出延迟"))
    );
}

#[test]
fn multiload_gives_its_best_bandwidth() {
    for (name, mibs) in [
        ("aarch64-multiload", "70192"),
        ("x86_64-multiload", "60802"),
    ] {
        let output = exited(0, &fixture(name));
        assert_eq!(output::multiload(&output), Ok(value(mibs)), "{name}");
    }
    let refused = exited(1, &fixture("aarch64-multiload-mbind-refused"));
    assert_eq!(output::multiload(&refused), Err(AttemptError::Refused));
    let whole = fixture("aarch64-multiload");
    let header = whole.lines().next().expect("a header");
    assert_eq!(
        output::multiload(&exited(0, header)),
        Err(failed("multiload 没有输出带宽")),
        "a header without its row"
    );
    let warned = format!("{whole}multiload: a line after the results\n");
    assert_eq!(
        output::multiload(&exited(0, &warned)),
        Ok(value("70192")),
        "only a line with the header's fields is the row"
    );
    assert_eq!(
        output::multiload(&exited(0, "70192\n")),
        Err(failed("multiload 没有输出带宽")),
        "a row without its header"
    );
}
