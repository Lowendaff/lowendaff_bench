#!/bin/sh
# China carrier speed (network.taier) against one or two slots of a server list, chosen at random,
# in tests of 2 seconds over 2 streams: exercises --taier-list and live CAICT GlobalSpeed servers
# from Lowendaff/bench_data. CI runs it in jobs that no merge waits on, so a busy or unreachable
# carrier node is reported in the job summary rather than failing the build; only a misbehaving
# program fails.
#
# Usage: scripts/taier-check.sh <path to lowendaff-bench> <taier server list>
# Linux only, as root or with sudo.
set -eu

fail() {
    echo "taier-check: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/taier-check.sh <path to lowendaff-bench> <taier server list>"
bin=$(realpath "$1")
list=$(realpath "$2")
if [ "$(id -u)" -eq 0 ]; then
    as_root=""
else
    as_root="sudo"
fi

# Verify unprivileged --taier-list against live GitHub and against an unreachable proxy (testing
# the compiled-in bench_data fallback snapshot when embedded).
"$bin" --taier-list > /dev/null || fail "--taier-list failed"
"$bin" --taier-list guangdong > /dev/null || fail "--taier-list guangdong failed"

count=$(($(od -An -N1 -tu1 /dev/urandom | tr -d ' ') % 2 + 1))
slots=$(jq -r '
    .locations[]
    | "\(.province)/\(.city)" as $place
    | ([.nodes[].operator] | unique)[]
    | "\($place):\(.)"
' "$list" | shuf -n "$count")
[ -n "$slots" ] || fail "$list names no carrier slot"
chosen=$(printf '%s\n' "$slots" | jq -R . | jq -s -c .)
work=$(mktemp -d "$PWD/taier.XXXXXX")
log=$work.log
trap 'rm -f "$log"' EXIT
overrides=$(jq -n -c --arg list "$list" --argjson selections "$chosen" \
    '{"network.taier": {list: $list, selections: $selections, seconds: 2, streams: 2}}')

# shellcheck disable=SC2086 # $as_root is one word or none
report=$($as_root "$bin" -v --only network.taier --internal-overrides "$overrides" \
    --work-dir "$work" 2> "$log") || {
    cat "$log" >&2
    fail "the run failed"
}
cat "$log" >&2
printf '%s\n' "$report"
status=$(printf '%s\n' "$report" | sed -n 's/^  China carrier speed  *//p')
{
    printf '### China carrier speed against %s\n\n```\n' "$(printf '%s' "$slots" | tr '\n' ' ')"
    printf '%s\n' "$report" | sed -n '/^\[Network\]/,/^\[Summary\]/p' | sed '$d'
    printf '```\n'
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
[ -z "$(ls -A "$work")" ] || fail "run directory left behind in $work"
rmdir "$work"
case $status in
    "done in "* | "failed: no China carrier node gave a speed result")
        echo "taier-check: $status"
        ;;
    *) fail "the item ended so: $status" ;;
esac
