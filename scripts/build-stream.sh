#!/bin/sh
# Builds static STREAM programs for the 内存性能 test item (design memory, §4) from the source
# pinned by SHA-256, one per array size, since STREAM fixes the size when it is compiled
# (ME-06). It runs inside the image named below on the architecture it builds for; the tools
# workflow (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-stream.sh <output dir>
# Writes stream-<arch>.tar.gz, whose single directory stream/ holds stream-2m to stream-64m, this
# script, stream.c, its license and the notice of musl, which is linked statically, to the output
# directory. Every program must be static, and the smallest must validate its results.
set -eu

RELEASE=stream-5.10-1
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
STREAM_VERSION=5.10
STREAM_URL=https://www.cs.virginia.edu/stream/FTP/Code/stream.c
STREAM_SHA256=a52bae5e175bea3f7832112af9c085adab47117f7d2ce219165379849231692b
# Millions of elements in each of the three arrays of doubles: 46 MiB to 1.4 GiB in all. The
# largest stays below the 2 GiB of static data that x86_64's default code model allows.
SIZES="2 4 8 16 32 64"
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4

fail() {
    echo "build-stream: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-stream.sh <output dir>"
case $RELEASE in
"stream-$STREAM_VERSION-"*) ;;
*) fail "the release $RELEASE is not of STREAM $STREAM_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
# A fixed directory rather than a random one, whose name would end up in the binaries.
work=/build
mkdir "$work" || fail "$work exists already"

# GNU tar and gzip write the archive reproducibly; gcc brings libgomp, OpenMP's runtime.
apk add --no-cache curl gcc gzip musl-dev tar >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$STREAM_URL" "$STREAM_SHA256" stream.c
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"
grep -q "Revision: \$Id: stream.c,v $STREAM_VERSION " stream.c ||
    fail "stream.c is not revision $STREAM_VERSION"

# No -march: each program runs on every machine of its architecture.
mkdir stream
for size in $SIZES; do
    gcc -O2 -static -fopenmp -DSTREAM_ARRAY_SIZE="${size}000000" stream.c \
        -o "stream/stream-${size}m"
    if readelf -l "stream/stream-${size}m" | grep -q 'program interpreter'; then
        fail "stream-${size}m is dynamically linked"
    fi
done
check=$(OMP_NUM_THREADS=2 ./stream/stream-2m) || fail "the check run failed: $check"
case $check in
*"Solution Validates"*) ;;
*) fail "the check run did not validate its results: $check" ;;
esac

tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -cf - stream |
    gzip -9 -n >"$out/stream-$arch.tar.gz"
cp "$0" "$out/build-stream.sh"
cp stream.c "$out/"
# The license is the comment at the top of the source.
sed -n '/^\/\* Copyright 1991-2013/,/Absolutely no warranty/p' stream.c >"$out/LICENSE-stream"
grep -q 'Absolutely no warranty' "$out/LICENSE-stream" ||
    fail "the license was not found in stream.c"
cp "musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-stream: built $out/stream-$arch.tar.gz (STREAM $STREAM_VERSION, sizes $SIZES million)"
