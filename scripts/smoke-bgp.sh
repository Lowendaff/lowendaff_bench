#!/bin/sh
# A local HTTP responder on 127.0.0.1 for the smoke test, answering RIPEstat bgp-state,
# rpki-validation, and as-overview requests, and updating network.bgp in the smoke overrides JSON.
# The smoke run then exercises network.bgp end to end without contacting external RIPE servers.
#
# Usage: scripts/smoke-bgp.sh <directory> <overrides json>
# Starts the server on 127.0.0.1:18083, writes server.pid into the directory, and updates
# network.bgp in <overrides json>.
set -eu

fail() {
    echo "smoke-bgp: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-bgp.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
port=18083

cat > "$dir/server.py" <<'PY'
import http.server
import socketserver
import sys

PORT = int(sys.argv[1])

BGP_STATE = (
    b'{"status":"ok","data":{"resource":"203.0.113.0/24","bgp_state":['
    b'{"target_prefix":"203.0.113.0/24","source_id":"00-198.51.100.1","path":[3356,4809,64500]},'
    b'{"target_prefix":"203.0.113.0/24","source_id":"01-198.51.100.2","path":[1299,9929,64500]}'
    b']}}'
)
RPKI = b'{"status":"ok","data":{"status":"valid"}}'
OVERVIEW = b'{"status":"ok","data":{"holder":"SMOKE-AS - Smoke Network"}}'


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path.endswith("/bgp-state/data.json"):
            body = BGP_STATE
        elif path.endswith("/rpki-validation/data.json"):
            body = RPKI
        elif path.endswith("/as-overview/data.json"):
            body = OVERVIEW
        else:
            body = b'{"status":"ok","data":{}}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

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
while ! curl --fail --silent "http://127.0.0.1:$port/rpki-validation/data.json" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local BGP server did not start on 127.0.0.1:$port"
    sleep 0.1
done

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg base "http://127.0.0.1:$port" \
    '. + {"network.bgp": {base_url: $base}}' "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-bgp: BGP server listens on 127.0.0.1:$port; $overrides updated"
