# Fixtures for 硬盘健康

smartctl's JSON output for the plugin's tests (design `docs/design/disk-health.md`, §7.2). Every file is `smartctl --json` output, real or written by hand as marked below. No file contains a real serial number, WWN or other unique identifier.

## `owner/`: the owner's dedicated server

Captured 2026-11-09 through `ssh` pipes only, nothing stored on the server, with the server's own smartctl 7.5 (its Debian package, so its drive database may differ from the one built into `smartctl-7.5-1`).

- `scan.json`: `smartctl --json --scan-open`, unchanged.
- `sda.json` to `sdd.json`: `smartctl --json -x -d sat /dev/sdX`.
- `megaraid-18.json`, `-21`, `-23`, `-24`: `smartctl --json -x -d sat+megaraid,N /dev/bus/0`, the same disks through the MegaRAID SAS 9361-8i: 18 is `sda`, 21 `sdb`, 23 `sdc` and 24 `sdd`.

On the server, `sed` replaced each disk's serial with `FIXTURE-SERIAL-<letter>`, the same for both paths of one disk, and set every number of `wwn` to 0. Before printing, the server counted the real serial in the output and printed only that count, which was 0 for all eight files. Nothing else was changed.

The disks are four Samsung SATA SSDs (860 EVO 250GB, 870 EVO 1TB, 860 EVO 1TB, 870 EVO 1TB). Through the RAID card the SCT commands fail, which sets exit status bit 2, and smartctl judges health from the attributes, with the warning "This result is based on an Attribute check".

## `runner/`: GitHub's runners

Captured 2026-11-09 with `smartctl-7.5-1` on `ubuntu-24.04` runners, by a temporary branch whose workflow replaced `serial_number`, `logical_unit_id`, `eui64`, `nguid` and `wwn` before uploading. The workflow re-serialized the JSON with two-space indentation.

- `scan-nvme.json`, `nvme0.json`: Azure's virtual NVMe disk ("MSFT NVMe Accelerator v1.0"). Its health log is complete but every count is 0. `/sys/class/nvme/nvme0` held one namespace, `nvme0n1`, although `nvme_number_of_namespaces` is 127.
- `scan-virtual-disk.json`, `sda-virtual-disk.json`: Hyper-V's virtual SCSI disk ("Msft Virtual Disk"). After its identity, a mandatory command fails, so smartctl exits with status 2 and no message.

## `local/`: this repository's smartctl in a container

Made 2026-11-09 with `smartctl-7.5-1` (aarch64) in `debian:13-slim`, unchanged.

- `open-failed.json`: `smartctl --json -x -d sat /dev/sdz`, a device that does not exist: exit status 2 with an error message.
- `scan-empty.json`: `smartctl --json --scan-open` without disks: no `devices` at all.

## `handwritten/`: written by hand

Written from the field names and structure in smartmontools 7.5's source (`ataprint.cpp`, `nvmeprint.cpp`, `scsiprint.cpp`), for hardware the project has no sample of. Model names and values are made up.

- `sata-hdd.json`: a failing SATA hard disk (exit status 88) whose Device Statistics (pages 1, 3 and 6) differ from attributes 5, 197 and 199, so that tests see which wins.
- `sas-hdd.json`: a SAS hard disk behind a MegaRAID card (`megaraid,8`), with its grown defect list and error counter logs.
- `nvme-worn.json`: an NVMe SSD past its rated endurance (104%), with a critical warning and media errors.
- `smart-off.json`: a SATA SSD with SMART switched off; smartctl stops after the identity, with exit status 4.
- `identify-failed.json`: a failed IDENTIFY, which smartctl reports only as plain output: exit status 2 and no message.
