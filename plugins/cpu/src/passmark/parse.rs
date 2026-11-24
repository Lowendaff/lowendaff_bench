//! Parser for PassMark PerformanceTest `results_cpu.yml` and missing-library diagnostics.

use std::collections::BTreeMap;

use bench_tool::job;

use super::result::Fault;
use super::result::Output;

/// Inspects a PassMark process exit/output before reading `results_cpu.yml`.
pub fn check_exit(output: &job::Output) -> Result<(), Fault> {
    if let Some(lib) = missing_shared_lib(&output.text) {
        return Err(Fault::MissingHostLib(lib));
    }
    if let Some(fault) = output.fault() {
        return Err(Fault::Tool(fault));
    }
    Ok(())
}

/// Parses `results_cpu.yml` written by `PerformanceTest_Linux_* -r 1`.
pub fn results_yml(yml: &str) -> Result<Output, Fault> {
    let mut section = "";
    let mut version_fields: BTreeMap<&str, &str> = BTreeMap::new();
    let mut results: BTreeMap<&str, &str> = BTreeMap::new();

    for raw_line in yml.lines() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            section = line.strip_suffix(':').unwrap_or("");
            continue;
        }
        let Some((key, val)) = line.trim().split_once(':') else {
            continue;
        };
        let (key, val) = (key.trim(), val.trim());
        match section {
            "Version" => {
                version_fields.insert(key, val);
            }
            "Results" => {
                results.insert(key, val);
            }
            _ => {}
        }
    }

    if results.get("Results Complete").copied() != Some("true") {
        return Err(Fault::NoScores);
    }

    let num = |key: &str| -> Option<f64> {
        let val: f64 = results.get(key)?.parse().ok()?;
        (val.is_finite() && val >= 0.0).then_some(val)
    };
    let cpu_mark = num("SUMM_CPU")
        .filter(|v| *v > 0.0)
        .ok_or(Fault::NoScores)?;
    let single_thread = num("CPU_SINGLETHREAD")
        .filter(|v| *v > 0.0)
        .ok_or(Fault::NoScores)?;

    let is_digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let version = match (
        version_fields.get("Major"),
        version_fields.get("Minor"),
        version_fields.get("Build"),
    ) {
        (Some(maj), Some(min), Some(bld)) if is_digits(maj) && is_digits(min) && is_digits(bld) => {
            Some(format!("{maj}.{min}.{bld}"))
        }
        _ => None,
    };

    Ok(Output {
        version,
        cpu_mark,
        single_thread,
        integer_math: num("CPU_INTEGER_MATH").unwrap_or(0.0),
        floating_math: num("CPU_FLOATINGPOINT_MATH").unwrap_or(0.0),
        prime: num("CPU_PRIME").unwrap_or(0.0),
        sorting: num("CPU_SORTING").unwrap_or(0.0),
        encryption: num("CPU_ENCRYPTION").unwrap_or(0.0),
        compression: num("CPU_COMPRESSION").unwrap_or(0.0),
        physics: num("CPU_PHYSICS").unwrap_or(0.0),
        extended_instructions: num("CPU_MATRIX_MULT_SSE").unwrap_or(0.0),
    })
}

fn missing_shared_lib(text: &str) -> Option<String> {
    for line in text.lines() {
        let marker = "error while loading shared libraries:";
        if let Some((_, rhs)) = line.split_once(marker) {
            let lib = rhs.split(':').next()?.trim();
            if !lib.is_empty() {
                return Some(lib.to_owned());
            }
        }
    }
    None
}
