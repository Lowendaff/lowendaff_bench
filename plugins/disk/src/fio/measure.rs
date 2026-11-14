//! The measurement as YABS v2026-10-31 runs fio: a setup run that lays out the test file, then a
//! mixed random read and write test at each of four block sizes, the result published as each
//! ends.

use std::ffi::OsString;
use std::future::Future;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Stop;
use bench_host::storage::FilesystemKind;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::parse;
use super::result::Failure;
use super::result::Fault;
use super::result::Outcome;
use super::result::Output;
use super::result::Test;

/// The block sizes YABS tests, as fio spells them, two per table.
pub const BLOCK_SIZES: [[&str; 2]; 2] = [["4k", "64k"], ["512k", "1m"]];

/// YABS stops each test after 35 seconds (`timeout 35`).
pub const TEST_LIMIT: Duration = Duration::from_secs(35);

/// The setup run has no limit of its own in YABS; the item's limit bounds it.
pub const SETUP_LIMIT: Duration = Duration::from_secs(10 * 60);

const MIB: u64 = 1024 * 1024;

/// The test file's size, which YABS chooses by architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSize {
    /// Size in bytes; the test needs this much free space.
    pub bytes: u64,
    /// fio's `--size` value.
    pub fio: &'static str,
}

impl FileSize {
    /// 512 MiB on ARM and RISC-V, 2 GiB elsewhere, as YABS's `disk_test` chooses.
    #[must_use]
    pub fn for_arch(arch: &str) -> Self {
        if matches!(arch, "aarch64" | "arm" | "riscv64") {
            Self {
                bytes: 512 * MIB,
                fio: "512M",
            }
        } else {
            Self {
                bytes: 2048 * MIB,
                fio: "2G",
            }
        }
    }
}

/// What the test needs to know about the filesystem of the test directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Host {
    /// What kind of filesystem it is, when known.
    pub kind: Option<FilesystemKind>,
    /// Its free bytes, when known.
    pub available: Option<u64>,
    /// ZFS's `spa_asize_inflation`, when the zfs module is loaded.
    pub inflation: Option<u64>,
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

/// The setup run of `program` on `file`.
#[must_use]
pub fn setup(program: &Path, file: &Path, size: FileSize) -> Job {
    Job::new(program, setup_args(file, size.fio), SETUP_LIMIT)
}

/// The test of `program` on `file` at `block_size`.
#[must_use]
pub fn test(program: &Path, file: &Path, size: FileSize, block_size: &str) -> Job {
    Job::new(program, test_args(file, size.fio, block_size), TEST_LIMIT)
}

/// The free space in GiB that YABS asks for on ZFS, twice `spa_asize_inflation`, when the
/// filesystem is ZFS and has less.
#[must_use]
pub fn zfs_needed(host: &Host, available: u64) -> Option<u64> {
    if host.kind != Some(FilesystemKind::Zfs) {
        return None;
    }
    let needed = host.inflation?.checked_mul(2)?;
    (available < needed.saturating_mul(1024 * MIB)).then_some(needed)
}

/// Lays out `file` with `program` on the filesystem that `host` describes, at the size for the
/// host's architecture, tests it at each block size, and removes it however the item ends.
/// `run` runs a job: the shared runner, or a recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    program: &Path,
    file: &Path,
    host: &Host,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let size = FileSize::for_arch(&ctx.host().arch);
    let result = run_tests(ctx, program, file, size, host, run).await;
    // Free the space for later items at once. The run directory is removed later anyway, so a
    // failure here does not matter.
    let _ = tokio::fs::remove_file(file).await;
    result
}

/// What [`measure`] does, short of removing the file.
async fn run_tests<F, Fut>(
    ctx: &Context<Output>,
    program: &Path,
    file: &Path,
    size: FileSize,
    host: &Host,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let Some(available) = host.available else {
        return Err(Stop::Failed(Failure::NoSpaceInfo));
    };
    if host.kind == Some(FilesystemKind::Memory) {
        return Err(Stop::Skipped(Failure::InMemory));
    }
    // Exactly enough passes, as in YABS.
    if available < size.bytes {
        return Err(Stop::Skipped(Failure::Space {
            needed: size.bytes,
            available,
        }));
    }
    let mut output = Output {
        version: None,
        size: size.bytes,
        zfs_needed: zfs_needed(host, available),
        tests: Vec::new(),
    };
    ctx.publish(&output);

    ctx.progress("生成测试文件");
    let setup = match run(setup(program, file, size)).await {
        Ok(setup) => setup,
        Err(RunError::Fault(fault)) => {
            return Err(Stop::Failed(Failure::Setup(Fault::Tool(fault))));
        }
        Err(RunError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
    };
    if let Some(fault) = parse::fault(&setup) {
        if parse::lacks_direct_io(&setup.text) {
            return Err(Stop::Skipped(Failure::NoDirectIo));
        }
        return Err(Stop::Failed(Failure::Setup(fault)));
    }
    output.version = parse::version(&setup.text, "setup");

    let block_sizes = BLOCK_SIZES.into_iter().flatten();
    for block_size in block_sizes {
        ctx.progress(format!("{} 随机读写测试", block_size.to_uppercase()));
        let outcome = match run(test(program, file, size, block_size)).await {
            Ok(finished) => parse::outcome(&finished, &job_name(block_size)),
            Err(RunError::Fault(fault)) => Outcome::Failed(Fault::Tool(fault)),
            Err(RunError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
        };
        output.tests.push(Test {
            block_size,
            outcome,
        });
        ctx.publish(&output);
    }
    let measured = output
        .tests
        .iter()
        .any(|test| matches!(test.outcome, Outcome::Done(_)));
    if !measured {
        return Err(Stop::Failed(Failure::NoResult));
    }
    Ok(output)
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
