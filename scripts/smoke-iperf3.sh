#!/bin/sh
# An iperf3 server on this host for the smoke test, of the build that the program pins, and the
# files that point the network item at it: a server list that names this server alone, and inline
# JSON overrides with that list and short tests. The smoke run then measures no public server and
# downloads no list, so that a merge never waits on a third party. CI runs it before smoke.
#
# Usage: scripts/smoke-iperf3.sh <directory>
# Writes iperf3, servers.json and overrides.json into the directory; the server listens on
# 127.0.0.1, port 5201, until the host stops. Linux only, with network access for the build, and
# cargo, which reads the pin.
set -eu

fail() {
    echo "smoke-iperf3: $*" >&2
    exit 1
}

[ $# -eq 1 ] || fail "usage: scripts/smoke-iperf3.sh <directory>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
arch=$(uname -m)
pin=$(cargo run -q --locked -p bench-tool --example pin -- iperf3 "$arch") ||
    fail "no pin of iperf3 for $arch"
url=$(printf '%s\n' "$pin" | sed -n 1p)
sha256=$(printf '%s\n' "$pin" | sed -n 2p)
size=$(printf '%s\n' "$pin" | sed -n 3p)
curl --fail --silent --show-error --location --output "$dir/iperf3" "$url"
[ "$(wc -c < "$dir/iperf3" | tr -d ' ')" = "$size" ] || fail "the download has not its pinned size"
echo "$sha256  $dir/iperf3" | sha256sum -c - > /dev/null ||
    fail "the download does not match its pinned SHA-256"
chmod 755 "$dir/iperf3"
"$dir/iperf3" --server --bind 127.0.0.1 --port 5201 --daemon --pidfile "$dir/server.pid"

cat > "$dir/servers.json" <<LIST
{
  "version": 1,
  "servers": [
    {
      "id": "loopback",
      "country": "ZZ",
      "location": "Loopback",
      "provider": "smoke",
      "host": "127.0.0.1",
      "ports": "5201",
      "families": ["ipv4"],
      "link": "lo",
      "default": true
    }
  ]
}
LIST
jq -n --arg list "$dir/servers.json" \
    '{"network.iperf3": {list: $list, seconds: 2, streams: 2}}' > "$dir/overrides.json"
echo "smoke-iperf3: iperf3 listens on 127.0.0.1:5201; overrides written to $dir/overrides.json"
