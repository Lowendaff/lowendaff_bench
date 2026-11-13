#!/bin/sh
# Runs the 系统基础信息 item in containers of many distributions.
# In each, the 系统 line must equal the image's own os-release PRETTY_NAME, the item must
# complete, and its uptime and load must be marked as the host's, which Docker shares.
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
images=
while read -r image arch_only; do
    case $image in '' | '#'*) continue ;; esac
    [ -z "$arch_only" ] || [ "$arch_only" = "$(uname -m)" ] || continue
    if [ $# -eq 2 ]; then
        image=$2/${image#*/}
    fi
    images="$images $image"
done <"$(dirname "$0")/distros.txt"

for image in $images; do
    name=${image%@*}
    expected=$(docker run --rm --entrypoint sh "$image" -c '. /etc/os-release && printf %s "$PRETTY_NAME"') ||
        fail "$name: cannot read os-release"
    output=$(docker run --rm -v "$bin:/usr/local/bin/lowendaff-bench:ro" -w /root "$image" \
        lowendaff-bench --only system.container 2>&1) || fail "$name: exited with $?: $output"
    printf '%s\n' "$output" | grep -qx '运行环境：Docker 容器' ||
        fail "$name: the platform was not detected as Docker: $output"
    reported=$(printf '%s\n' "$output" | sed -n 's/^  系统  *//p')
    [ "$reported" = "$expected" ] || fail "$name: 系统 is \"$reported\", expected \"$expected\""
    printf '%s\n' "$output" | grep -q '^  完成，用时' || fail "$name: the item did not complete: $output"
    # Docker shares the kernel's uptime and load, so both are the host's.
    for label in 运行时间 负载; do
        printf '%s\n' "$output" | grep -q "^  $label  *.*（宿主机）\$" ||
            fail "$name: $label is not marked as the host's: $output"
    done

    # glibc's version as the image's ldd reports it: "ldd (GNU libc) 2.34". musl has none.
    glibc=$(docker run --rm --entrypoint sh "$image" -c 'ldd --version 2>&1 | head -n 1' |
        sed -n 's/^ldd (.*) \([0-9][0-9]*\.[0-9][0-9]*\)$/\1/p')
    major=${glibc%%.*}
    minor=${glibc#*.}
    if [ -z "$glibc" ]; then
        reason='需要 glibc 2.35 或更新，本机没有 glibc'
    elif [ "$major" -gt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -ge 35 ]; }; then
        reason='需要 IPv4 网络连接'
    else
        reason="需要 glibc 2.35 或更新，本机是 $glibc"
    fi
    output=$(docker run --rm --network none -v "$bin:/usr/local/bin/lowendaff-bench:ro" -w /root \
        "$image" lowendaff-bench --only cpu.geekbench 2>&1) || fail "$name: exited with $?: $output"
    printf '%s\n' "$output" | grep -qx "  跳过：$reason" ||
        fail "$name: Geekbench 7 was not skipped with \"$reason\": $output"
    echo "distros: $name: $reported; Geekbench 7: $reason"
done
echo "distros: ok"
