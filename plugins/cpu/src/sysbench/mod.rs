//! CPU 性能: sysbench's CPU test with one thread and with every usable thread, as the owner's
//! `linux_bench` runs it, each run tried up to four times.

pub mod measure;
pub mod parse;
pub mod result;

use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_tool::SYSBENCH;
use serde::Deserialize;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// CPU 性能. It has no settings: every parameter is `linux_bench`'s.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sysbench {}

impl Plugin for Sysbench {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "cpu.sysbench",
        requires: Requirements {
            tools: &[SYSBENCH],
            ..Requirements::NONE
        },
        // Two runs of up to four attempts of up to 60 seconds each.
        timeout: Duration::from_secs(10 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(SYSBENCH.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let threads = crate::usable_threads(ctx.host());
        measure::measure(ctx, &program, threads, |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}
