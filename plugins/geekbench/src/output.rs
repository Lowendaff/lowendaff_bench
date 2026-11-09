//! What Geekbench's console says (design cpu, §6.5). The free version has no other output
//! (§3.2), so this reads the English lines that 7.1.0 prints.

/// The scores of a run that uploaded its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scores {
    /// The single-core score.
    pub single: u32,
    /// The multi-core score.
    pub multi: u32,
    /// The public result page, without the claim link.
    pub url: Option<String>,
}

/// How a run ended, from its exit code and its output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The upload succeeded and the scores were printed.
    Scores(Scores),
    /// The benchmark ran, but the upload failed for this reason. The free version then
    /// shows no scores.
    UploadFailed(String),
    /// Anything else, with the reason.
    Failed(String),
}

/// The verdict on a run, given its exit code (`None` after a signal) and its standard output
/// followed by its standard error.
#[must_use]
pub fn verdict(code: Option<i32>, output: &str) -> Verdict {
    if let Some(scores) = scores(output) {
        return Verdict::Scores(scores);
    }
    let lines: Vec<&str> = output.lines().map(str::trim).collect();
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
    if uploading && !succeeded {
        return Verdict::UploadFailed(last_line(output).to_owned());
    }
    match code {
        Some(0) => Verdict::Failed("Geekbench 没有输出分数".to_owned()),
        Some(code) => Verdict::Failed(format!("Geekbench 退出码 {code}：{}", last_line(output))),
        None => Verdict::Failed(format!("Geekbench 被信号终止：{}", last_line(output))),
    }
}

/// The scores under `Benchmark Summary`, and the result link after `Upload succeeded`.
fn scores(output: &str) -> Option<Scores> {
    let summary = output
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
    let url = output
        .lines()
        .skip_while(|line| !line.trim().starts_with("Upload succeeded"))
        .map(str::trim)
        .find(|line| line.starts_with("https://browser.geekbench.com/") && !line.contains("/claim"))
        .map(str::to_owned);
    Some(Scores {
        single: single?,
        multi: multi?,
        url,
    })
}

/// The last line with text, never the claim link, for a reason that quotes Geekbench.
fn last_line(output: &str) -> &str {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.contains("claim?key="))
        .unwrap_or("没有输出")
}
