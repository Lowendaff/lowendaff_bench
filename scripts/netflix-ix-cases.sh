#!/bin/sh
# Cases for scripts/netflix-ix.sh: every fixture in plugins/network/tests/fixtures/exchange-lists,
# which the program's reader reads too, gets the verdict that its name gives; then lists and
# PeeringDB's answers in a scratch directory, each of which the script must accept or refuse,
# writing what it should and nothing else.
#
# Usage: scripts/netflix-ix-cases.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
script=$here/netflix-ix.sh
fixtures=$here/../plugins/network/tests/fixtures/exchange-lists
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

failures=0
cases=0

# verdict <accept|reject> <name> <part of the expected output> <command...>
verdict() {
    cases=$((cases + 1))
    expected=$1 name=$2 part=$3
    shift 3
    if output=$("$@" 2>&1); then
        got=accept
    else
        got=reject
    fi
    if [ "$got" != "$expected" ]; then
        echo "FAIL: $name: expected $expected, got $got: $output" >&2
        failures=$((failures + 1))
    elif [ -n "$part" ] && ! printf '%s\n' "$output" | grep -q -F -e "$part"; then
        echo "FAIL: $name: the output lacks \"$part\": $output" >&2
        failures=$((failures + 1))
    else
        echo "ok: $name"
    fi
}

# holds <name> <command...>: the command succeeds.
holds() {
    cases=$((cases + 1))
    name=$1
    shift
    if "$@"; then
        echo "ok: $name"
    else
        echo "FAIL: $name" >&2
        failures=$((failures + 1))
    fi
}

# named <part>: the output of the last case holds the part.
named() {
    printf '%s\n' "$output" | grep -q -F -e "$1"
}

# same <name> <expected> <got>: two values agree.
same() {
    cases=$((cases + 1))
    if [ "$2" = "$3" ]; then
        echo "ok: $1"
    else
        echo "FAIL: $1: expected $2, got $3" >&2
        failures=$((failures + 1))
    fi
}

seen=0
for fixture in "$fixtures"/*.json; do
    name=$(basename "$fixture")
    case $name in
        good-*) expected=accept ;;
        bad-*) expected=reject ;;
        *) echo "FAIL: $name names no verdict" >&2; failures=$((failures + 1)); continue ;;
    esac
    verdict "$expected" "fixture $name" '' sh "$script" check-list "$fixture"
    seen=$((seen + 1))
done
[ "$seen" -ge 36 ] || { echo "FAIL: only $seen fixtures" >&2; failures=$((failures + 1)); }

cd "$scratch"
mkdir servers

# A list of exactly 64 KiB passes; a byte more does not.
padded() { # <bytes> <file>
    printf '{"version": 1, "ports": []}' > "$2"
    size=$(wc -c < "$2" | tr -d ' ')
    head -c $(($1 - size)) /dev/zero | tr '\0' ' ' >> "$2"
}
padded 65536 limit.json
verdict accept "a list of 64 KiB" '' sh "$script" check-list limit.json
padded 65537 large.json
verdict reject "a list of 64 KiB and a byte" 'more than 65536' sh "$script" check-list large.json
verdict reject "no list" 'no list' sh "$script" check-list missing.json

# answer <first> <count> [<jq of the entries>]: PeeringDB's answer for Netflix, with entries
# `first` to `first + count - 1`, each with an address of each family, as `filter` changes them.
answer() {
    jq -n --argjson first "$1" --argjson count "$2" "
        {data: [range(\$first; \$first + \$count) | {net_id: 457, name: \"IX \\(.)\",
            ipaddr4: \"198.51.100.\\(.)\", ipaddr6: \"2001:db8::\\(.)\"}] | ${3:-.}}" \
        > answer.json
}

printf '{' > answer.json
verdict reject "an answer that is not JSON" "not PeeringDB's list" sh "$script" update answer.json
printf '[]' > answer.json
verdict reject "an answer that is no object" "not PeeringDB's list" sh "$script" update answer.json
answer 1 30 'map(.net_id = 714)'
verdict reject "the ports of another network" "not PeeringDB's list" \
    sh "$script" update answer.json
answer 1 24
verdict reject "an answer of 48 ports" 'fewer than 50' sh "$script" update answer.json
verdict reject "no export" 'no export' sh "$script" update none.json
same "nothing written so far" no "$([ -e servers/netflix-ix.json ] && echo yes || echo no)"

answer 1 25
verdict accept "an answer of 50 ports" 'created' sh "$script" update answer.json
verdict accept "the list just written" 'follows the rules' sh "$script" check
same "every port" 50 "$(jq '.ports | length' servers/netflix-ix.json)"
same "one port a line, sorted by address" \
    '{"address":"198.51.100.1","exchange":"IX 1"},' "$(sed -n 2p servers/netflix-ix.json)"
same "sorted by address as text, not as the answer lists them" \
    '{"address":"198.51.100.10","exchange":"IX 10"},' "$(sed -n 3p servers/netflix-ix.json)"
same "the list's last line" ']}' "$(tail -n 1 servers/netflix-ix.json)"
cp servers/netflix-ix.json first.json
verdict accept "the same answer again" 'unchanged' sh "$script" update answer.json
holds "the same bytes" cmp -s first.json servers/netflix-ix.json
answer 1 25 'reverse'
verdict accept "the same ports in another order" 'unchanged' sh "$script" update answer.json
holds "the same bytes again" cmp -s first.json servers/netflix-ix.json

# A port lost, one added and one renamed: each named, the list written anew.
answer 2 25 'map(if .name == "IX 3" then .name = "IX three" else . end)'
verdict accept "a changed answer" '198.51.100.1: IX 1, removed' sh "$script" update answer.json
holds "an added port named" named '198.51.100.26: IX 26, added'
holds "a renamed port named" named '198.51.100.3: IX 3 to IX three'
cp servers/netflix-ix.json changed.json

# Losing more than a third at once is not trusted, and nothing is written.
answer 100 25
verdict reject "an answer that would lose every port" 'not trusted' \
    sh "$script" update answer.json
holds "the list kept" cmp -s changed.json servers/netflix-ix.json

# An address that is not written as it prints breaks a rule, and nothing is written.
answer 2 25 'map(if .name == "IX 4" then .ipaddr6 = "2001:DB8::4" else . end)'
verdict reject "an address in capitals" 'breaks the rules' sh "$script" update answer.json
holds "the list kept again" cmp -s changed.json servers/netflix-ix.json

# An exchange named outside ASCII keeps its name.
answer 2 25 'map(if .name == "IX 5" then .name = "IX.br (PTT.br) São Paulo" else . end)'
verdict accept "an exchange named outside ASCII" 'IX 5 to IX.br (PTT.br) São Paulo' \
    sh "$script" update answer.json
verdict reject "a command that does not exist" 'usage' sh "$script" rewrite

if [ "$failures" -gt 0 ]; then
    echo "netflix-ix-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "netflix-ix-cases: all $cases cases passed"
