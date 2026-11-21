//! Unified CPU benchmark plugin (`cpu.bench`): runs the user-selected subset of `sysbench`,
//! `7zip`, `unixbench`, `hpl`, `passmark`, `ycruncher`, `prime95`, and `geekbench` in canonical
//! order, publishing partial results as each sub-suite finishes.

pub mod result;

use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Measurement;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_api::ToolSpec;
use bench_host::glibc::GlibcVersion;
use bench_host::glibc::Libc;
use bench_host::profile::HostProfile;
use bench_tool::GEEKBENCH;
use bench_tool::HPL;
use bench_tool::PASSMARK;
use bench_tool::PASSMARK_LIBS;
use bench_tool::PRIME95;
use bench_tool::SEVENZIP;
use bench_tool::SYSBENCH;
use bench_tool::UNIXBENCH;
use bench_tool::YCRUNCHER;
use serde::Deserialize;

use crate::geekbench;
use crate::hpl;
use crate::passmark;
use crate::prime95;
use crate::seven_zip;
use crate::sysbench;
use crate::unixbench;
use crate::ycruncher;

pub use result::DEFAULT_SUITES;
pub use result::Failure;
pub use result::Output;
pub use result::SUITE_CODES;
pub use result::SuiteOutcome;
pub use result::SuiteSkip;

/// Every tool `cpu.bench` may use across all 8 sub-suites (validated by `registry::validate` and
/// `tests/tools.rs`).
pub const ALL_CPU_TOOLS: [ToolSpec; 9] = [
    SYSBENCH,
    SEVENZIP,
    UNIXBENCH,
    HPL,
    PASSMARK,
    PASSMARK_LIBS,
    YCRUNCHER,
    PRIME95,
    GEEKBENCH,
];

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawCpuBench {
    suites: Vec<String>,
}

/// Unified CPU benchmark plugin (`cpu.bench`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawCpuBench")]
pub struct CpuBench {
    enabled: [bool; 8],
    custom_codes: Vec<String>,
}

impl Default for CpuBench {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_SUITES,
            custom_codes: Vec::new(),
        }
    }
}

impl TryFrom<RawCpuBench> for CpuBench {
    type Error = String;

    fn try_from(raw: RawCpuBench) -> Result<Self, Self::Error> {
        if raw.suites.is_empty() {
            return Ok(Self::default());
        }
        let mut enabled = [false; 8];
        for token in &raw.suites {
            let trimmed = token.trim();
            if trimmed == "all" {
                enabled = [true; 8];
            } else if let Some(idx) = SUITE_CODES.iter().position(|code| *code == trimmed) {
                enabled[idx] = true;
            } else {
                return Err(format!("unknown CPU benchmark suite `{trimmed}`"));
            }
        }
        let custom_codes = if enabled == DEFAULT_SUITES {
            Vec::new()
        } else {
            SUITE_CODES
                .iter()
                .zip(enabled.iter())
                .filter(|(_, on)| **on)
                .map(|(code, _)| (*code).to_owned())
                .collect()
        };
        Ok(Self {
            enabled,
            custom_codes,
        })
    }
}

impl CpuBench {
    /// Builds a `CpuBench` from explicit boolean flags in [`SUITE_CODES`] order.
    #[must_use]
    pub fn from_flags(enabled: [bool; 8]) -> Self {
        let custom_codes = if enabled == DEFAULT_SUITES {
            Vec::new()
        } else {
            SUITE_CODES
                .iter()
                .zip(enabled.iter())
                .filter(|(_, on)| **on)
                .map(|(code, _)| (*code).to_owned())
                .collect()
        };
        Self {
            enabled,
            custom_codes,
        }
    }

    /// Enabled flags in [`SUITE_CODES`] order.
    #[must_use]
    pub const fn enabled(&self) -> &[bool; 8] {
        &self.enabled
    }

    fn suite_precondition(idx: usize, host: &HostProfile) -> Result<(), SuiteSkip> {
        match idx {
            // sysbench, 7zip, unixbench, hpl: static musl builds on x86_64 & aarch64, offline.
            0..=3 => Ok(()),
            // passmark: x86_64 & aarch64, needs glibc >= 2.28.
            4 => check_glibc(passmark::MIN_GLIBC, host),
            // ycruncher: static x86_64 only.
            5 => {
                if host.arch == "x86_64" {
                    Ok(())
                } else {
                    Err(SuiteSkip::ArchUnsupported(host.arch.clone()))
                }
            }
            // prime95: x86_64 + glibc >= 2.18.
            6 => {
                if host.arch != "x86_64" {
                    return Err(SuiteSkip::ArchUnsupported(host.arch.clone()));
                }
                check_glibc(prime95::MIN_GLIBC, host)
            }
            // geekbench: x86_64 & aarch64, needs glibc >= 2.35 and IPv4.
            7 => {
                check_glibc(geekbench::MIN_GLIBC, host)?;
                if host.network.ipv4.reachable() {
                    Ok(())
                } else {
                    Err(SuiteSkip::Ipv4Unavailable)
                }
            }
            _ => Ok(()),
        }
    }
}

fn check_glibc(needed: GlibcVersion, host: &HostProfile) -> Result<(), SuiteSkip> {
    match host.libc {
        Libc::Glibc(found) if found < needed => Err(SuiteSkip::Glibc {
            needed: (needed.major, needed.minor),
            found: Some((found.major, found.minor)),
        }),
        Libc::NoGlibc => Err(SuiteSkip::Glibc {
            needed: (needed.major, needed.minor),
            found: None,
        }),
        Libc::Glibc(_) | Libc::GlibcUnknown => Ok(()),
    }
}

fn suite_glibc(idx: usize) -> Option<GlibcVersion> {
    match idx {
        4 => Some(passmark::MIN_GLIBC),
        6 => Some(prime95::MIN_GLIBC),
        7 => Some(geekbench::MIN_GLIBC),
        _ => None,
    }
}

impl Plugin for CpuBench {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "cpu.bench",
        requires: Requirements {
            tools: &ALL_CPU_TOOLS,
            net: Net::None,
            platforms: &[],
            glibc: None,
            numa: false,
        },
        // Covers Geekbench retries + UnixBench + PassMark + HPL + 7-Zip + Prime95 + y-cruncher.
        timeout: Duration::from_secs(320 * 60),
    };

    fn host_requirements(&self, _host: &HostProfile) -> Requirements {
        let selected: Vec<usize> = self
            .enabled
            .iter()
            .enumerate()
            .filter_map(|(idx, on)| (*on).then_some(idx))
            .collect();
        if selected.is_empty() {
            return Requirements::NONE;
        }
        // When every selected sub-suite requires glibc (e.g. `--cpu-bench geekbench`), enforce
        // the minimum glibc across selected suites at preflight; similarly enforce IPv4 when
        // Geekbench is the sole selected suite.
        let glibc = if selected.iter().all(|idx| suite_glibc(*idx).is_some()) {
            selected.iter().filter_map(|idx| suite_glibc(*idx)).min()
        } else {
            None
        };
        let net = if selected == [7] {
            Net::Ipv4
        } else {
            Net::None
        };
        Requirements {
            tools: &ALL_CPU_TOOLS,
            net,
            platforms: &[],
            glibc,
            numa: false,
        }
    }

    fn tools_for(&self, host: &HostProfile) -> Vec<ToolSpec> {
        let mut out = Vec::new();
        for (idx, on) in self.enabled.iter().enumerate() {
            if !*on || Self::suite_precondition(idx, host).is_err() {
                continue;
            }
            match idx {
                0 => out.push(SYSBENCH),
                1 => out.push(SEVENZIP),
                2 => out.push(UNIXBENCH),
                3 => out.push(HPL),
                4 => {
                    out.push(PASSMARK);
                    out.push(PASSMARK_LIBS);
                }
                5 => out.push(YCRUNCHER),
                6 => out.push(PRIME95),
                7 => out.push(GEEKBENCH),
                _ => {}
            }
        }
        out
    }

    fn partial_tools(&self) -> bool {
        true
    }

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        if !self.enabled.iter().any(|on| *on) {
            return Err(Stop::Skipped(Failure::NoSuitesSelected));
        }
        let threads = crate::usable_threads(ctx.host());
        let avail_mem = crate::available_memory_bytes(ctx.host());
        let low_memory = avail_mem.is_some_and(|b| b < (1024 << 20));
        let mut output = Output {
            suites: self.custom_codes.clone(),
            ..Output::default()
        };

        // 0: sysbench
        if self.enabled[0] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.sysbench = Some(run_sysbench(ctx, threads, &output).await);
            ctx.publish(&output);
        }

        // 1: 7zip
        if self.enabled[1] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.seven_zip = Some(run_seven_zip(ctx, threads, low_memory, &output).await);
            ctx.publish(&output);
        }

        // 2: unixbench
        if self.enabled[2] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.unixbench = Some(run_unixbench(ctx, threads, &output).await);
            ctx.publish(&output);
        }

        // 3: hpl
        if self.enabled[3] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.hpl = Some(run_hpl(ctx, threads, avail_mem).await);
            ctx.publish(&output);
        }

        // 4: passmark
        if self.enabled[4] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.passmark = Some(run_passmark(ctx).await);
            ctx.publish(&output);
        }

        // 5: ycruncher
        if self.enabled[5] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.ycruncher = Some(run_ycruncher(ctx, threads, avail_mem).await);
            ctx.publish(&output);
        }

        // 6: prime95
        if self.enabled[6] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.prime95 = Some(run_prime95(ctx, threads).await);
            ctx.publish(&output);
        }

        // 7: geekbench
        if self.enabled[7] {
            if ctx.cancel_token().is_cancelled() {
                return Err(Stop::Failed(Failure::Cancelled));
            }
            output.geekbench = Some(run_geekbench(ctx, &output).await);
            ctx.publish(&output);
        }

        if ctx.cancel_token().is_cancelled() {
            return Err(Stop::Failed(Failure::Cancelled));
        }
        if output.completed_count() > 0 {
            Ok(output)
        } else if output.any_failed() {
            Err(Stop::Failed(Failure::AllFailed))
        } else {
            Err(Stop::Skipped(Failure::AllSkipped))
        }
    }
}

fn require_tool<'a, O: Measurement>(
    ctx: &'a Context<O>,
    spec: &ToolSpec,
) -> Result<&'a Path, SuiteSkip> {
    ctx.tool(spec.name)
        .ok_or_else(|| SuiteSkip::ToolUnavailable {
            tool: spec.name,
            why: ctx.missing_tool(spec.name).unwrap_or_default().to_owned(),
        })
}

async fn run_sysbench(
    ctx: &Context<Output>,
    threads: usize,
    base: &Output,
) -> SuiteOutcome<sysbench::Output, sysbench::Failure> {
    let program = match require_tool(ctx, &SYSBENCH) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let mut partial_out: Option<sysbench::Output> = None;
    let res = sysbench::measure::measure_suite(
        ctx,
        &program,
        threads,
        |partial| {
            partial_out = Some(partial.clone());
            let mut snap = base.clone();
            snap.sysbench = Some(SuiteOutcome::Done {
                output: partial.clone(),
            });
            ctx.publish(&snap);
        },
        |job| async move { job.run(ctx).await.ended },
    )
    .await;
    match res {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err(Stop::Failed(failure) | Stop::Skipped(failure)) => SuiteOutcome::Failed {
            partial: partial_out,
            failure,
        },
    }
}

async fn run_seven_zip(
    ctx: &Context<Output>,
    threads: usize,
    low_memory: bool,
    base: &Output,
) -> SuiteOutcome<seven_zip::Output, seven_zip::Failure> {
    let program = match require_tool(ctx, &SEVENZIP) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let res = seven_zip::measure::measure(
        ctx,
        &program,
        threads,
        low_memory,
        |partial| {
            let mut snap = base.clone();
            snap.seven_zip = Some(SuiteOutcome::Done {
                output: partial.clone(),
            });
            ctx.publish(&snap);
        },
        |job| async move { job.run(ctx).await.ended },
    )
    .await;
    match res {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err((partial, failure)) => SuiteOutcome::Failed {
            partial: partial.map(|out| *out),
            failure,
        },
    }
}

async fn run_unixbench(
    ctx: &Context<Output>,
    threads: usize,
    base: &Output,
) -> SuiteOutcome<unixbench::Output, unixbench::Failure> {
    let program = match require_tool(ctx, &UNIXBENCH) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let work_dir = ctx.work_dir().join("unixbench");
    let res = unixbench::measure::measure(
        ctx,
        &program,
        &work_dir,
        threads,
        |partial| {
            let mut snap = base.clone();
            snap.unixbench = Some(SuiteOutcome::Done {
                output: partial.clone(),
            });
            ctx.publish(&snap);
        },
        |job| async move { job.run(ctx).await.ended },
    )
    .await;
    match res {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err((partial, failure)) => SuiteOutcome::Failed {
            partial: partial.map(|out| *out),
            failure,
        },
    }
}

async fn run_hpl(
    ctx: &Context<Output>,
    threads: usize,
    avail_mem: Option<u64>,
) -> SuiteOutcome<hpl::Output, hpl::Failure> {
    let program = match require_tool(ctx, &HPL) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let work_dir = ctx.work_dir().join("hpl");
    match hpl::measure::measure(
        ctx,
        &program,
        &work_dir,
        threads,
        avail_mem,
        |job| async move { job.run(ctx).await.ended },
    )
    .await
    {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err(failure) => SuiteOutcome::Failed {
            partial: None,
            failure,
        },
    }
}

async fn run_passmark(ctx: &Context<Output>) -> SuiteOutcome<passmark::Output, passmark::Failure> {
    if let Err(skip) = CpuBench::suite_precondition(4, ctx.host()) {
        return SuiteOutcome::Skipped { reason: skip };
    }
    let program = match require_tool(ctx, &PASSMARK) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let libs_file = match require_tool(ctx, &PASSMARK_LIBS) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let work_dir = ctx.work_dir().join("passmark");
    match passmark::measure::measure(ctx, &program, &libs_file, &work_dir, |job| async move {
        job.run(ctx).await.ended
    })
    .await
    {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err(failure) => SuiteOutcome::Failed {
            partial: None,
            failure,
        },
    }
}

async fn run_ycruncher(
    ctx: &Context<Output>,
    threads: usize,
    avail_mem: Option<u64>,
) -> SuiteOutcome<ycruncher::Output, ycruncher::Failure> {
    if let Err(skip) = CpuBench::suite_precondition(5, ctx.host()) {
        return SuiteOutcome::Skipped { reason: skip };
    }
    let program = match require_tool(ctx, &YCRUNCHER) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let work_dir = ctx.work_dir().join("ycruncher");
    match ycruncher::measure::measure(
        ctx,
        &program,
        &work_dir,
        threads,
        avail_mem,
        |job| async move { job.run(ctx).await.ended },
    )
    .await
    {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err(failure) => SuiteOutcome::Failed {
            partial: None,
            failure,
        },
    }
}

async fn run_prime95(
    ctx: &Context<Output>,
    threads: usize,
) -> SuiteOutcome<prime95::Output, prime95::Failure> {
    if let Err(skip) = CpuBench::suite_precondition(6, ctx.host()) {
        return SuiteOutcome::Skipped { reason: skip };
    }
    let program = match require_tool(ctx, &PRIME95) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let work_dir = ctx.work_dir().join("prime95");
    let duration = Duration::from_secs(prime95::measure::TORTURE_SECONDS);
    match prime95::measure::measure(
        ctx,
        &program,
        &work_dir,
        threads,
        duration,
        |job| async move { job.run(ctx).await.ended },
    )
    .await
    {
        Ok(out) => SuiteOutcome::Done { output: out },
        Err(failure) => SuiteOutcome::Failed {
            partial: None,
            failure,
        },
    }
}

async fn run_geekbench(
    ctx: &Context<Output>,
    base: &Output,
) -> SuiteOutcome<geekbench::Output, geekbench::Failure> {
    if let Err(skip) = CpuBench::suite_precondition(7, ctx.host()) {
        return SuiteOutcome::Skipped { reason: skip };
    }
    let program = match require_tool(ctx, &GEEKBENCH) {
        Ok(path) => path.to_path_buf(),
        Err(skip) => return SuiteOutcome::Skipped { reason: skip },
    };
    let (output, result) = geekbench::run_in_suite(ctx, &program, |partial| {
        let mut snap = base.clone();
        snap.geekbench = Some(SuiteOutcome::Done {
            output: partial.clone(),
        });
        ctx.publish(&snap);
    })
    .await;
    match result {
        Ok(()) => SuiteOutcome::Done { output },
        Err(failure) => {
            let has_notes = output != geekbench::Output::default();
            SuiteOutcome::Failed {
                partial: has_notes.then_some(output),
                failure,
            }
        }
    }
}
