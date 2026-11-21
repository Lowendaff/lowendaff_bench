#!/bin/sh
# A local CAICT GlobalSpeed responder on 127.0.0.1 for the smoke test, answering dovalid queue
# admission and release, multi-stream HTTP/1.1 downloads and uploads, and 64 B / 1400 B TCP probes,
# and updating network.taier in the smoke overrides JSON. The smoke run then exercises
# network.taier end to end without contacting external carrier servers.
#
# Usage: scripts/smoke-taier.sh <directory> <overrides json>
# Starts the server on 127.0.0.1:18084, writes server.pid and taier.json into the directory, and
# updates network.taier in <overrides json>.
set -eu

fail() {
    echo "smoke-taier: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-taier.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
port=18084

cat > "$dir/server.py" <<'PY'
import socketserver
import sys

PORT = int(sys.argv[1])
CHUNK = b"\x5a" * 65536


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        buf = b""
        while b"\r\n\r\n" not in buf and len(buf) < 8192:
            try:
                more = self.request.recv(4096)
            except OSError:
                return
            if not more:
                return
            buf += more

        head = buf.split(b"\r\n\r\n", 1)[0].decode("latin1", errors="replace")
        first = head.splitlines()[0] if head else ""

        try:
            if first.startswith("GET /speed/dovalid?"):
                if "X-Pad:" in head:
                    body = b"0-"
                else:
                    body = b"1-smoke-taier-uuid-123"
                chunk = f"{len(body):x}\r\n".encode("ascii") + body + b"\r\n0\r\n\r\n"
                resp = (
                    b"HTTP/1.1 200 \r\n"
                    b"Content-Type: text/plain\r\n"
                    b"Transfer-Encoding: chunked\r\n"
                    b"Connection: close\r\n\r\n" + chunk
                )
                self.request.sendall(resp)
            elif first.startswith("POST /speed/dovalid?"):
                self.request.sendall(
                    b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                )
            elif first.startswith("GET /speed/File(1G).dl?"):
                self.request.sendall(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                while True:
                    self.request.sendall(CHUNK)
            elif first.startswith("POST /speed/doAnalsLoad.do"):
                while True:
                    data = self.request.recv(65536)
                    if not data:
                        break
            else:
                self.request.sendall(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
        except OSError:
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
while ! curl --fail --silent "http://127.0.0.1:$port/speed/dovalid?key=" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local Taier server did not start on 127.0.0.1:$port"
    sleep 0.1
done

cat > "$dir/taier.json" <<LIST
{
  "version": 1,
  "locations": [
    {
      "id": "loopback",
      "province": "Loopback",
      "city": "Loopback",
      "default": true,
      "nodes": [
        {
          "operator": "telecom",
          "ip": "127.0.0.1",
          "port": $port
        }
      ]
    }
  ]
}
LIST

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg list "$dir/taier.json" \
    '. + {"network.taier": {list: $list, seconds: 1, streams: 2}}' \
    "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-taier: Taier server listens on 127.0.0.1:$port; $overrides updated"
