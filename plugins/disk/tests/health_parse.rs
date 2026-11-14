//! What smartctl prints: the scan's devices and version, and one disk's verdict and values.

mod common;

use plugin_disk::health::Fault;
use plugin_disk::health::device::Device;
use plugin_disk::health::parse;
use plugin_disk::health::parse::Reading;
use plugin_disk::health::result::BadBlocks;
use plugin_disk::health::result::Health;
use plugin_disk::health::result::Values;

fn device(name: &str, kind: &str) -> Device {
    Device {
        name: name.to_owned(),
        kind: kind.to_owned(),
    }
}

fn read(path: &str) -> Reading {
    Reading::new(device("/dev/sdx", "sat"), &common::fixture(path))
}

#[test]
fn the_owners_system_ssd_reads_its_device_statistics_and_attributes() {
    let reading = read("owner/sda.json");
    assert_eq!(reading.health, Health::Passed);
    assert_eq!(reading.serial.as_deref(), Some("FIXTURE-SERIAL-a"));
    assert_eq!(
        reading.values,
        Values {
            hours: Some(46_167),
            temperature: Some(25),
            endurance_used: Some(23),
            // 94195914966 logical sectors of 512 bytes (Device Statistics page 1).
            written: Some(48_228_308_462_592),
            // No page 3 on an SSD, so attribute 5; the disk has no attribute 197.
            bad_blocks: BadBlocks::Sectors {
                reallocated: Some(0),
                pending: None,
            },
            // Device Statistics page 6.
            interface_errors: Some(3),
        }
    );
}

#[test]
fn errors_in_the_logs_do_not_make_a_read_fail() {
    // Exit status 64: the disk's error log has entries.
    let reading = read("owner/sdb.json");
    assert_eq!(reading.health, Health::Passed);
    assert_eq!(
        reading.values.bad_blocks,
        BadBlocks::Sectors {
            reallocated: Some(380),
            pending: None,
        }
    );
}

#[test]
fn the_raid_card_gives_the_same_values_despite_failed_sct_commands() {
    // Exit status 4 (bit 2): the SCT commands fail through the card. The verdict comes from
    // smartctl's attribute check there.
    let reading = read("owner/megaraid-18.json");
    assert_eq!(reading.health, Health::Passed);
    assert_eq!(reading.serial.as_deref(), Some("FIXTURE-SERIAL-a"));
    assert_eq!(reading.values.hours, Some(46_167));
    assert_eq!(reading.values.written, Some(94_195_914_998 * 512));
    assert_eq!(reading.values.interface_errors, Some(3));
}

#[test]
fn a_device_that_cannot_be_opened_fails_with_smartctls_message() {
    let reading = read("local/open-failed.json");
    let reason = "Smartctl open device: /dev/sdz [SAT] failed: No such device";
    assert_eq!(
        reading.health,
        Health::ReadFailed(Fault::Message(reason.to_owned()))
    );
    assert_eq!(reading.serial, None);
    assert_eq!(reading.values, Values::default());
}

#[test]
fn a_failure_without_a_message_is_explained_by_the_exit_status() {
    let reading = read("handwritten/identify-failed.json");
    assert_eq!(reading.health, Health::ReadFailed(Fault::Unopened), "bit 1");

    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 1}}"#;
    let reading = Reading::new(device("/dev/sdx", "sat"), output);
    assert_eq!(reading.health, Health::ReadFailed(Fault::Refused), "bit 0");
}

#[test]
fn only_an_error_message_explains_a_failure() {
    let warning = r#"{"string": "Warning: this disk is new to smartctl", "severity": "warning"}"#;
    let error = r#"{"string": "Read Device Identity failed", "severity": "error"}"#;
    let failed = |messages: &[&str]| {
        let output = format!(
            r#"{{"json_format_version": [1, 0],
                "smartctl": {{"exit_status": 2, "messages": [{}]}}}}"#,
            messages.join(", ")
        );
        Reading::new(device("/dev/sdx", "sat"), &output).health
    };
    let message = Fault::Message("Read Device Identity failed".to_owned());
    assert_eq!(failed(&[warning, error]), Health::ReadFailed(message));
    assert_eq!(failed(&[warning]), Health::ReadFailed(Fault::Unopened));
}

#[test]
fn a_read_without_json_fails_and_what_follows_the_json_does_not_count() {
    let reading = Reading::new(device("/dev/sdx", "sat"), "Segmentation fault");
    assert_eq!(reading.health, Health::ReadFailed(Fault::Unparsable));
    let reading = Reading::new(device("/dev/sdx", "sat"), "{}");
    assert_eq!(
        reading.health,
        Health::ReadFailed(Fault::Unparsable),
        "JSON without an exit status"
    );

    // Standard error follows standard output in what the shared runner reads.
    let with_errors = format!(
        "{}\nwarning: something on stderr\n",
        common::fixture("owner/sda.json")
    );
    let reading = Reading::new(device("/dev/sdx", "sat"), &with_errors);
    assert_eq!(reading.health, Health::Passed);
}

#[test]
fn device_statistics_win_over_attributes_and_a_failing_verdict_is_not_a_read_failure() {
    // Exit status 88: DISK FAILING, a prefail attribute at its threshold, errors in the log.
    let reading = read("handwritten/sata-hdd.json");
    assert_eq!(reading.health, Health::Failed);
    assert_eq!(
        reading.values,
        Values {
            hours: Some(51_208),
            temperature: Some(44),
            // A hard disk has no endurance.
            endurance_used: None,
            written: Some(219_512_345_678 * 512),
            // Page 3 says 1536 and 16; attributes 5 and 197 say 1532 and 12.
            bad_blocks: BadBlocks::Sectors {
                reallocated: Some(1536),
                pending: Some(16),
            },
            // Page 6 says 2; attribute 199 says 0.
            interface_errors: Some(2),
        }
    );
}

#[test]
fn invalid_device_statistics_fall_back_to_the_attributes() {
    let output = r#"{
        "json_format_version": [1, 0], "smartctl": {"exit_status": 0},
        "device": {"protocol": "ATA"}, "smart_status": {"passed": true},
        "logical_block_size": 4096,
        "ata_device_statistics": {"pages": [
            {"number": 1, "table": [{"offset": 24, "flags": {"valid": false}}]},
            {"number": 3, "table": [{"offset": 32, "flags": {"valid": false}}]}
        ]},
        "ata_smart_attributes": {"table": [
            {"id": 5, "raw": {"value": 8589934608, "string": "16 (0 2)"}},
            {"id": 197, "raw": {"value": 3, "string": "3"}},
            {"id": 199, "raw": {"value": 7, "string": "7"}}
        ]}
    }"#;
    let values = Reading::new(device("/dev/sdx", "sat"), output).values;
    assert_eq!(values.written, None, "an invalid entry has no value");
    assert_eq!(
        values.bad_blocks,
        BadBlocks::Sectors {
            reallocated: Some(16),
            pending: Some(3),
        },
        "attributes 5 and 197: the counter that starts raw.string, not raw.value"
    );
    assert_eq!(
        values.interface_errors,
        Some(7),
        "no page 6, so attribute 199"
    );
}

#[test]
fn a_sas_disk_reads_its_error_counter_log_and_defect_list() {
    let reading = read("handwritten/sas-hdd.json");
    assert_eq!(reading.health, Health::Passed);
    assert_eq!(
        reading.values,
        Values {
            hours: Some(41_000),
            temperature: Some(36),
            endurance_used: None,
            // The write log's "123456.789" gigabytes, not the read log's.
            written: Some(123_456_789_000_000),
            bad_blocks: BadBlocks::Count(Some(3)),
            interface_errors: None,
        }
    );
}

#[test]
fn smart_switched_off_leaves_no_verdict() {
    let reading = read("handwritten/smart-off.json");
    assert_eq!(reading.health, Health::Unknown { smart_off: true });
    let nothing = Values {
        bad_blocks: BadBlocks::Sectors {
            reallocated: None,
            pending: None,
        },
        ..Values::default()
    };
    assert_eq!(
        reading.values, nothing,
        "smartctl stops before reading any value"
    );

    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 0},
        "device": {"protocol": "ATA"}, "smart_support": {"available": true, "enabled": true}}"#;
    let reading = Reading::new(device("/dev/sdx", "sat"), output);
    assert_eq!(reading.health, Health::Unknown { smart_off: false });
}

#[test]
fn a_read_failure_comes_before_the_lack_of_smart() {
    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 2},
        "smart_support": {"available": false}}"#;
    let reading = Reading::new(device("/dev/sdx", "sat"), output);
    assert!(matches!(reading.health, Health::ReadFailed(_)));
}

#[test]
fn the_count_is_the_number_that_starts_the_raw_string() {
    assert_eq!(parse::leading_number("0"), Some(0));
    assert_eq!(parse::leading_number("380"), Some(380));
    assert_eq!(parse::leading_number("0 (2000)"), Some(0));
    assert_eq!(parse::leading_number("16 (0 2)"), Some(16));
    assert_eq!(parse::leading_number("0x00000000005a"), None);
    assert_eq!(parse::leading_number(""), None);
    assert_eq!(parse::leading_number("-"), None);
}

#[test]
fn gigabytes_processed_become_bytes_exactly() {
    assert_eq!(parse::gigabytes("123456.789"), Some(123_456_789_000_000));
    assert_eq!(parse::gigabytes("0.000"), Some(0));
    assert_eq!(parse::gigabytes("7"), Some(7_000_000_000));
    assert_eq!(parse::gigabytes("0.000000001"), Some(1));
    assert_eq!(parse::gigabytes("0.0000000001"), None, "finer than a byte");
    assert_eq!(parse::gigabytes("1.2e3"), None);
    assert_eq!(parse::gigabytes("-1.000"), None);
    assert_eq!(parse::gigabytes("99999999999.000"), None, "beyond u64");
}

#[test]
fn the_runners_nvme_disk_reads_its_health_log() {
    // Azure's virtual NVMe disk on a GitHub runner: real structure, zero counts.
    let reading = read("runner/nvme0.json");
    assert_eq!(reading.health, Health::Passed);
    assert_eq!(
        reading.values,
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

#[test]
fn a_worn_nvme_disk_fails_with_its_counts() {
    let reading = read("handwritten/nvme-worn.json");
    assert_eq!(
        reading.health,
        Health::Failed,
        "the critical warning is not 0"
    );
    assert_eq!(
        reading.values,
        Values {
            hours: Some(28_761),
            temperature: Some(61),
            endurance_used: Some(104),
            // Data units of 512000 bytes.
            written: Some(42_382_812 * 512_000),
            bad_blocks: BadBlocks::Count(Some(2)),
            interface_errors: None,
        }
    );
}

#[test]
fn a_virtual_disk_that_fails_a_mandatory_command_cannot_be_read() {
    // Hyper-V's virtual disk on a GitHub runner: exit status 2 without any message.
    let reading = read("runner/sda-virtual-disk.json");
    assert_eq!(reading.health, Health::ReadFailed(Fault::Unopened));
}

#[test]
fn the_owners_scan_lists_each_disk_directly_and_through_the_raid_card_and_the_version() {
    let (version, devices) =
        parse::scan(&common::fixture("owner/scan.json")).expect("the scan succeeded");
    assert_eq!(version.as_deref(), Some("7.5"));
    let listed: Vec<(&str, &str)> = devices
        .iter()
        .map(|device| (device.name.as_str(), device.kind.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            ("/dev/sda", "sat"),
            ("/dev/sdb", "sat"),
            ("/dev/sdc", "sat"),
            ("/dev/sdd", "sat"),
            ("/dev/bus/0", "sat+megaraid,18"),
            ("/dev/bus/0", "sat+megaraid,21"),
            ("/dev/bus/0", "sat+megaraid,23"),
            ("/dev/bus/0", "sat+megaraid,24"),
        ],
        "smartctl's order is kept"
    );
}

#[test]
fn a_scan_without_disks_lists_nothing() {
    // smartctl prints no `devices` at all then (fixture from a container without disks).
    let scanned = parse::scan(&common::fixture("local/scan-empty.json"));
    assert_eq!(scanned, Ok((Some("7.5".to_owned()), Vec::new())));
}

#[test]
fn a_failed_scan_gives_smartctls_error() {
    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 2,
        "messages": [{"string": "Unable to open /dev", "severity": "error"}]}}"#;
    assert_eq!(
        parse::scan(output),
        Err(Fault::Message("Unable to open /dev".to_owned()))
    );
}

#[test]
fn output_that_is_not_smartctls_json_fails_the_scan() {
    assert_eq!(
        parse::scan("Segmentation fault"),
        Err(Fault::Unparsable),
        "not JSON"
    );
    assert_eq!(
        parse::scan("{}"),
        Err(Fault::Unparsable),
        "JSON without an exit status"
    );
}

#[test]
fn a_version_is_its_numbers_joined_by_dots() {
    let scan = |version: &str| {
        let output = format!(
            "{{\"json_format_version\": [1, 0], \"smartctl\": \
             {{\"version\": {version}, \"exit_status\": 0}}}}"
        );
        parse::scan(&output).map(|(version, _)| version)
    };
    assert_eq!(scan("[7, 5]"), Ok(Some("7.5".to_owned())));
    assert_eq!(scan("[8, 0, 1]"), Ok(Some("8.0.1".to_owned())));
    assert_eq!(scan("[]"), Ok(None));
    assert_eq!(scan(r#"["7", 5]"#), Ok(None), "not numbers");
}

#[test]
fn the_runners_scans_list_their_single_disk() {
    let (_, nvme) = parse::scan(&common::fixture("runner/scan-nvme.json")).expect("a scan");
    let expected = Device {
        name: "/dev/nvme0".to_owned(),
        kind: "nvme".to_owned(),
    };
    assert_eq!(nvme, [expected]);
    let (_, scsi) = parse::scan(&common::fixture("runner/scan-virtual-disk.json")).expect("a scan");
    let expected = Device {
        name: "/dev/sda".to_owned(),
        kind: "scsi".to_owned(),
    };
    assert_eq!(scsi, [expected]);
}
