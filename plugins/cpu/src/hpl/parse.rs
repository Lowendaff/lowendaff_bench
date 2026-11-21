//! Parser for High-Performance Linpack (`xhpl`) stdout.

use bench_tool::job;

use super::result::Fault;
use super::result::Output;

/// Parses `xhpl` output for `threads` worker threads.
pub fn reading(output: &job::Output, threads: usize) -> Result<Output, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    let text = &output.text;
    if text.lines().any(|line| line.contains("...... FAILED")) {
        return Err(Fault::ResidualFailed);
    }
    let passed = text.lines().any(|line| {
        line.contains("...... PASSED") || line.contains("1 tests completed and passed")
    });
    if !passed {
        return Err(Fault::ResidualFailed);
    }
    let (n, nb, seconds, gflops) = parse_row(text).ok_or(Fault::NoResult)?;
    Ok(Output {
        version: version(text),
        threads,
        n,
        nb,
        seconds,
        gflops,
        residual: parse_residual(text),
    })
}

/// Parses `WR11C2R4         512    64     1     1               0.00             2.6810e+01`.
fn parse_row(text: &str) -> Option<(u32, u32, f64, f64)> {
    let mut after_header = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("T/V") && trimmed.contains("Gflops") {
            after_header = true;
            continue;
        }
        if !after_header || !trimmed.starts_with('W') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() != 7 {
            continue;
        }
        let n: u32 = cols[1].parse().ok()?;
        let nb: u32 = cols[2].parse().ok()?;
        let seconds: f64 = cols[5].parse().ok()?;
        let gflops: f64 = cols[6].parse().ok()?;
        if n > 0
            && nb > 0
            && seconds.is_finite()
            && seconds >= 0.0
            && gflops.is_finite()
            && gflops > 0.0
        {
            return Some((n, nb, seconds, gflops));
        }
    }
    None
}

/// Parses `||Ax-b||_oo/(eps*(||A||_oo*||x||_oo+||b||_oo)*N)=   5.20140296e-03 ...... PASSED`.
fn parse_residual(text: &str) -> Option<f64> {
    for line in text.lines() {
        let Some((_, rhs)) = line.split_once("||Ax-b||_oo") else {
            continue;
        };
        let Some((_, after_eq)) = rhs.split_once('=') else {
            continue;
        };
        let token = after_eq.split_whitespace().next()?;
        let val: f64 = token.parse().ok()?;
        if val.is_finite() && val >= 0.0 {
            return Some(val);
        }
    }
    None
}

/// Extracts `2.3` from `HPLinpack 2.3  --  High-Performance Linpack benchmark`.
fn version(text: &str) -> Option<String> {
    let header_lines = text.lines().take(8);
    for line in header_lines {
        let Some(rest) = line.trim().strip_prefix("HPLinpack ") else {
            continue;
        };
        let token = rest.split_whitespace().next()?;
        let plain = token.starts_with(|c: char| c.is_ascii_digit())
            && token.chars().all(|c| c.is_ascii_digit() || c == '.');
        if plain {
            return Some(token.to_owned());
        }
    }
    None
}
