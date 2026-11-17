#!/bin/sh
# The network item against one or two servers of a server list, chosen at random, in tests of 3
# seconds over 2 streams of at most 100 Mbit/s each: about 300 MB at most. CI runs it in a job
# that no merge waits for, with the list of the change under test, to show a server that keeps
# failing from GitHub's runners. It fails only when the program misbehaves: a server that is
# busy, cannot be reached, or gives no speed is reported in the job's summary, not failed.
#
# Usage: scripts/network-check.sh <path to lowendaff-bench> <server list>
# Linux only, as root or with sudo.
set -eu

fail() {
    echo "network-check: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/network-check.sh <path to lowendaff-bench> <server list>"
bin=$(realpath "$1")
list=$(realpath "$2")
if [ "$(id -u)" -eq 0 ]; then
    as_root=""
else
    as_root="sudo"
fi

count=$(($(od -An -N1 -tu1 /dev/urandom | tr -d ' ') % 2 + 1))
servers=$(jq -r '.servers[].id' "$list" | shuf -n "$count")
[ -n "$servers" ] || fail "$list names no server"
chosen=$(printf '%s\n' "$servers" | jq -R . | jq -s -c .)
work=$(mktemp -d "$PWD/network.XXXXXX")
config=$work.toml
log=$work.log
trap 'rm -f "$config" "$log"' EXIT
cat > "$config" <<CONFIG
[plugins."network.iperf3"]
list = "$list"
servers = $chosen
seconds = 3
streams = 2
rate = 100
CONFIG

# shellcheck disable=SC2086 # $as_root is one word or none
report=$($as_root "$bin" -v --only network.iperf3 --config "$config" --work-dir "$work" \
    2> "$log") || {
    cat "$log" >&2
    fail "the run failed"
}
cat "$log" >&2
printf '%s\n' "$report"
status=$(printf '%s\n' "$report" | sed -n 's/^  Network speed  *//p')
{
    printf '### Network speed against %s\n\n```\n' "$(printf '%s' "$servers" | tr '\n' ' ')"
    printf '%s\n' "$report" | sed -n '/^\[Network\]/,/^\[Summary\]/p' | sed '$d'
    printf '```\n'
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
[ -z "$(ls -A "$work")" ] || fail "run directory left behind in $work"
rmdir "$work"
case $status in
    "done in "* | "failed: no server gave a speed") echo "network-check: $status" ;;
    *) fail "the item ended so: $status" ;;
esac
