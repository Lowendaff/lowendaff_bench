# Report Formatting Reference Data _(fixtures)_

> Reference YABS throughput and IOPS formatting vectors for `bench` report compatibility tests.

This directory (`crates/bench/tests/fixtures/`) holds golden formatting inputs and outputs verified
by `crates/bench/tests/disk.rs` inside `lowendaff_bench`.

## Background

`yabs-format.txt` records what YABS's own `format_speed` and `format_iops` functions print across
655 tab-separated test vectors (`speed` in KiB/s or `iops`, input, expected output):

- **Source**: `yabs-format.sh` executes both functions unchanged from `yabs.sh` at YABS commit
  `21f62408ecde9b23d4eec878f46a8292f1944aa0` (v2026-10-31, SHA-256
  `dd6414ad37460cb6c872a15c79e09dd52cf7fc7f417f664dfba6473b775debf1`).
- **Inputs**: Unit and truncation boundaries followed by deterministic linear-congruential values
  spanning every magnitude. The `FOUR_K` and `SIXTY_FOUR_K` cells in `tests/disk.rs` come from the
  same functions.

## Usage

Verify identical output across both `mawk` (Debian/Ubuntu default, tested with 1.3.4) and `gawk`
(RHEL family default, tested with 5.2.1) with `yabs.sh` placed in `<dir>`:

```sh
docker run --rm -v "$PWD/yabs-format.sh:/gen.sh:ro" -v <dir>:/w debian:13-slim bash -c '
  apt-get update -qq && apt-get install -y -qq gawk
  mkdir /m /g && ln -s "$(command -v mawk)" /m/awk && ln -s "$(command -v gawk)" /g/awk
  PATH=/m:$PATH bash /gen.sh /w/yabs.sh > /w/mawk.txt
  PATH=/g:$PATH bash /gen.sh /w/yabs.sh > /w/gawk.txt
  cmp /w/mawk.txt /w/gawk.txt'
```

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); see
[`CONTRIBUTING.md`](../../../../CONTRIBUTING.md) for repository conventions.

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
