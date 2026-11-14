//! sysbench's rate and version, from its real output on both architectures
//! (`tests/fixtures/README.md`) and from damaged output.

mod common;

use bench_tool::job;
use plugin_memory::sysbench;

#[test]
fn sysbench_gives_its_transfer_rate_and_version() {
    for (name, rate) in [
        ("aarch64-sysbench-read-1-thread", "42552.53"),
        ("aarch64-sysbench-write-4-threads", "39311.82"),
        ("x86_64-sysbench-read-1-thread", "21266.25"),
    ] {
        let reading =
            sysbench::parse::reading(&common::printed(&format!("{name}.txt"))).expect("a reading");
        assert_eq!(reading.rate, common::value(rate), "{name}");
        assert_eq!(reading.version.as_deref(), Some("1.0.20"), "{name}");
    }
    let bad = common::output(1, &common::fixture("aarch64-sysbench-bad-option.txt"));
    assert_eq!(
        sysbench::parse::reading(&bad),
        Err(sysbench::Fault::Tool(common::exit(
            1,
            "FATAL: Invalid value for memory-block-size: abc"
        )))
    );
    let zero = common::output(0, "1.00 MiB transferred (0.00 MiB/sec)\n");
    assert_eq!(
        sysbench::parse::reading(&zero),
        Err(sysbench::Fault::NoRate),
        "a rate of zero is no result"
    );
    let signalled = job::Output {
        code: None,
        text: "Threads started!\n".to_owned(),
    };
    assert_eq!(
        sysbench::parse::reading(&signalled),
        Err(sysbench::Fault::Tool(job::Fault::Signal {
            last: Some("Threads started!".to_owned()),
        }))
    );
    let rate = "sysbench 1.0.x\n1.00 MiB transferred (5.00 MiB/sec)\n";
    assert_eq!(
        sysbench::parse::reading(&common::output(0, rate)).map(|reading| reading.version),
        Ok(None),
        "a version that is not numbers"
    );
}
