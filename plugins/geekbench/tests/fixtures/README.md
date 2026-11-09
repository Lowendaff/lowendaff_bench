# Geekbench fixtures

Console output of Geekbench 7.1.0 (`geekbench7 --cpu`), as the design describes (design cpu,
§3.2). All four were captured on 2026-11-09.

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
