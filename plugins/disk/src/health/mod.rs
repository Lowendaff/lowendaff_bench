//! 硬盘健康: the SMART health of every physical disk
//! of a dedicated server, read with smartctl.

use std::future::Future;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Requirements;
use bench_host::Root;
use bench_host::platform::PlatformKind;
use bench_tool::SMARTCTL;
use serde::Deserialize;

use crate::health::nodes::Nodes;
use crate::health::reading::Health;
use crate::health::reading::Reading;

pub mod device;
pub mod location;
pub mod nodes;
pub mod output;
pub mod reading;
pub mod report;

/// 硬盘健康. It has no settings.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Smartctl {}

impl Plugin for Smartctl {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.health",
        title: "硬盘健康",
        version: "0.1.0",
        requires: Requirements {
            tools: &[SMARTCTL],
            platforms: &[PlatformKind::Dedicated],
            ..Requirements::NONE
        },
        // A read takes about 50 ms on the owner's server, but each may take 60 seconds.
        timeout: Duration::from_secs(5 * 60),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let smartctl = ctx.tool("smartctl")?.to_path_buf();
        check(ctx, &Root::system(), |args| run_smartctl(&smartctl, args)).await
    }
}

/// The item's flow, with smartctl injected: the item runs the pinned binary,
/// tests return fixtures. `smartctl` gets the arguments and returns the standard output.
/// Device nodes are looked up and removed under `root`.
pub async fn check<F, Fut>(ctx: &Context, root: &Root, smartctl: F) -> Result<(), PluginError>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: Future<Output = io::Result<String>>,
{
    // First, because the scan itself may create a node. Declared first, dropped last: after
    // every smartctl run has been stopped.
    let mut nodes = Nodes::note(root);
    let scan_args = device::SCAN_ARGS.map(str::to_owned).to_vec();
    let Some(scan) = limited(ctx, smartctl(scan_args)).await? else {
        let limit = reading::READ_LIMIT.as_secs();
        return Err(PluginError::fail(format!(
            "smartctl 扫描硬盘失败：超过 {limit} 秒仍未完成"
        )));
    };
    let devices = device::scan(&scan?)
        .map_err(|reason| PluginError::fail(format!("smartctl 扫描硬盘失败：{reason}")))?;
    if devices.is_empty() {
        return Err(PluginError::skip("smartctl 没有找到硬盘"));
    }

    let mut readings = Vec::new();
    for device in devices {
        let output = match limited(ctx, smartctl(device.read_args())).await? {
            Some(output) => Some(output?),
            None => None,
        };
        readings.push(Reading::new(device, output.as_deref()));
    }

    let rows = report::rows(readings, root);
    ctx.emit(report::table(&rows)?);
    for note in report::notes(&rows) {
        ctx.emit(note);
    }
    for (index, (path, error)) in nodes.remove_created().into_iter().enumerate() {
        let id = format!("note-node-{}", index + 1);
        let text = format!("无法删除 {}：{error}", path.display());
        ctx.emit(Record::fact(&id, "说明", text));
    }
    if rows
        .iter()
        .all(|row| matches!(row.reading.health, Health::ReadFailed(_)))
    {
        return Err(PluginError::fail("所有硬盘都读取失败"));
    }
    Ok(())
}

/// Runs `future` for at most [`reading::READ_LIMIT`]: `None` when it took longer. Ctrl-C
/// or the item's timeout ends it at once, which kills smartctl.
async fn limited<T>(
    ctx: &Context,
    future: impl Future<Output = T>,
) -> Result<Option<T>, PluginError> {
    let cancel = ctx.cancel_token();
    tokio::select! {
        result = tokio::time::timeout(reading::READ_LIMIT, future) => Ok(result.ok()),
        () = cancel.cancelled() => Err(PluginError::fail("已中断")),
    }
}

/// Runs the pinned smartctl without a shell; its exit status is in its JSON.
async fn run_smartctl(smartctl: &Path, args: Vec<String>) -> io::Result<String> {
    let output = tokio::process::Command::new(smartctl)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
