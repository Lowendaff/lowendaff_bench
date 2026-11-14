//! The disk section of the report, from constructed runs: for disk health the tool, the disks in
//! a table, the notes and each way the item ends; for disk performance YABS's number formats and
//! layout, the notes and each way the item ends. Compared with snapshots, and with the words that
//! the items' own tests asserted before the reporter wrote them.

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bench::orchestrator::Run;
use bench::orchestrator::Status;
use bench::report;
use bench::report::terminal::Page;
use bench::results::Measured;
use bench::results::Report;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use plugin_disk::fio;
use plugin_disk::fio::result::Outcome;
use plugin_disk::fio::result::Rates;
use plugin_disk::fio::result::Test;
use plugin_disk::health;
use plugin_disk::health::location::Location;
use plugin_disk::health::result::BadBlocks;
use plugin_disk::health::result::Disk;
use plugin_disk::health::result::Health;
use plugin_disk::health::result::NodeLeft;
use plugin_disk::health::result::Values;

fn health_run(
    status: Status,
    output: Option<health::Output>,
    stop: Option<Stop<health::Failure>>,
) -> Run<Measured> {
    Run {
        id: "disk.health",
        group: "disk",
        title: Some("Disk health"),
        started: Some(Duration::from_secs(1)),
        elapsed: Some(Duration::from_millis(400)),
        status,
        result: Some(Measured::DiskHealth(Report { output, stop })),
    }
}

/// The report of `runs` from its disk section on, with the summary.
fn section(runs: &[Run<Measured>]) -> String {
    let host = HostProfile::default();
    let page = Page {
        version: "0.1.0",
        host: &host,
        show_ip: false,
        not_applicable: &[],
        runs,
        total: Duration::from_secs(5),
    };
    let text = report::terminal::render(&page, None);
    let start = text.find("[Disks]").expect("a disk section");
    text[start..].to_owned()
}

fn named(name: &str) -> Location {
    Location::Named(name.to_owned())
}

fn ssd(name: &str, numbers: [u64; 4], written: u64, reallocated: u64, interface: u64) -> Disk {
    let [hours, temperature, endurance, _] = numbers;
    Disk {
        location: named(name),
        health: Health::Passed,
        values: Values {
            hours: Some(hours),
            temperature: Some(i64::try_from(temperature).expect("a temperature")),
            endurance_used: Some(endurance),
            written: Some(written),
            bad_blocks: BadBlocks::Sectors {
                reallocated: Some(reallocated),
                pending: None,
            },
            interface_errors: Some(interface),
        },
    }
}

/// The owner's four SATA SSDs, with the values that their fixtures hold.
fn owners_disks() -> Vec<Disk> {
    vec![
        ssd("sda", [46_167, 25, 23, 0], 48_228_308_462_592, 0, 3),
        ssd("sdb", [38_112, 28, 49, 0], 169_012_345_678_912, 380, 0),
        ssd("sdc", [53_544, 27, 79, 0], 277_298_765_432_100, 0, 0),
        ssd("sdd", [35_124, 27, 30, 0], 155_678_901_234_567, 13, 0),
    ]
}

fn output(disks: Vec<Disk>) -> health::Output {
    health::Output {
        version: Some("7.5".to_owned()),
        disks,
        nodes_left: Vec::new(),
    }
}

fn shows(text: &str, line: &str) -> bool {
    text.lines().any(|shown| shown == line)
}

#[test]
fn the_owners_server_shows_four_disks_with_their_values() {
    let run = health_run(Status::Completed, Some(output(owners_disks())), None);
    let text = section(&[run]);
    insta::assert_snapshot!(text);
    for line in [
        "  smartctl 7.5",
        "  Device  Health  Power-on  Temp.  Worn  Written   Bad blocks  Link errors",
        "  sda     passed  46167 h   25 °C  23%   48.2 TB   0 / —                 3",
        "  sdb     passed  38112 h   28 °C  49%   169.0 TB  380 / —               0",
        "  sdc     passed  53544 h   27 °C  79%   277.2 TB  0 / —                 0",
        "  sdd     passed  35124 h   27 °C  30%   155.6 TB  13 / —                0",
        "  done in 0.4 s",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert!(!text.contains("Note"), "nothing to explain: {text}");
}

#[test]
fn disks_that_need_it_get_a_note_after_the_table_in_row_order() {
    let blank = |name: &str, health| Disk {
        location: named(name),
        health,
        values: Values::default(),
    };
    let disks = vec![
        ssd("sda", [46_167, 25, 23, 0], 48_228_308_462_592, 0, 3),
        blank("sdc", Health::ReadFailed(health::Fault::Unopened)),
        blank("sdd", Health::Unsupported),
        blank("sde", Health::Unknown { smart_off: true }),
        blank("sdf", Health::Unknown { smart_off: false }),
        blank("sdg", Health::Failed),
    ];
    let mut output = output(disks);
    output.nodes_left = vec![NodeLeft {
        path: PathBuf::from("/dev/megaraid_sas_ioctl_node"),
        error: "Is a directory (os error 21)".to_owned(),
    }];
    let text = section(&[health_run(Status::Completed, Some(output), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  sdc     read failed  —         —      —     —        —           —",
        "  Note                sdc: read failed: cannot open the device, or read its identity",
        "  Note                sdd does not support SMART",
        "  Note                sde has SMART turned off, and this program does not turn it on",
        "  Note                cannot delete /dev/megaraid_sas_ioctl_node: Is a directory (os \
         error 21)",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    let cells: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("  sd"))
        .filter_map(|line| {
            line.split("  ")
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .nth(1)
        })
        .collect();
    assert_eq!(
        cells,
        [
            "passed",
            "read failed",
            "unsupported",
            "unknown",
            "unknown",
            "failed"
        ],
        "the Health column"
    );
    assert_eq!(
        text.matches("Note").count(),
        4,
        "no note without a known reason: {text}"
    );
}

#[test]
fn each_reason_a_disk_cannot_be_read_has_its_words() {
    let note = |fault: health::Fault| {
        let disk = Disk {
            location: named("sdb"),
            health: Health::ReadFailed(fault),
            values: Values::default(),
        };
        let run = health_run(Status::Completed, Some(output(vec![disk])), None);
        section(&[run])
            .lines()
            .find_map(|line| line.strip_prefix("  Note                sdb: read failed: "))
            .expect("a note")
            .to_owned()
    };
    let cases = [
        (
            health::Fault::Tool(job::Fault::TimedOut(Duration::from_secs(60))),
            "still reading after 60 s, stopped",
        ),
        (
            health::Fault::Tool(job::Fault::Spawn("Text file busy (os error 26)".to_owned())),
            "cannot run smartctl: Text file busy (os error 26)",
        ),
        (
            health::Fault::Tool(job::Fault::Read("Broken pipe (os error 32)".to_owned())),
            "cannot read the output of smartctl: Broken pipe (os error 32)",
        ),
        (
            health::Fault::Unparsable,
            "smartctl's output cannot be parsed",
        ),
        (
            health::Fault::Message(
                "Smartctl open device: /dev/sdz [SAT] failed: No such device".to_owned(),
            ),
            "Smartctl open device: /dev/sdz [SAT] failed: No such device",
        ),
        (
            health::Fault::Unopened,
            "cannot open the device, or read its identity",
        ),
        (health::Fault::Refused, "smartctl refused its arguments"),
    ];
    for (fault, words) in cases {
        assert_eq!(note(fault), words);
    }
}

#[test]
fn raid_card_disks_name_their_card_only_when_there_are_several() {
    let disk = |location| Disk {
        location,
        health: Health::Passed,
        values: Values::default(),
    };
    let card = |host, numbers: &[u32]| Location::Card {
        host,
        numbers: numbers.to_vec(),
    };
    let names = |disks: Vec<Disk>| -> Vec<String> {
        let run = health_run(Status::Completed, Some(output(disks)), None);
        section(&[run])
            .lines()
            .filter(|line| line.ends_with("—") && line.contains("passed"))
            .map(|line| {
                line.trim()
                    .split("  ")
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    };
    assert_eq!(
        names(vec![disk(card(0, &[8])), disk(card(0, &[9]))]),
        ["RAID card disk 8", "RAID card disk 9"]
    );
    assert_eq!(
        names(vec![
            disk(named("sda")),
            disk(card(0, &[9])),
            disk(card(1, &[9]))
        ]),
        [
            "sda",
            "disk 9 of RAID card host0",
            "disk 9 of RAID card host1"
        ]
    );
    assert_eq!(names(vec![disk(card(3, &[0, 5]))]), ["RAID card disk 0:5"]);
}

#[test]
fn the_values_print_as_the_item_printed_them() {
    let cell = |values: Values| {
        let disk = Disk {
            location: named("sdx"),
            health: Health::Passed,
            values,
        };
        let run = health_run(Status::Completed, Some(output(vec![disk])), None);
        let text = section(&[run]);
        let row = text
            .lines()
            .find(|line| line.starts_with("  sdx "))
            .expect("the disk's row")
            .to_owned();
        row.split("  ")
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
            .map(str::to_owned)
            .collect::<Vec<String>>()
    };
    let written = |bytes| {
        cell(Values {
            written: Some(bytes),
            ..Values::default()
        })[5]
            .clone()
    };
    for (bytes, shown) in [
        (0, "0.0 GB"),
        (512_349_999_999, "512.3 GB"),
        (999_999_999_999, "999.9 GB"),
        (1_000_000_000_000, "1.0 TB"),
        (48_228_308_462_592, "48.2 TB"),
        (21_699_999_744_000, "21.6 TB"),
        (999_999_999_999_999, "999.9 TB"),
        (1_000_000_000_000_000, "1.0 PB"),
        (u64::MAX, "18446.7 PB"),
    ] {
        assert_eq!(written(bytes), shown, "cut, never rounded: {bytes}");
    }
    let bad = |bad_blocks| {
        cell(Values {
            bad_blocks,
            ..Values::default()
        })[6]
            .clone()
    };
    let sectors = |reallocated, pending| BadBlocks::Sectors {
        reallocated,
        pending,
    };
    assert_eq!(bad(sectors(Some(0), None)), "0 / —");
    assert_eq!(bad(sectors(Some(8), Some(0))), "8 / 0");
    assert_eq!(bad(sectors(None, Some(2))), "— / 2");
    assert_eq!(bad(sectors(None, None)), "—");
    assert_eq!(bad(BadBlocks::Count(Some(5))), "5");
    assert_eq!(bad(BadBlocks::Count(None)), "—");
    let worn = cell(Values {
        hours: Some(28_761),
        temperature: Some(61),
        endurance_used: Some(104),
        written: Some(42_382_812 * 512_000),
        bad_blocks: BadBlocks::Count(Some(2)),
        interface_errors: None,
    });
    assert_eq!(
        worn,
        [
            "sdx", "passed", "28761 h", "61 °C", "104%", "21.6 TB", "2", "—"
        ]
    );
}

#[test]
fn every_way_the_item_ends_shows_its_words() {
    let read_failed = |name: &str| Disk {
        location: named(name),
        health: Health::ReadFailed(health::Fault::Unopened),
        values: Values::default(),
    };
    let failed = |failure| health_run(Status::Failed(None), None, Some(Stop::Failed(failure)));
    let runs = [
        health_run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(health::Failure::NoDisks)),
        ),
        failed(health::Failure::Scan(health::Fault::Tool(
            job::Fault::TimedOut(Duration::from_secs(60)),
        ))),
        failed(health::Failure::Scan(health::Fault::Message(
            "Smartctl open device: /dev/sdz [SAT] failed: No such device".to_owned(),
        ))),
        failed(health::Failure::Scan(health::Fault::Unparsable)),
        health_run(
            Status::Failed(None),
            Some(output(vec![read_failed("sda"), read_failed("sdb")])),
            Some(Stop::Failed(health::Failure::AllFailed)),
        ),
        failed(health::Failure::Cancelled),
        health_run(Status::TimedOut, Some(output(owners_disks())), None),
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    skipped: smartctl found no disks",
        "    failed: smartctl's scan for disks failed: still running after 60 s",
        "    failed: smartctl's scan for disks failed: Smartctl open device: /dev/sdz [SAT] \
         failed: No such device",
        "    failed: smartctl's scan for disks failed: smartctl's output cannot be parsed",
        "    failed: no disk could be read",
        "    failed: interrupted",
        "    timed out; the results until then are kept",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}

const MIB: u64 = 1024 * 1024;

const GIB: u64 = 1024 * MIB;

/// Rates whose cells YABS's own functions printed as below (`tests/fixtures/README.md`).
const FOUR_K: Rates = Rates {
    read_kib: 90_645,
    read_iops: 22_661,
    write_kib: 90_889,
    write_iops: 22_722,
};

const SIXTY_FOUR_K: Rates = Rates {
    read_kib: 1_065_063,
    read_iops: 16_641,
    write_kib: 1_066_789,
    write_iops: 16_668,
};

fn fio_run(
    status: Status,
    output: Option<fio::Output>,
    stop: Option<Stop<fio::Failure>>,
) -> Run<Measured> {
    Run {
        id: "disk.fio",
        group: "disk",
        title: Some("Disk performance"),
        started: Some(Duration::from_secs(2)),
        elapsed: Some(Duration::from_millis(150_300)),
        status,
        result: Some(Measured::DiskFio(Report { output, stop })),
    }
}

fn fio_output(size: u64, tests: Vec<(&'static str, Outcome)>) -> fio::Output {
    fio::Output {
        version: Some("3.43".to_owned()),
        size,
        zfs_needed: None,
        tests: tests
            .into_iter()
            .map(|(block_size, outcome)| Test {
                block_size,
                outcome,
            })
            .collect(),
    }
}

/// The row of `name` in the text: its cells, as two spaces or more split them.
fn row(text: &str, name: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|line| line.trim_start().starts_with(name))
        .map(|line| {
            line.split("  ")
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .collect()
}

#[test]
fn speeds_and_iops_print_exactly_as_yabs_prints_them() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yabs-format.txt");
    let cases = fs::read_to_string(path).expect("fixture is readable");
    let mut checked = 0;
    for line in cases.lines() {
        let mut fields = line.split('\t');
        let (Some(kind), Some(input), Some(expected), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            panic!("malformed fixture line: {line:?}");
        };
        let input: u64 = input.parse().expect("the input is an integer");
        let actual = match kind {
            "speed" => report::terminal::speed(input),
            "iops" => report::terminal::iops(input),
            other => panic!("unknown kind {other}"),
        };
        assert_eq!(actual, expected, "{kind} of {input}");
        checked += 1;
    }
    assert_eq!(checked, 655, "every case of the fixture was checked");
}

#[test]
fn impossible_rates_still_print() {
    // Far beyond any disk, where awk changes notation; the cell must still print.
    let speed = report::terminal::speed(u64::MAX);
    assert!(speed.ends_with(" GB/s"), "{speed}");
    let iops = report::terminal::iops(u64::MAX);
    assert!(iops.ends_with('k'), "{iops}");
}

#[test]
fn the_tables_have_yabss_rows_one_column_per_block_size_and_a_note_per_failed_test() {
    let output = fio_output(
        2 * GIB,
        vec![
            ("4k", Outcome::Done(FOUR_K)),
            ("64k", Outcome::Done(SIXTY_FOUR_K)),
            (
                "512k",
                Outcome::Failed(fio::Fault::Tool(job::Fault::TimedOut(Duration::from_secs(
                    35,
                )))),
            ),
            (
                "1m",
                Outcome::Failed(fio::Fault::Tool(job::Fault::Exit {
                    code: 1,
                    last: Some("fio: error=No space left on device".to_owned()),
                })),
            ),
        ],
    );
    let text = section(&[fio_run(Status::Completed, Some(output), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  fio 3.43",
        "  Method              as YABS: random reads and writes, half each, on a test file of 2 \
         GiB",
        "  4K and 64K",
        "  512K and 1M",
        "  Note                512K still running after 35 s, stopped",
        "  Note                1M failed: fio: error=No space left on device",
        "  done in 150.3 s",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    assert_eq!(
        row(&text, "Block size"),
        [["Block size", "4K", "64K"], ["Block size", "512K", "1M"]]
    );
    assert_eq!(
        row(&text, "Read"),
        [
            ["Read", "92.82 MB/s (22.6k)", "1.09 GB/s (16.6k)"],
            ["Read", "timed out", "error"]
        ]
    );
    assert_eq!(
        row(&text, "Write"),
        [
            ["Write", "93.07 MB/s (22.7k)", "1.09 GB/s (16.6k)"],
            ["Write", "timed out", "error"]
        ]
    );
    assert_eq!(
        row(&text, "Total"),
        [
            ["Total", "185.89 MB/s (45.3k)", "2.18 GB/s (33.3k)"],
            ["Total", "timed out", "error"]
        ]
    );
}

#[test]
fn the_arm_file_size_and_yabss_note_on_zfs_follow_the_method() {
    let mut output = fio_output(512 * MIB, vec![("4k", Outcome::Done(FOUR_K))]);
    output.zfs_needed = Some(48);
    let text = section(&[fio_run(Status::Cancelled, Some(output), None)]);
    insta::assert_snapshot!(text);
    for line in [
        "  Method              as YABS: random reads and writes, half each, on a test file of \
         512 MiB",
        "  Note                the test directory is on ZFS with less than 48 GiB available, so \
         the results may be off (as YABS warns)",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
    // Interrupted after 4K: 64K never ended, and the second table never started.
    assert_eq!(row(&text, "Read"), [["Read", "92.82 MB/s (22.6k)", "—"]]);
    assert!(!text.contains("512K"), "{text}");
}

#[test]
fn each_fault_of_a_test_has_its_words() {
    let note = |fault: fio::Fault| {
        let output = fio_output(2 * GIB, vec![("4k", Outcome::Failed(fault))]);
        let text = section(&[fio_run(Status::Completed, Some(output), None)]);
        text.lines()
            .find_map(|line| line.strip_prefix("  Note                4K failed: "))
            .expect("a note")
            .to_owned()
    };
    let cases = [
        (fio::Fault::NoResult, "fio printed no result"),
        (fio::Fault::Error(28), "fio reported error 28"),
        (fio::Fault::Field(7), "cannot parse field 7 of fio's output"),
        (
            fio::Fault::Field(48),
            "cannot parse field 48 of fio's output",
        ),
        (
            fio::Fault::Tool(job::Fault::Exit {
                code: 1,
                last: None,
            }),
            "fio printed nothing",
        ),
        (
            fio::Fault::Tool(job::Fault::Signal {
                last: Some("fio: terminating on signal 9".to_owned()),
            }),
            "fio: terminating on signal 9",
        ),
        (
            fio::Fault::Tool(job::Fault::Spawn("Text file busy (os error 26)".to_owned())),
            "cannot run fio: Text file busy (os error 26)",
        ),
        (
            fio::Fault::Tool(job::Fault::Read("Broken pipe (os error 32)".to_owned())),
            "cannot read the output of fio: Broken pipe (os error 32)",
        ),
    ];
    for (fault, words) in cases {
        assert_eq!(note(fault), words);
    }
}

#[test]
fn every_way_fio_ends_shows_its_words() {
    let skipped = |failure| fio_run(Status::Skipped(None), None, Some(Stop::Skipped(failure)));
    let failed = |failure| fio_run(Status::Failed(None), None, Some(Stop::Failed(failure)));
    let setup = |fault| failed(fio::Failure::Setup(fault));
    let runs = [
        skipped(fio::Failure::InMemory),
        skipped(fio::Failure::Space {
            needed: 2 * GIB,
            available: 2 * GIB - 1,
        }),
        failed(fio::Failure::NoSpaceInfo),
        skipped(fio::Failure::NoDirectIo),
        setup(fio::Fault::Tool(job::Fault::Exit {
            code: 1,
            last: Some(
                "fio: pid=0, err=28/file:filesetup.c:241, func=write, error=No space left on device"
                    .to_owned(),
            ),
        })),
        setup(fio::Fault::Tool(job::Fault::Spawn(
            "Text file busy (os error 26)".to_owned(),
        ))),
        failed(fio::Failure::NoResult),
        failed(fio::Failure::Cancelled),
    ];
    let text = section(&runs);
    insta::assert_snapshot!(text);
    for line in [
        "    skipped: the test directory is on a RAM disk, which would measure memory; run in a \
         directory on a disk, or name one with --work-dir",
        "    skipped: the test directory has less than 2 GiB free (only 1.9 GiB)",
        "    failed: cannot read the free space of the test directory",
        "    skipped: the test directory's filesystem does not support direct I/O",
        "    failed: cannot create the test file: fio: pid=0, err=28/file:filesetup.c:241, \
         func=write, error=No space left on device",
        "    failed: cannot create the test file: cannot run fio: Text file busy (os error 26)",
        "    failed: none of the four block sizes has a result",
        "    failed: interrupted",
    ] {
        assert!(shows(&text, line), "{line:?} in {text}");
    }
}

#[test]
fn the_free_space_shown_is_cut_not_rounded() {
    let reason = |needed, available| {
        let run = fio_run(
            Status::Skipped(None),
            None,
            Some(Stop::Skipped(fio::Failure::Space { needed, available })),
        );
        section(&[run])
            .lines()
            .find_map(|line| line.strip_prefix("  skipped: "))
            .expect("the reason")
            .to_owned()
    };
    for (available, shown) in [
        (2 * GIB - 1, "1.9 GiB"),
        (GIB + GIB / 4, "1.2 GiB"),
        (GIB, "1.0 GiB"),
        (GIB - 1, "1023.9 MiB"),
        (812 * MIB + MIB / 2, "812.5 MiB"),
        (0, "0.0 MiB"),
    ] {
        assert_eq!(
            reason(2 * GIB, available),
            format!("the test directory has less than 2 GiB free (only {shown})")
        );
    }
    assert_eq!(
        reason(512 * MIB, 512 * MIB - 1),
        "the test directory has less than 512 MiB free (only 511.9 MiB)"
    );
}
