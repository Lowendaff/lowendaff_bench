//! multiload's bandwidth and multichase's latency in the NUMA matrix, from their real output on
//! both architectures (`tests/fixtures/README.md`) and from damaged output, and a refused binding.

mod common;

use plugin_memory::numa;

#[test]
fn multiload_gives_its_best_bandwidth() {
    for (name, mibs) in [
        ("aarch64-multiload", "70192"),
        ("x86_64-multiload", "60802"),
    ] {
        assert_eq!(
            numa::parse::bandwidth(&common::printed(&format!("{name}.txt"))),
            Ok(common::value(mibs)),
            "{name}"
        );
    }
    let refused = common::output(1, &common::fixture("aarch64-multiload-mbind-refused.txt"));
    assert_eq!(numa::parse::bandwidth(&refused), Err(numa::Fault::Refused));
    let whole = common::fixture("aarch64-multiload.txt");
    let header = whole.lines().next().expect("a header");
    assert_eq!(
        numa::parse::bandwidth(&common::output(0, header)),
        Err(numa::Fault::NoBandwidth),
        "a header without its row"
    );
    let warned = format!("{whole}multiload: a line after the results\n");
    assert_eq!(
        numa::parse::bandwidth(&common::output(0, &warned)),
        Ok(common::value("70192")),
        "only a line with the header's fields is the row"
    );
    assert_eq!(
        numa::parse::bandwidth(&common::output(0, "70192\n")),
        Err(numa::Fault::NoBandwidth),
        "a row without its header"
    );
    assert_eq!(
        numa::parse::bandwidth(&common::output(2, "boom\n")),
        Err(numa::Fault::Tool(common::exit(2, "boom")))
    );
}

#[test]
fn multichase_in_the_matrix_gives_its_one_number_unless_the_binding_is_refused() {
    for (name, ns) in [
        ("aarch64-multichase", "0.712"),
        ("x86_64-multichase", "1.035"),
    ] {
        let output = common::printed(&format!("{name}.txt"));
        assert_eq!(
            numa::parse::latency(&output),
            Ok(common::value(ns)),
            "{name}"
        );
    }
    let refused = common::output(1, &common::fixture("aarch64-multichase-mbind-refused.txt"));
    assert_eq!(numa::parse::latency(&refused), Err(numa::Fault::Refused));
    let failed = common::output(1, "mbind: Invalid argument\n");
    assert_eq!(
        numa::parse::latency(&failed),
        Err(numa::Fault::Tool(common::exit(
            1,
            "mbind: Invalid argument"
        ))),
        "only a refusal is not worth retrying"
    );
    assert_eq!(
        numa::parse::latency(&common::output(0, "")),
        Err(numa::Fault::NoLatency)
    );
}

#[test]
fn only_a_refused_binding_is_not_tried_again() {
    use plugin_memory::Retry;

    assert!(!numa::Fault::Refused.again());
    for fault in [
        numa::Fault::NoBandwidth,
        numa::Fault::NoLatency,
        numa::Fault::Tool(common::exit(1, "boom")),
    ] {
        assert!(fault.again(), "{fault:?}");
    }
}
