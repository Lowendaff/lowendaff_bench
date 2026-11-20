#!/bin/sh
# Builds static Byte-UnixBench programs for the CPU benchmark item from the source commit pinned by
# SHA-256. Every C benchmark binary and looper are linked statically with musl and without
# -march=native so they run on any machine of their architecture without Perl on the host. It runs
# inside Alpine 3.22 on the architecture it builds for; the tools workflow
# (.github/workflows/tools.yml) starts it and publishes what it writes as RELEASE.
#
# Usage: build-unixbench.sh <output dir>
# Writes unixbench-<arch>.tar.gz (holding dhry2reg, whetstone-double, execl, fstime, pipe,
# context1, spawn, syscall, looper, multi.sh, tst.sh, index.base and sort.src under unixbench/),
# this script, the source archive, and the license texts of UnixBench and musl to the output
# directory.
set -eu

RELEASE=unixbench-6.0.1-1
# The image the tools workflow runs this script in: Alpine 3.22 of scripts/distros.txt.
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/alpine:3.22
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
UNIXBENCH_VERSION=6.0.1
UNIXBENCH_COMMIT=ca521bb0f7a24f9e42bf7823fcfaf0014f1cebac
UNIXBENCH_ARCHIVE=byte-unixbench-$UNIXBENCH_COMMIT.tar.gz
UNIXBENCH_URL=https://github.com/kdlucas/byte-unixbench/archive/$UNIXBENCH_COMMIT.tar.gz
UNIXBENCH_SHA256=300ddaaeee151de5500e15d004e5e0092e71ee6ae934a787a10047f89d976095
# Only for its COPYRIGHT: Alpine links musl statically but ships no copy of the notice.
MUSL_VERSION=1.2.5
MUSL_URL=https://musl.libc.org/releases/musl-$MUSL_VERSION.tar.gz
MUSL_SHA256=a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4

PROGRAMS="dhry2reg whetstone-double execl fstime pipe context1 spawn syscall looper"

fail() {
    echo "build-unixbench: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-unixbench.sh <output dir>"
case $RELEASE in
"unixbench-$UNIXBENCH_VERSION-"*) ;;
*) fail "the release $RELEASE is not of UnixBench $UNIXBENCH_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
work=/build
mkdir "$work" || fail "$work exists already"

apk add --no-cache binutils curl gcc gzip linux-headers make musl-dev tar >/dev/null
musl=$(apk list --installed musl)
case $musl in
"musl-$MUSL_VERSION-"*) ;;
*) fail "the image has $musl, but the notice is pinned for musl $MUSL_VERSION" ;;
esac
cd "$work"
fetch "$UNIXBENCH_URL" "$UNIXBENCH_SHA256" "$UNIXBENCH_ARCHIVE"
fetch "$MUSL_URL" "$MUSL_SHA256" "musl-$MUSL_VERSION.tar.gz"
tar -xzf "$UNIXBENCH_ARCHIVE"
tar -xzf "musl-$MUSL_VERSION.tar.gz" "musl-$MUSL_VERSION/COPYRIGHT"

cd "byte-unixbench-$UNIXBENCH_COMMIT/UnixBench"
grep -q "my \$version = \"$UNIXBENCH_VERSION\";" Run ||
    fail "UnixBench/Run does not report version $UNIXBENCH_VERSION"

# UB_GCC_OPTIONS overrides UnixBench's default -march=native so binaries run on any CPU of $arch.
make -j"$(nproc)" HZ=100 UB_GCC_OPTIONS="-O2 -ffast-math -static" LDFLAGS="-static"

pkg="$work/package/unixbench"
mkdir -p "$pkg"
for prog in $PROGRAMS; do
    [ -f "pgms/$prog" ] || fail "missing built program pgms/$prog"
    strip "pgms/$prog"
    if readelf -l "pgms/$prog" | grep -q 'program interpreter'; then
        fail "$prog is dynamically linked"
    fi
    cp "pgms/$prog" "$pkg/$prog"
    chmod 755 "$pkg/$prog"
done
for script in multi.sh tst.sh; do
    cp "pgms/$script" "$pkg/$script"
    chmod 755 "$pkg/$script"
done
cp pgms/index.base "$pkg/index.base"
cp testdir/sort.src "$pkg/sort.src"

# Self-check core C programs and looper + multi.sh in an isolated work directory.
check_dir="$work/check"
mkdir "$check_dir"
cp "$pkg/sort.src" "$check_dir/sort.src"
dhry=$("$pkg/dhry2reg" 1 2>&1) || fail "dhry2reg check failed: $dhry"
case $dhry in
*"COUNT|"*) ;;
*) fail "dhry2reg printed no COUNT line: $dhry" ;;
esac
whets=$("$pkg/whetstone-double" 1 2>&1) || fail "whetstone-double check failed: $whets"
case $whets in
*"COUNT|"*) ;;
*) fail "whetstone-double printed no COUNT line: $whets" ;;
esac
sh_out=$(cd "$check_dir" && UB_BINDIR="$pkg" "$pkg/looper" 1 /bin/sh "$pkg/multi.sh" 1 2>&1) ||
    fail "shell1 check failed: $sh_out"
case $sh_out in
*"COUNT|"*) ;;
*) fail "looper multi.sh printed no COUNT line: $sh_out" ;;
esac

tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -C "$work/package" \
    -cf - unixbench | gzip -9 -n >"$out/unixbench-$arch.tar.gz"
cp "$0" "$out/build-unixbench.sh"
cp "$work/$UNIXBENCH_ARCHIVE" "$out/"
license=""
for candidate in ../LICENSE ../LICENSE.txt LICENSE LICENSE.txt COPYING; do
    if [ -f "$candidate" ]; then
        license=$candidate
        break
    fi
done
[ -n "$license" ] || fail "UnixBench license file not found"
cp "$license" "$out/COPYING-unixbench"
cp "$work/musl-$MUSL_VERSION/COPYRIGHT" "$out/COPYRIGHT-musl"
rm -rf "$work"
echo "build-unixbench: built $out/unixbench-$arch.tar.gz (UnixBench $UNIXBENCH_VERSION)"
