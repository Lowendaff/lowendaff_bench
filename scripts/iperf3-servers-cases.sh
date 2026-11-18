#!/bin/sh
# Cases for scripts/iperf3-servers.sh: every fixture in plugins/network/tests/fixtures/server-lists,
# which the program's reader reads too, gets the verdict that its name gives; then lists,
# candidates and exports in a scratch directory, each of which the script must accept or refuse,
# writing what it should and nothing else.
#
# Usage: scripts/iperf3-servers-cases.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
script=$here/iperf3-servers.sh
fixtures=$here/../plugins/network/tests/fixtures/server-lists
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

# holds <name> <part>: the output of the last case holds the part.
holds() {
    cases=$((cases + 1))
    if printf '%s\n' "$output" | grep -q -F -e "$2"; then
        echo "ok: $1"
    else
        echo "FAIL: $1: the output lacks \"$2\": $output" >&2
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
[ "$seen" -ge 30 ] || { echo "FAIL: only $seen fixtures" >&2; failures=$((failures + 1)); }

cd "$scratch"
# A repository, where check searches for the files that name the list.
git init -q
mkdir servers

# A list of exactly 64 KiB passes; a byte more does not.
padded() { # <bytes> <file>
    printf '{"version": 1, "servers": []}' > "$2"
    size=$(wc -c < "$2" | tr -d ' ')
    head -c $(($1 - size)) /dev/zero | tr '\0' ' ' >> "$2"
}
padded 65536 limit.json
verdict accept "a list of 64 KiB" '' sh "$script" check-list limit.json
padded 65537 large.json
verdict reject "a list of 64 KiB and a byte" 'more than 65536' sh "$script" check-list large.json
verdict reject "no list" 'no list' sh "$script" check-list missing.json

# candidates <locations as JSON>: the candidates file.
candidates() {
    printf '{"version": 1, "locations": %s}\n' "$1" > servers/iperf3-candidates.json
}
place() { # <location> <provider> <host> <ports>
    printf '{"location": "%s", "provider": "%s", "host": "%s", "ports": "%s",' "$1" "$2" "$3" "$4"
    printf ' "families": ["ipv4", "ipv6"], "link": "10G"}'
}
location() { # <id> <country> <default> <candidates>
    printf '{"id": "%s", "country": "%s", "default": %s, "candidates": [%s]}' "$1" "$2" "$3" "$4"
}
la_clouvider=$(place "Los Angeles" Clouvider la.speedtest.clouvider.net 5200-5209)
la_leaseweb=$(place "Los Angeles" Leaseweb speedtest.lax12.us.leaseweb.net 5201-5210)
tokyo=$(place Tokyo Leaseweb speedtest.tyo11.jp.leaseweb.net 5201-5210)
osaka=$(place Osaka Example osaka.example.net 5201-5210)
four="[$(location los-angeles US true "$la_clouvider, $la_leaseweb"),
    $(location tokyo JP true "$tokyo"),
    $(location osaka JP false "$osaka"),
    $(location sao-paulo BR false "$(place "Sao Paulo" Edgoo speedtest.sao1.edgoo.net 9204-9240)")]"

# export <entries as JSON lines>: the export, padded to 50 servers with others of no candidate.
export_of() {
    {
        printf '[\n'
        for entry in "$@"; do
            printf '%s,\n' "$entry"
        done
        index=0
        while [ "$index" -lt 50 ]; do
            printf '{"IP/HOST": "filler-%d.example.org", "PORT": "5201", "OPTIONS": "-R"},\n' \
                "$index"
            index=$((index + 1))
        done
        printf '{"IP/HOST": "last.example.org", "PORT": "5201", "OPTIONS": "-R"}\n]\n'
    } > export.json
}
listed() { # <host> <ports> <options>
    printf '{"IP/HOST": "%s", "PORT": "%s", "OPTIONS": "%s", "COUNTRY": "XX"}' "$1" "$2" "$3"
}

# The first candidate listed with its host, ports and downloads wins; the Leaseweb server of Los
# Angeles stands in for its Clouvider one, Tokyo is listed without downloads, Osaka on other
# ports, and Sao Paulo not at all, so they are left out.
candidates "$four"
export_of "$(listed speedtest.lax12.us.leaseweb.net 5201-5210 '-R, -6')" \
    "$(listed la.speedtest.clouvider.net 5200-5209 '-6,-u')" \
    "$(listed speedtest.tyo11.jp.leaseweb.net 5201-5210 '-6')" \
    "$(listed osaka.example.net 5201-5205 '-R')"
verdict accept "a first list" 'created' sh "$script" update export.json
servers=$(jq -c '[.servers[] | [.id, .provider, .default]]' servers/iperf3.json)
cases=$((cases + 1))
if [ "$servers" = '[["los-angeles","Leaseweb",true]]' ]; then
    echo "ok: the list holds the first candidate listed with downloads"
else
    echo "FAIL: the list holds $servers" >&2
    failures=$((failures + 1))
fi
verdict accept "the list just written" '' sh "$script" check

# The same inputs give the same bytes.
cp servers/iperf3.json before.json
verdict accept "the same export again" 'unchanged' sh "$script" update export.json
verdict accept "nothing written" '' cmp before.json servers/iperf3.json

# A change is named by its location and providers.
export_of "$(listed la.speedtest.clouvider.net 5200-5209 '-R,-6,-u')" \
    "$(listed speedtest.tyo11.jp.leaseweb.net 5201-5210 '-R, -6')" \
    "$(listed osaka.example.net 5201-5210 '-R')"
verdict accept "a provider changed and two added" 'los-angeles: Leaseweb to Clouvider' \
    sh "$script" update export.json
holds "an addition is named" 'tokyo: Leaseweb, added'
holds "another addition is named" 'osaka: Example, added'
verdict accept "the list after the change" '' sh "$script" check
servers=$(jq -c '[.servers[].id]' servers/iperf3.json)
cases=$((cases + 1))
if [ "$servers" = '["los-angeles","tokyo","osaka"]' ]; then
    echo "ok: the list follows the candidates' order"
else
    echo "FAIL: the list holds $servers" >&2
    failures=$((failures + 1))
fi

# Safety stops: each leaves the list as it was.
cp servers/iperf3.json before.json
export_of "$(listed speedtest.tyo11.jp.leaseweb.net 5201-5210 '-R')"
verdict reject "losing two of three servers at once" 'lose 2 of the 3' \
    sh "$script" update export.json
verdict accept "nothing written after losing too many" '' cmp before.json servers/iperf3.json
# A third of the servers may go at once, but no more.
export_of "$(listed la.speedtest.clouvider.net 5200-5209 '-R')" \
    "$(listed speedtest.tyo11.jp.leaseweb.net 5201-5210 '-R')"
verdict accept "losing a third of the servers at once" 'osaka: Example, removed' \
    sh "$script" update export.json
export_of "$(listed la.speedtest.clouvider.net 5200-5209 '-R')"
verdict reject "losing half of the servers at once" 'lose 1 of the 2' \
    sh "$script" update export.json
cp before.json servers/iperf3.json
export_of "$(listed osaka.example.net 5201-5210 '-R')"
verdict reject "a list without a default server" 'no default server' \
    sh "$script" update export.json
printf '[%s]\n' "$(listed la.speedtest.clouvider.net 5200-5209 '-R')" > export.json
verdict reject "an export of one server" 'fewer than 50' sh "$script" update export.json
printf '<html>503</html>\n' > export.json
verdict reject "an export that is not JSON" 'not JSON' sh "$script" update export.json
printf '{"servers": []}\n' > export.json
verdict reject "an export that is no array" 'fewer than 50' sh "$script" update export.json
verdict accept "nothing written after a bad export" '' cmp before.json servers/iperf3.json

# The candidates follow their rules, and the list holds candidates only.
candidates "[$(location tokyo JP true "$tokyo"), $(location tokyo JP false "$osaka")]"
verdict reject "a location twice" 'location tokyo twice' sh "$script" check
candidates "[$(location Tokyo JP true "$tokyo")]"
verdict reject "a location id with a capital" 'location 1: id' sh "$script" check
candidates "[$(location tokyo JP '"yes"' "$tokyo")]"
verdict reject "a default that is a word" 'location 1: default' sh "$script" check
candidates "[$(location tokyo JP true '')]"
verdict reject "a location without candidates" 'location 1: candidates' sh "$script" check
candidates "[$(location tokyo JP true "$(place Tokyo Leaseweb -R 5201)")]"
verdict reject "a candidate whose host is an option" 'a candidate of location 1: host' \
    sh "$script" check
candidates "[$(location tokyo JP true "$(place Tokyo Leaseweb '' 5201)")]"
verdict reject "a candidate whose host is empty" 'a candidate of location 1: host' \
    sh "$script" check
candidates "$four"
jq '.servers[0].ports = "5201-5202"' servers/iperf3.json > list.json
cp list.json servers/iperf3.json
verdict reject "a server that is no candidate" 'server los-angeles is no candidate' \
    sh "$script" check

# Nothing builds from a list in servers/: besides Markdown, only the files that test and update
# the lists name them, and Rust code only by the address that the program downloads them from.
url=https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/iperf3.json
cp before.json servers/iperf3.json
mkdir -p src scripts .github/workflows
printf 'The list, servers/iperf3.json.\n' > README.md
printf 'jq . servers/iperf3.json\n' > scripts/ci-scope.sh
printf 'grep -x servers/iperf3.json\n' > scripts/check-commits.sh
printf 'run: scripts/network-check.sh bench servers/iperf3.json\n' > .github/workflows/ci.yml
printf 'pub const URL: &str =\n    "%s";\n' "$url" > src/lib.rs
verdict accept "the list named where it may be" '' sh "$script" check
# The route tracing item's lists are lists of servers/ too: their scripts and cases may name them,
# Rust code only by their addresses, and a mention of the folder alone names no list.
printf 'jq . servers/traceroute.json\n' > scripts/traceroute-targets.sh
printf 'cp x servers/traceroute.json\n' > scripts/traceroute-targets-cases.sh
printf 'jq . servers/netflix-ix.json\n' > scripts/netflix-ix.sh
printf 'cp x servers/netflix-ix.json\n' > scripts/netflix-ix-cases.sh
printf 'pub const PORTS: &str =\n    "%snetflix-ix.json";\n' "${url%iperf3.json}" > src/trace.rs
printf '# The rules that the lists in servers/ share.\n' > scripts/list-rules.jq
verdict accept "the route tracing lists named where they may be" '' sh "$script" check
printf 'const PORTS: &str = include_str!("../servers/netflix-ix.json");\n' > src/exchanges.rs
verdict reject "Rust code that builds the exchange ports in" \
    'these name one: src/exchanges.rs' sh "$script" check
rm src/exchanges.rs
printf 'cp servers/traceroute.json out/\n' > scripts/targets.sh
printf 'run: jq . servers/traceroute.json\n' > .github/workflows/release.yml
verdict reject "a script and a workflow that build from the target list" \
    'these name one: .github/workflows/release.yml scripts/targets.sh' sh "$script" check
rm scripts/targets.sh .github/workflows/release.yml
printf 'const LIST: &str = include_str!("../servers/iperf3.json");\n' > src/list.rs
verdict reject "Rust code that builds the list in" 'these name one: src/list.rs' \
    sh "$script" check
rm src/list.rs
printf '// %s, servers/iperf3-candidates.json\n' "$url" > src/both.rs
verdict reject "Rust code that names the list beside its address" 'these name one: src/both.rs' \
    sh "$script" check
rm src/both.rs
printf 'cp servers/iperf3.json build/\n' > build.sh
printf 'tar -cf list.tar servers/iperf3.json\n' > scripts/release.sh
verdict reject "scripts that build from the list" 'these name one: build.sh scripts/release.sh' \
    sh "$script" check
rm build.sh scripts/release.sh
verdict reject "a check outside a repository" 'cannot search the repository' \
    env GIT_DIR="$scratch/none" sh "$script" check
verdict accept "the list named where it may be, again" '' sh "$script" check

if [ "$failures" -gt 0 ]; then
    echo "iperf3-servers-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "iperf3-servers-cases: all $cases cases passed"
