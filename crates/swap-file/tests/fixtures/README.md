# Swap Header Reference Pages _(fixtures)_

> Binary Linux `mkswap` header pages for testing `swap-file` header layout across 4K and 64K pages.

This directory (`crates/swap-file/tests/fixtures/`) stores reference binary swap headers verified
by `crates/swap-file` unit tests inside `lowendaff_bench`.

## Background

Each `.bin` file is the first page of a 1 MiB swap file initialized by `util-linux` `mkswap`
(`2.41.5-0+deb13u1` in `debian:13-slim` on `arm64`, captured 2026-11-09) with `--uuid clear` (an
all-zero UUID matching `swap_file::header`) in little-endian byte order.

## Usage

Regenerate both 4 KiB and 64 KiB page-size reference headers inside `debian:13-slim`:

```sh
head -c 1048576 /dev/zero > swap
mkswap --uuid clear -p 4096 swap     # mkswap-1MiB-page-4096.bin: last page 255
mkswap --uuid clear -p 65536 swap    # mkswap-1MiB-page-65536.bin: last page 15
head -c <page size> swap > mkswap-1MiB-page-<page size>.bin
```

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); see
[`CONTRIBUTING.md`](../../../../CONTRIBUTING.md) for repository conventions.

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
