#!/bin/sh
# Builds a static smartctl for the 硬盘健康 test item (design disk-health, §4) from sources pinned
# by SHA-256. It runs inside Alpine 3.22 on the architecture it builds for; the smartctl workflow
# (.github/workflows/smartctl.yml) starts it and publishes what it writes.
#
# Usage: build-smartctl.sh <output dir>
# Writes smartctl-<arch>, this script, the smartmontools tarball and the license texts of
# smartmontools and of musl, which is linked statically, to the output directory. The binary must
# be static, report the pinned version, and scan without reading drive databases from the host.
set -eu

SMARTMONTOOLS_VERSION=7.5
SMARTMONTOOLS_RELEASE=https://github.com/smartmontools/smartmontools/releases/download/RELEASE_7_5
SMARTMONTOOLS_URL=$SMARTMONTOOLS_RELEASE/smartmontools-$SMARTMONTOOLS_VERSION.tar.gz
SMARTMONTOOLS_SHA256=690b83ca331378da9ea0d9d61008c4b22dde391387b9bbad7f29387f2595f76e
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4

fail() {
    echo "build-smartctl: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-smartctl.sh <output dir>"
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
work=$(mktemp -d)

apk add --no-cache curl g++ linux-headers make >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$SMARTMONTOOLS_URL" "$SMARTMONTOOLS_SHA256" "smartmontools-$SMARTMONTOOLS_VERSION.tar.gz"
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
tar -xzf "smartmontools-$SMARTMONTOOLS_VERSION.tar.gz"
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"

cd "smartmontools-$SMARTMONTOOLS_VERSION"
# Only the drive database built into smartctl: no drivedb.h from the host, and smart_drivedb.h
# looked up under /proc, where it cannot exist.
./configure LDFLAGS=-static --sysconfdir=/proc/nonexistent --with-drivedbdir=no \
    --with-update-smart-drivedb=no --with-nvme-devicescan=yes --with-libsystemd=no \
    --with-libcap-ng=no --with-selinux=no
make -j"$(nproc)" smartctl

if readelf -l smartctl | grep -q 'program interpreter'; then
    fail "smartctl is dynamically linked"
fi
version=$(./smartctl --version | sed -n 1p)
case $version in
"smartctl $SMARTMONTOOLS_VERSION "*) ;;
*) fail "smartctl reports $version, expected smartctl $SMARTMONTOOLS_VERSION" ;;
esac
# Broken drive databases where a default build reads them. With --json, such a build exits with 1
# and prints no message, which would fail every scan on a host with a broken file.
mkdir -p /usr/local/etc
echo 'not a drive database {' | tee /usr/local/etc/smart_drivedb.h /etc/smart_drivedb.h >/dev/null
scan=$(./smartctl --json --scan-open) || fail "the scan failed: $scan"
case $scan in
'{'*'"json_format_version"'*) ;;
*) fail "the scan printed no JSON: $scan" ;;
esac

cp smartctl "$out/smartctl-$arch"
cp "$0" "$out/build-smartctl.sh"
cp "../smartmontools-$SMARTMONTOOLS_VERSION.tar.gz" "$out/"
cp COPYING "$out/COPYING-smartmontools"
cp "../musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-smartctl: built $out/smartctl-$arch ($version)"
