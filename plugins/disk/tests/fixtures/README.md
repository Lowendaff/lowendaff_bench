# Disk Benchmark Output Captures _(fixtures)_

> Recorded `fio` random read/write and filesystem error outputs for `plugin-disk` parser tests.

This directory (`plugins/disk/tests/fixtures/`) stores `fio` test outputs (and [`health/`](health/README.md)
SMART captures) for `plugin-disk` inside `lowendaff_bench`.

## Background

Files under `fio/` hold combined stdout and stderr (`2>&1`) from `fio-3.43-1`
(`scripts/build-fio.sh`), recorded on 2026-11-08 in `debian:13-slim` under Docker Desktop on Apple
silicon (`aarch64` natively and `x86_64` under emulation). Only output structure and error codes
are tested, not host disk throughput.

## Recorded Runs

| File | Run |
|---|---|
| `fio/rand_rw_<block size>-<arch>.txt` | YABS mixed 50/50 random I/O at each block size (`--size=64M --runtime=2` in place of `2G` and `30`) |
| `fio/rand_rw_4k-note.txt` | 4K run with `--ioengine=psync`, emitting `note:` lines ahead of the result |
| `fio/setup-full.txt` | Layout run on a 16 MiB `tmpfs` hitting `ENOSPC` (error 28) |
| `fio/setup-ramfs.txt` | Layout run on `ramfs` refusing `O_DIRECT` (error 22) |

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); see
[`CONTRIBUTING.md`](../../../../CONTRIBUTING.md) for repository conventions.

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
