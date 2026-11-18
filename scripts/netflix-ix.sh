#!/bin/sh
# Netflix's exchange ports for the route tracing item (README "Route tracing"):
# servers/netflix-ix.json, which the program downloads at run time to name a hop at an address
# that Netflix's network takes at an internet exchange, written from PeeringDB's list of those
# ports. It runs in the directory that holds servers/, the repository's top in CI.
#
# Usage: scripts/netflix-ix.sh check
#            checks servers/netflix-ix.json against the rules
#        scripts/netflix-ix.sh check-list <file>
#            checks a list in the program's format against the rules that the program checks
#        scripts/netflix-ix.sh update <export>
#            writes servers/netflix-ix.json anew from PeeringDB's answer for Netflix
#            (https://www.peeringdb.com/api/netixlan?net_id=457): each port with an address,
#            IPv4's and IPv6's, with its exchange's name, sorted by address, one a line; prints
#            what changed, or "unchanged"
#
# The rules match the program's reader (plugins/network/src/trace/exchanges.rs): at most 64 KiB
# and 1000 ports of version 1; each address once, an IPv4 address, or an IPv6 address without an
# IPv4 part, written as the address prints (RFC 5952: in lowercase, without leading zeros, its
# longest run of zeros cut short); an exchange of 1 to 60 characters without control characters.
# Both read the fixtures in plugins/network/tests/fixtures/exchange-lists/, so that they agree.
# update writes nothing when the answer is not PeeringDB's list of Netflix's ports, names fewer
# than 50 ports, or would lose more than a third of the current list's ports at once.
set -eu

LIST=servers/netflix-ix.json
LIMIT=65536
# Netflix's network in PeeringDB.
NETFLIX=457

fail() {
    echo "netflix-ix: $*" >&2
    exit 1
}

# The checks of one port, as jq definitions after the rules that the lists share
# (scripts/list-rules.jq): `port_problems` gives the broken rules of the port it gets, each as a
# word.
# shellcheck disable=SC2016 # jq's variables and interpolations, not the shell's
RULES=$(cat "$(dirname "$0")/list-rules.jq")'
def ipv6_groups:
    if contains("::") then
        (split("::") as [$left, $right]
         | ($left | if . == "" then [] else split(":") end) as $before
         | ($right | if . == "" then [] else split(":") end) as $after
         | $before + [range(8 - ($before | length) - ($after | length)) | "0"] + $after)
    else split(":") end;
def groups: [ipv6_groups[] | ascii_downcase | sub("^0+(?<rest>.)"; "\(.rest)")];
def mapped: groups | .[:6] == ["0", "0", "0", "0", "0", "ffff"];
def canonical:
    groups as $groups
    | [ range(8)
        | select($groups[.] == "0" and (. == 0 or $groups[. - 1] != "0"))
        | . as $start
        | { start: $start,
            length: (([range($start; 8) | select($groups[.] != "0")] | first // 8) - $start) } ]
    | map(select(.length >= 2)) | sort_by([-.length, .start]) | first as $run
    | if $run == null then $groups | join(":")
      else ($groups[:$run.start] | join(":")) + "::"
          + ($groups[$run.start + $run.length:] | join(":")) end;
# An IPv4 address, or an IPv6 address without an IPv4 part, which the program refuses, written as
# the address prints.
def address_rule:
    type == "string"
    and (ipv4 or ipv6 and (contains(".") | not) and (mapped | not) and canonical == .);
def port_problems:
    [ (if .address | address_rule then empty else "address" end),
      (if .exchange | printable(60) then empty else "exchange" end) ];
def list_problems:
    if type != "object" then ["not an object"]
    elif .version != 1 then ["version \(.version) is not 1"]
    elif .ports | type != "array" then ["no ports"]
    elif .ports | length > 1000 then ["\(.ports | length) ports, more than 1000"]
    else
        [ .ports | to_entries[]
          | .key as $index
          | .value | if type == "object" then port_problems else ["not an object"] end
          | .[] | "port \($index + 1): \(.)" ]
        + [ [.ports[].address] | group_by(.)[] | select(length > 1)
            | "address \(.[0]) twice" ]
    end;
'

# check_list <file>: the list follows the rules.
check_list() {
    [ -f "$1" ] || fail "no list at $1"
    size=$(wc -c < "$1" | tr -d ' ')
    [ "$size" -le "$LIMIT" ] || fail "$1 has $size bytes, more than $LIMIT"
    problems=$(jq -r "$RULES list_problems[]" "$1" 2>&1) || fail "$1 is not JSON: $problems"
    [ -z "$problems" ] || fail "$1 breaks the rules: $(printf '%s' "$problems" | tr '\n' ';')"
}

# update <export>
update() {
    [ -f "$1" ] || fail "no export at $1"
    jq -e --argjson net "$NETFLIX" '
        type == "object" and (.data | type == "array")
        and all(.data[]; type == "object" and .net_id == $net)' "$1" > /dev/null 2>&1 ||
        fail "the answer is not PeeringDB's list of Netflix's ports: not trusted"
    count=$(jq '[.data[] | .ipaddr4, .ipaddr6 | select(. != null)] | length' "$1")
    [ "$count" -ge 50 ] || fail "the answer names $count ports, fewer than 50: not trusted"
    new=$(mktemp)
    trap 'rm -f "$new"' EXIT
    jq -r '
        [ .data[] | .name as $exchange
          | (.ipaddr4, .ipaddr6) | select(. != null)
          | {address: ., exchange: $exchange} ]
        | sort_by(.address)
        | "{\"version\": 1, \"ports\": [", (map(tojson) | join(",\n")), "]}"' "$1" > "$new"
    check_list "$new"
    if [ -f "$LIST" ]; then
        old=$(jq '.ports | length' "$LIST")
        lost=$(jq -n --slurpfile old "$LIST" --slurpfile new "$new" '
            [$old[0].ports[].address] - [$new[0].ports[].address] | length')
        [ $((lost * 3)) -le "$old" ] ||
            fail "the new list would lose $lost of the $old ports at once: not trusted"
        if cmp -s "$new" "$LIST"; then
            echo "unchanged"
            return
        fi
        jq -r -n --slurpfile old "$LIST" --slurpfile new "$new" '
            ($old[0].ports | map({key: .address, value: .exchange}) | from_entries) as $before
            | ($new[0].ports | map({key: .address, value: .exchange}) | from_entries) as $after
            | ([$before, $after] | add | keys)[]
            | . as $address
            | if $before[$address] == null then "\($address): \($after[$address]), added"
              elif $after[$address] == null then "\($address): \($before[$address]), removed"
              elif $before[$address] != $after[$address] then
                  "\($address): \($before[$address]) to \($after[$address])"
              else empty end'
    else
        echo "created"
    fi
    cp "$new" "$LIST"
}

case ${1:-} in
    check)
        check_list "$LIST"
        echo "netflix-ix: $LIST follows the rules"
        ;;
    check-list)
        [ $# -eq 2 ] || fail "usage: scripts/netflix-ix.sh check-list <file>"
        check_list "$2"
        ;;
    update)
        [ $# -eq 2 ] || fail "usage: scripts/netflix-ix.sh update <export>"
        update "$2"
        ;;
    *)
        echo "usage: scripts/netflix-ix.sh check | check-list <file> | update <export>" >&2
        exit 2
        ;;
esac
