//! `memory.sysbench`: sysbench's sequential read and write bandwidth, with one thread and with
//! all the usable ones.

pub mod measure;
pub mod parse;
pub mod result;

use std::time::Duration;

use bench_api::Requirements;
use bench_api::typed::Context;
use bench_api::typed::Descriptor;
use bench_api::typed::Plugin;
use bench_api::typed::Stop;
use bench_tool::SYSBENCH;
use serde::Deserialize;

use crate::plan::Plan;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// sysbench's chapter of 内存性能. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sysbench {}

impl Plugin for Sysbench {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "memory.sysbench",
        requires: Requirements {
            tools: &[SYSBENCH],
            ..Requirements::NONE
        },
        // Four runs of up to four attempts of up to 60 seconds each.
        timeout: Duration::from_secs(20 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(SYSBENCH.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let plan = Plan::read(ctx.host().platform.kind()).await;
        measure::measure(ctx, &program, plan.as_ref(), |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}
