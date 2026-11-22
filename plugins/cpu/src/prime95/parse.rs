//! Parser for `mprime -t -d` console output after timed `SIGINT` shutdown.

use bench_tool::job;

use super::result::Fault;
use super::result::Output;

/// Parses `mprime -t -d` torture output.
pub fn reading(output: &job::Output, duration_secs: u64) -> Result<Output, Fault> {
    let text = &output.text;
    if let Some(lib) = missing_shared_lib(text) {
        return Err(Fault::MissingHostLib(lib));
    }
    if let Some(hw_err) = hardware_error_line(text) {
        return Err(Fault::HardwareError(hw_err));
    }
    // `mprime` stopped cleanly via `SIGINT` prints `Torture Test completed ...` and `Execution
    // halted.`, exiting with 0 (or ending on SIGINT on some kernels). Non-zero exit codes without
    // torture completion lines are process faults.
    let mut completed_workers: usize = 0;
    let mut started_workers: usize = 0;
    let mut tests_completed: u64 = 0;
    let mut errors: u64 = 0;
    let mut warnings: u64 = 0;
    let mut fft_desc: Option<String> = None;
    let mut ver: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if ver.is_none()
            && let Some(parsed) = parse_version(line)
        {
            ver = Some(parsed);
        }
        if line.ends_with("Worker starting") {
            started_workers = started_workers.saturating_add(1);
        }
        if fft_desc.is_none()
            && let Some(desc) = parse_fft_desc(line)
        {
            fft_desc = Some(desc);
        }
        if let Some((t, e, w)) = parse_torture_summary(line) {
            completed_workers = completed_workers.saturating_add(1);
            tests_completed = tests_completed.saturating_add(t);
            errors = errors.saturating_add(e);
            warnings = warnings.saturating_add(w);
        }
    }

    if completed_workers == 0 {
        if let Some(fault) = output.fault() {
            return Err(Fault::Tool(fault));
        }
        return Err(Fault::NoSummary);
    }
    if errors > 0 {
        return Err(Fault::HardwareError(format!(
            "Torture Test reported {errors} error(s)"
        )));
    }

    Ok(Output {
        version: ver,
        workers: completed_workers.max(started_workers),
        duration_secs,
        fft_desc,
        tests_completed,
        errors,
        warnings,
        passed: true,
    })
}

/// Parses `... Mersenne number primality test program version 30.19 build 20`.
fn parse_version(line: &str) -> Option<String> {
    let (_, rhs) = line.split_once("primality test program version ")?;
    let mut parts = rhs.split_whitespace();
    let ver = parts.next()?;
    if parts.next() == Some("build")
        && let Some(bld) = parts.next()
    {
        return Some(format!("{ver}b{bld}"));
    }
    Some(ver.to_owned())
}

/// Parses `... using FMA3 FFT length 18K, Pass1=384, Pass2=48, clm=2.` -> `FMA3 FFT length 18K`.
fn parse_fft_desc(line: &str) -> Option<String> {
    let (_, rhs) = line.split_once(" using ")?;
    let head = rhs.split(',').next()?.trim().trim_end_matches('.');
    (!head.is_empty()).then(|| head.to_owned())
}

/// Parses `... Torture Test completed 2 tests in 1 minutes - 0 errors, 0 warnings.`.
fn parse_torture_summary(line: &str) -> Option<(u64, u64, u64)> {
    let (_, rhs) = line.split_once("Torture Test completed ")?;
    let mut words = rhs.split_whitespace();
    let tests: u64 = words.next()?.parse().ok()?;
    let (_, after_dash) = rhs.split_once(" - ")?;
    let mut parts = after_dash.split(',');
    let err_tok = parts.next()?.split_whitespace().next()?;
    let warn_tok = parts.next()?.split_whitespace().next()?;
    let errors: u64 = err_tok.parse().ok()?;
    let warnings: u64 = warn_tok.parse().ok()?;
    Some((tests, errors, warnings))
}

fn hardware_error_line(text: &str) -> Option<String> {
    for raw in text.lines() {
        let line = raw.trim();
        let upper = line.to_ascii_uppercase();
        if upper.contains("FATAL ERROR:")
            || upper.contains("HARDWARE FAILURE")
            || upper.contains("SUM(INPUTS) !=")
            || upper.contains("ROUND OFF (")
        {
            let clean = line.split_once(']').map_or(line, |(_, rest)| rest.trim());
            return Some(clean.to_owned());
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use bench_tool::job;

    #[test]
    fn reading_saturates_extreme_worker_counts_without_overflow() {
        let sample = "\
            [Worker #1] Torture Test completed 18446744073709551615 tests in 1 minutes - \
            0 errors, 18446744073709551615 warnings.\n\
            [Worker #2] Torture Test completed 10 tests in 1 minutes - \
            0 errors, 5 warnings.\n";
        let out = super::reading(
            &job::Output {
                code: Some(0),
                text: sample.to_owned(),
            },
            60,
        )
        .expect("parsed");
        assert_eq!(out.tests_completed, u64::MAX);
        assert_eq!(out.warnings, u64::MAX);
    }
}
