//! `memory.numa`: the bandwidth and latency from each NUMA node's CPUs to each node's memory,
//! on a host with at least two nodes that can be used.

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

/// The NUMA chapter of 内存性能. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Numa {}

impl Plugin for Numa {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "memory.numa",
        requires: Requirements {
            tools: &[MULTICHASE],
            numa: true,
            ..Requirements::NONE
        },
        // Enough for the matrix of up to 13 nodes, as the memory item had.
        timeout: Duration::from_secs(30 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let multichase = ctx
            .tool(MULTICHASE.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let plan = Plan::read(ctx.host().platform.kind()).await;
        measure::measure(ctx, &multichase, plan.as_ref(), |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}
