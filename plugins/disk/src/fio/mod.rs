//! 硬盘性能: fio runs YABS's disk test on the filesystem of the test directory, on every platform.

pub mod measure;
pub mod parse;
pub mod result;

use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::Requirements;
use bench_api::Stop;
use bench_host::Root;
use bench_host::storage;
use bench_tool::FIO;
use serde::Deserialize;

use measure::Host;

pub use result::Failure;
pub use result::Fault;
pub use result::Output;

/// 硬盘性能. It has no settings: every parameter is YABS's.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fio {}

impl Plugin for Fio {
    type Output = Output;
    type Failure = Failure;
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.fio",
        requires: Requirements {
            tools: &[FIO],
            ..Requirements::NONE
        },
        // Room to lay out 2 GiB on a slow disk, plus four tests of up to 35 seconds.
        timeout: Duration::from_secs(10 * 60),
    };

    async fn run(&self, ctx: &Context<Output>) -> Result<Output, Stop<Failure>> {
        let program = ctx
            .tool(FIO.name)
            .expect("the orchestrator runs a plugin only when its tools are ready")
            .to_path_buf();
        let dir = ctx.work_dir().to_path_buf();
        let host = tokio::task::spawn_blocking(move || read_host(&dir))
            .await
            .expect("reading the filesystem does not panic");
        let file = ctx.work_dir().join("test.fio");
        measure::measure(ctx, &program, &file, &host, |job| async move {
            job.run(ctx).await.ended
        })
        .await
    }
}

/// The filesystem of the test directory, read when the item starts: its free space changes.
fn read_host(dir: &Path) -> Host {
    Host {
        kind: storage::filesystem_kind(dir),
        available: dir
            .to_str()
            .and_then(storage::statvfs)
            .map(|space| space.available),
        inflation: Root::system()
            .read_line("/sys/module/zfs/parameters/spa_asize_inflation")
            .and_then(|line| line.parse().ok()),
    }
}
