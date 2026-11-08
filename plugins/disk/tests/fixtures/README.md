# Fixtures for 硬盘性能

## `yabs-format.txt`

What YABS's own `format_speed` and `format_iops` print for 655 inputs, one tab-separated line each: kind (`speed` in KiB/s, or `iops`), input, output. `tests/report.rs` requires the plugin to print every case identically.

- **Source:** `yabs-format.sh` runs the two functions unchanged, taken from `yabs.sh` of YABS commit `21f62408ecde9b23d4eec878f46a8292f1944aa0` (v2026-10-31). The script checks the file's SHA-256 (`dd6414ad37460cb6c872a15c79e09dd52cf7fc7f417f664dfba6473b775debf1`).
- **Inputs:** the unit and truncation boundaries, then pseudo-random values of every magnitude from a fixed linear congruential generator.
- **awk:** the functions call `awk`, so the script ran twice in `debian:13-slim` on 2026-11-08, with mawk 1.3.4 (the awk of Debian and Ubuntu) and with gawk 5.2.1 (the awk of the RHEL family). Both outputs were identical. To repeat it, with `yabs.sh` in `<dir>`:

  ```sh
  docker run --rm -v "$PWD/yabs-format.sh:/gen.sh:ro" -v <dir>:/w debian:13-slim bash -c '
    apt-get update -qq && apt-get install -y -qq gawk
    mkdir /m /g && ln -s "$(command -v mawk)" /m/awk && ln -s "$(command -v gawk)" /g/awk
    PATH=/m:$PATH bash /gen.sh /w/yabs.sh > /w/mawk.txt
    PATH=/g:$PATH bash /gen.sh /w/yabs.sh > /w/gawk.txt
    cmp /w/mawk.txt /w/gawk.txt'
  ```

- The cells of `FOUR_K` and `SIXTY_FOUR_K` in `tests/report.rs` were printed by the same two functions.

## `fio/`

Real output of fio 3.43, standard output and standard error together (`2>&1`). fio was built by `scripts/build-fio.sh`, the source and script of the release `fio-3.43-1`. It ran on 2026-11-08 in `debian:13-slim` under Docker Desktop on Apple silicon: aarch64 natively, x86_64 under emulation. The numbers measure no disk; only the format matters.

| File | Run |
|------|-----|
| `rand_rw_<block size>-<arch>.txt` | YABS's test at each block size, with `--size=64M --runtime=2` instead of 2G and 30, to finish quickly. |
| `rand_rw_4k-note.txt` | The 4k test with `--ioengine=psync`, which makes fio print `note:` lines before the result. |
| `setup-full.txt` | YABS's setup run on a 16 MiB tmpfs: ENOSPC while laying out the file (error 28). |
| `setup-ramfs.txt` | YABS's setup run on ramfs, which refuses O_DIRECT: both of fio's messages, then error 22. |
