#!/bin/sh
# Cases for scripts/taier-servers.sh: every fixture in plugins/network/tests/fixtures/taier-lists,
# which the program's reader checks too, gets the verdict that its name gives; then export updates,
# size limits, default-location guards and sudden-loss guards in a scratch directory.
#
# Usage: scripts/taier-servers-cases.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
script=$here/taier-servers.sh
fixtures=$here/../plugins/network/tests/fixtures/taier-lists
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

failures=0
cases=0

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

for fixture in "$fixtures"/*.json; do
    name=$(basename "$fixture")
    case $name in
        good-*) expected=accept ;;
        bad-*) expected=reject ;;
        *) echo "FAIL: $name names no verdict" >&2; failures=$((failures + 1)); continue ;;
    esac
    verdict "$expected" "fixture $name" '' sh "$script" check-list "$fixture"
done

cd "$scratch"
mkdir servers

padded() {
    printf '{"version": 1, "locations": [{"id": "beijing", "province": "Beijing", "city": "Beijing", "nodes": [{"operator": "telecom", "ip": "192.0.2.1", "port": 65499}]}]}' > "$2"
    size=$(wc -c < "$2" | tr -d ' ')
    head -c $(($1 - size)) /dev/zero | tr '\0' ' ' >> "$2"
}
padded 65536 limit.json
verdict accept "a list of 64 KiB" '' sh "$script" check-list limit.json
padded 65537 large.json
verdict reject "a list of 64 KiB and a byte" 'more than 65536' sh "$script" check-list large.json

# Synthetic upstream export with Beijing, Shanghai, Guangzhou + N extra cities.
make_export() {
    jq -n --argjson extra "$1" '
        [ {ip: "198.51.100.1", port: "65499", name: "BJ CT", p: "\u5317\u4eac", c: "\u5317\u4eac", o: "\u7535\u4fe1"},
          {ip: "198.51.100.2", port: "65499", name: "SH CU", p: "\u4e0a\u6d77", c: "\u4e0a\u6d77", o: "\u8054\u901a"},
          {ip: "198.51.100.3", port: "9443",  name: "GZ CM", p: "\u5e7f\u4e1c", c: "\u5e7f\u5dde", o: "\u79fb\u52a8"} ]
        + [ range(1; $extra + 1) | {
              ip: "203.0.113.\(.)",
              port: "65499",
              name: "Node \(.)",
              p: "Jiangsu",
              c: "City\(.)",
              o: "telecom"
          } ]' > export.json
}

make_export 40
verdict reject "fewer than 50 nodes" 'fewer than 50' sh "$script" update export.json
make_export 50
verdict accept "53 valid nodes with all 3 defaults" 'created' sh "$script" update export.json
verdict accept "check written list" 'follows the rules' sh "$script" check
verdict accept "idempotent update" 'unchanged' sh "$script" update export.json

make_export 10
# 53 nodes minimum check fires first; test losing > 1/3 with 50 nodes in different cities:
jq -n '
    [ {ip: "198.51.100.1", port: "65499", name: "BJ", p: "\u5317\u4eac", c: "\u5317\u4eac", o: "\u7535\u4fe1"},
      {ip: "198.51.100.2", port: "65499", name: "SH", p: "\u4e0a\u6d77", c: "\u4e0a\u6d77", o: "\u8054\u901a"},
      {ip: "198.51.100.3", port: "9443",  name: "GZ", p: "\u5e7f\u4e1c", c: "\u5e7f\u5dde", o: "\u79fb\u52a8"} ]
    + [ range(101; 155) | {
          ip: "192.0.2.\(.-100)",
          port: "65499",
          name: "Other \(.)",
          p: "Zhejiang",
          c: "Other\(.)",
          o: "unicom"
      } ]' > drop.json
cp servers/taier.json kept.json
verdict reject "losing more than a third of locations at once" 'not trusted'     sh "$script" update drop.json
holds "existing list preserved on rejected update" cmp -s kept.json servers/taier.json

if [ "$failures" -gt 0 ]; then
    echo "taier-servers-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "taier-servers-cases: all $cases cases passed"
