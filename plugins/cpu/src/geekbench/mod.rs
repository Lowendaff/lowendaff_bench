//! Geekbench 7: Primate Labs' own build, downloaded at run time because its license forbids
//! redistribution, and run as the free version, which uploads its result. Temporary swap helps
//! when memory is short, and the kernel kills Geekbench first when memory runs out anyway. Only
//! a failed upload is run again: it published nothing, and its cause is usually the network.

pub mod measure;
pub mod memory;
pub mod parse;
pub mod result;

use std::io;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Measurement;
use bench_api::Net;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Step;
use bench_api::Stop;
use bench_host::Root;
use bench_host::glibc::GlibcVersion;
use bench_tool::GEEKBENCH;
use serde::Deserialize;
use swap_file::SwapFile;

use measure::Ran;
use memory::Plan;
use result::Memory;
use result::NoSwap;
use result::SwapLeft;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// Minimum glibc version required by Geekbench 7.1.0.
pub const MIN_GLIBC: GlibcVersion = GlibcVersion {
    major: 2,
    minor: 35,
};

/// Geekbench 7. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Geekbench {}

impl Plugin for Geekbench {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "cpu.geekbench",
        requires: Requirements {
            tools: &[GEEKBENCH],
            // cdn.geekbench.com has IPv4 addresses only.
            net: Net::Ipv4,
            platforms: &[],
            glibc: Some(MIN_GLIBC),
            numa: false,
        },
        // Up to four runs of up to an hour each, and the swap.
        timeout: Duration::from_secs(250 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(GEEKBENCH.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let (output, result) = run_with(ctx, &program, false, |partial| ctx.publish(partial)).await;
        match result {
            Ok(()) => Ok(output),
            Err(failure) => {
                ctx.publish(&output);
                Err(Stop::Failed(failure))
            }
        }
    }
}

/// Runs Geekbench 7 inside `cpu.bench`, returning the [`Output`] collected so far alongside the
/// run result.
pub async fn run_in_suite<O, P>(
    ctx: &Context<O>,
    program: &Path,
    on_partial: P,
) -> (Output, Result<(), Failure>)
where
    O: Measurement,
    P: FnMut(&Output),
{
    ctx.progress(Step::CpuSuite {
        suite: "geekbench",
        stage: None,
        threads: None,
    });
    run_with(ctx, program, true, on_partial).await
}

async fn run_with<O, P>(
    ctx: &Context<O>,
    program: &Path,
    in_subdir: bool,
    mut on_partial: P,
) -> (Output, Result<(), Failure>)
where
    O: Measurement,
    P: FnMut(&Output),
{
    let threads = crate::usable_threads(ctx.host());
    let kind = ctx.host().platform.kind();
    let work_dir = if in_subdir {
        let dir = ctx.work_dir().join("geekbench");
        let _ = tokio::fs::create_dir_all(&dir).await;
        dir
    } else {
        ctx.work_dir().to_path_buf()
    };
    let probe_dir = work_dir.clone();
    let host = tokio::task::spawn_blocking(move || {
        memory::Host::read(&Root::system(), threads, kind, &probe_dir)
    })
    .await
    .unwrap_or(memory::Host {
        threads,
        available: None,
        free_space: None,
        swap_unused: false,
    });
    let mut output = Output::default();
    let swap = match prepare(ctx, &host, &work_dir, &mut output).await {
        Ok(swap) => swap,
        Err(failure) => return (output, Err(failure)),
    };
    on_partial(&output);
    let root = Root::system();
    let (mut output, result) =
        measure::measure_in(ctx, output, program, &work_dir, &mut on_partial, |job| {
            let root = &root;
            async move {
                let before = memory::oom_kills(root);
                let outcome = job.run(ctx).await;
                let after = memory::oom_kills(root);
                let killed =
                    matches!((before, after), (Some(before), Some(after)) if after > before);
                Ran { outcome, killed }
            }
        })
        .await;
    if let Some(swap) = swap {
        output.swap_left = remove_swap(swap).await;
    }
    (output, result)
}

/// Adds temporary swap when Geekbench's estimated need exceeds what the host has, and notes in
/// `output` whenever memory may be short.
async fn prepare<O: Measurement>(
    ctx: &Context<O>,
    host: &memory::Host,
    work_dir: &Path,
    output: &mut Output,
) -> Result<Option<SwapFile>, Failure> {
    let Some(available) = host.available else {
        return Ok(None);
    };
    let need = memory::need(host.threads);
    let short = |why| Memory::Short {
        need,
        available,
        why,
    };
    let gib = match memory::plan(need, available, host.free_space, host.swap_unused) {
        Plan::Enough => return Ok(None),
        Plan::Unused => {
            output.memory = Some(short(NoSwap::Unused));
            return Ok(None);
        }
        Plan::NoRoom => {
            output.memory = Some(short(NoSwap::NoRoom(host.free_space)));
            return Ok(None);
        }
        Plan::Swap(gib) => gib,
    };
    let path = work_dir.join("swapfile");
    let cancel = ctx.cancel_token();
    let created = tokio::task::spawn_blocking(move || {
        SwapFile::try_new(&path, gib * memory::GIB, &|| cancel.is_cancelled())
    })
    .await
    .unwrap_or_else(|err| Err(io::Error::other(err.to_string())));
    match created {
        Ok(swap) => {
            output.memory = Some(Memory::Swapped {
                need,
                available,
                gib,
            });
            Ok(Some(swap))
        }
        Err(err) if err.kind() == io::ErrorKind::Interrupted => Err(Failure::Cancelled),
        Err(err) => {
            output.memory = Some(short(measure::no_swap(&err)));
            Ok(None)
        }
    }
}

/// Turns the swap off and removes it; what to tell the user when it cannot.
async fn remove_swap(swap: SwapFile) -> Option<SwapLeft> {
    let path = swap.path().to_path_buf();
    let removed = tokio::task::spawn_blocking(move || swap.remove())
        .await
        .unwrap_or_else(|err| Err(io::Error::other(err.to_string())));
    removed.err().map(|err| SwapLeft {
        path,
        error: err.to_string(),
    })
}
