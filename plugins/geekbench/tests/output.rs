//! Geekbench 7.1.0's console output (`tests/fixtures/README.md`, design cpu, §6.5).

use std::path::Path;

use plugin_geekbench::output;
use plugin_geekbench::output::Scores;
use plugin_geekbench::output::Verdict;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/geekbench")
        .join(name);
    std::fs::read_to_string(&path).expect("fixtures are committed next to the tests")
}

fn scores(single: u32, multi: u32, url: &str) -> Verdict {
    Verdict::Scores(Scores {
        single,
        multi,
        url: Some(url.to_owned()),
    })
}

#[test]
fn an_upload_gives_the_scores_and_the_result_link_but_not_the_claim_link() {
    assert_eq!(
        output::verdict(Some(0), &fixture("x86_64-upload.txt")),
        scores(1771, 4347, "https://browser.geekbench.com/v7/cpu/840201")
    );
    assert_eq!(
        output::verdict(Some(0), &fixture("aarch64-upload.txt")),
        scores(1463, 5098, "https://browser.geekbench.com/v7/cpu/840200")
    );
}

#[test]
fn a_failed_upload_gives_its_reason() {
    for name in ["x86_64-offline.txt", "aarch64-offline.txt"] {
        assert_eq!(
            output::verdict(Some(255), &fixture(name)),
            Verdict::UploadFailed("unable to resolve host browser.geekbench.com".to_owned()),
            "{name}"
        );
    }
    let without_the_key = "Uploading results to the Geekbench Browser.\nConnection reset\n";
    assert_eq!(
        output::verdict(Some(255), without_the_key),
        Verdict::UploadFailed("Connection reset".to_owned()),
        "an upload that never succeeded, without console_upload_failed"
    );
}

#[test]
fn other_ends_quote_the_last_line_but_never_the_claim_link() {
    let refused = "Error: The `--no-upload` switch is only available in Geekbench 7 Pro.\n";
    assert_eq!(
        output::verdict(Some(255), refused),
        Verdict::Failed(
            "Geekbench 退出码 255：Error: The `--no-upload` switch is only available in \
             Geekbench 7 Pro."
                .to_owned()
        )
    );
    // An upload that succeeded, but a run that ended badly without scores: not a failed upload.
    let claim = "Uploading results to the Geekbench Browser.\n\
                 Upload succeeded. Visit the following link and view your results online:\n\n  \
                 https://browser.geekbench.com/v7/cpu/1\n\n  \
                 https://browser.geekbench.com/v7/cpu/1/claim?key=secret\n";
    let Verdict::Failed(reason) = output::verdict(Some(1), claim) else {
        panic!("no scores without the summary");
    };
    assert_eq!(
        reason,
        "Geekbench 退出码 1：https://browser.geekbench.com/v7/cpu/1"
    );
    assert!(!reason.contains("secret"));
    assert_eq!(
        output::verdict(None, ""),
        Verdict::Failed("Geekbench 被信号终止：没有输出".to_owned())
    );
    assert_eq!(
        output::verdict(Some(0), "Geekbench 7.1.0\n"),
        Verdict::Failed("Geekbench 没有输出分数".to_owned())
    );
}

#[test]
fn scores_without_a_link_still_count() {
    let summary = "Benchmark Summary\n  Single-Core Score             1771\n  \
                   Multi-Core Score              4347\n";
    assert_eq!(
        output::verdict(Some(0), summary),
        Verdict::Scores(Scores {
            single: 1771,
            multi: 4347,
            url: None
        })
    );
    let half = "Benchmark Summary\n  Single-Core Score             1771\n";
    assert_eq!(
        output::verdict(Some(0), half),
        Verdict::Failed("Geekbench 没有输出分数".to_owned()),
        "both scores or none"
    );
}

#[test]
fn the_claim_link_is_never_the_result_link() {
    let only_the_claim = "Upload succeeded. Visit the following link and add this result to your \
                          profile:\n\n  \
                          https://browser.geekbench.com/v7/cpu/1/claim?key=secret\n\n\
                          Benchmark Summary\n  Single-Core Score             1771\n  \
                          Multi-Core Score              4347\n";
    assert_eq!(
        output::verdict(Some(0), only_the_claim),
        Verdict::Scores(Scores {
            single: 1771,
            multi: 4347,
            url: None
        })
    );
}
