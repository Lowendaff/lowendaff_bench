#!/bin/sh
# A local Cloudflare Speed + RFC 5389 STUN responder on 127.0.0.1 for the smoke test, answering
# /meta, /locations, /__down, /__up over HTTP and STUN Binding Requests over UDP on 127.0.0.1:18085,
# and updating network.cloudflare in the smoke overrides JSON so CI never waits on a third party.
#
# Usage: scripts/smoke-cloudflare.sh <directory> <overrides json>
set -eu

fail() {
    echo "smoke-cloudflare: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-cloudflare.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
port=18085

cat > "$dir/server.py" <<'PY'
import http.server
import socket
import socketserver
import sys
import threading

PORT = int(sys.argv[1])
META = (
    b'{"clientIp":"198.51.100.42","colo":"LAX","asn":64500,'
    b'"asOrganization":"Smoke ISP"}'
)
LOCATIONS = b'[{"iata":"LAX","city":"Los Angeles","country":"United States"}]'
PAYLOAD = b"\x42" * 65536


def run_stun():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", PORT))
    while True:
        try:
            data, peer = sock.recvfrom(1500)
        except OSError:
            return
        if len(data) >= 20 and data[0:2] == b"\x00\x01" and data[4:8] == b"\x21\x12\xa4\x42":
            resp = b"\x01\x01\x00\x00" + data[4:20]
            try:
                sock.sendto(resp, peer)
            except OSError:
                pass


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        path = self.path
        if path.startswith("/meta"):
            body = META
            ctype = "application/json"
        elif path.startswith("/locations"):
            body = LOCATIONS
            ctype = "application/json"
        elif path.startswith("/__down") and "bytes=0" in path:
            body = b""
            ctype = "application/octet-stream"
        elif path.startswith("/__down"):
            body = PAYLOAD * 4
            ctype = "application/octet-stream"
        else:
            body = b"ip=198.51.100.42\ncolo=LAX\nloc=US\n"
            ctype = "text/plain"
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            try:
                self.wfile.write(body)
            except OSError:
                return

    def do_POST(self):
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
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", "2")
        self.end_headers()
        try:
            self.wfile.write(b"ok")
        except OSError:
            return

    def log_message(self, fmt, *args):
        return


class ReuseServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


t = threading.Thread(target=run_stun, daemon=True)
t.start()
with ReuseServer(("127.0.0.1", PORT), Handler) as httpd:
    httpd.serve_forever()
PY

python3 "$dir/server.py" "$port" >/dev/null 2>&1 &
echo "$!" > "$dir/server.pid"

tries=0
while ! curl --fail --silent "http://127.0.0.1:$port/meta" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local Cloudflare server did not start on 127.0.0.1:$port"
    sleep 0.1
done

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg base "http://127.0.0.1:$port" --argjson port "$port" \
    '. + {"network.cloudflare": {base_url: $base, stun_host: "127.0.0.1", stun_port: $port, seconds: 1, concurrency: 2, idle_ms: 200, udp_packets: 4}}' \
    "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-cloudflare: server listens on 127.0.0.1:$port; $overrides updated"
