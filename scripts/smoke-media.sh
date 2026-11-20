#!/bin/sh
# A local HTTP server on 127.0.0.1 for the smoke test, answering the probes of one streaming
# service per regional group, and its entry in the smoke overrides JSON. The smoke run then
# exercises ip.media end to end without contacting any external streaming platform. CI runs it
# before smoke, after scripts/smoke-ip.sh.
#
# Usage: scripts/smoke-media.sh <directory> <overrides json>
# Starts the server on 127.0.0.1:18080, writes server.pid into the directory, and updates ip.media
# in <overrides json>.
set -eu

fail() {
    echo "smoke-media: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-media.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
port=18080

cat > "$dir/server.py" <<'PY'
import http.server
import socketserver
import sys

PORT = int(sys.argv[1])

RESPONSES = {
    "/netflix/licensed": (200, "https://127.0.0.1/us/title/81280792", b"ok"),
    "/netflix/original": (200, "https://127.0.0.1/us/title/80018499", b"ok"),
    "/abema/check": (200, None, b'{"isoCountryCode":"JP"}'),
    "/bbc-iplayer/check": (200, None, b'{"media":[]}'),
    "/starz/check": (200, None, b'{"isAllowedCountry":true,"country":"US"}'),
    "/stan/check": (400, None, b'{"errors":["InvalidCredentials"]}'),
}

class Handler(http.server.BaseHTTPRequestHandler):
    def respond(self):
        path = self.path.split("?", 1)[0]
        status, _, body = RESPONSES.get(path, (200, None, b"{}"))
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self.respond()

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        if length > 0:
            self.rfile.read(length)
        self.respond()

    def log_message(self, fmt, *args):
        return

class ReuseServer(socketserver.TCPServer):
    allow_reuse_address = True

with ReuseServer(("127.0.0.1", PORT), Handler) as httpd:
    httpd.serve_forever()
PY

python3 "$dir/server.py" "$port" >/dev/null 2>&1 &
echo "$!" > "$dir/server.pid"

tries=0
while ! curl --fail --silent "http://127.0.0.1:$port/abema/check" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local media server did not start on 127.0.0.1:$port"
    sleep 0.1
done

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg base "http://127.0.0.1:$port" \
    '. + {"ip.media": {
        services: ["netflix", "abema", "bbc-iplayer", "starz", "stan"], base_url: $base
    }}' "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-media: media server listens on 127.0.0.1:$port; $overrides updated"
