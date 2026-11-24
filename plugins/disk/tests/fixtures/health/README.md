# Disk SMART Health Captures _(health)_

> Scrubbed `smartctl --json` captures and edge-case fixtures for `disk.health` parser tests.

This directory (`plugins/disk/tests/fixtures/health/`) stores `smartctl` 7.5 JSON samples for the
`disk.health` plugin inside `lowendaff_bench`.

## Security

No file contains a real drive serial number, `logical_unit_id`, `eui64`, `nguid`, or `wwn`. Real
serials were replaced prior to storage with `FIXTURE-SERIAL-<letter>` and WWN/UID fields were
zeroed or scrubbed, verified to zero occurrences of original identifiers before leaving the host.

## Background

Samples cover physical SATA/SAS/NVMe drives (direct and behind MegaRAID controllers), cloud virtual
disks, container missing-device responses, and handwritten failure edge cases derived from
`smartmontools` 7.5 (`ataprint.cpp`, `nvmeprint.cpp`, `scsiprint.cpp`).

## Fixture Sets

- **`owner/` (dedicated server, 2026-11-09)**: Four Samsung SATA SSDs (`sda`..`sdd`) captured both
  directly (`-x -d sat`) and behind a MegaRAID SAS 9361-8i (`-x -d sat+megaraid,N /dev/bus/0`,
  where SCT commands fail with status bit 2 and health falls back to SMART attributes), plus
  `scan.json` (`--scan-open`).
- **`runner/` (GitHub `ubuntu-24.04` runners, 2026-11-09)**: `scan-nvme.json` / `nvme0.json`
  (Azure virtual NVMe disk) and `scan-virtual-disk.json` / `sda-virtual-disk.json` (Hyper-V virtual
  SCSI disk exiting with status 2).
- **`local/` (`smartctl-7.5-1` in `debian:13-slim`, 2026-11-09)**: `open-failed.json` (missing
  `/dev/sdz`, exit status 2) and `scan-empty.json` (`--scan-open` with no devices).
- **`handwritten/`**: `sata-hdd.json` (failing SATA HDD, exit status 88, Device Statistics vs SMART
  attributes precedence), `sas-hdd.json` (MegaRAID SAS HDD with grown defect list), `nvme-worn.json`
  (NVMe SSD at 104% wear with media errors), `smart-off.json` (SMART disabled, exit status 4), and
  `identify-failed.json` (failed `IDENTIFY`, exit status 2 without JSON messages).

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); scrub all drive identifiers
and follow [`CONTRIBUTING.md`](../../../../../CONTRIBUTING.md).

## License

[MIT](../../../../../LICENSE) (c) 2026 Lowendaff
