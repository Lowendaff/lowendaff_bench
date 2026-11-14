# CPU fixtures

## sysbench

Output of the aarch64 build of `scripts/build-sysbench.sh`, byte for byte the binary of the
release `sysbench-1.0.20-1`, in `alpine:3.22` in Docker on the developer's Mac (2026-11-09).
Standard output and standard error together, with `LC_ALL=C`; two seconds instead of 20,
which changes no line but the numbers:

| File | Command | Exit status |
|---|---|---|
| `sysbench/aarch64-1-thread.txt` | `sysbench cpu --cpu-max-prime=10000 --threads=1 --time=2 run` | 0 |
| `sysbench/aarch64-4-threads.txt` | `sysbench cpu --cpu-max-prime=10000 --threads=4 --time=2 run` | 0 |
| `sysbench/aarch64-bad-option.txt` | `sysbench cpu --cpu-max-prime=abc --threads=1 --time=2 run` | 1 |

## Geekbench

Console output of Geekbench 7.1.0 (`geekbench7 --cpu`). All four were captured on 2026-11-09.

| File | Where | Network | Output |
|---|---|---|---|
| `geekbench/x86_64-upload.txt` | `ubuntu-24.04` runner, 4 vCPUs, with `--upload` | yes | standard output; result 840201 |
| `geekbench/aarch64-upload.txt` | `ubuntu-24.04-arm` runner, 4 vCPUs, with `--upload` | yes | standard output; result 840200 |
| `geekbench/x86_64-offline.txt` | Docker on the developer's Mac, under Rosetta, with `--upload` | none | both streams |
| `geekbench/aarch64-offline.txt` | Docker on the developer's Mac | none | both streams |

- The runners' results are public on the Geekbench Browser. Each output also held the link that
  adds the result to an account; its key is replaced by `FIXTURE`, and the runs were deleted
  together with their artifacts.
- Both offline runs ran the whole benchmark, failed to upload, exited with 255 and showed no
  scores.
