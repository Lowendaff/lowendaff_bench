//! `memory.latency`: multichase's latency in each cache level and in memory.

pub mod measure;
pub mod parse;
pub mod result;

use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_tool::MULTICHASE;
use serde::Deserialize;

use crate::plan::Plan;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// The latency chapter of 内存性能. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Latency {}

impl Plugin for Latency {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "memory.latency",
        requires: Requirements {
            tools: &[MULTICHASE],
            ..Requirements::NONE
        },
        // Four levels of up to four attempts of up to 60 seconds each.
        timeout: Duration::from_secs(20 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(MULTICHASE.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let plan = Plan::read(ctx.host().platform.kind()).await;
        measure::measure(ctx, &program, plan.as_ref(), |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}
