# sysbench fixtures

Output of the aarch64 build of `scripts/build-sysbench.sh`, byte for byte the binary of the
release `sysbench-1.0.20-1`, in `alpine:3.22` in Docker on the developer's Mac (2026-11-09).
Standard output and standard error together, with `LC_ALL=C`; two seconds instead of 20,
which changes no line but the numbers:

| File | Command | Exit status |
|---|---|---|
| `sysbench/aarch64-1-thread.txt` | `sysbench cpu --cpu-max-prime=10000 --threads=1 --time=2 run` | 0 |
| `sysbench/aarch64-4-threads.txt` | `sysbench cpu --cpu-max-prime=10000 --threads=4 --time=2 run` | 0 |
| `sysbench/aarch64-bad-option.txt` | `sysbench cpu --cpu-max-prime=abc --threads=1 --time=2 run` | 1 |
