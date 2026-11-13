#!/bin/sh
# Smoke test for a static release binary: static linking, the root requirement, and a
# full run in which no test item is skipped, fails or times out, and nothing is left behind.
# A skip counts because every test item applies on the CI runners: 硬盘性能 skipped for want
# of its tool would hide a broken download or pin. The report must come on stdout, with the
# host's public IPv4 address masked, and the progress log on stderr; both are printed. A second,
# short run checks that the address never comes through a proxy. CI runs it; README "Gates"
# documents it.
#
# Usage: scripts/smoke.sh <path to lowendaff-bench> [<argument>...]
# Arguments after the path go to the full run, for example --skip disk.fio. Linux only, with
# network access for the tools. Run it as root, or as a user with sudo. As root, the non-root
# check runs as nobody, so nobody must be able to reach the binary.
set -eu

fail() {
    echo "smoke: $*" >&2
    exit 1
}

[ $# -ge 1 ] || fail "usage: scripts/smoke.sh <path to lowendaff-bench> [<argument>...]"
bin=$(realpath "$1")
shift

[ "$(head -c 4 "$bin" | od -An -tx1 | tr -d ' \n')" = 7f454c46 ] || fail "$bin is not an ELF executable"
# ldd prints "not a dynamic executable" for static binaries, "statically linked" for static-pie.
ldd "$bin" 2>&1 | grep -Eq 'not a dynamic executable|statically linked' || fail "$bin is not statically linked"
"$bin" --list | grep -q '个测试项' || fail "--list printed no plugin count"

# Commands that run the binary as root and as an unprivileged user. They are split on
# purpose, so they stay unquoted below.
if [ "$(id -u)" -eq 0 ]; then
    as_root=""
    as_user="setpriv --reuid=65534 --regid=65534 --clear-groups"
else
    as_root="sudo"
    as_user=""
fi

# The non-root run must stop at the root check. Exit code 2 alone does not show that: usage
# errors and an unwritable run directory exit with 2 too.
set +e
refusal=$($as_user "$bin" 2>&1 >/dev/null)
code=$?
set -e
[ "$code" -eq 2 ] || fail "non-root run exited with $code, expected 2"
case $refusal in
    *"必须以 root 身份运行"*) ;;
    *) fail "non-root run exited with 2 but not at the root check: $refusal" ;;
esac

# Not under /tmp: the run directory must sit on the tested disk.
work=$(mktemp -d "$PWD/smoke.XXXXXX")
log=$(mktemp)
trap 'rm -f "$log"' EXIT
output=$($as_root "$bin" --work-dir "$work" "$@" 2>"$log") || {
    cat "$log" >&2
    fail "root run failed"
}
cat "$log" >&2
printf '%s\n' "$output"
stamp='^\[[0-9][0-9]:[0-9][0-9]:[0-9][0-9]\] '
grep -q "${stamp}主机概况：" "$log" || fail "stderr shows no host"
grep -q "${stamp}网络：IPv4 " "$log" || fail "stderr shows no network"
items=$(printf '%s\n' "$output" | sed -n 's/^共 \([0-9]*\) 项.*/\1/p')
if [ "${items:-0}" -gt 0 ]; then
    grep -q "${stamp}[a-z][a-z0-9.-]* 开始\$" "$log" || fail "stderr shows no test item starting"
    grep -q "${stamp}[a-z][a-z0-9.-]* 完成，用时 " "$log" ||
        fail "stderr shows no test item completing"
fi
# The runners reach the Internet over IPv4 only: its line shows a masked address and its source.
masked='[0-9]+\.[0-9]+\.\*\.\*'
sources='Cloudflare|ipify|icanhazip'
printf '%s\n' "$output" | grep -Eq "^  IPv4 +可用；公网地址 $masked（来自 ($sources)）\$" ||
    fail "the IPv4 line shows no masked public address and its source"
if printf '%s\n' "$output" | grep -q '^\[[0-9][0-9]:'; then
    fail "progress lines on stdout"
fi
# The public address never comes through a proxy, which would report its own: with proxy
# variables that point nowhere, the IPv4 line still shows one.
proxied=$($as_root env HTTPS_PROXY=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9 \
    "$bin" --work-dir "$work" --skip cpu,memory,disk 2>/dev/null) ||
    fail "the run with a proxy set failed"
printf '%s\n' "$proxied" | grep -Eq "^  IPv4 +可用；公网地址 $masked（来自 ($sources)）\$" ||
    fail "the public address went through the proxy"
printf '%s\n' "$output" | grep -q '^共 [0-9]* 项' || fail "root run printed no summary"
printf '%s\n' "$output" | grep -q '，跳过 0，失败 0，超时 0，' ||
    fail "a test item was skipped, failed or timed out"
[ -z "$(ls -A "$work")" ] || fail "run directory left behind in $work"
rmdir "$work"
echo "smoke: ok"
