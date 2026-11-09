#!/bin/sh
# Builds a static fio with the libaio engine for the 硬盘性能 test item (design disk, §4) from
# sources pinned by SHA-256. It runs inside Alpine 3.22 on the architecture it builds for; the tools
# workflow (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-fio.sh <output dir>
# Writes fio-<arch>, this script, both source tarballs and both license texts to the output
# directory. The binary must be static, report the pinned version, offer the libaio engine and
# complete a short run with YABS's parameters.
set -eu

RELEASE=fio-3.43-1
FIO_VERSION=3.43
FIO_URL=https://brick.kernel.dk/snaps/fio-$FIO_VERSION.tar.gz
FIO_SHA256=6f57893c7a73d9ed72e9e2b6e15f136a99c137bd6a7645fdeeb4a40b3e9da78d
LIBAIO_VERSION=0.3.113
LIBAIO_URL=https://deb.debian.org/debian/pool/main/liba/libaio/libaio_$LIBAIO_VERSION.orig.tar.gz
LIBAIO_SHA256=2c44d1c5fd0d43752287c9ae1eb9c023f04ef848ea8d4aafa46e9aedb678200b

fail() {
    echo "build-fio: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-fio.sh <output dir>"
case $RELEASE in
"fio-$FIO_VERSION-"*) ;;
*) fail "the release $RELEASE is not of fio $FIO_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
# A fixed directory rather than a random one, whose name would end up in the binary.
work=/build
mkdir "$work" || fail "$work exists already"

apk add --no-cache curl gcc linux-headers make musl-dev >/dev/null
cd "$work"
fetch "$LIBAIO_URL" "$LIBAIO_SHA256" "libaio_$LIBAIO_VERSION.orig.tar.gz"
fetch "$FIO_URL" "$FIO_SHA256" "fio-$FIO_VERSION.tar.gz"
tar -xzf "libaio_$LIBAIO_VERSION.orig.tar.gz"
tar -xzf "fio-$FIO_VERSION.tar.gz"

# Only the static library, so that fio cannot link libaio dynamically.
make -C "libaio-$LIBAIO_VERSION/src" ENABLE_SHARED=0 prefix=/usr install >/dev/null

cd "fio-$FIO_VERSION"
./configure --build-static --disable-native
make -j"$(nproc)" fio

if readelf -l fio | grep -q 'program interpreter'; then
    fail "fio is dynamically linked"
fi
version=$(./fio --version)
[ "$version" = "fio-$FIO_VERSION" ] || fail "fio reports $version, expected fio-$FIO_VERSION"
./fio --enghelp | grep -qw libaio || fail "fio lacks the libaio engine"
check=$(./fio --name=check --ioengine=libaio --rw=randrw --rwmixread=50 --bs=4k --iodepth=64 \
    --numjobs=2 --size=64m --runtime=2 --gtod_reduce=1 --direct=1 --filename="$work/check.fio" \
    --group_reporting --minimal | grep '^3;') || fail "the check run printed no result"
case $check in
"3;fio-$FIO_VERSION;check;0;0;"*) ;;
*) fail "the check run failed: $check" ;;
esac

cp fio "$out/fio-$arch"
cp "$0" "$out/build-fio.sh"
cp "../fio-$FIO_VERSION.tar.gz" "../libaio_$LIBAIO_VERSION.orig.tar.gz" "$out/"
cp COPYING "$out/COPYING-fio"
cp "../libaio-$LIBAIO_VERSION/COPYING" "$out/COPYING-libaio"
rm -rf "$work"
echo "build-fio: built $out/fio-$arch ($version)"
