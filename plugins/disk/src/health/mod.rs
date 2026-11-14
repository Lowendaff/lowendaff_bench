//! 硬盘健康: the SMART health of every physical disk of a dedicated server, read with smartctl.

pub mod device;
pub mod location;
pub mod measure;
pub mod nodes;
pub mod parse;
pub mod result;

use std::time::Duration;

use bench_api::Requirements;
use bench_api::typed::Context;
use bench_api::typed::Descriptor;
use bench_api::typed::Plugin;
use bench_api::typed::Stop;
use bench_host::Root;
use bench_host::platform::PlatformKind;
use bench_tool::SMARTCTL;
use serde::Deserialize;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// 硬盘健康. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Smartctl {}

impl Plugin for Smartctl {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.health",
        requires: Requirements {
            tools: &[SMARTCTL],
            platforms: &[PlatformKind::Dedicated],
            ..Requirements::NONE
        },
        // A read takes about 50 ms on the owner's server, but each may take 60 seconds.
        timeout: Duration::from_secs(5 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(SMARTCTL.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let root = Root::system();
        measure::measure(ctx, &program, &root, |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}
