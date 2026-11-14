//! The measurement with recorded runs instead of smartctl: the scan and the reads, what it
//! returns and publishes, a disk listed twice, the failures, a cancellation, and the device
//! nodes that smartctl creates.

mod common;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Channels;
use bench_api::Context;
use bench_api::Stop;
use bench_host::Root;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use plugin_disk::health::Failure;
use plugin_disk::health::Fault;
use plugin_disk::health::Output;
use plugin_disk::health::device::Device;
use plugin_disk::health::location::Location;
use plugin_disk::health::measure;
use plugin_disk::health::parse::Reading;
use plugin_disk::health::result::BadBlocks;
use plugin_disk::health::result::Health;
use plugin_disk::health::result::Values;
use tokio::time::Instant;

const SMARTCTL: &str = "/tools/smartctl";

const NODE: &str = "dev/megaraid_sas_ioctl_node";

struct Outcome {
    result: Result<Output, Stop<Failure>>,
    published: Option<Output>,
}

fn context(cancel: &CancellationToken) -> (Context<Output>, Channels<Output>) {
    let deadline = Instant::now() + Duration::from_secs(300);
    Context::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        deadline,
        ".".into(),
        cancel.clone(),
    )
}

/// Measures against `root`, with `run` instead of smartctl.
async fn measure<F, Fut>(root: &Path, run: F) -> Outcome
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = Result<job::Output, RunError>>,
{
    let (ctx, channels) = context(&CancellationToken::new());
    let result = measure::measure(&ctx, Path::new(SMARTCTL), &Root::new(root), run).await;
    let published = channels.published.borrow().clone();
    Outcome { result, published }
}

/// The arguments of a job, as strings.
fn args(job: &Job) -> Vec<String> {
    job.args
        .iter()
        .map(|arg| {
            arg.to_str()
                .expect("smartctl's arguments are UTF-8")
                .to_owned()
        })
        .collect()
}

fn scan_args() -> Vec<String> {
    ["--scan-open", "--json"].map(str::to_owned).to_vec()
}

fn printed(text: String) -> Result<job::Output, RunError> {
    Ok(job::Output {
        code: Some(0),
        text,
    })
}

/// The owner's fixtures: the scan, and each device by its type and name.
fn owners_answer(args: &[String]) -> String {
    if args == scan_args() {
        return common::fixture("owner/scan.json");
    }
    let [_, _, _, kind, name] = args else {
        panic!("a read has five arguments: {args:?}");
    };
    match kind.strip_prefix("sat+megaraid,") {
        Some(id) => common::fixture(&format!("owner/megaraid-{id}.json")),
        None => common::fixture(&format!("owner/{}.json", name.trim_start_matches("/dev/"))),
    }
}

/// A scan that lists `devices` as (name, type) pairs.
fn scan_of(devices: &[(&str, &str)]) -> String {
    let mut listed = Vec::new();
    for (name, kind) in devices {
        listed.push(format!(
            r#"{{"name": "{name}", "type": "{kind}", "protocol": "ATA"}}"#
        ));
    }
    let devices = listed.join(", ");
    let head = r#""json_format_version": [1, 0], "smartctl": {"exit_status": 0}"#;
    format!(r#"{{{head}, "devices": [{devices}]}}"#)
}

fn root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::create_dir(dir.path().join("dev")).expect("dev is writable");
    dir
}

fn named(name: &str) -> Location {
    Location::Named(name.to_owned())
}

fn locations(output: &Output) -> Vec<Location> {
    output
        .disks
        .iter()
        .map(|disk| disk.location.clone())
        .collect()
}

#[tokio::test]
async fn the_owners_server_reads_each_device_once_in_scan_order_and_shows_four_disks() {
    let dir = root();
    let jobs = RefCell::new(Vec::new());
    let outcome = measure(dir.path(), |job: Job| {
        let answer = owners_answer(&args(&job));
        jobs.borrow_mut().push(job);
        async move { printed(answer) }
    })
    .await;
    let Ok(output) = &outcome.result else {
        panic!("the item completes: {:?}", outcome.result);
    };

    let jobs = jobs.into_inner();
    assert_eq!(
        jobs[0],
        measure::scan(Path::new(SMARTCTL)),
        "the scan comes first"
    );
    let reads: Vec<String> = jobs[1..]
        .iter()
        .map(|job| args(job)[3..].join(" "))
        .collect();
    assert_eq!(
        reads,
        [
            "sat /dev/sda",
            "sat /dev/sdb",
            "sat /dev/sdc",
            "sat /dev/sdd",
            "sat+megaraid,18 /dev/bus/0",
            "sat+megaraid,21 /dev/bus/0",
            "sat+megaraid,23 /dev/bus/0",
            "sat+megaraid,24 /dev/bus/0",
        ]
    );
    for job in &jobs {
        assert_eq!(job.limit, Duration::from_secs(60));
        assert!(
            !job.quotable,
            "smartctl's output holds serials, so the log never quotes it"
        );
    }
    assert!(
        jobs[1..]
            .iter()
            .all(|job| args(job)[..3] == ["--json", "-x", "-d"])
    );

    assert_eq!(output.version.as_deref(), Some("7.5"));
    assert_eq!(
        locations(output),
        [named("sda"), named("sdb"), named("sdc"), named("sdd")],
        "the RAID card's four paths merge into sda to sdd"
    );
    assert!(output.nodes_left.is_empty());
    assert_eq!(outcome.published.as_ref(), Some(output));
}

#[tokio::test]
async fn the_result_is_published_after_each_read() {
    let dir = root();
    let (ctx, channels) = context(&CancellationToken::new());
    let seen = RefCell::new(Vec::new());
    let scan = scan_of(&[
        ("/dev/sda", "sat"),
        ("/dev/sdb", "sat"),
        ("/dev/sdc", "sat"),
    ]);
    let result = measure::measure(&ctx, Path::new(SMARTCTL), &Root::new(dir.path()), |job| {
        let published = channels
            .published
            .borrow()
            .as_ref()
            .map(|output| output.disks.len());
        seen.borrow_mut().push(published);
        let answer = if args(&job) == scan_args() {
            scan.clone()
        } else {
            common::fixture("owner/sda.json")
        };
        async move { printed(answer) }
    })
    .await;
    assert!(result.is_ok(), "{result:?}");
    // One disk, listed three times with the same serial: one entry from the first read on.
    assert_eq!(seen.into_inner(), [None, None, Some(1), Some(1)]);
}

#[tokio::test]
async fn a_disk_that_cannot_be_read_is_marked_and_the_others_are_read() {
    let dir = root();
    let timed_out = job::Fault::TimedOut(Duration::from_secs(60));
    let outcome = measure(dir.path(), |job: Job| {
        let args = args(&job);
        let answer = match args.last().map(String::as_str) {
            Some("/dev/sdb") => Err(RunError::Fault(timed_out.clone())),
            Some("/dev/sdc") => Err(RunError::Fault(job::Fault::Spawn(
                "Text file busy (os error 26)".to_owned(),
            ))),
            _ => printed(owners_answer(&args)),
        };
        async move { answer }
    })
    .await;
    let Ok(output) = &outcome.result else {
        panic!("the item completes: {:?}", outcome.result);
    };
    // Without sdb's and sdc's serials, their paths through the card cannot be merged.
    assert_eq!(
        locations(output),
        [
            named("sda"),
            named("sdb"),
            named("sdc"),
            named("sdd"),
            Location::Card {
                host: 0,
                numbers: vec![21],
            },
            Location::Card {
                host: 0,
                numbers: vec![23],
            },
        ]
    );
    assert_eq!(
        output.disks[1].health,
        Health::ReadFailed(Fault::Tool(timed_out))
    );
    assert_eq!(
        output.disks[2].health,
        Health::ReadFailed(Fault::Tool(job::Fault::Spawn(
            "Text file busy (os error 26)".to_owned()
        )))
    );
    assert_eq!(output.disks[2].values, Values::default());
}

#[tokio::test]
async fn a_scan_that_times_out_fails_the_item() {
    let dir = root();
    let limit = job::Fault::TimedOut(Duration::from_secs(60));
    let outcome = measure(dir.path(), |_job: Job| {
        let answer = Err(RunError::Fault(limit.clone()));
        async move { answer }
    })
    .await;
    assert_eq!(
        outcome.result,
        Err(Stop::Failed(Failure::Scan(Fault::Tool(limit.clone()))))
    );
    assert_eq!(outcome.published, None);
}

#[tokio::test]
async fn a_scan_without_disks_skips_the_item() {
    let dir = root();
    let outcome = measure(dir.path(), |_job: Job| async {
        printed(common::fixture("local/scan-empty.json"))
    })
    .await;
    assert_eq!(outcome.result, Err(Stop::Skipped(Failure::NoDisks)));
    assert_eq!(outcome.published, None);
}

#[tokio::test]
async fn a_failed_scan_fails_the_item_with_smartctls_reason() {
    let dir = root();
    let outcome = measure(dir.path(), |_job: Job| async {
        printed(common::fixture("local/open-failed.json"))
    })
    .await;
    let reason = "Smartctl open device: /dev/sdz [SAT] failed: No such device";
    assert_eq!(
        outcome.result,
        Err(Stop::Failed(Failure::Scan(Fault::Message(
            reason.to_owned()
        ))))
    );
}

#[tokio::test]
async fn when_every_disk_fails_the_published_result_still_shows_why() {
    let dir = root();
    let scan = scan_of(&[("/dev/sda", "sat"), ("/dev/sdb", "sat")]);
    let outcome = measure(dir.path(), |job: Job| {
        let answer = if args(&job) == scan_args() {
            scan.clone()
        } else {
            common::fixture("local/open-failed.json")
        };
        async move { printed(answer) }
    })
    .await;
    assert_eq!(outcome.result, Err(Stop::Failed(Failure::AllFailed)));
    let published = outcome.published.expect("the disks were published");
    assert_eq!(locations(&published), [named("sda"), named("sdb")]);
    let reason = "Smartctl open device: /dev/sdz [SAT] failed: No such device";
    for disk in &published.disks {
        assert_eq!(
            disk.health,
            Health::ReadFailed(Fault::Message(reason.to_owned()))
        );
    }
}

#[tokio::test]
async fn when_every_disk_fails_the_published_result_names_a_node_left_behind() {
    let dir = root();
    let node = dir.path().join(NODE);
    let scan = scan_of(&[("/dev/sda", "sat")]);
    let outcome = measure(dir.path(), |job: Job| {
        // A non-empty directory where the node goes cannot be removed as a file.
        fs::create_dir_all(node.join("inside")).expect("dev is writable");
        let answer = if args(&job) == scan_args() {
            scan.clone()
        } else {
            common::fixture("local/open-failed.json")
        };
        async move { printed(answer) }
    })
    .await;
    assert_eq!(outcome.result, Err(Stop::Failed(Failure::AllFailed)));
    let published = outcome.published.expect("the disk was published");
    assert_eq!(published.nodes_left.len(), 1, "{:?}", published.nodes_left);
    assert_eq!(published.nodes_left[0].path, node);
}

#[tokio::test]
async fn disks_without_smart_complete_the_item() {
    let dir = root();
    let scan = scan_of(&[("/dev/sda", "scsi")]);
    let unsupported = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 0},
        "device": {"protocol": "SCSI"}, "smart_support": {"available": false}}"#;
    let outcome = measure(dir.path(), |job: Job| {
        let answer = if args(&job) == scan_args() {
            scan.clone()
        } else {
            unsupported.to_owned()
        };
        async move { printed(answer) }
    })
    .await;
    let Ok(output) = outcome.result else {
        panic!("the item completes: {:?}", outcome.result);
    };
    assert_eq!(output.disks[0].health, Health::Unsupported);
}

#[tokio::test]
async fn a_cancellation_stops_at_once_and_still_removes_a_created_node() {
    let dir = root();
    let outcome = measure(dir.path(), |job: Job| {
        let is_scan = args(&job) == scan_args();
        let answer = if is_scan {
            // The scan creates the node, as smartctl does on a MegaRAID host.
            fs::write(dir.path().join(NODE), "").expect("dev is writable");
            printed(owners_answer(&args(&job)))
        } else {
            Err(RunError::Cancelled)
        };
        async move { answer }
    })
    .await;
    assert_eq!(outcome.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(outcome.published, None, "no disk was read");
    assert!(!dir.path().join(NODE).exists());
}

#[tokio::test]
async fn a_cancellation_during_the_scan_ends_the_item_as_cancelled() {
    let dir = root();
    let outcome = measure(dir.path(), |_: Job| async { Err(RunError::Cancelled) }).await;
    assert_eq!(outcome.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(outcome.published, None);
}

#[tokio::test]
async fn a_node_created_during_the_item_is_removed_and_one_present_before_is_kept() {
    let dir = root();
    let old = dir.path().join("dev/megadev0");
    fs::write(&old, "").expect("dev is writable");
    let outcome = measure(dir.path(), |job: Job| {
        fs::write(dir.path().join(NODE), "").expect("dev is writable");
        let answer = owners_answer(&args(&job));
        async move { printed(answer) }
    })
    .await;
    let Ok(output) = outcome.result else {
        panic!("the item completes: {:?}", outcome.result);
    };
    assert!(output.nodes_left.is_empty());
    assert!(!dir.path().join(NODE).exists());
    assert!(old.exists(), "it was there before the item");
}

#[tokio::test]
async fn a_node_that_cannot_be_removed_is_kept_in_the_result() {
    let dir = root();
    let node = dir.path().join(NODE);
    let outcome = measure(dir.path(), |job: Job| {
        // A non-empty directory where the node goes cannot be removed as a file.
        fs::create_dir_all(node.join("inside")).expect("dev is writable");
        let answer = owners_answer(&args(&job));
        async move { printed(answer) }
    })
    .await;
    let Ok(output) = outcome.result else {
        panic!("the item completes: {:?}", outcome.result);
    };
    assert_eq!(output.nodes_left.len(), 1, "{:?}", output.nodes_left);
    assert_eq!(output.nodes_left[0].path, node);
    assert!(
        !output.nodes_left[0].error.is_empty(),
        "the system's reason"
    );
}

#[tokio::test]
async fn the_runners_nvme_disk_shows_as_its_namespace() {
    let dir = root();
    // The runner's layout: one namespace, although the controller reports 127 as its maximum.
    fs::create_dir_all(dir.path().join("sys/class/nvme/nvme0/nvme0n1")).expect("writable");
    let outcome = measure(dir.path(), |job: Job| {
        let file = if args(&job) == scan_args() {
            "runner/scan-nvme.json"
        } else {
            "runner/nvme0.json"
        };
        let answer = common::fixture(file);
        async move { printed(answer) }
    })
    .await;
    let Ok(output) = outcome.result else {
        panic!("the item completes: {:?}", outcome.result);
    };
    assert_eq!(locations(&output), [named("nvme0n1")]);
    assert_eq!(output.disks[0].health, Health::Passed);
    assert_eq!(
        output.disks[0].values,
        Values {
            hours: Some(0),
            temperature: Some(50),
            endurance_used: Some(0),
            written: Some(0),
            bad_blocks: BadBlocks::Count(Some(0)),
            interface_errors: None,
        }
    );
}

/// A reading made up for one test.
fn made_up(name: &str, kind: &str, serial: Option<&str>, health: Health) -> Reading {
    Reading {
        device: Device {
            name: name.to_owned(),
            kind: kind.to_owned(),
        },
        serial: serial.map(str::to_owned),
        health,
        values: Values::default(),
    }
}

fn card(host: u32, number: u32) -> Location {
    Location::Card {
        host,
        numbers: vec![number],
    }
}

fn nowhere() -> Root {
    Root::new("/nonexistent")
}

#[test]
fn a_named_device_is_kept_over_a_raid_card_path_whichever_comes_first() {
    let readings = [
        made_up("/dev/bus/0", "sat+megaraid,18", Some("A"), Health::Failed),
        made_up("/dev/sda", "sat", Some("A"), Health::Passed),
    ];
    let disks = measure::disks(&readings, &nowhere());
    assert_eq!(disks.len(), 1);
    assert_eq!(disks[0].location, named("sda"));
    assert_eq!(disks[0].health, Health::Passed);
}

#[test]
fn among_raid_card_paths_and_among_named_devices_the_first_is_kept() {
    let readings = [
        made_up("/dev/bus/0", "megaraid,8", Some("A"), Health::Passed),
        made_up("/dev/bus/1", "megaraid,9", Some("A"), Health::Failed),
        made_up("/dev/sdb", "sat", Some("B"), Health::Passed),
        made_up("/dev/sda", "sat", Some("B"), Health::Failed),
    ];
    let disks = measure::disks(&readings, &nowhere());
    let kept: Vec<Location> = disks.into_iter().map(|disk| disk.location).collect();
    assert_eq!(kept, [named("sdb"), card(0, 8)]);
}

#[test]
fn readings_without_a_serial_are_never_merged() {
    let failed = || Health::ReadFailed(Fault::Unparsable);
    let readings = [
        made_up("/dev/sda", "sat", None, failed()),
        made_up("/dev/bus/0", "sat+megaraid,18", None, failed()),
    ];
    assert_eq!(measure::disks(&readings, &nowhere()).len(), 2);
}

#[test]
fn the_disks_left_after_merging_come_in_the_tables_order() {
    // The second card's only disk is sdb, so one card remains.
    let readings = [
        made_up("/dev/sdb", "sat", Some("B"), Health::Passed),
        made_up("/dev/bus/0", "megaraid,8", Some("A"), Health::Passed),
        made_up("/dev/bus/1", "megaraid,9", Some("B"), Health::Passed),
        made_up("/dev/sda", "sat", Some("C"), Health::Passed),
    ];
    let disks = measure::disks(&readings, &nowhere());
    let kept: Vec<Location> = disks.into_iter().map(|disk| disk.location).collect();
    assert_eq!(kept, [named("sda"), named("sdb"), card(0, 8)]);
}
