#!/bin/sh
# Builds a static sysbench for the CPU 性能 test item from sources pinned by
# SHA-256. It runs inside Alpine 3.22 on the architecture it builds for; the tools workflow
# (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-sysbench.sh <output dir>
# Writes sysbench-<arch>, this script, the sysbench tarball and the license texts of sysbench, of
# the LuaJIT and Concurrency Kit that it bundles, and of musl, which is linked statically, to the
# output directory. The binary must be static, report the pinned version and run the CPU test.
set -eu

RELEASE=sysbench-1.0.20-1
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
SYSBENCH_VERSION=1.0.20
SYSBENCH_URL=https://github.com/akopytov/sysbench/archive/refs/tags/$SYSBENCH_VERSION.tar.gz
SYSBENCH_SHA256=e8ee79b1f399b2d167e6a90de52ccc90e52408f7ade1b9b7135727efe181347f
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4

fail() {
    echo "build-sysbench: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-sysbench.sh <output dir>"
case $RELEASE in
"sysbench-$SYSBENCH_VERSION-"*) ;;
*) fail "the release $RELEASE is not of sysbench $SYSBENCH_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
# A fixed directory rather than a random one, whose name would end up in the binary.
work=/build
mkdir "$work" || fail "$work exists already"

# bash for autogen.sh; autoconf, automake, libtool and pkgconf for the build system.
apk add --no-cache autoconf automake bash curl gcc libtool linux-headers make musl-dev pkgconf \
    >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$SYSBENCH_URL" "$SYSBENCH_SHA256" "sysbench-$SYSBENCH_VERSION.tar.gz"
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
tar -xzf "sysbench-$SYSBENCH_VERSION.tar.gz"
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"

cd "sysbench-$SYSBENCH_VERSION"
./autogen.sh
# No database drivers: the CPU test needs none, and they would link client libraries.
./configure --without-mysql --without-pgsql
make -j"$(nproc)"
# Static for sysbench alone: for the whole build, -all-static reaches the makefile of the bundled
# LuaJIT, whose gcc rejects it.
rm src/sysbench
make -C src sysbench LDFLAGS=-all-static

if readelf -l src/sysbench | grep -q 'program interpreter'; then
    fail "sysbench is dynamically linked"
fi
version=$(./src/sysbench --version)
[ "$version" = "sysbench $SYSBENCH_VERSION" ] ||
    fail "sysbench reports $version, expected sysbench $SYSBENCH_VERSION"
check=$(./src/sysbench cpu --cpu-max-prime=10000 --time=1 run) ||
    fail "the check run failed: $check"
case $check in
*"events per second:"*) ;;
*) fail "the check run printed no events per second: $check" ;;
esac

cp src/sysbench "$out/sysbench-$arch"
cp "$0" "$out/build-sysbench.sh"
cp "../sysbench-$SYSBENCH_VERSION.tar.gz" "$out/"
cp COPYING "$out/COPYING-sysbench"
cp third_party/luajit/luajit/COPYRIGHT "$out/COPYRIGHT-luajit"
cp third_party/concurrency_kit/ck/LICENSE "$out/COPYING-concurrency-kit"
cp "../musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-sysbench: built $out/sysbench-$arch ($version)"
