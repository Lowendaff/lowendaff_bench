//! 系统基础信息 (design `docs/design/system-info.md`): one plugin per kind of platform.
//! They share everything except the parts that differ between dedicated servers,
//! virtual machines and containers.

use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::PlatformKind;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Requirements;
use bench_host::Root;
use bench_host::storage;
use serde::Deserialize;

pub mod hardware;
pub mod report;

/// The title all three variants share, so only one 系统基础信息 section appears.
const TITLE: &str = "系统基础信息";

const fn descriptor(id: &'static str, platforms: &'static [PlatformKind]) -> Descriptor {
    Descriptor {
        id,
        title: TITLE,
        version: "0.1.0",
        requires: Requirements {
            platforms,
            ..Requirements::NONE
        },
        timeout: Duration::from_secs(30),
    }
}

/// 系统基础信息 on a machine without detected virtualization.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dedicated {}

impl Plugin for Dedicated {
    const DESCRIPTOR: Descriptor = descriptor("system.dedicated", &[PlatformKind::Dedicated]);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        emit(ctx, PlatformKind::Dedicated).await
    }
}

/// 系统基础信息 in a virtual machine.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vm {}

impl Plugin for Vm {
    const DESCRIPTOR: Descriptor = descriptor("system.vm", &[PlatformKind::Vm]);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        emit(ctx, PlatformKind::Vm).await
    }
}

/// 系统基础信息 in a container.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Container {}

impl Plugin for Container {
    const DESCRIPTOR: Descriptor = descriptor("system.container", &[PlatformKind::Container]);

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        emit(ctx, PlatformKind::Container).await
    }
}

/// Reads the host off the async executor, then emits the records.
async fn emit(ctx: &Context, kind: PlatformKind) -> Result<(), PluginError> {
    let env = ctx.env().clone();
    let work_dir = ctx.work_dir().to_path_buf();
    let records = tokio::task::spawn_blocking(move || {
        let host = report::Host {
            root: Root::system(),
            env,
            work_dir: storage::device_of(Path::new(&work_dir)),
            space: storage::statvfs,
        };
        report::records(&host, kind)
    })
    .await??;
    for record in records {
        ctx.emit(record);
    }
    Ok(())
}
