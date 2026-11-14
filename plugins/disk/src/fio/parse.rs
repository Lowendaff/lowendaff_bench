//! What fio prints: its terse (`--minimal`) result with its version and rates, its error line,
//! and whether the filesystem refused direct I/O.

use bench_tool::job;

use super::result::Fault;
use super::result::Outcome;
use super::result::Rates;

/// The rates of `job` in fio's terse output, which may hold other lines too.
///
/// # Errors
///
/// When the output has no terse line for the job, reports an error, or holds a field that is
/// not a number.
pub fn rates(text: &str, job: &str) -> Result<Rates, Fault> {
    let Some(line) = terse(text, job) else {
        return Err(Fault::NoResult);
    };
    let fields: Vec<&str> = line.split(';').collect();
    let field = |number: usize| -> Result<u64, Fault> {
        fields
            .get(number - 1)
            .and_then(|text| text.parse().ok())
            .ok_or(Fault::Field(number))
    };
    let error = field(5)?;
    if error != 0 {
        return Err(Fault::Error(error));
    }
    Ok(Rates {
        read_kib: field(7)?,
        read_iops: field(8)?,
        write_kib: field(48)?,
        write_iops: field(49)?,
    })
}

/// What a test run that finished produced.
#[must_use]
pub fn outcome(output: &job::Output, job: &str) -> Outcome {
    if let Some(fault) = fault(output) {
        return Outcome::Failed(fault);
    }
    match rates(&output.text, job) {
        Ok(rates) => Outcome::Done(rates),
        Err(fault) => Outcome::Failed(fault),
    }
}

/// The fault of a run that did not exit with 0, with fio's error line; `None` when it did.
#[must_use]
pub fn fault(output: &job::Output) -> Option<Fault> {
    let last = error_line(&output.text).map(str::to_owned);
    match output.code {
        Some(0) => None,
        Some(code) => Some(Fault::Tool(job::Fault::Exit { code, last })),
        None => Some(Fault::Tool(job::Fault::Signal { last })),
    }
}

/// The version in the terse line of `job`, such as `3.43` from `fio-3.43`.
#[must_use]
pub fn version(text: &str, job: &str) -> Option<String> {
    let version = terse(text, job)?.split(';').nth(1)?.strip_prefix("fio-")?;
    let plain = version.starts_with(|c: char| c.is_ascii_digit())
        && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    plain.then(|| version.to_owned())
}

/// Whether fio failed because the filesystem refuses direct I/O. fio 3.43 prints one of these
/// messages in `filesetup.c` and `ioengines.c`.
#[must_use]
pub fn lacks_direct_io(output: &str) -> bool {
    output.contains("does not support direct=1/buffered=0")
        || output.contains("does not support O_DIRECT")
}

/// The line of fio's output that names its error: the last non-empty line that is not a terse
/// result. fio prints its messages and its results in either order.
#[must_use]
pub fn error_line(output: &str) -> Option<&str> {
    output
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with("3;"))
}

/// The terse line (format 3) of `job`.
fn terse<'a>(text: &'a str, job: &str) -> Option<&'a str> {
    text.lines().find(|line| {
        let mut fields = line.split(';');
        fields.next() == Some("3") && fields.nth(1) == Some(job)
    })
}
