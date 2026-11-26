#!/bin/sh
# A local TCP handshake responder on 127.0.0.1 for the smoke test, accepting TCP handshakes and
# closing cleanly while updating network.tcping in the smoke overrides JSON so CI exercises
# network.tcping end to end without contacting external carrier or cloud servers.
#
# Usage: scripts/smoke-tcping.sh <directory> <overrides json>
# Starts the server on 127.0.0.1:18089, writes server.pid and tcping.json into the directory, and
# updates network.tcping in <overrides json>.
set -eu

fail() {
    echo "smoke-tcping: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-tcping.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
port=18089

cat > "$dir/server.py" <<'PY'
import socketserver
import sys

PORT = int(sys.argv[1])


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        return


class ReuseServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


with ReuseServer(("127.0.0.1", PORT), Handler) as srv:
    srv.serve_forever()
PY

python3 "$dir/server.py" "$port" >/dev/null 2>&1 &
echo "$!" > "$dir/server.pid"

tries=0
while ! python3 -c "import socket; s=socket.create_connection(('127.0.0.1', $port), 0.5); s.close()" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local TCP responder did not start on 127.0.0.1:$port"
    sleep 0.1
done

cat > "$dir/tcping.json" <<LIST
{
  "version": 1,
  "targets": [
    {
      "id": "loopback-ct",
      "group": "china",
      "region": "Loopback",
      "city": "Loopback",
      "name": "Loopback Telecom",
      "operator": "telecom",
      "asn": 4134,
      "default": true,
      "ipv4": [
        {
          "host": "127.0.0.1",
          "port": $port
        }
      ]
    },
    {
      "id": "loopback-global",
      "group": "global",
      "region": "Loopback",
      "city": "Loopback",
      "name": "Loopback Hub",
      "operator": "other",
      "asn": 64512,
      "default": true,
      "ipv4": [
        {
          "host": "127.0.0.1",
          "port": $port
        }
      ]
    }
  ]
}
LIST

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg targets "$dir/tcping.json" \
    '. + {"network.tcping": {targets: $targets, count: 5}}' \
    "$overrides" > "$tmp"
mv "$tmp" "$overrides"
