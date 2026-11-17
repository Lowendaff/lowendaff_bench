#!/bin/sh
# The server list of the network test item (README "Network speed"): servers/iperf3.json, which
# the program downloads at run time, written from the reviewed candidates of each location in
# servers/iperf3-candidates.json. It runs in the directory that holds servers/, the repository's
# top in CI.
#
# Usage: scripts/iperf3-servers.sh check
#            checks both files: each follows its rules, and every server of the list is one of
#            the candidates of its location, which has no other server in the list; and that
#            nothing builds from the list (check_names)
#        scripts/iperf3-servers.sh check-list <file>
#            checks a list in the program's format against the rules that the program checks
#        scripts/iperf3-servers.sh update <export>
#            writes servers/iperf3.json anew: for each location, its first candidate that
#            iperf3serverlist.net's export (listed_iperf3_servers.json) lists with the same host
#            and ports and with downloads (-R); prints what changed, or "unchanged"
#
# The rules match the program's reader (plugins/network/src/iperf3/servers.rs): at most 64 KiB
# and 100 servers of version 1; ids of 1 to 40 characters of a to z, digits and -, each once; a
# country of two capital letters; a host that is a DNS name or an IP address; one port or a range
# from 1024 to 65535, at most 64; families ipv4, ipv6 or both, each once; a place, provider and
# link of 1 to 40 characters without control characters; default true or false when it is
# given. Both read the fixtures in plugins/network/tests/fixtures/server-lists/, so that they
# agree. update writes nothing when the export is not a JSON array of 50 servers at least, or
# when the new list would have no default server or lose more than a third of the current one.
set -eu

LIST=servers/iperf3.json
CANDIDATES=servers/iperf3-candidates.json
LIMIT=65536
# The address that the program downloads the list from.
URL=https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/iperf3.json

fail() {
    echo "iperf3-servers: $*" >&2
    exit 1
}

# The checks of one server, as jq definitions: `server_problems` gives the broken rules of the
# server it gets, each as a word.
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
    length <= 253
    and all(split(".")[]; test("^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?$"));
def text_rule: type == "string" and length >= 1 and length <= 40 and (test("\\p{Cc}") | not);
def ports_rule:
    type == "string" and test("^[0-9]+(-[0-9]+)?$")
    and (split("-") as $ends
        | ($ends[0] | tonumber) as $first
        | ($ends[-1] | tonumber) as $last
        | $first >= 1024 and $last <= 65535 and $first <= $last and $last - $first < 64);
def families_rule:
    type == "array" and length >= 1
    and all(.[]; . == "ipv4" or . == "ipv6") and (unique | length) == length;
def place_problems:
    [ (if .host | type == "string" and (ipv4 or ipv6 or dns_name) then empty else "host" end),
      (if .ports | ports_rule then empty else "ports" end),
      (if .families | families_rule then empty else "families" end),
      (if [.location, .provider, .link] | all(.[]; text_rule) then empty else "text" end) ];
def id_rule: type == "string" and test("^[a-z0-9-]{1,40}$");
def country_rule: type == "string" and test("^[A-Z]{2}$");
def default_rule: . == null or type == "boolean";
def server_problems:
    [ (if .id | id_rule then empty else "id" end),
      (if .country | country_rule then empty else "country" end),
      (if .default | default_rule then empty else "default" end) ] + place_problems;
def list_problems:
    if type != "object" then ["not an object"]
    elif .version != 1 then ["version \(.version) is not 1"]
    elif .servers | type != "array" then ["no servers"]
    elif .servers | length > 100 then ["\(.servers | length) servers, more than 100"]
    else
        [ .servers | to_entries[]
          | .key as $index
          | .value | if type == "object" then server_problems else ["not an object"] end
          | .[] | "server \($index + 1): \(.)" ]
        + [ [.servers[].id] | group_by(.)[] | select(length > 1) | "id \(.[0]) twice" ]
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

# check_candidates: the candidates follow their rules: each location has an id, a country, a
# default and candidates, each of which follows the rules of a server's place.
check_candidates() {
    problems=$(jq -r "$RULES"'
        if .version != 1 then "version \(.version) is not 1"
        elif .locations | type != "array" or length == 0 then "no locations"
        else
            ( .locations | to_entries[]
              | .key as $index | .value
              | ( ( (if .id | id_rule then empty else "id" end),
                    (if .country | country_rule then empty else "country" end),
                    (if .default | type == "boolean" then empty else "default" end),
                    (if .candidates | type == "array" and length > 0 then empty
                     else "candidates" end) )
                  | "location \($index + 1): \(.)" ),
                ( .candidates[]? | place_problems[]
                  | "a candidate of location \($index + 1): \(.)" ) ),
            ( [.locations[].id] | group_by(.)[] | select(length > 1)
              | "location \(.[0]) twice" )
        end' "$CANDIDATES" 2>&1) || fail "$CANDIDATES is not JSON: $problems"
    [ -z "$problems" ] ||
        fail "$CANDIDATES breaks the rules: $(printf '%s' "$problems" | tr '\n' ';')"
}

# The candidates as the servers that they would be, in the list's order: one array per location.
# shellcheck disable=SC2016 # jq's variables, not the shell's
SERVERS_OF_LOCATIONS='
    [ .locations[] | . as $location
      | [ .candidates[] | {
            id: $location.id, country: $location.country,
            location, provider, host, ports, families, link,
            default: $location.default } ] ]'

# check_names: nothing builds from the list, so that a change of the list alone needs no build,
# and CI may skip the build for it: besides Markdown, only this script and its cases, ci-scope
# and its cases, and the workflows that test and update the list name it, and Rust code only by
# the address above. It searches every file that git tracks or would track.
check_names() {
    if found=$(git grep -n -I --untracked -F servers/iperf3 -- . ':(exclude)*.md' \
        ':(exclude)scripts/iperf3-servers.sh' ':(exclude)scripts/iperf3-servers-cases.sh' \
        ':(exclude)scripts/ci-scope.sh' ':(exclude)scripts/ci-scope-cases.sh' \
        ':(exclude).github/workflows/ci.yml' ':(exclude).github/workflows/data.yml'); then
        :
    else
        [ $? -eq 1 ] || fail "cannot search the repository for the files that name the list"
        found=
    fi
    named=$(printf '%s\n' "$found" | awk -v url="$URL" '
        {
            file = $0; sub(/:.*/, "", file)
            text = $0; sub(/^[^:]*:[^:]*:/, "", text)
            if (file ~ /\.rs$/)
                while ((at = index(text, url)) > 0)
                    text = substr(text, 1, at - 1) substr(text, at + length(url))
            if (index(text, "servers/iperf3") > 0) print file
        }' | sort -u | tr '\n' ' ')
    [ -z "$named" ] || fail "nothing may build from the list, but these name it: ${named% }"
}

check() {
    check_candidates
    check_list "$LIST"
    check_names
    problems=$(jq -r -n --slurpfile candidates "$CANDIDATES" --slurpfile list "$LIST" '
        ($candidates[0] | '"$SERVERS_OF_LOCATIONS"') as $locations
        | $list[0].servers[]
        | . as $server
        | if any($locations[][]; . == $server) then empty
          else "server \($server.id) is no candidate of its location" end') ||
        fail "the files cannot be compared: $problems"
    [ -z "$problems" ] || fail "$(printf '%s' "$problems" | tr '\n' ';')"
    echo "iperf3-servers: $CANDIDATES and $LIST follow the rules"
}

# update <export>
update() {
    [ -f "$1" ] || fail "no export at $1"
    count=$(jq 'if type == "array" then length else 0 end' "$1" 2>/dev/null) ||
        fail "the export is not JSON"
    [ "$count" -ge 50 ] || fail "the export lists $count servers, fewer than 50: not trusted"
    check_candidates
    new=$(mktemp)
    trap 'rm -f "$new"' EXIT
    jq --slurpfile export "$1" '
        def downloads:
            (.OPTIONS // "") | split(",") | map(gsub("^ +| +$"; "")) | index("-R") != null;
        '"$SERVERS_OF_LOCATIONS"' as $locations
        | ($export[0]) as $listed
        | { version: 1,
            servers: [ $locations[]
                       | map(select(. as $server
                             | any($listed[];
                                 .["IP/HOST"] == $server.host and .PORT == $server.ports
                                 and downloads)))
                       | first // empty ] }' "$CANDIDATES" > "$new"
    check_list "$new"
    defaults=$(jq '[.servers[] | select(.default)] | length' "$new")
    [ "$defaults" -gt 0 ] || fail "the new list would have no default server"
    if [ -f "$LIST" ]; then
        old=$(jq '.servers | length' "$LIST")
        lost=$(jq -n --slurpfile old "$LIST" --slurpfile new "$new" '
            [$old[0].servers[].id] - [$new[0].servers[].id] | length')
        [ $((lost * 3)) -le "$old" ] ||
            fail "the new list would lose $lost of the $old servers at once: not trusted"
        if cmp -s "$new" "$LIST"; then
            echo "unchanged"
            return
        fi
        jq -r -n --slurpfile old "$LIST" --slurpfile new "$new" '
            ($old[0].servers | map({key: .id, value: .}) | from_entries) as $before
            | ($new[0].servers | map({key: .id, value: .}) | from_entries) as $after
            | ([$before, $after] | add | keys_unsorted | unique)[]
            | . as $id
            | if $before[$id] == null then "\($id): \($after[$id].provider), added"
              elif $after[$id] == null then "\($id): \($before[$id].provider), removed"
              elif $before[$id] != $after[$id] then
                  "\($id): \($before[$id].provider) to \($after[$id].provider)"
              else empty end'
    else
        echo "created"
    fi
    cp "$new" "$LIST"
}

case ${1:-} in
    check) check ;;
    check-list)
        [ $# -eq 2 ] || fail "usage: scripts/iperf3-servers.sh check-list <file>"
        check_list "$2"
        ;;
    update)
        [ $# -eq 2 ] || fail "usage: scripts/iperf3-servers.sh update <export>"
        update "$2"
        ;;
    *)
        echo "usage: scripts/iperf3-servers.sh check | check-list <file> | update <export>" >&2
        exit 2
        ;;
esac
