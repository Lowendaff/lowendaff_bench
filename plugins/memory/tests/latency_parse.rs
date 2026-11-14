//! multichase's latency, from its real output on both architectures (`tests/fixtures/README.md`)
//! and from damaged output; and what every value must be.

mod common;

use plugin_memory::Value;
use plugin_memory::latency;

#[test]
fn multichase_gives_its_one_number() {
    for (name, ns) in [
        ("aarch64-multichase", "0.712"),
        ("x86_64-multichase", "1.035"),
    ] {
        let output = common::printed(&format!("{name}.txt"));
        assert_eq!(
            latency::parse::latency(&output),
            Ok(common::value(ns)),
            "{name}"
        );
    }
    let failed = common::output(1, "mbind: Invalid argument\n");
    assert_eq!(
        latency::parse::latency(&failed),
        Err(latency::Fault::Tool(common::exit(
            1,
            "mbind: Invalid argument"
        )))
    );
    assert_eq!(
        latency::parse::latency(&common::output(0, "")),
        Err(latency::Fault::NoLatency)
    );
    assert_eq!(
        latency::parse::latency(&common::output(0, "latency\n")),
        Err(latency::Fault::NoLatency),
        "the last line is the number"
    );
}

#[test]
fn a_value_is_a_finite_positive_number_as_printed() {
    assert_eq!(
        Value::parse(" 12.000 "),
        Some(Value {
            text: "12.000".to_owned(),
            value: 12.0,
        })
    );
    for text in ["0", "-1.5", "inf", "NaN", "x", ""] {
        assert_eq!(Value::parse(text), None, "{text:?}");
    }
}
