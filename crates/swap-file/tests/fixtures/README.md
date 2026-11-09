# Swap header fixtures

The first page of a 1 MiB file that util-linux's `mkswap` set up, in `debian:13-slim` for
arm64 with util-linux 2.41.5-0+deb13u1 (2026-11-09):

```sh
head -c 1048576 /dev/zero > swap
mkswap --uuid clear -p 4096 swap     # mkswap-1MiB-page-4096.bin: last page 255
mkswap --uuid clear -p 65536 swap    # mkswap-1MiB-page-65536.bin: last page 15
head -c <page size> swap > mkswap-1MiB-page-<page size>.bin
```

`--uuid clear` gives a UUID of zeros, as `swap_file::header` writes. Both files are
little-endian, like every architecture the program is built for.
