#!/bin/sh
# Builds static multichase and multiload for the 内存性能 test item (design memory, §4) from the
# source pinned by SHA-256. Unlike the other tools it builds in Debian and links glibc statically:
# multichase uses glibc's random_r, which musl lacks (ME-15). It runs inside the image named
# below on the architecture it builds for; the tools workflow (.github/workflows/tools.yml)
# starts it and publishes what it writes as RELEASE.
#
# Usage: build-multichase.sh <output dir>
# Writes multichase-<arch>.tar.gz, whose single directory multichase/ holds multichase and
# multiload, this script, the source archive, multichase's license and that of glibc to the output
# directory. Both programs must be static, compiled for no particular CPU, and measure a small
# buffer.
set -eu

RELEASE=multichase-2026.10.01-1
# The image the tools workflow runs this script in: Debian 13 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/debian:13
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:9c6e381421d0f48d1b8133e573f33c185ee16cbf505eda9c4a8a4791b25ea8a6
# The upstream repository has no releases; the version is the date of this commit.
MULTICHASE_VERSION=2026.10.01
MULTICHASE_COMMIT=8b68e918f8d2e84794f2c20cdb6c7ebb9c3952bb
MULTICHASE_URL=https://github.com/google/multichase/archive/$MULTICHASE_COMMIT.tar.gz
MULTICHASE_SHA256=38d4adbd7e6cbe9d7ae9d96a48f22aa18d4338b40308d8f36a255c9fc0f2d620
GLIBC_VERSION=2.41
GCC_VERSION=14

fail() {
    echo "build-multichase: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-multichase.sh <output dir>"
case $RELEASE in
"multichase-$MULTICHASE_VERSION-"*) ;;
*) fail "the release $RELEASE is not of multichase $MULTICHASE_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
# A fixed directory rather than a random one, whose name would end up in the binaries.
work=/build
mkdir "$work" || fail "$work exists already"

# perl generates a header of the build; apt verifies the packages by Debian's signatures.
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends ca-certificates curl gcc libc6-dev make perl \
    >/dev/null
glibc=$(dpkg-query -W -f "\${Version}" libc6)
case $glibc in
"$GLIBC_VERSION-"*) ;;
*) fail "the image has glibc $glibc, but the build is pinned for glibc $GLIBC_VERSION" ;;
esac
[ "$(gcc -dumpversion)" = "$GCC_VERSION" ] ||
    fail "the image has gcc $(gcc -dumpversion), but the build is pinned for gcc $GCC_VERSION"
cd "$work"
archive=multichase-$MULTICHASE_COMMIT.tar.gz
fetch "$MULTICHASE_URL" "$MULTICHASE_SHA256" "$archive"
tar -xzf "$archive"

cd "multichase-$MULTICHASE_COMMIT"
# The Makefile links statically. On aarch64 it also adds a -march for the CPU of the machine it
# builds on, read from /proc/cpuinfo (SVE, else LSE atomics), unless CAP is empty; the programs
# have to run on any CPU of their architecture.
make CAP= multichase multiload >/dev/null
for program in multichase multiload; do
    if readelf -l "$program" | grep -q 'program interpreter'; then
        fail "$program is dynamically linked"
    fi
done
# gcc records the flags it compiled with in the programs. No machine flag beyond its defaults may
# appear there, since one would tie the programs to a kind of CPU.
flags=$(readelf --debug-dump=info multichase multiload | grep DW_AT_producer) ||
    fail "the programs record no compiler flags"
if machine=$(echo "$flags" | tr ' ' '\n' | grep -E -x -e '-m.+' | sort -u | grep -v -x -E \
    -e '-m(little-endian|abi=lp64|tune=generic|arch=x86-64)'); then
    fail "the programs are built with machine flags beyond gcc's defaults: $machine"
fi
latency=$(./multichase -m 4m -n 1) || fail "the check run of multichase failed: $latency"
echo "$latency" | grep -Eq '^ *[0-9]+\.[0-9]+$' ||
    fail "multichase printed no latency: $latency"
load=$(./multiload -n 1 -t 1 -m 4m -l stream-triad) ||
    fail "the check run of multiload failed: $load"
case $load in
*stream-triad*) ;;
*) fail "multiload did not measure stream-triad: $load" ;;
esac

mkdir -p "$work/package/multichase"
cp multichase multiload "$work/package/multichase/"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -C "$work/package" \
    -cf - multichase | gzip -9 -n >"$out/multichase-$arch.tar.gz"
cp "$0" "$out/build-multichase.sh"
cp "../$archive" "$out/"
cp LICENSE "$out/LICENSE-multichase"
# glibc is under the LGPL 2.1: multichase's source and this script let anyone relink both
# programs with another glibc.
cp /usr/share/common-licenses/LGPL-2.1 "$out/COPYING-glibc"
rm -rf "$work"
echo "build-multichase: built $out/multichase-$arch.tar.gz (commit $MULTICHASE_COMMIT)"
