#!/bin/sh
# Builds the open-source shared runtime libraries (ncurses ABI 5 with bundled xterm terminfo, and
# minimal libcurl ABI 4) needed by PassMark PerformanceTest Linux on modern glibc distributions
# that ship ncurses 6 only or lack libcurl. Built inside AlmaLinux 8 (glibc 2.28) from sources
# pinned by SHA-256; the tools workflow (.github/workflows/tools.yml) starts it and publishes what
# it writes as RELEASE.
#
# Usage: build-passmark-libs.sh <output dir>
# Writes passmark-libs-<arch>.tar.gz (holding libncurses.so.5, libcurl.so.4, terminfo/x/xterm and
# ready under passmark-libs/), this script, both source tarballs, and the license texts of ncurses
# and curl to the output directory.
set -eu

RELEASE=passmark-libs-1.0-1
# The image the tools workflow runs this script in: AlmaLinux 8 of scripts/distros.txt (glibc 2.28).
# shellcheck disable=SC2034 # tools.yml reads this line and the next
IMAGE=ghcr.io/lowendaff/mirror/docker/library/almalinux:8
# shellcheck disable=SC2034
IMAGE_DIGEST=sha256:2d0f2eb2a83cc09dbc2eb92fade4dbe8f709628149e81aa00aec70630406603b
LIBS_VERSION=1.0
NCURSES_VERSION=6.5
NCURSES_URL=https://ftpmirror.gnu.org/gnu/ncurses/ncurses-$NCURSES_VERSION.tar.gz
NCURSES_SHA256=136d91bc269a9a5785e5f9e980bc76ab57428f604ce3e5a5a90cebc767971cc6
CURL_VERSION=8.12.1
CURL_URL=https://curl.se/download/curl-$CURL_VERSION.tar.gz
CURL_SHA256=7b40ea64947e0b440716a4d7f0b7aa56230a5341c8377d7b609649d4aea8dbcf
GLIBC_VERSION=2.28

fail() {
    echo "build-passmark-libs: $*" >&2
    exit 1
}

fetch() { # <url> <sha256> <file>
    curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum -c - >/dev/null || fail "$3 does not match its pinned SHA-256"
}

[ $# -eq 1 ] || fail "usage: build-passmark-libs.sh <output dir>"
case $RELEASE in
"passmark-libs-$LIBS_VERSION-"*) ;;
*) fail "the release $RELEASE is not of passmark-libs $LIBS_VERSION" ;;
esac
mkdir -p "$1"
out=$(cd "$1" && pwd)
arch=$(uname -m)
work=/build
mkdir "$work" || fail "$work exists already"

dnf install -y -q binutils curl diffutils gcc glibc-devel gzip make tar >/dev/null
glibc=$(rpm -q --qf '%{VERSION}' glibc)
[ "$glibc" = "$GLIBC_VERSION" ] ||
    fail "the image has glibc $glibc, but the build is pinned for glibc $GLIBC_VERSION"

cd "$work"
fetch "$NCURSES_URL" "$NCURSES_SHA256" "ncurses-$NCURSES_VERSION.tar.gz"
fetch "$CURL_URL" "$CURL_SHA256" "curl-$CURL_VERSION.tar.gz"
tar -xzf "ncurses-$NCURSES_VERSION.tar.gz"
tar -xzf "curl-$CURL_VERSION.tar.gz"

# 1. Build unified libncurses.so.5 (ABI 5, without split libtinfo) and compile xterm terminfo.
cd "$work/ncurses-$NCURSES_VERSION"
./configure \
    --prefix="$work/ncurses-inst" \
    --with-shared \
    --without-normal \
    --without-debug \
    --without-cxx \
    --without-cxx-binding \
    --without-ada \
    --without-manpages \
    --without-tests \
    --disable-stripping \
    --with-abi-version=5 >/dev/null
make -j"$(nproc)" >/dev/null
make install.libs install.progs >/dev/null
mkdir -p "$work/terminfo"
LD_LIBRARY_PATH="$work/ncurses-inst/lib:$work/ncurses-inst/lib64" \
    "$work/ncurses-inst/bin/tic" -x -o "$work/terminfo" misc/terminfo.src >/dev/null

# 2. Build minimal libcurl.so.4 without external TLS/compression libraries (PassMark -r 1 runs
# locally and never uploads, so it only needs libcurl.so.4's symbols resolved at load time).
cd "$work/curl-$CURL_VERSION"
./configure \
    --prefix="$work/curl-inst" \
    --enable-shared \
    --disable-static \
    --without-ssl \
    --without-zlib \
    --without-libpsl \
    --without-nghttp2 \
    --without-brotli \
    --without-zstd \
    --without-libidn2 \
    --disable-ldap \
    --disable-ldaps \
    --disable-rtsp \
    --disable-dict \
    --disable-telnet \
    --disable-tftp \
    --disable-pop3 \
    --disable-imap \
    --disable-smb \
    --disable-smtp \
    --disable-gopher \
    --disable-mqtt \
    --disable-docs \
    --disable-manual >/dev/null
make -j"$(nproc)" -C lib >/dev/null
make -C lib install >/dev/null

pkg="$work/package/passmark-libs"
mkdir -p "$pkg/terminfo/x"
ncurses_lib=""
for candidate in "$work/ncurses-inst/lib/libncurses.so.5" "$work/ncurses-inst/lib64/libncurses.so.5"; do
    if [ -e "$candidate" ]; then
        ncurses_lib=$candidate
        break
    fi
done
[ -n "$ncurses_lib" ] || fail "libncurses.so.5 was not built"
cp -L "$ncurses_lib" "$pkg/libncurses.so.5"
curl_lib=""
for candidate in "$work/curl-inst/lib/libcurl.so.4" "$work/curl-inst/lib64/libcurl.so.4"; do
    if [ -e "$candidate" ]; then
        curl_lib=$candidate
        break
    fi
done
[ -n "$curl_lib" ] || fail "libcurl.so.4 was not built"
cp -L "$curl_lib" "$pkg/libcurl.so.4"
strip --strip-unneeded "$pkg/libncurses.so.5" "$pkg/libcurl.so.4"
chmod 755 "$pkg/libncurses.so.5" "$pkg/libcurl.so.4"

# Keep xterm and dumb terminfo entries so ncurses initscr() works on minimal hosts without terminfo.
[ -f "$work/terminfo/x/xterm" ] || fail "xterm terminfo was not compiled"
cp "$work/terminfo/x/xterm" "$pkg/terminfo/x/xterm"
if [ -f "$work/terminfo/d/dumb" ]; then
    mkdir -p "$pkg/terminfo/d"
    cp "$work/terminfo/d/dumb" "$pkg/terminfo/d/dumb"
fi
printf '%s\n' "$LIBS_VERSION" >"$pkg/ready"

# Verify neither shared library pulls in anything beyond glibc's own base libraries.
for lib in "$pkg/libncurses.so.5" "$pkg/libcurl.so.4"; do
    extra=$(readelf -d "$lib" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p' |
        grep -v -E '^(libc\.so\.6|libdl\.so\.2|libpthread\.so\.0|librt\.so\.1|ld-linux.*\.so\.[0-9]+)$' || true)
    if [ -n "$extra" ]; then
        fail "$(basename "$lib") links unexpected shared libraries: $extra"
    fi
done

tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -C "$work/package" \
    -cf - passmark-libs | gzip -9 -n >"$out/passmark-libs-$arch.tar.gz"
cp "$0" "$out/build-passmark-libs.sh"
cp "$work/ncurses-$NCURSES_VERSION.tar.gz" "$work/curl-$CURL_VERSION.tar.gz" "$out/"
cp "$work/ncurses-$NCURSES_VERSION/COPYING" "$out/LICENSE-ncurses"
cp "$work/curl-$CURL_VERSION/COPYING" "$out/LICENSE-curl"
rm -rf "$work"
echo "build-passmark-libs: built $out/passmark-libs-$arch.tar.gz (ncurses $NCURSES_VERSION, curl $CURL_VERSION)"
