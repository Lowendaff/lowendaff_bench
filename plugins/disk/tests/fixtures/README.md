# Fixtures for 硬盘性能

## `fio/`

Real output of fio 3.43, standard output and standard error together (`2>&1`). fio was built by `scripts/build-fio.sh`, the source and script of the release `fio-3.43-1`. It ran on 2026-11-08 in `debian:13-slim` under Docker Desktop on Apple silicon: aarch64 natively, x86_64 under emulation. The numbers measure no disk; only the format matters.

| File | Run |
|------|-----|
| `rand_rw_<block size>-<arch>.txt` | YABS's test at each block size, with `--size=64M --runtime=2` instead of 2G and 30, to finish quickly. |
| `rand_rw_4k-note.txt` | The 4k test with `--ioengine=psync`, which makes fio print `note:` lines before the result. |
| `setup-full.txt` | YABS's setup run on a 16 MiB tmpfs: ENOSPC while laying out the file (error 28). |
| `setup-ramfs.txt` | YABS's setup run on ramfs, which refuses O_DIRECT: both of fio's messages, then error 22. |
