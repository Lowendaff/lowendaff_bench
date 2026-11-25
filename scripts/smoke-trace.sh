#!/bin/sh
# The files that point route tracing at the loopback for the smoke test, and its entry in the
# smoke overrides JSON: a target list that names 127.0.0.1 alone; a list of exchange ports that
# names it too; an answer of Google's that names no server; an answer of fast.com's whose first
# server is 127.0.0.1; and NextTrace without its service's lookups. CI runs it before smoke, after
# scripts/smoke-iperf3.sh.
#
# Usage: scripts/smoke-trace.sh <directory> <overrides json>
set -eu

fail() {
    echo "smoke-trace: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-trace.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2

cat > "$dir/targets.json" <<LIST
{
  "version": 1,
  "targets": [
    {
      "id": "loopback",
      "group": "international",
      "name": "Loopback",
      "asn": 64496,
      "ipv4": "127.0.0.1",
      "default": true
    },
    {
      "id": "loopback-cn",
      "group": "china",
      "name": "Loopback China TCP",
      "asn": 64496,
      "ipv4": "127.0.0.1",
      "default": true,
      "tcp": {
        "ipv4": 80
      }
    }
  ]
}
LIST
cat > "$dir/exchanges.json" <<LIST
{"version": 1, "ports": [
{"address": "127.0.0.1", "exchange": "Loopback IX"}
]}
LIST
printf 'no mapping\n' > "$dir/youtube.txt"
cat > "$dir/netflix.json" <<ANSWER
{"client": {"ip": "127.0.0.1"}, "targets": [{"name": "https://127.0.0.1/speedtest",
 "url": "https://127.0.0.1/speedtest", "location": {"city": "Loopback", "country": "ZZ"}}]}
ANSWER
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg list "$dir/targets.json" \
    --arg exchanges "$dir/exchanges.json" \
    --arg yt "$dir/youtube.txt" \
    --arg nf "$dir/netflix.json" \
    '. + {"network.trace": {
        list: $list, exchanges: $exchanges, youtube_api: $yt, netflix_api: $nf, geo: false
    }}' "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-trace: route tracing traces the loopback; $overrides updated"
