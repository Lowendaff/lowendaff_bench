#!/bin/sh
# Builds a static iperf3 for the network test item from sources pinned by SHA-256. It leaves out
# OpenSSL, and with it iperf3's authentication code, where several of iperf3's recent flaws were,
# and SCTP, which the item does not use. It runs inside Alpine 3.22 on the architecture it builds
# for; the tools workflow (.github/workflows/tools.yml) starts it and publishes what it writes as
# RELEASE.
#
# Usage: build-iperf3.sh <output dir>
# Writes iperf3-<arch>, stripped, this script, the iperf tarball and the license texts of iperf,
# which also cover the code it bundles, and of musl, which is linked statically, to the output
# directory. The binary must be static, report the pinned version with neither authentication nor
# SCTP among its features, and measure a short test over the loopback in both directions.
set -eu

RELEASE=iperf3-3.22-1
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
IPERF_VERSION=3.22
IPERF_URL=https://downloads.es.net/pub/iperf/iperf-$IPERF_VERSION.tar.gz
IPERF_SHA256=1c0d0fb02c52626111d6e132db80edfbf27bbaff8bd9245df2a371dcb0b35a92
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4
# The loopback port of the check runs.
PORT=5299

fail() {
    echo "build-iperf3: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

# check <upload|download>: one second over two streams against a server on the loopback, which
# must count bits received, and for an upload the smallest round-trip time of each stream, which
# the item reports as the latency.
check() {
    reverse=
    [ "$1" = download ] && reverse=--reverse
    ./src/iperf3 --server --one-off --port "$PORT" >"$work/server.log" 2>&1 &
    server=$!
    sleep 1
    # shellcheck disable=SC2086 # $reverse is one option or none
    if ! result=$(./src/iperf3 --client 127.0.0.1 --port "$PORT" --time 1 --parallel 2 \
        --json $reverse); then
        kill "$server" 2>/dev/null || true
        fail "the $1 check failed: $(printf '%s' "$result" | jq -r '.error // "no output"')"
    fi
    wait "$server" || fail "the server of the $1 check failed: $(cat "$work/server.log")"
    printf '%s' "$result" | jq -e --argjson reverse "$([ -n "$reverse" ] && echo 1 || echo 0)" '
        .start.test_start.reverse == $reverse and .start.test_start.num_streams == 2
        and .end.sum_received.bits_per_second > 0
        and ($reverse == 1 or ([.end.streams[].sender.min_rtt] | length == 2 and all(. > 0)))
        ' >/dev/null ||
        fail "the $1 check measured nothing: $(printf '%s' "$result" | jq -c '.end.sum_received')"
}

[ $# -eq 1 ] || fail "usage: build-iperf3.sh <output dir>"
case $RELEASE in
"iperf3-$IPERF_VERSION-"*) ;;
*) fail "the release $RELEASE is not of iperf3 $IPERF_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
# A fixed directory rather than a random one, whose name would end up in the binary.
work=/build
mkdir "$work" || fail "$work exists already"

# jq only for the checks.
apk add --no-cache curl gcc jq linux-headers make musl-dev >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$IPERF_URL" "$IPERF_SHA256" "iperf-$IPERF_VERSION.tar.gz"
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
tar -xzf "iperf-$IPERF_VERSION.tar.gz"
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"

cd "iperf-$IPERF_VERSION"
./configure --enable-static-bin --without-openssl --without-sctp
make -j"$(nproc)"
strip src/iperf3

if readelf -l src/iperf3 | grep -q 'program interpreter'; then
    fail "iperf3 is dynamically linked"
fi
version=$(./src/iperf3 --version | sed -n 1p)
case $version in
"iperf $IPERF_VERSION "*) ;;
*) fail "iperf3 reports $version, expected iperf $IPERF_VERSION" ;;
esac
features=$(./src/iperf3 --version | sed -n 's/^Optional features available: //p')
[ -n "$features" ] || fail "iperf3 names no optional features"
case $features in
*authentication* | *SCTP*) fail "iperf3 offers authentication or SCTP: $features" ;;
esac
check upload
check download

cp src/iperf3 "$out/iperf3-$arch"
cp "$0" "$out/build-iperf3.sh"
cp "../iperf-$IPERF_VERSION.tar.gz" "$out/"
cp LICENSE "$out/LICENSE-iperf3"
cp "../musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-iperf3: built $out/iperf3-$arch ($version; $features)"
