//! What sysbench's CPU test prints: its version on the first line, and its rate.

use bench_tool::job;

use super::result::Fault;
use super::result::Rate;

/// What an attempt that worked printed.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Its rate.
    pub rate: Rate,
    /// The version on sysbench's first line, such as `1.0.20`.
    pub version: Option<String>,
}

/// The reading of an attempt, or why there is none.
pub fn reading(output: &job::Output) -> Result<Reading, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    let rate = rate(&output.text).ok_or(Fault::NoRate)?;
    Ok(Reading {
        rate,
        version: version(&output.text),
    })
}

/// The value of the line `events per second: 12842.07`.
fn rate(text: &str) -> Option<Rate> {
    let text = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("events per second:"))?
        .trim();
    let value: f64 = text.parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| Rate {
        text: text.to_owned(),
        value,
    })
}

/// The version in the first line, `sysbench 1.0.20 (using bundled LuaJIT 2.1.0-beta2)`.
fn version(text: &str) -> Option<String> {
    let version = text
        .lines()
        .next()?
        .strip_prefix("sysbench ")?
        .split_whitespace()
        .next()?;
    let plain = version.starts_with(|c: char| c.is_ascii_digit())
        && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    plain.then(|| version.to_owned())
}
