//! One disk's health and values (design disk-health, §5.4, §5.6).

mod common;

use plugin_disk_health::device::Device;
use plugin_disk_health::reading;
use plugin_disk_health::reading::BadBlocks;
use plugin_disk_health::reading::Health;
use plugin_disk_health::reading::Reading;
use plugin_disk_health::reading::Values;

fn device(name: &str, kind: &str) -> Device {
    Device {
        name: name.to_owned(),
        kind: kind.to_owned(),
    }
}

fn read(path: &str) -> Reading {
    Reading::new(device("/dev/sdx", "sat"), Some(&common::fixture(path)))
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
    assert_eq!(reading.health, Health::ReadFailed(reason.to_owned()));
    assert_eq!(reading.serial, None);
    assert_eq!(reading.values, Values::default());
}

#[test]
fn a_failure_without_a_message_is_explained_by_the_exit_status() {
    let reading = read("handwritten/identify-failed.json");
    let reason = "打不开设备，或读不到设备识别信息";
    assert_eq!(
        reading.health,
        Health::ReadFailed(reason.to_owned()),
        "bit 1"
    );

    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 1}}"#;
    let reading = Reading::new(device("/dev/sdx", "sat"), Some(output));
    let reason = "smartctl 不接受命令参数";
    assert_eq!(
        reading.health,
        Health::ReadFailed(reason.to_owned()),
        "bit 0"
    );
}

#[test]
fn a_read_over_the_limit_or_without_json_fails() {
    let reading = Reading::new(device("/dev/sdx", "sat"), None);
    let reason = "超过 60 秒仍未读完，已停止";
    assert_eq!(reading.health, Health::ReadFailed(reason.to_owned()));

    let reading = Reading::new(device("/dev/sdx", "sat"), Some("Segmentation fault"));
    let reason = "smartctl 的输出无法解析";
    assert_eq!(reading.health, Health::ReadFailed(reason.to_owned()));
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
    let values = Reading::new(device("/dev/sdx", "sat"), Some(output)).values;
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
    assert_eq!(reading.values.bad_blocks.to_string(), "—");
    let note = reading.health.note();
    assert_eq!(
        note.as_deref(),
        Some("的 SMART 功能已关闭，本程序不会替它打开")
    );

    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 0},
        "device": {"protocol": "ATA"}, "smart_support": {"available": true, "enabled": true}}"#;
    let reading = Reading::new(device("/dev/sdx", "sat"), Some(output));
    assert_eq!(reading.health, Health::Unknown { smart_off: false });
    assert_eq!(
        reading.health.note(),
        None,
        "no 说明 line without a known reason"
    );
}

#[test]
fn a_read_failure_comes_before_the_lack_of_smart() {
    let output = r#"{"json_format_version": [1, 0], "smartctl": {"exit_status": 2},
        "smart_support": {"available": false}}"#;
    let reading = Reading::new(device("/dev/sdx", "sat"), Some(output));
    assert!(matches!(reading.health, Health::ReadFailed(_)));
}

#[test]
fn health_shows_as_its_chinese_word_and_note() {
    let shown: Vec<String> = [
        Health::Passed,
        Health::Failed,
        Health::Unknown { smart_off: false },
        Health::Unsupported,
        Health::ReadFailed("x".to_owned()),
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(shown, ["通过", "未通过", "未知", "不支持", "读取失败"]);
    assert_eq!(Health::Unsupported.note().as_deref(), Some("不支持 SMART"));
    let failed = Health::ReadFailed("超过 60 秒仍未读完，已停止".to_owned());
    assert_eq!(
        failed.note().as_deref(),
        Some("读取失败：超过 60 秒仍未读完，已停止")
    );
    assert_eq!(Health::Passed.note(), None);
    assert_eq!(Health::Failed.note(), None);
}

#[test]
fn bad_blocks_show_both_sata_counts_or_one_count() {
    let sectors = |reallocated, pending| BadBlocks::Sectors {
        reallocated,
        pending,
    };
    assert_eq!(sectors(Some(0), None).to_string(), "0 / —");
    assert_eq!(sectors(Some(8), Some(0)).to_string(), "8 / 0");
    assert_eq!(sectors(None, Some(2)).to_string(), "— / 2");
    assert_eq!(sectors(None, None).to_string(), "—");
    assert_eq!(BadBlocks::Count(Some(5)).to_string(), "5");
    assert_eq!(BadBlocks::Count(None).to_string(), "—");
}

#[test]
fn the_count_is_the_number_that_starts_the_raw_string() {
    assert_eq!(reading::leading_number("0"), Some(0));
    assert_eq!(reading::leading_number("380"), Some(380));
    assert_eq!(reading::leading_number("0 (2000)"), Some(0));
    assert_eq!(reading::leading_number("16 (0 2)"), Some(16));
    assert_eq!(reading::leading_number("0x00000000005a"), None);
    assert_eq!(reading::leading_number(""), None);
    assert_eq!(reading::leading_number("-"), None);
}

#[test]
fn gigabytes_processed_become_bytes_exactly() {
    assert_eq!(reading::gigabytes("123456.789"), Some(123_456_789_000_000));
    assert_eq!(reading::gigabytes("0.000"), Some(0));
    assert_eq!(reading::gigabytes("7"), Some(7_000_000_000));
    assert_eq!(reading::gigabytes("0.000000001"), Some(1));
    assert_eq!(
        reading::gigabytes("0.0000000001"),
        None,
        "finer than a byte"
    );
    assert_eq!(reading::gigabytes("1.2e3"), None);
    assert_eq!(reading::gigabytes("-1.000"), None);
    assert_eq!(reading::gigabytes("99999999999.000"), None, "beyond u64");
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
    let reason = "打不开设备，或读不到设备识别信息";
    assert_eq!(reading.health, Health::ReadFailed(reason.to_owned()));
}
