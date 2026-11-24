# Memory Benchmark Output Captures _(fixtures)_

> Recorded `sysbench`, `STREAM`, and `multichase`/`multiload` outputs for `plugin-memory` tests.

This directory (`plugins/memory/tests/fixtures/`) holds recorded tool outputs for the four memory
plugins (`memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa`) in `lowendaff_bench`.

## Background

Outputs come from `sysbench-1.0.20-1`, `stream-5.10-1`, and `multichase-2026.10.01-1`, recorded on
2026-11-10 in `debian:13-slim` (`--cpuset-cpus 0-3`, `LC_ALL=C`, stdout followed by stderr) on
Apple silicon (`aarch64` natively and `x86_64` under emulation), plus one handwritten STREAM
validation-failure sample matching upstream `stream.c`.

## Recorded Runs

| File | Command | Exit |
|---|---|---|
| `aarch64-sysbench-read-1-thread.txt`, `x86_64-sysbench-read-1-thread.txt` | `sysbench memory --memory-block-size=67108864 --memory-total-size=0 --memory-oper=read --memory-access-mode=seq --threads=1 --time=10 run` | 0 |
| `aarch64-sysbench-write-4-threads.txt` | Same with `--memory-block-size=16777216 --memory-oper=write --memory-scope=local --threads=4` | 0 |
| `aarch64-sysbench-bad-option.txt` | `sysbench memory --memory-block-size=abc run` | 1 |
| `aarch64-stream-1-thread.txt`, `x86_64-stream-1-thread.txt` | `OMP_NUM_THREADS=1 stream/stream-2m` | 0 |
| `aarch64-stream-4-threads.txt` | `OMP_NUM_THREADS=4 OMP_PROC_BIND=spread stream/stream-2m` | 0 |
| `aarch64-multichase.txt`, `x86_64-multichase.txt` | `multichase -m 16384 -n 5` | 0 |
| `aarch64-multichase-mbind-refused.txt` | `multichase -m 16384 -n 5 -W 0:1` (refused by default Docker seccomp) | 1 |
| `aarch64-multiload.txt`, `x86_64-multiload.txt` | `multiload -l stream-triad -n 3 -t 4 -m 33554432` | 0 |
| `aarch64-multiload-mbind-refused.txt` | Same with `-W 0:1` | 1 |
| `handwritten-stream-failed-validation.txt` | STREAM output with failed validation message replacing `Solution Validates` | 0 |

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); see
[`CONTRIBUTING.md`](../../../../CONTRIBUTING.md) for repository conventions.

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
