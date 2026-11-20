//! Parser for `y-cruncher skip-warnings bench <size>` console output.

use bench_tool::job;

use super::result::Fault;
use super::result::Output;

/// Parses one finished y-cruncher Pi benchmark output.
pub fn reading(output: &job::Output, fallback_digits: u64) -> Result<Output, Fault> {
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    // Normalize `\r` progress overwrites into clean lines.
    let normalized = output.text.replace('\r', "\n");
    let mut spot_good = false;
    let mut spot_seen = false;
    let mut digits: Option<u64> = None;
    let mut comp_secs: Option<f64> = None;
    let mut wall_secs: Option<f64> = None;
    let mut cpu_util: Option<f64> = None;
    let mut mc_eff: Option<f64> = None;
    let mut tuning: Option<String> = None;
    let mut ver: Option<String> = None;

    for raw in normalized.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Decimal Digits:") {
            digits = parse_commas_u64(rest);
        } else if let Some(rest) = line.strip_prefix("Total Computation Time:") {
            comp_secs = parse_leading_f64(rest);
        } else if let Some(rest) = line.strip_prefix("Start-to-End Wall Time:") {
            wall_secs = parse_leading_f64(rest);
        } else if let Some(rest) = line.strip_prefix("CPU Utilization:") {
            cpu_util = parse_leading_f64(rest);
        } else if let Some(rest) = line.strip_prefix("Multi-core Efficiency:") {
            mc_eff = parse_leading_f64(rest);
        } else if let Some(rest) = line.strip_prefix("Spot Check:") {
            spot_seen = true;
            spot_good = rest.trim().starts_with("Good");
        } else if let Some(rest) = line.strip_prefix("Tuning Profile:") {
            let val = rest.trim();
            if !val.is_empty() {
                tuning = Some(val.to_owned());
            }
        } else if tuning.is_none()
            && let Some(rest) = line.strip_prefix("Auto-Selecting:")
        {
            let val = rest.trim();
            if !val.is_empty() {
                tuning = Some(val.to_owned());
            }
        } else if ver.is_none()
            && let Some(parsed) = parse_version(line)
        {
            ver = Some(parsed);
        }
    }

    if spot_seen && !spot_good {
        return Err(Fault::ValidationFailed);
    }
    let computation_secs = comp_secs.filter(|v| *v > 0.0).ok_or(Fault::NoResult)?;
    let wall_secs = wall_secs.filter(|v| *v > 0.0).unwrap_or(computation_secs);
    if !spot_good {
        return Err(Fault::ValidationFailed);
    }

    Ok(Output {
        version: ver,
        digits: digits.unwrap_or(fallback_digits),
        computation_secs,
        wall_secs,
        cpu_utilization_pct: cpu_util,
        multicore_efficiency_pct: mc_eff,
        tuning,
    })
}

fn parse_commas_u64(raw: &str) -> Option<u64> {
    let token = raw.split_whitespace().next()?;
    let digits_only: String = token.chars().filter(|c| *c != ',').collect();
    digits_only.parse::<u64>().ok().filter(|v| *v > 0)
}

fn parse_leading_f64(raw: &str) -> Option<f64> {
    let token = raw.split_whitespace().next()?;
    let val: f64 = token.parse().ok()?;
    val.is_finite().then_some(val)
}

/// Parses `y-cruncher v0.8.7 Build 9547...` or `Program: y-cruncher v0.8.7 Build 9547...`.
fn parse_version(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Program:")
        .map(str::trim)
        .unwrap_or(line)
        .strip_prefix("y-cruncher v")?;
    let mut parts = rest.split_whitespace();
    let base = parts.next()?;
    if !base.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if parts.next() == Some("Build")
        && let Some(build_tok) = parts.next()
    {
        let build_num: String = build_tok
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !build_num.is_empty() {
            return Some(format!("{base}.{build_num}"));
        }
    }
    Some(base.to_owned())
}
