//! Reading iperf3's JSON: the speed each way, an upload's round-trip time and retransmissions, the
//! version, and the errors of a busy server, a closed port and an address that never answers.

use bench_tool::job;
use plugin_network::iperf3::parse;
use plugin_network::iperf3::parse::Attempt;

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/iperf3/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

fn output(code: Option<i32>, text: &str) -> job::Output {
    job::Output {
        code,
        text: text.to_owned(),
    }
}

fn exited(code: i32, name: &str) -> job::Output {
    output(Some(code), &fixture(name))
}

#[test]
fn an_upload_has_the_speed_that_the_server_counted_its_smallest_rtt_and_its_retransmits() {
    assert_eq!(
        parse::attempt(&exited(0, "upload.json"), false),
        Attempt::Measured {
            bits: 283_115_282_384,
            rtt: Some(8),
            retransmits: Some(60),
        }
    );
    assert_eq!(
        parse::version(&exited(0, "upload.json")),
        Some("3.22".to_owned())
    );
}

#[test]
fn a_download_has_the_speed_that_the_host_counted_and_no_rtt() {
    assert_eq!(
        parse::attempt(&exited(0, "download.json"), true),
        Attempt::Measured {
            bits: 213_527_856_726,
            rtt: None,
            retransmits: None,
        }
    );
}

#[test]
fn an_upload_over_ipv6_and_one_with_a_limit_read_alike() {
    assert_eq!(
        parse::attempt(&exited(0, "upload6.json"), false),
        Attempt::Measured {
            bits: 110_375_265_776,
            rtt: Some(72),
            retransmits: Some(3),
        }
    );
    let Attempt::Measured { bits, .. } = parse::attempt(&exited(0, "capped.json"), false) else {
        panic!("a speed");
    };
    assert_eq!(bits, 100_505_151, "two streams of 50 Mbit/s at most");
}

#[test]
fn a_stream_without_a_round_trip_time_does_not_count_as_the_smallest() {
    let json = r#"{"start": {"version": "iperf 3.22"}, "end": {
        "sum_received": {"bits_per_second": 1000000.0},
        "sum_sent": {"retransmits": 0},
        "streams": [{"sender": {"min_rtt": 0}}, {"sender": {"min_rtt": 4200}}]}}"#;
    assert_eq!(
        parse::attempt(&output(Some(0), json), false),
        Attempt::Measured {
            bits: 1_000_000,
            rtt: Some(4200),
            retransmits: Some(0),
        }
    );
}

#[test]
fn a_busy_server_a_closed_port_and_an_address_that_never_answers_are_told_apart() {
    assert_eq!(
        parse::attempt(&exited(1, "busy.json"), false),
        Attempt::Busy
    );
    assert_eq!(
        parse::attempt(&exited(1, "refused.json"), true),
        Attempt::Unreachable
    );
    assert_eq!(
        parse::attempt(&exited(1, "silent.json"), false),
        Attempt::Unreachable
    );
    assert_eq!(
        parse::version(&exited(1, "busy.json")),
        Some("3.22".to_owned())
    );
}

#[test]
fn another_error_is_kept_without_control_characters() {
    let json =
        r#"{"start": {}, "end": {}, "error": "control socket has closed unexpectedly\u001b[2J"}"#;
    assert_eq!(
        parse::attempt(&output(Some(1), json), false),
        Attempt::Error("control socket has closed unexpectedly[2J".to_owned())
    );
}

#[test]
fn what_follows_the_json_on_standard_error_is_not_read() {
    let text = format!(
        "{}iperf3: interrupt - the client has terminated\n",
        fixture("upload.json")
    );
    let Attempt::Measured { bits, .. } = parse::attempt(&output(Some(0), &text), false) else {
        panic!("a speed");
    };
    assert_eq!(bits, 283_115_282_384);
}

#[test]
fn a_run_without_a_speed_measured_nothing() {
    let zero = r#"{"start": {"version": "iperf 3.22"},
        "end": {"sum_received": {"bits_per_second": 0.0}}}"#;
    assert_eq!(
        parse::attempt(&output(Some(0), zero), true),
        Attempt::Nothing
    );
    let missing = r#"{"start": {}, "end": {}}"#;
    assert_eq!(
        parse::attempt(&output(Some(0), missing), true),
        Attempt::Nothing
    );
    assert_eq!(parse::attempt(&output(Some(0), ""), true), Attempt::Nothing);
}

#[test]
fn output_that_is_not_json_is_never_quoted_since_it_names_the_hosts_address() {
    let text = "Connecting to host 203.0.113.9, port 5201\nsomething went wrong\n";
    assert_eq!(
        parse::attempt(&output(Some(2), text), false),
        Attempt::Fault(job::Fault::Exit {
            code: 2,
            last: None
        })
    );
    assert_eq!(
        parse::attempt(&output(None, text), false),
        Attempt::Fault(job::Fault::Signal { last: None })
    );
    assert_eq!(parse::version(&output(Some(2), text)), None);
}
