#!/bin/sh
# Builds a static NextTrace for the route tracing item from sources pinned by SHA-256: its tiny
# flavor, without the DNS client, the CDN speed test, Globalping, the web UI and the MCP server,
# which the item does not use. It adds Go's fallback roots, which NextTrace's HTTPS client uses
# only on a host that has no root certificates of its own. It runs inside Alpine 3.22 on the
# architecture it builds for, with Go from go.dev; the tools workflow
# (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-nexttrace.sh <output dir>
# Writes nexttrace-<arch>, this script, NextTrace's source tarball, a tarball of what the build
# added to it (the Go modules it was built from, the module files that name them, and the file
# that brings in the fallback roots: with the source tarball, the binary's corresponding source),
# and the license texts of NextTrace and of Go, whose standard library is linked in, to the output
# directory. The binary must be static,
# report the pinned version, offer no Globalping, hold the fallback roots, and trace 127.0.0.1.
set -eu

RELEASE=nexttrace-1.7.6-1
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
NEXTTRACE_VERSION=1.7.6
NEXTTRACE_URL=https://github.com/nxtrace/NTrace-core/archive/refs/tags/v$NEXTTRACE_VERSION.tar.gz
NEXTTRACE_SHA256=4956737ace65a2ae7623e291287c258493016f481fae96699c693f3c3fdd5e27
# The tag's commit and its date, which NextTrace's own builds write into the binary.
NEXTTRACE_COMMIT=c93eabdb873313d19bab00d820551e2167c961c5
NEXTTRACE_DATE=2026-10-31T20:33:44Z
GO_VERSION=1.27.4
GO_SHA256_X86_64=7226736c2339719cda28fbf3a54d71aa63bcefe376e98cef00034268dab24840
GO_SHA256_AARCH64=3392582988a273a87ab5043ee1b2c9c17a2c3c00580bf9326c47a2e0aaaeac92
# Go's fallback roots, a module of golang.org/x/crypto, at a pinned version; the Go checksum
# database confirms its hash.
FALLBACK=golang.org/x/crypto/x509roots/fallback@v0.0.0-20261115193255-739070949446

fail() {
    echo "build-nexttrace: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

# A tarball whose bytes depend on its files alone: sorted, without times or owners.
pack() { # <tarball> <path>...
    tarball=$1
    shift
    tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -cf - "$@" | gzip -n -9 \
        > "$tarball"
}

[ $# -eq 1 ] || fail "usage: build-nexttrace.sh <output dir>"
case $RELEASE in
"nexttrace-$NEXTTRACE_VERSION-"*) ;;
*) fail "the release $RELEASE is not of NextTrace $NEXTTRACE_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
case $arch in
x86_64) go_arch=amd64 go_sha256=$GO_SHA256_X86_64 ;;
aarch64) go_arch=arm64 go_sha256=$GO_SHA256_AARCH64 ;;
*) fail "no Go pinned for $arch" ;;
esac
# A fixed directory rather than a random one; -trimpath keeps it out of the binary all the same.
work=/build
mkdir "$work" || fail "$work exists already"

# GNU tar for pack; jq only for the checks.
apk add --no-cache binutils curl jq tar >/dev/null
cd "$work"
fetch "https://go.dev/dl/go$GO_VERSION.linux-$go_arch.tar.gz" "$go_sha256" go.tar.gz
tar -xzf go.tar.gz
fetch "$NEXTTRACE_URL" "$NEXTTRACE_SHA256" "NTrace-core-$NEXTTRACE_VERSION.tar.gz"
tar -xzf "NTrace-core-$NEXTTRACE_VERSION.tar.gz"

export PATH="$work/go/bin:$PATH" GOPATH="$work/gopath" GOCACHE="$work/cache"
# This Go only, no other toolchain fetched; modules from the Go proxy, checked against go.sum
# and the checksum database.
export GOTOOLCHAIN=local CGO_ENABLED=0 GOFLAGS=-modcacherw
cd "NTrace-core-$NEXTTRACE_VERSION"
cat > fallback_roots.go <<'EOF'
// Added by lowendaff_bench's build-nexttrace.sh: Mozilla's roots, which Go uses only where the
// host has no root certificates of its own.
package main

import _ "golang.org/x/crypto/x509roots/fallback"
EOF
go get "$FALLBACK"
go mod vendor
pkg=github.com/nxtrace/NTrace-core/config
go build -mod=vendor -tags flavor_tiny -trimpath -o nexttrace -ldflags "-s -w -buildid= \
    -X $pkg.Version=v$NEXTTRACE_VERSION -X $pkg.CommitID=$NEXTTRACE_COMMIT \
    -X $pkg.BuildDate=$NEXTTRACE_DATE" .

if readelf -l nexttrace | grep -q 'program interpreter'; then
    fail "nexttrace is dynamically linked"
fi
version=$(./nexttrace --version 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | sed -n 1p)
case $version in
*"v$NEXTTRACE_VERSION"*) ;;
*) fail "nexttrace reports $version, expected v$NEXTTRACE_VERSION" ;;
esac
# The build records its tags and modules: the tiny flavor's tag, the fallback roots, and none of
# the modules that only the full flavor links (Globalping's client, the web UI's server).
modules=$(go version -m nexttrace)
printf '%s\n' "$modules" | grep -q -e '-tags=flavor_tiny' ||
    fail "nexttrace is not the tiny flavor"
printf '%s\n' "$modules" | grep -q "x509roots/fallback" ||
    fail "the fallback roots are not built in"
if printf '%s\n' "$modules" | grep -q -e globalping -e gin-gonic; then
    fail "nexttrace links modules of the full flavor"
fi
# As root, a classic trace of the loopback, without lookups or the map upload.
trace=$(./nexttrace --traceroute -j -M -C -n -d disable-geoip -q 1 -m 3 127.0.0.1) ||
    fail "the trace of 127.0.0.1 failed: $trace"
printf '%s\n' "$trace" | sed -n '/^{/,$p' |
    jq -e '.StopReason.reason == "destination_reached" and (.Hops | length) >= 1' >/dev/null ||
    fail "the trace of 127.0.0.1 did not reach it: $trace"

cp nexttrace "$out/nexttrace-$arch"
cp "$0" "$out/build-nexttrace.sh"
cp "../NTrace-core-$NEXTTRACE_VERSION.tar.gz" "$out/"
pack "$out/nexttrace-$NEXTTRACE_VERSION-modules.tar.gz" go.mod go.sum fallback_roots.go vendor
cp LICENSE "$out/LICENSE-nexttrace"
cp ../go/LICENSE "$out/LICENSE-go"
rm -rf "$work"
echo "build-nexttrace: built $out/nexttrace-$arch ($version)"
