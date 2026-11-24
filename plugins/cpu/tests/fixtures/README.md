# CPU Benchmark Output Captures _(fixtures)_

> Scrubbed console and report outputs from all eight CPU sub-suites for `plugin-cpu` parser tests.

This directory (`plugins/cpu/tests/fixtures/`) stores recorded outputs from pinned `crates/bench-tool`
CPU benchmark binaries for `plugin-cpu` inside `lowendaff_bench`.

## Table of Contents

- [Security](#security)
- [Background](#background)
- [Suite Captures](#suite-captures)
- [Contributing](#contributing)
- [License](#license)

## Security

Host-specific processor identifiers, kernel strings, and Geekbench Browser account claim keys are
scrubbed before commit (`claim` keys are replaced with `FIXTURE` and temporary runner artifacts are
deleted).

## Background

Every file records verbatim stdout/stderr or structured output from the exact pinned tool releases
built by `scripts/build-*.sh` (with shortened run durations where noted so captures complete
quickly without altering output structure).

## Suite Captures

### sysbench

Output of `sysbench-1.0.20-1` (`scripts/build-sysbench.sh`) on `aarch64` in `alpine:3.22`
(2026-11-09, `LC_ALL=C`, stdout and stderr combined):

| File | Command | Exit status |
|---|---|---|
| `sysbench/aarch64-1-thread.txt` | `sysbench cpu --cpu-max-prime=10000 --threads=1 --time=2 run` | 0 |
| `sysbench/aarch64-4-threads.txt` | `sysbench cpu --cpu-max-prime=10000 --threads=4 --time=2 run` | 0 |
| `sysbench/aarch64-bad-option.txt` | `sysbench cpu --cpu-max-prime=abc --threads=1 --time=2 run` | 1 |

### 7-Zip

Output of static `7zz` 26.06 (`scripts/build-7zip.sh`) in `alpine:3.22` (2026-11-20):

| File | Command | Exit status |
|---|---|---|
| `7zip/aarch64-2-threads.txt` | `7zz b -mmt2 -md18` | 0 |

### UnixBench

Output of static Byte-UnixBench 6.0.1 (`scripts/build-unixbench.sh`) in `alpine:3.22` (2026-11-20):

| File | Command | Exit status |
|---|---|---|
| `unixbench/whetstone-double.txt` | `whetstone-double` | 0 |
| `unixbench/fstime-copy.txt` | `fstime -c -t 2 -d <work> -b 1024 -m 2000` | 0 |

### HPL (Linpack)

Output of static HPL 2.3 + OpenBLAS 0.3.34 (`scripts/build-hpl.sh`) in `alpine:3.22` (2026-11-20):

| File | Command | Exit status |
|---|---|---|
| `hpl/aarch64-passed.txt` | `xhpl` (`N=512, NB=64, P=1, Q=1`) | 0 |

### PassMark

`results_cpu.yml` from PassMark PerformanceTest Linux 11.0 Build 1004 (`-r 1`) in `debian:trixie`
with `scripts/build-passmark-libs.sh` (2026-11-20):

| File | Command | Exit status |
|---|---|---|
| `passmark/aarch64-results_cpu.yml` | `PerformanceTest_Linux_ARM64 -r 1` | 0 |

### y-cruncher

Console output of `y-cruncher` v0.8.7 Build 9547 static Linux `x86_64` in `ubuntu:24.04`
(2026-11-20):

| File | Command | Exit status |
|---|---|---|
| `ycruncher/x86_64-25m.txt` | `y-cruncher skip-warnings bench 25m` | 0 |

### Prime95

Console output of `mprime` v30.19 build 20 Linux64 Small-FFT torture mode stopped cleanly via
`SIGINT` in `ubuntu:24.04` (2026-11-20):

| File | Command | Exit status |
|---|---|---|
| `prime95/x86_64-torture.txt` | `mprime -t -d -W<work>` | 0 |

### Geekbench

Console output of Geekbench 7.1.0 (`geekbench7 --cpu`, captured 2026-11-09):

| File | Where | Network | Output |
|---|---|---|---|
| `geekbench/x86_64-upload.txt` | `ubuntu-24.04` runner, 4 vCPUs, with `--upload` | yes | stdout; result 840201 |
| `geekbench/aarch64-upload.txt` | `ubuntu-24.04-arm` runner, 4 vCPUs, with `--upload` | yes | stdout; result 840200 |
| `geekbench/x86_64-offline.txt` | Docker under Rosetta, with `--upload` | none | stdout + stderr (exit 255) |
| `geekbench/aarch64-offline.txt` | Docker, with `--upload` | none | stdout + stderr (exit 255) |

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); see
[`CONTRIBUTING.md`](../../../../CONTRIBUTING.md) for parser fixture guidelines.

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
