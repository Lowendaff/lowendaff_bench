//! The fixtures of server lists in `tests/fixtures/server-lists`, which the program's reader and
//! `scripts/iperf3-servers.sh` both read: a list named `good-*` follows every rule and one named
//! `bad-*` breaks one, so that the two readers agree on each of them.

use std::path::Path;

use plugin_network::iperf3::servers;

#[test]
fn each_fixture_gets_the_verdict_that_its_name_gives() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server-lists");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("the fixtures' directory reads") {
        let path = entry.expect("a directory entry").path();
        let name = path
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        let list = std::fs::read(&path).expect("the fixture reads");
        let verdict = servers::parse(&list);
        if name.starts_with("good-") {
            assert!(verdict.is_ok(), "{name}: {verdict:?}");
        } else if name.starts_with("bad-") {
            assert!(verdict.is_err(), "{name}: {verdict:?}");
        } else {
            panic!("{name} names no verdict");
        }
        seen += 1;
    }
    assert!(seen >= 30, "only {seen} fixtures");
}
