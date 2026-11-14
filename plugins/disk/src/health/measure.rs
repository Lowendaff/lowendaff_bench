//! The measurement: smartctl's scan, then each disk read in the scan's order, the result
//! published after each read; a disk listed twice, directly and through a RAID card, shows once.

use std::future::Future;
use std::path::Path;
use std::time::Duration;

use bench_api::Context;
use bench_api::Stop;
use bench_host::Root;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;

use super::device::Device;
use super::device::SCAN_ARGS;
use super::location::Location;
use super::nodes::Nodes;
use super::parse;
use super::parse::Reading;
use super::result::Disk;
use super::result::Failure;
use super::result::Fault;
use super::result::Health;
use super::result::NodeLeft;
use super::result::Output;

/// How long one smartctl run may take before it is stopped.
pub const READ_LIMIT: Duration = Duration::from_secs(60);

/// The scan of `program`. Like every run of smartctl here, the log never quotes its output,
/// which holds the disks' serial numbers and WWNs.
#[must_use]
pub fn scan(program: &Path) -> Job {
    let mut job = Job::new(program, SCAN_ARGS, READ_LIMIT);
    job.quotable = false;
    job
}

/// The read of `device` by `program`.
#[must_use]
pub fn read(program: &Path, device: &Device) -> Job {
    let mut job = Job::new(program, device.read_args(), READ_LIMIT);
    job.quotable = false;
    job
}

/// Scans with `program` and reads every disk found. Device nodes that smartctl creates are
/// noted in `root` and removed on every way out. `run` runs a job: the shared runner, or a
/// recording in tests.
pub async fn measure<F, Fut>(
    ctx: &Context<Output>,
    program: &Path,
    root: &Root,
    run: F,
) -> Result<Output, Stop<Failure>>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    // First, because the scan itself may create a node. Declared first, dropped last: after
    // every smartctl run has been stopped.
    let mut nodes = Nodes::note(root);
    let scanned = match run(scan(program)).await {
        Ok(output) => parse::scan(&output.text),
        Err(RunError::Fault(fault)) => Err(Fault::Tool(fault)),
        Err(RunError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
    };
    let (version, devices) = scanned.map_err(|fault| Stop::Failed(Failure::Scan(fault)))?;
    if devices.is_empty() {
        return Err(Stop::Skipped(Failure::NoDisks));
    }
    let mut output = Output {
        version,
        ..Output::default()
    };
    let mut readings = Vec::new();
    for device in devices {
        let reading = match run(read(program, &device)).await {
            Ok(text) => Reading::new(device, &text.text),
            Err(RunError::Fault(fault)) => Reading::failed(device, Fault::Tool(fault)),
            Err(RunError::Cancelled) => return Err(Stop::Failed(Failure::Cancelled)),
        };
        readings.push(reading);
        output.disks = disks(&readings, root);
        ctx.publish(&output);
    }
    output.nodes_left = nodes
        .remove_created()
        .into_iter()
        .map(|(path, error)| NodeLeft {
            path,
            error: error.to_string(),
        })
        .collect();
    let every_read_failed = output
        .disks
        .iter()
        .all(|disk| matches!(disk.health, Health::ReadFailed(_)));
    if every_read_failed {
        ctx.publish(&output);
        return Err(Stop::Failed(Failure::AllFailed));
    }
    Ok(output)
}

/// One entry per physical disk, in the table's order. Readings with the same serial are one
/// disk listed twice: a named device is kept over a RAID-card path, otherwise the first in scan
/// order. Readings without a serial are never merged.
#[must_use]
pub fn disks(readings: &[Reading], root: &Root) -> Vec<Disk> {
    let mut kept: Vec<(Location, &Reading)> = Vec::new();
    for reading in readings {
        let location = Location::of(&reading.device, root);
        let same = |(_, other): &(Location, &Reading)| {
            other.serial.is_some() && other.serial == reading.serial
        };
        match kept.iter().position(same) {
            Some(index) => {
                let named = matches!(location, Location::Named(_));
                if named && matches!(kept[index].0, Location::Card { .. }) {
                    kept[index] = (location, reading);
                }
            }
            None => kept.push((location, reading)),
        }
    }
    // Stable, so disks with the same place keep the scan's order.
    kept.sort_by(|a, b| a.0.cmp(&b.0));
    kept.into_iter()
        .map(|(location, reading)| Disk {
            location,
            health: reading.health.clone(),
            values: reading.values.clone(),
        })
        .collect()
}
