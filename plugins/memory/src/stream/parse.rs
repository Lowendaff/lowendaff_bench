//! What STREAM prints: its version, whether its results validated, and the best rate of each
//! kernel.

use bench_tool::job;

use super::result::Fault;
use super::result::KERNELS;
use crate::Value;

/// What an attempt that worked printed.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// The best rate of each kernel in MB/s, in [`KERNELS`]' order.
    pub rates: [Value; 4],
    /// The version in `STREAM version $Revision: 5.10 $`.
    pub version: Option<String>,
}

/// The reading of an attempt whose results validated, or why there is none.
pub fn reading(output: &job::Output) -> Result<Reading, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    let lines: Vec<&str> = output.text.lines().map(str::trim).collect();
    if !lines
        .iter()
        .any(|line| line.starts_with("Solution Validates"))
    {
        return Err(
            match lines
                .iter()
                .find(|line| line.starts_with("Failed Validation"))
            {
                Some(line) => Fault::Invalid((*line).to_owned()),
                None => Fault::NoResult,
            },
        );
    }
    let rate = |kernel: &str| {
        lines.iter().find_map(|line| {
            let rest = line.strip_prefix(kernel)?.strip_prefix(':')?;
            Value::parse(rest.split_whitespace().next()?)
        })
    };
    let [Some(copy), Some(scale), Some(add), Some(triad)] = KERNELS.map(rate) else {
        return Err(Fault::NoResult);
    };
    Ok(Reading {
        rates: [copy, scale, add, triad],
        version: version(&lines),
    })
}

/// The version in `STREAM version $Revision: 5.10 $`.
fn version(lines: &[&str]) -> Option<String> {
    let version = lines.iter().find_map(|line| {
        line.strip_prefix("STREAM version $Revision:")?
            .strip_suffix('$')
    })?;
    let version = version.trim();
    let plain = version.starts_with(|c: char| c.is_ascii_digit())
        && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    plain.then(|| version.to_owned())
}
