#!/bin/sh
# The files that point route tracing at the loopback for the smoke test, and its part of the config
# file: a target list that names 127.0.0.1 alone; a list of exchange ports that names it too, so
# that its hop is named with an exchange; an answer of Google's that names no server, so that
# YouTube's cache server is unavailable and nothing is looked up; an answer of fast.com's whose
# first server is 127.0.0.1; and NextTrace without its service's lookups. The smoke run then
# traces the loopback alone, as nobody with the file capability that the program gives NextTrace,
# and downloads no list and asks no third party, so that a merge never waits on one. CI runs it
# before smoke, after scripts/smoke-iperf3.sh.
#
# Usage: scripts/smoke-trace.sh <directory> <config file>
# Writes targets.json, exchanges.json, youtube.txt and netflix.json into the directory, and adds
# the item's table to the config file. Linux only, for smoke.
set -eu

fail() {
    echo "smoke-trace: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-trace.sh <directory> <config file>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
config=$2

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
cat >> "$config" <<CONFIG
[plugins."network.trace"]
list = "$dir/targets.json"
exchanges = "$dir/exchanges.json"
youtube_api = "$dir/youtube.txt"
netflix_api = "$dir/netflix.json"
geo = false
CONFIG
echo "smoke-trace: route tracing traces the loopback; $config holds its table"
