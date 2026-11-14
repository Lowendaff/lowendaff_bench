//! `memory.stream`: STREAM's Copy, Scale, Add and Triad, with one thread and with all the usable
//! ones.

pub mod measure;
pub mod parse;
pub mod result;

use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_tool::STREAM;
use serde::Deserialize;

use crate::plan::Plan;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// STREAM's chapter of 内存性能. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stream {}

impl Plugin for Stream {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "memory.stream",
        requires: Requirements {
            tools: &[STREAM],
            ..Requirements::NONE
        },
        // Two runs of up to four attempts of up to three minutes each.
        timeout: Duration::from_secs(30 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let smallest = ctx
            .tool(STREAM.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let plan = Plan::read(ctx.host().platform.kind()).await;
        measure::measure(ctx, &smallest, plan.as_ref(), |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}
