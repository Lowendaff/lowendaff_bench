//! The item's flow with a fake smartctl (design disk-health, §5.2, §5.6, §6).

mod common;

use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Env;
use bench_api::PluginError;
use bench_api::Record;
use bench_host::Root;
use tokio::time::Instant;

const NODE: &str = "dev/megaraid_sas_ioctl_node";

/// Runs the item against `root` with `smartctl`, and returns its result and its records.
async fn check<F, Fut>(root: &Path, cancel: &CancellationToken, smartctl: F) -> Outcome
where
    F: Fn(Vec<String>) -> Fut,
    Fut: Future<Output = io::Result<String>>,
{
    let work = tempfile::tempdir().expect("a temporary directory");
    let deadline = Instant::now() + Duration::from_secs(300);
    let env = Arc::new(Env::default());
    let (ctx, mut receiver) = Context::new(env, deadline, work.path().into(), cancel.clone());
    let result = plugin_disk_health::check(&ctx, &Root::new(root), smartctl).await;
    let mut records = Vec::new();
    while let Ok(record) = receiver.try_recv() {
        records.push(record);
    }
    Outcome { result, records }
}

struct Outcome {
    result: Result<(), PluginError>,
    records: Vec<Record>,
}

impl Outcome {
    fn table(&self) -> Vec<Vec<String>> {
        let Some(Record::Table(table)) = self.records.first() else {
            panic!("the first record is the table: {:?}", self.records);
        };
        table.rows().to_vec()
    }

    fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        for record in &self.records {
            if let Record::Fact { label, value, .. } = record {
                assert_eq!(label, "说明");
                notes.push(value.clone());
            }
        }
        notes
    }
}

fn scan_args() -> Vec<String> {
    ["--scan-open", "--json"].map(str::to_owned).to_vec()
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

fn is_fail(result: &Result<(), PluginError>, reason: &str) -> bool {
    matches!(result, Err(PluginError::Fail(text)) if text == reason)
}

#[tokio::test]
async fn the_owners_server_reads_each_device_once_in_scan_order_and_shows_four_disks() {
    let dir = root();
    let calls = RefCell::new(Vec::new());
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            let answer = owners_answer(&args);
            calls.borrow_mut().push(args);
            async move { Ok(answer) }
        },
    )
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);

    let calls = calls.into_inner();
    assert_eq!(calls[0], scan_args(), "the scan comes first");
    let reads: Vec<String> = calls[1..].iter().map(|args| args[3..].join(" ")).collect();
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
    assert!(
        calls[1..]
            .iter()
            .all(|args| args[..3] == ["--json", "-x", "-d"])
    );

    assert_eq!(outcome.records.len(), 1, "one table and nothing to explain");
    let names: Vec<String> = outcome
        .table()
        .into_iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(names, ["sda", "sdb", "sdc", "sdd"]);
}

#[tokio::test(start_paused = true)]
async fn a_disk_that_hangs_is_stopped_after_60_seconds_and_the_others_are_read() {
    let dir = root();
    let start = Instant::now();
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            let hangs = args.last().is_some_and(|name| name == "/dev/sdb");
            let answer = owners_answer(&args);
            async move {
                if hangs {
                    std::future::pending::<()>().await;
                }
                Ok(answer)
            }
        },
    )
    .await;
    assert!(start.elapsed() >= Duration::from_secs(60));
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let table = outcome.table();
    let names: Vec<&str> = table.iter().map(|row| row[0].as_str()).collect();
    // Without sdb's serial, its path through the card cannot be merged (design §9).
    assert_eq!(names, ["sda", "sdb", "sdc", "sdd", "RAID 卡 21 号盘"]);
    assert_eq!(table[1][1], "读取失败");
    assert_eq!(
        outcome.notes(),
        ["sdb 读取失败：超过 60 秒仍未读完，已停止"]
    );
}

#[tokio::test(start_paused = true)]
async fn a_scan_that_hangs_fails_the_item() {
    let dir = root();
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |_args: Vec<String>| async { std::future::pending::<io::Result<String>>().await },
    )
    .await;
    assert!(is_fail(
        &outcome.result,
        "smartctl 扫描硬盘失败：超过 60 秒仍未完成"
    ));
    assert!(outcome.records.is_empty());
}

#[tokio::test]
async fn a_scan_without_disks_skips_the_item() {
    let dir = root();
    let empty = common::fixture("local/scan-empty.json");
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |_args: Vec<String>| {
            let answer = empty.clone();
            async move { Ok(answer) }
        },
    )
    .await;
    let reason = "smartctl 没有找到硬盘";
    assert!(
        matches!(&outcome.result, Err(PluginError::Skip(text)) if text == reason),
        "{:?}",
        outcome.result
    );
    assert!(outcome.records.is_empty());
}

#[tokio::test]
async fn a_failed_scan_fails_the_item_with_smartctls_reason() {
    let dir = root();
    let failed = common::fixture("local/open-failed.json");
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |_args: Vec<String>| {
            let answer = failed.clone();
            async move { Ok(answer) }
        },
    )
    .await;
    let reason =
        "smartctl 扫描硬盘失败：Smartctl open device: /dev/sdz [SAT] failed: No such device";
    assert!(is_fail(&outcome.result, reason), "{:?}", outcome.result);
}

#[tokio::test]
async fn when_every_disk_fails_the_table_still_shows_why() {
    let dir = root();
    let scan = scan_of(&[("/dev/sda", "sat"), ("/dev/sdb", "sat")]);
    let failed = common::fixture("local/open-failed.json");
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            let answer = if args == scan_args() {
                scan.clone()
            } else {
                failed.clone()
            };
            async move { Ok(answer) }
        },
    )
    .await;
    assert!(
        is_fail(&outcome.result, "所有硬盘都读取失败"),
        "{:?}",
        outcome.result
    );
    assert_eq!(outcome.table().len(), 2);
    assert_eq!(outcome.notes().len(), 2);
}

#[tokio::test]
async fn disks_without_smart_complete_the_item() {
    let dir = root();
    let scan = scan_of(&[("/dev/sda", "scsi")]);
    let unsupported = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 0},
        "device": {"protocol": "SCSI"}, "smart_support": {"available": false}}"#;
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            let answer = if args == scan_args() {
                scan.clone()
            } else {
                unsupported.to_owned()
            };
            async move { Ok(answer) }
        },
    )
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(outcome.notes(), ["sda 不支持 SMART"]);
}

#[tokio::test]
async fn ctrl_c_stops_at_once_without_a_table_and_still_removes_a_created_node() {
    let dir = root();
    let cancel = CancellationToken::new();
    let outcome = check(dir.path(), &cancel, |args: Vec<String>| {
        let answer = owners_answer(&args);
        let is_scan = args == scan_args();
        if is_scan {
            // The scan creates the node, as smartctl does on a MegaRAID host.
            fs::write(dir.path().join(NODE), "").expect("dev is writable");
        } else {
            cancel.cancel();
        }
        async move {
            if !is_scan {
                std::future::pending::<()>().await;
            }
            Ok(answer)
        }
    })
    .await;
    assert!(is_fail(&outcome.result, "已中断"), "{:?}", outcome.result);
    assert!(
        outcome.records.is_empty(),
        "no table: {:?}",
        outcome.records
    );
    assert!(!dir.path().join(NODE).exists());
}

#[tokio::test]
async fn a_node_created_during_the_item_is_removed_and_one_present_before_is_kept() {
    let dir = root();
    let old = dir.path().join("dev/megadev0");
    fs::write(&old, "").expect("dev is writable");
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            fs::write(dir.path().join(NODE), "").expect("dev is writable");
            let answer = owners_answer(&args);
            async move { Ok(answer) }
        },
    )
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert!(!dir.path().join(NODE).exists());
    assert!(old.exists(), "it was there before the item");
}

#[tokio::test]
async fn a_node_that_cannot_be_removed_adds_a_note() {
    let dir = root();
    let node = dir.path().join(NODE);
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            // A non-empty directory where the node goes cannot be removed as a file.
            fs::create_dir_all(node.join("inside")).expect("dev is writable");
            let answer = owners_answer(&args);
            async move { Ok(answer) }
        },
    )
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    let notes = outcome.notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    let prefix = format!("无法删除 {}：", node.display());
    assert!(notes[0].starts_with(&prefix), "{notes:?}");
    assert_eq!(outcome.records.last().map(Record::id), Some("note-node-1"));
}

#[tokio::test]
async fn the_runners_nvme_disk_shows_as_its_namespace() {
    let dir = root();
    // The runner's layout: one namespace, although the controller reports 127 as its maximum.
    fs::create_dir_all(dir.path().join("sys/class/nvme/nvme0/nvme0n1")).expect("writable");
    let outcome = check(
        dir.path(),
        &CancellationToken::new(),
        |args: Vec<String>| {
            let file = if args == scan_args() {
                "runner/scan-nvme.json"
            } else {
                "runner/nvme0.json"
            };
            let answer = common::fixture(file);
            async move { Ok(answer) }
        },
    )
    .await;
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.table(),
        [[
            "nvme0n1", "通过", "0 小时", "50 °C", "0%", "0.0 GB", "0", "—"
        ]]
    );
}
