#!/bin/sh
# Temporary (never merged): runs YABS v2026-10-31's disk procedure with this repository's fio
# (release fio-3.43-1) and with YABS's own fio (its release fio-3.43), alternating, twice each,
# on the runner's disk. Prints read and write KiB/s and IOPS for every test.
set -eu

case $(uname -m) in
x86_64) ours=fio-x86_64 ours_sha=0196f44814d91d8af1015f5102ad004765a3197d7f23ae5b321332df2e2245b6
    yabs=fio_x64 yabs_sha=cb7c6570da16df37328d9acdd613d5ff48317cf6b5122bfafe97b4d85038d5a9
    size=2G ;;
aarch64) ours=fio-aarch64 ours_sha=40720e705be65f9fbf059664db9dd048040e3d1ab8836c67339110bcd8a2f73a
    yabs=fio_aarch64 yabs_sha=3054eb476d0b70e8e0b3ee0109d1d2ad057b5443411510d825775a4b79d518e4
    size=512M ;;
*) echo "unsupported architecture" >&2; exit 1 ;;
esac
dir=$PWD/compare
mkdir -p "$dir"
# Authenticated downloads: GitHub was timing out unauthenticated ones (2026-11-08 incident).
gh release download fio-3.43-1 --repo Lowendaff/lowendaff_bench --pattern "$ours" --output "$dir/ours"
gh release download fio-3.43 --repo masonr/yet-another-bench-script --pattern "$yabs" --output "$dir/yabs"
printf '%s  %s\n%s  %s\n' "$ours_sha" "$dir/ours" "$yabs_sha" "$dir/yabs" | sha256sum -c -
chmod +x "$dir/ours" "$dir/yabs"
echo "kernel $(uname -r), $(nproc) CPUs, file size $size, filesystem $(df -T "$dir" | awk 'NR == 2 { print $2 }')"

run() { # <label> <fio>
    "$2" --name=setup --ioengine=libaio --rw=read --bs=64k --iodepth=64 --numjobs=2 --size=$size \
        --runtime=1 --gtod_reduce=1 --filename="$dir/test.fio" --direct=1 --minimal >/dev/null
    for bs in 4k 64k 512k 1m; do
        timeout 35 "$2" --name=rand_rw_$bs --ioengine=libaio --rw=randrw --rwmixread=50 --bs=$bs \
            --iodepth=64 --numjobs=2 --size=$size --runtime=30 --gtod_reduce=1 --direct=1 \
            --filename="$dir/test.fio" --group_reporting --minimal |
            awk -F';' -v label="$1" -v bs=$bs '$1 == 3 && $3 == "rand_rw_" bs {
                printf "%-5s %-5s read %9d KiB/s %8d IOPS  write %9d KiB/s %8d IOPS\n",
                    label, bs, $7, $8, $48, $49 }'
    done
    rm -f "$dir/test.fio"
}
for _ in 1 2; do
    run ours "$dir/ours"
    run yabs "$dir/yabs"
done
