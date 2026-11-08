//! fio as YABS v2026-10-31 runs it (design disk, §5): the jobs, their arguments and their
//! terse results.

use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

/// The block sizes YABS tests, as fio spells them, two per printed table.
pub const BLOCK_SIZES: [[&str; 2]; 2] = [["4k", "64k"], ["512k", "1m"]];

/// Read and write rates of one test, as fio's terse output reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rates {
    /// Read bandwidth in KiB/s (terse field 7).
    pub read_kib: u64,
    /// Read operations per second (field 8).
    pub read_iops: u64,
    /// Write bandwidth in KiB/s (field 48).
    pub write_kib: u64,
    /// Write operations per second (field 49).
    pub write_iops: u64,
}

/// What one test produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The test finished and reported its rates.
    Done(Rates),
    /// The test ran past YABS's 35-second limit and was stopped.
    TimedOut,
    /// The test failed, for the reason given.
    Failed(String),
}

/// The job name YABS gives the test at `block_size`.
#[must_use]
pub fn job_name(block_size: &str) -> String {
    format!("rand_rw_{block_size}")
}

/// Arguments for YABS's setup run: fio lays out the test file, then reads for one second.
#[must_use]
pub fn setup_args(file: &Path, size: &str) -> Vec<OsString> {
    let mut args = strings(&[
        "--name=setup",
        "--ioengine=libaio",
        "--rw=read",
        "--bs=64k",
        "--iodepth=64",
        "--numjobs=2",
        &format!("--size={size}"),
        "--runtime=1",
        "--gtod_reduce=1",
    ]);
    args.push(filename(file));
    args.extend(strings(&["--direct=1", "--minimal"]));
    args
}

/// Arguments for YABS's mixed random read and write test at `block_size`.
#[must_use]
pub fn test_args(file: &Path, size: &str, block_size: &str) -> Vec<OsString> {
    let mut args = strings(&[
        &format!("--name={}", job_name(block_size)),
        "--ioengine=libaio",
        "--rw=randrw",
        "--rwmixread=50",
        &format!("--bs={block_size}"),
        "--iodepth=64",
        "--numjobs=2",
        &format!("--size={size}"),
        "--runtime=30",
        "--gtod_reduce=1",
        "--direct=1",
    ]);
    args.push(filename(file));
    args.extend(strings(&["--group_reporting", "--minimal"]));
    args
}

/// The rates of `job` in fio's terse (`--minimal`) output, which may hold other lines too.
///
/// # Errors
///
/// A Chinese reason when the output has no terse line for the job, reports an error, or
/// holds a field that is not a number.
pub fn parse(output: &str, job: &str) -> Result<Rates, String> {
    let Some(line) = output.lines().find(|line| {
        let mut fields = line.split(';');
        fields.next() == Some("3") && fields.nth(1) == Some(job)
    }) else {
        return Err("fio 没有输出结果".to_owned());
    };
    let fields: Vec<&str> = line.split(';').collect();
    let field = |number: usize| -> Result<u64, String> {
        fields
            .get(number - 1)
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| format!("无法解析 fio 输出的第 {number} 个字段"))
    };
    let error = field(5)?;
    if error != 0 {
        return Err(format!("fio 报告错误 {error}"));
    }
    Ok(Rates {
        read_kib: field(7)?,
        read_iops: field(8)?,
        write_kib: field(48)?,
        write_iops: field(49)?,
    })
}

/// What a test run that ended by itself produced, from fio's exit status and output.
#[must_use]
pub fn outcome(success: bool, output: &str, job: &str) -> Outcome {
    if !success {
        return Outcome::Failed(error_line(output));
    }
    match parse(output, job) {
        Ok(rates) => Outcome::Done(rates),
        Err(reason) => Outcome::Failed(reason),
    }
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
pub fn error_line(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with("3;"))
        .map_or_else(|| "fio 没有输出".to_owned(), str::to_owned)
}

/// `--filename=<file>`, with every `:` escaped as `\:`: fio splits unescaped names at colons.
fn filename(file: &Path) -> OsString {
    let mut bytes = b"--filename=".to_vec();
    for &byte in file.as_os_str().as_bytes() {
        if byte == b':' {
            bytes.push(b'\\');
        }
        bytes.push(byte);
    }
    OsString::from_vec(bytes)
}

fn strings(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}
