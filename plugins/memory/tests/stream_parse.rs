//! STREAM's rates and version, from its real output on both architectures
//! (`tests/fixtures/README.md`) and from damaged output.

mod common;

use plugin_memory::stream;

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
        let reading =
            stream::parse::reading(&common::printed(&format!("{name}.txt"))).expect("a reading");
        assert_eq!(reading.rates, rates.map(common::value), "{name}");
        assert_eq!(reading.version.as_deref(), Some("5.10"), "{name}");
    }
    let invalid = common::printed("handwritten-stream-failed-validation.txt");
    assert_eq!(
        stream::parse::reading(&invalid),
        Err(stream::Fault::Invalid(
            "Failed Validation on array a[], AvgRelAbsErr > epsilon (1.000000e-13)".to_owned()
        ))
    );
    let whole = common::fixture("aarch64-stream-1-thread.txt");
    let cut = &whole[..whole.find("Function").expect("the table")];
    assert_eq!(
        stream::parse::reading(&common::output(0, cut)),
        Err(stream::Fault::NoResult)
    );
    let without_add = whole.replace("Add:", "Plus:");
    assert_eq!(
        stream::parse::reading(&common::output(0, &without_add)),
        Err(stream::Fault::NoResult),
        "every kernel has a rate"
    );
    assert_eq!(
        stream::parse::reading(&common::output(1, "boom\n")),
        Err(stream::Fault::Tool(common::exit(1, "boom")))
    );
    let unnumbered = whole.replace("$Revision: 5.10 $", "$Revision: x $");
    assert_eq!(
        stream::parse::reading(&common::output(0, &unnumbered)).map(|reading| reading.version),
        Ok(None)
    );
}
