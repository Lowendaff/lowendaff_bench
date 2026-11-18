#!/bin/sh
# Cases for scripts/traceroute-targets.sh: every fixture in
# plugins/network/tests/fixtures/target-lists, which the program's reader reads too, gets the
# verdict that its name gives; then lists in a scratch directory, each of which the script must
# accept or refuse with the rule it names.
#
# Usage: scripts/traceroute-targets-cases.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
script=$here/traceroute-targets.sh
fixtures=$here/../plugins/network/tests/fixtures/target-lists
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
[ "$seen" -ge 47 ] || { echo "FAIL: only $seen fixtures" >&2; failures=$((failures + 1)); }

cd "$scratch"
mkdir servers

# A list of exactly 64 KiB passes; a byte more does not.
padded() { # <bytes> <file>
    printf '{"version": 1, "targets": []}' > "$2"
    size=$(wc -c < "$2" | tr -d ' ')
    head -c $(($1 - size)) /dev/zero | tr '\0' ' ' >> "$2"
}
padded 65536 limit.json
verdict accept "a list of 64 KiB" '' sh "$script" check-list limit.json
padded 65537 large.json
verdict reject "a list of 64 KiB and a byte" 'more than 65536' sh "$script" check-list large.json
verdict reject "no list" 'no list' sh "$script" check-list missing.json

# list <field>: a list of one target, Beijing CN2, with `field` added or set, as JSON.
list() {
    jq -n --argjson set "{$1}" '{version: 1, targets: [{id: "pek-4809", group: "china",
        name: "Beijing Telecom CN2", asn: 4809, ipv4: "ipv4.pek-4809.endpoint.nxtrace.org",
        default: true} + $set]}' > case.json
}
case_of() { # <accept|reject> <name> <part> <field>
    list "$4"
    verdict "$1" "$2" "$3" sh "$script" check-list case.json
}
case_of accept "a name that only looks like an address, over IPv6" '' '"ipv6": "01.2.3.4"'
case_of reject "an IPv4 address over IPv6" 'target 1: ipv6' '"ipv6": "192.0.2.1"'
case_of accept "an IPv6 address over IPv6" '' '"ipv6": "2001:db8::1"'
case_of accept "an IPv6 address with an IPv4 part over IPv6" '' '"ipv6": "::ffff:192.0.2.1"'
case_of reject "an IPv6 address over IPv4" 'target 1: ipv4' '"ipv4": "2001:db8::1"'
case_of accept "an IPv4 address over IPv4" '' '"ipv4": "192.0.2.1"'
case_of accept "a name that only looks like an address, over IPv4" '' '"ipv4": "256.1.1.1"'
case_of reject "an empty name over IPv6" 'target 1: ipv6' '"ipv6": ""'
case_of accept "no IPv6 name" '' '"ipv6": null'
case_of reject "an AS number written as a fraction" 'target 1: asn' '"asn": 4809.0'
case_of reject "a group in capitals" 'target 1: group' '"group": "China"'
case_of reject "a name of 41 characters" 'target 1: name' \
    "\"name\": \"$(printf '%041d' 0 | tr 0 N)\""
case_of accept "a name of 40 characters" '' "\"name\": \"$(printf '%040d' 0 | tr 0 N)\""
case_of reject "the id of a cache server" 'target 1: id' '"id": "youtube"'
case_of reject "a default that is a word" 'target 1: default' '"default": "yes"'

# check reads servers/traceroute.json.
verdict reject "check without a list" 'no list at servers/traceroute.json' sh "$script" check
cp "$fixtures/good-every-shape.json" servers/traceroute.json
verdict accept "check of a good list" 'follows the rules' sh "$script" check
cp "$fixtures/bad-id-twice.json" servers/traceroute.json
verdict reject "check of a list with an id twice" 'id pek-4809 twice' sh "$script" check
verdict reject "a command that does not exist" 'usage' sh "$script" update

if [ "$failures" -gt 0 ]; then
    echo "traceroute-targets-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "traceroute-targets-cases: all $cases cases passed"
