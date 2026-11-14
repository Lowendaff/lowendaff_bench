# Tool output fixtures

What the released tools printed: `sysbench-1.0.20-1`, `stream-5.10-1` and `multichase-2026.10.01-1`. Recorded on 2026-11-10 in `debian:13-slim`, limited to four CPUs (`--cpuset-cpus 0-3`), under Docker Desktop on Apple silicon: natively for aarch64, and under emulation for x86_64, whose numbers therefore say nothing about speed. Each file is the tool's standard output followed by its standard error, as the plugins read them, with `LC_ALL=C`.

| File | Command | Exit |
|------|---------|------|
| `aarch64-sysbench-read-1-thread.txt`, `x86_64-sysbench-read-1-thread.txt` | `sysbench memory --memory-block-size=67108864 --memory-total-size=0 --memory-oper=read --memory-access-mode=seq --threads=1 --time=10 run` | 0 |
| `aarch64-sysbench-write-4-threads.txt` | the same with `--memory-block-size=16777216 --memory-oper=write --memory-scope=local --threads=4` | 0 |
| `aarch64-sysbench-bad-option.txt` | `sysbench memory --memory-block-size=abc run` | 1 |
| `aarch64-stream-1-thread.txt`, `x86_64-stream-1-thread.txt` | `OMP_NUM_THREADS=1 stream/stream-2m` | 0 |
| `aarch64-stream-4-threads.txt` | `OMP_NUM_THREADS=4 OMP_PROC_BIND=spread stream/stream-2m` | 0 |
| `aarch64-multichase.txt`, `x86_64-multichase.txt` | `multichase -m 16384 -n 5` | 0 |
| `aarch64-multichase-mbind-refused.txt` | `multichase -m 16384 -n 5 -W 0:1`, which Docker's default seccomp profile refuses | 1 |
| `aarch64-multiload.txt`, `x86_64-multiload.txt` | `multiload -l stream-triad -n 3 -t 4 -m 33554432` | 0 |
| `aarch64-multiload-mbind-refused.txt` | the same with `-W 0:1` | 1 |
| `handwritten-stream-failed-validation.txt` | Handwritten: `aarch64-stream-1-thread.txt` with STREAM's report of a failed validation, in the format of its source, in place of `Solution Validates`. No real run failed. | 0 |
