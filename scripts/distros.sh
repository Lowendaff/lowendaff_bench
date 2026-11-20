#!/bin/sh
# Reads the host in containers of many distributions, with every test item skipped: the report
# must name Docker as the platform, its OS line must equal the image's own os-release
# PRETTY_NAME, and its uptime and load must be marked as the host's, which Docker shares.
# Then Geekbench 7 runs without a network, so that nothing is downloaded or uploaded:
# its reason to skip must fit the image's own glibc. CI runs it on both
# architectures; README "Gates" documents it.
#
# Usage: scripts/distros.sh <path to a static lowendaff-bench> [<mirror>]
# Needs Docker. The images are listed in scripts/distros.txt, pinned by digest; Arch Linux has no
# aarch64 image. With <mirror>, for example ghcr.io/lowendaff/mirror, each image is pulled from
# <mirror>/<name>:<tag> instead: the copy that scripts/mirror.sh made under the same digest. CI
# does this (README "Gates").
set -eu

fail() {
    echo "distros: $*" >&2
    exit 1
}

case $# in
1 | 2) ;;
*) fail "usage: scripts/distros.sh <path to lowendaff-bench> [<mirror>]" ;;
esac
bin=$(realpath "$1")
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

count=0
while read -r image arch_only; do
    case $image in '' | '#'*) continue ;; esac
    [ -z "$arch_only" ] || [ "$arch_only" = "$(uname -m)" ] || continue
    if [ $# -eq 2 ]; then
        image=$2/${image#*/}
    fi
    count=$((count + 1))
    printf '%s\n' "$image" >"$scratch/$count.image"
done <"$(dirname "$0")/distros.txt"

check_image() {
    idx=$1
    image=$(cat "$scratch/$idx.image")
    name=${image%@*}
    # Read os-release and ldd's version in one container; pull progress goes to stderr.
    probe=$(docker run --rm --entrypoint sh "$image" -c '
        . /etc/os-release || exit 1
        printf "%s\n" "$PRETTY_NAME"
        ldd --version 2>&1 | head -n 1 || true
    ') || fail "$name: cannot read os-release"
    expected=$(printf '%s\n' "$probe" | sed -n 1p)
    glibc=$(printf '%s\n' "$probe" |
        sed -n '2s/^ldd (.*) \([0-9][0-9]*\.[0-9][0-9]*\)$/\1/p')

    output=$(docker run --rm -v "$bin:/usr/local/bin/lowendaff-bench:ro" -w /root "$image" \
        lowendaff-bench --skip cpu,memory,disk,network,ip 2>&1) ||
        fail "$name: exited with $?: $output"
    printf '%s\n' "$output" | grep -qx 'Environment: Docker container' ||
        fail "$name: the platform was not detected as Docker: $output"
    reported=$(printf '%s\n' "$output" | sed -n 's/^  OS  *//p')
    [ "$reported" = "$expected" ] ||
        fail "$name: OS is \"$reported\", expected \"$expected\""
    # Docker shares the kernel's uptime and load, so both are the host's.
    for label in Uptime Load; do
        printf '%s\n' "$output" | grep -q "^  $label  *.* (host)\$" ||
            fail "$name: $label is not marked as the host's: $output"
    done

    # glibc's version as the image's ldd reports it: "ldd (GNU libc) 2.34". musl has none.
    major=${glibc%%.*}
    minor=${glibc#*.}
    if [ -z "$glibc" ]; then
        reason='needs glibc 2.35 or newer; this host has no glibc'
    elif [ "$major" -gt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -ge 35 ]; }; then
        reason='needs an IPv4 connection'
    else
        reason="needs glibc 2.35 or newer; this host has $glibc"
    fi
    output=$(docker run --rm --network none -v "$bin:/usr/local/bin/lowendaff-bench:ro" -w /root \
        "$image" lowendaff-bench --only cpu.geekbench 2>&1) ||
        fail "$name: exited with $?: $output"
    printf '%s\n' "$output" | grep -qx "  skipped: $reason" ||
        fail "$name: Geekbench 7 was not skipped with \"$reason\": $output"
    echo "distros: $name: $reported; Geekbench 7: $reason"
}

# Check up to 4 images concurrently while keeping output deterministic in list order.
workers=4
pids=""
worker=0
while [ "$worker" -lt "$workers" ]; do
    worker=$((worker + 1))
    (
        idx=$worker
        while [ "$idx" -le "$count" ]; do
            if ! check_image "$idx" >"$scratch/$idx.out" 2>"$scratch/$idx.err"; then
                cat "$scratch/$idx.err" >&2
                exit 1
            fi
            idx=$((idx + workers))
        done
    ) &
    pids="$pids $!"
done

failed=0
for pid in $pids; do
    wait "$pid" || failed=1
done
[ "$failed" -eq 0 ] || exit 1

idx=1
while [ "$idx" -le "$count" ]; do
    cat "$scratch/$idx.out"
    idx=$((idx + 1))
done
echo "distros: ok"
