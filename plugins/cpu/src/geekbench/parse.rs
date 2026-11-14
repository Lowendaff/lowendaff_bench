//! What Geekbench's console says. The free version has no other output, so this reads the
//! English lines that 7.1.0 prints.

use bench_tool::job;

use super::result::Fault;
use super::result::Scores;

/// How a run ended, from its exit code and its output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The upload succeeded and the scores were printed.
    Scores(Scores),
    /// The benchmark ran, but the upload failed for this reason. The free version then shows no
    /// scores.
    UploadFailed(String),
    /// Anything else.
    Failed(Fault),
}

/// The verdict on a run that finished.
#[must_use]
pub fn verdict(output: &job::Output) -> Verdict {
    let text = &output.text;
    if let Some(scores) = scores(text) {
        return Verdict::Scores(scores);
    }
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    if let Some(reason) = lines
        .iter()
        .find_map(|line| line.strip_prefix("console_upload_failed "))
    {
        return Verdict::UploadFailed(reason.trim().to_owned());
    }
    let uploading = lines
        .iter()
        .any(|line| line.starts_with("Uploading results"));
    let succeeded = lines
        .iter()
        .any(|line| line.starts_with("Upload succeeded"));
    let last = last_line(text);
    if uploading && !succeeded {
        return Verdict::UploadFailed(last.unwrap_or_default().to_owned());
    }
    let last = last.map(str::to_owned);
    Verdict::Failed(match output.code {
        Some(0) => Fault::NoScores,
        Some(code) => Fault::Tool(job::Fault::Exit { code, last }),
        None => Fault::Tool(job::Fault::Signal { last }),
    })
}

/// The scores under `Benchmark Summary`, the result link after `Upload succeeded`, and the
/// version in the first line.
fn scores(text: &str) -> Option<Scores> {
    let summary = text
        .lines()
        .skip_while(|line| line.trim() != "Benchmark Summary");
    let mut single = None;
    let mut multi = None;
    for line in summary {
        let line = line.trim();
        if let Some(score) = line.strip_prefix("Single-Core Score") {
            single = score.trim().parse().ok();
        } else if let Some(score) = line.strip_prefix("Multi-Core Score") {
            multi = score.trim().parse().ok();
        }
    }
    let url = text
        .lines()
        .skip_while(|line| !line.trim().starts_with("Upload succeeded"))
        .map(str::trim)
        .find(|line| line.starts_with("https://browser.geekbench.com/") && !line.contains("/claim"))
        .map(str::to_owned);
    Some(Scores {
        version: version(text),
        single: single?,
        multi: multi?,
        url,
    })
}

/// The version in the first line, `Geekbench 7.1.0 : https://www.geekbench.com/`, or
/// `Geekbench 7.1.0 Preview : ...` for the build for aarch64.
fn version(text: &str) -> Option<String> {
    let (name, _) = text.lines().next()?.split_once(" : ")?;
    let version = name.strip_prefix("Geekbench ")?.trim();
    version
        .starts_with(|c: char| c.is_ascii_digit())
        .then(|| version.to_owned())
}

/// The last line with text, never the link that claims the result, to quote in a reason.
fn last_line(text: &str) -> Option<&str> {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.contains("claim?key="))
}
