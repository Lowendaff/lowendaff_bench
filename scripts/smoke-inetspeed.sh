#!/bin/sh
# A local Apple CDN (mensura / iNetSpeed) responder on 127.0.0.1 for the smoke test, answering
# GET /api/v1/gm/small, GET /api/v1/gm/large, and chunked PUT /api/v1/gm/slurp on 127.0.0.1:18086,
# and updating network.inetspeed in the smoke overrides JSON so CI never waits on a third party.
#
# Usage: scripts/smoke-inetspeed.sh <directory> <overrides json>
set -eu

fail() {
    echo "smoke-inetspeed: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-inetspeed.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
port=18086

cat > "$dir/server.py" <<'PY'
import http.server
import socketserver
import sys

PORT = int(sys.argv[1])
LARGE = b"\x55" * (512 * 1024)


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        if self.path.startswith("/api/v1/gm/large"):
            body = LARGE
        else:
            body = b"ok"
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except OSError:
            return

    def do_PUT(self):
        te = (self.headers.get("Transfer-Encoding") or "").lower()
        if "chunked" in te:
            while True:
                line = self.rfile.readline()
                if not line:
                    break
                size_hex = line.split(b";", 1)[0].strip()
                try:
                    size = int(size_hex, 16)
                except ValueError:
                    break
                if size == 0:
                    self.rfile.readline()
                    break
                remaining = size + 2
                while remaining > 0:
                    chunk = self.rfile.read(remaining)
                    if not chunk:
                        remaining = 0
                        break
                    remaining -= len(chunk)
        else:
            length = int(self.headers.get("Content-Length", "0"))
            if length > 0:
                self.rfile.read(length)
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, fmt, *args):
        return


class ReuseServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


with ReuseServer(("127.0.0.1", PORT), Handler) as httpd:
    httpd.serve_forever()
PY

python3 "$dir/server.py" "$port" >/dev/null 2>&1 &
echo "$!" > "$dir/server.pid"

tries=0
while ! curl --fail --silent "http://127.0.0.1:$port/api/v1/gm/small" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local iNetSpeed server did not start on 127.0.0.1:$port"
    sleep 0.1
done

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg base "http://127.0.0.1:$port" \
    '. + {"network.inetspeed": {dl_url: ($base + "/api/v1/gm/large"), ul_url: ($base + "/api/v1/gm/slurp"), latency_url: ($base + "/api/v1/gm/small"), threads: 2, timeout: 1, max: "512KiB", latency_count: 3}}' \
    "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-inetspeed: server listens on 127.0.0.1:$port; $overrides updated"
