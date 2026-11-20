#!/bin/sh
# Builds a static 7zz (7-Zip console) for the CPU benchmark item from sources pinned by SHA-256,
# without the proprietary unRAR codec (DISABLE_RAR=1) so the binary is purely under LGPL-2.1 and
# BSD-3-Clause. It runs inside Alpine 3.22 on the architecture it builds for; the tools workflow
# (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-7zip.sh <output dir>
# Writes 7zip-<arch>, stripped, this script, the 7-Zip source archive, and the license texts of
# 7-Zip and of musl, which is linked statically, to the output directory. The binary must be
# static, report the pinned version, and complete a 1-pass benchmark.
set -eu

RELEASE=7zip-26.06-1
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
SEVENZIP_VERSION=26.06
SEVENZIP_TAG_FLAT=2606
SEVENZIP_ARCHIVE=7z${SEVENZIP_TAG_FLAT}-src.tar.xz
SEVENZIP_URL=https://github.com/ip7z/7zip/releases/download/$SEVENZIP_VERSION/$SEVENZIP_ARCHIVE
SEVENZIP_SHA256=a4e374c8f864f51df48c2cfd0e1413ec3b2ea030deae852d6a655402ebb61ac4
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4

fail() {
    echo "build-7zip: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-7zip.sh <output dir>"
case $RELEASE in
"7zip-$SEVENZIP_VERSION-"*) ;;
*) fail "the release $RELEASE is not of 7zip $SEVENZIP_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
# A fixed directory rather than a random one, whose name would end up in the binary.
work=/build
mkdir "$work" || fail "$work exists already"

apk add --no-cache binutils curl g++ linux-headers make musl-dev xz >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$SEVENZIP_URL" "$SEVENZIP_SHA256" "$SEVENZIP_ARCHIVE"
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
mkdir src
tar -xJf "$SEVENZIP_ARCHIVE" -C src
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"

cd "$work/src/CPP/7zip/Bundles/Alone2"
# DISABLE_RAR=1 omits the unRAR codec, keeping the build under LGPL-2.1 and BSD-3-Clause.
# FLAGS_BASE= keeps baseline architecture flags so the binary runs on any CPU of $arch.
make -j"$(nproc)" -f makefile.gcc \
    DISABLE_RAR=1 \
    FLAGS_BASE= \
    CFLAGS_WARN_WALL= \
    LDFLAGS_STATIC_2=-static \
    _o/7zz
strip _o/7zz

if readelf -l _o/7zz | grep -q 'program interpreter'; then
    fail "7zz is dynamically linked"
fi
banner=$(./_o/7zz | sed -n '/7-Zip/p' | sed -n 1p)
case $banner in
*"7-Zip"*"$SEVENZIP_VERSION"*) ;;
*) fail "7zz reports $banner, expected 7-Zip $SEVENZIP_VERSION" ;;
esac
bench=$(./_o/7zz b 1 -mmt1 -md18) || fail "the check benchmark failed: $bench"
case $bench in
*"Tot:"*) ;;
*) fail "the check benchmark printed no Tot line: $bench" ;;
esac

cp _o/7zz "$out/7zip-$arch"
cp "$0" "$out/build-7zip.sh"
cp "$work/$SEVENZIP_ARCHIVE" "$out/"
cp "$work/src/DOC/License.txt" "$out/LICENSE-7zip"
cp "$work/src/DOC/copying.txt" "$out/COPYING-7zip"
cp "$work/musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-7zip: built $out/7zip-$arch ($banner)"
