//! sysbench's command, and what its CPU test prints, from sysbench 1.0.20's real output
//! (`tests/fixtures/README.md`).

use std::path::Path;
use std::time::Duration;

use bench_tool::job;
use plugin_cpu::sysbench::Fault;
use plugin_cpu::sysbench::measure;
use plugin_cpu::sysbench::parse;
use plugin_cpu::sysbench::parse::Reading;
use plugin_cpu::sysbench::result::Rate;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sysbench")
        .join(name);
    std::fs::read_to_string(&path).expect("fixtures are committed next to the tests")
}

fn output(code: Option<i32>, text: &str) -> job::Output {
    job::Output {
        code,
        text: text.to_owned(),
    }
}

fn rate(text: &str) -> Rate {
    Rate {
        text: text.to_owned(),
        value: text.parse().expect("a number"),
    }
}

#[test]
fn the_command_is_linux_benchs_cpu_test() {
    let args: Vec<String> = measure::args(4)
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
    let attempt = measure::attempt(Path::new("/tools/sysbench"), 4);
    assert_eq!(attempt.program, Path::new("/tools/sysbench"));
    assert_eq!(attempt.args, measure::args(4));
    assert_eq!(attempt.limit, Duration::from_secs(60));
    assert!(attempt.envs.is_empty(), "the C locale is the runner's");
    assert_eq!(attempt.dir, None);
    assert!(
        !attempt.group && !attempt.first_to_go,
        "sysbench starts no workers"
    );
    assert!(attempt.quotable, "the log may quote sysbench's errors");
}

#[test]
fn the_rate_and_the_version_are_read_from_sysbenchs_output() {
    assert_eq!(
        parse::reading(&output(Some(0), &fixture("aarch64-1-thread.txt"))),
        Ok(Reading {
            rate: rate("12802.94"),
            version: Some("1.0.20".to_owned()),
        })
    );
    assert_eq!(
        parse::reading(&output(Some(0), &fixture("aarch64-4-threads.txt"))),
        Ok(Reading {
            rate: rate("45915.29"),
            version: Some("1.0.20".to_owned()),
        })
    );
}

#[test]
fn a_version_is_read_only_from_sysbenchs_first_line() {
    let reading =
        |text: &str| parse::reading(&output(Some(0), text)).map(|reading| reading.version);
    let rate = "\n    events per second: 1.00\n";
    assert_eq!(
        reading(&format!("sysbench 1.0.20{rate}")),
        Ok(Some("1.0.20".to_owned()))
    );
    assert_eq!(
        reading(&format!("sysbench 1.0.20-dirty (x){rate}")),
        Ok(None)
    );
    assert_eq!(reading(&format!("sysbench v1.0.20{rate}")), Ok(None));
    assert_eq!(reading(&format!("sysbench{rate}")), Ok(None));
    assert_eq!(reading(rate), Ok(None), "no header line");
}

#[test]
fn failures_quote_sysbenchs_last_line() {
    assert_eq!(
        parse::reading(&output(Some(1), &fixture("aarch64-bad-option.txt"))),
        Err(Fault::Tool(job::Fault::Exit {
            code: 1,
            last: Some("Invalid value for the 'cpu_max_prime' option: 'abc'".to_owned()),
        }))
    );
    assert_eq!(
        parse::reading(&output(None, "")),
        Err(Fault::Tool(job::Fault::Signal { last: None }))
    );
    assert_eq!(
        parse::reading(&output(Some(0), "sysbench 1.0.20\n\nCPU speed:\n")),
        Err(Fault::NoRate)
    );
    assert_eq!(
        parse::reading(&output(Some(0), "    events per second: nan\n")),
        Err(Fault::NoRate),
        "not a usable number"
    );
    assert_eq!(
        parse::reading(&output(Some(0), "    events per second: 0.00\n")),
        Err(Fault::NoRate),
        "no events means no result"
    );
}
