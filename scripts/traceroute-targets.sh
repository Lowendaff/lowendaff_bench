#!/bin/sh
# The target list of the route tracing item (README "Route tracing"): servers/traceroute.json,
# which the program downloads at run time. It changes by pull request only; nothing writes it. It
# runs in the directory that holds servers/, the repository's top in CI.
#
# Usage: scripts/traceroute-targets.sh check
#            checks servers/traceroute.json against the rules
#        scripts/traceroute-targets.sh check-list <file>
#            checks a list in the program's format against the rules that the program checks
#
# The rules match the program's reader (plugins/network/src/trace/targets.rs): at most 64 KiB and
# 200 targets of version 1; ids of 1 to 40 characters of a to z, digits and -, each once, and
# neither youtube nor netflix, the cache servers' ids; a group, china or international; a name of
# 1 to 40 characters without control characters; an AS number from 1 to 4294967295; an ipv4 that
# is a DNS name or an IPv4 address, and an ipv6, when it is given, that is a DNS name or an IPv6
# address; default true or false when it is given. Both read the fixtures in
# plugins/network/tests/fixtures/target-lists/, so that they agree.
set -eu

LIST=servers/traceroute.json
LIMIT=65536

fail() {
    echo "traceroute-targets: $*" >&2
    exit 1
}

# The checks of one target, as jq definitions: `target_problems` gives the broken rules of the
# target it gets, each as a word. An address is of its field's family; a name that is no address,
# such as 01.2.3.4, a DNS name.
# shellcheck disable=SC2016 # jq's variables and interpolations, not the shell's
RULES='
def ipv4_octet: "(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])";
def ipv4: test("^(" + ipv4_octet + "\\.){3}" + ipv4_octet + "$");
def ipv6:
    (split("::") as $halves
    | ($halves | length) as $parts
    | [$halves[] | if . == "" then [] else split(":") end] as $sides
    | ($sides | add // []) as $groups
    | ($groups | length) as $count
    | ($groups | if length > 0 and (.[-1] | ipv4) then $count + 1 else $count end) as $width
    | test("^[0-9A-Fa-f:.]+$") and contains(":") and $parts <= 2
        and all($groups | to_entries[];
            (.value | test("^[0-9A-Fa-f]{1,4}$"))
            or (.key == $count - 1 and (.value | ipv4)))
        and (if $parts == 2 then $width <= 7 else $width == 8 end));
def dns_name:
    length >= 1 and length <= 253
    and all(split(".")[]; test("^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?$"));
def name_of(family):
    type == "string"
    and (if family == "ipv4" then ipv4 or (ipv6 | not) and dns_name
         else ipv6 or (ipv4 | not) and dns_name end);
def id_rule:
    type == "string" and test("^[a-z0-9-]{1,40}$") and . != "youtube" and . != "netflix";
def group_rule: . == "china" or . == "international";
def name_rule: type == "string" and length >= 1 and length <= 40 and (test("\\p{Cc}") | not);
def asn_rule:
    type == "number" and (tojson | test("^[0-9]+$")) and . >= 1 and . <= 4294967295;
def default_rule: . == null or type == "boolean";
def target_problems:
    [ (if .id | id_rule then empty else "id" end),
      (if .group | group_rule then empty else "group" end),
      (if .name | name_rule then empty else "name" end),
      (if .asn | asn_rule then empty else "asn" end),
      (if .ipv4 | name_of("ipv4") then empty else "ipv4" end),
      (if .ipv6 == null or (.ipv6 | name_of("ipv6")) then empty else "ipv6" end),
      (if .default | default_rule then empty else "default" end) ];
def list_problems:
    if type != "object" then ["not an object"]
    elif .version != 1 then ["version \(.version) is not 1"]
    elif .targets | type != "array" then ["no targets"]
    elif .targets | length > 200 then ["\(.targets | length) targets, more than 200"]
    else
        [ .targets | to_entries[]
          | .key as $index
          | .value | if type == "object" then target_problems else ["not an object"] end
          | .[] | "target \($index + 1): \(.)" ]
        + [ [.targets[].id] | group_by(.)[] | select(length > 1) | "id \(.[0]) twice" ]
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

case ${1:-} in
    check)
        check_list "$LIST"
        echo "traceroute-targets: $LIST follows the rules"
        ;;
    check-list)
        [ $# -eq 2 ] || fail "usage: scripts/traceroute-targets.sh check-list <file>"
        check_list "$2"
        ;;
    *)
        echo "usage: scripts/traceroute-targets.sh check | check-list <file>" >&2
        exit 2
        ;;
esac
