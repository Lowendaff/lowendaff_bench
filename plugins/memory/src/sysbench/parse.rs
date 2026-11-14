//! What sysbench's memory test prints: its version on the first line, and its transfer rate.

use bench_tool::job;

use super::result::Fault;
use crate::Value;

/// What an attempt that worked printed.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Its rate in MiB/s.
    pub rate: Value,
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

/// The rate in `324864.00 MiB transferred (64956.37 MiB/sec)`.
fn rate(text: &str) -> Option<Value> {
    text.lines().find_map(|line| {
        let (_, rate) = line.trim().split_once("MiB transferred (")?;
        Value::parse(rate.strip_suffix("MiB/sec)")?)
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
