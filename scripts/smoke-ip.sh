#!/bin/sh
# Local HTTP, UDP DNSBL and TCP port-25 responders on 127.0.0.1 for the smoke test, and their
# entry in the smoke overrides JSON. The smoke run then exercises ip.quality end to end without
# contacting any external intelligence API, DNSBL or mail server. CI runs it before smoke, after
# scripts/smoke-trace.sh.
#
# Usage: scripts/smoke-ip.sh <directory> <overrides json>
# Starts the responders on 127.0.0.1:18081 (HTTP), 127.0.0.1:18053 (UDP DNS) and 127.0.0.1:18025
# (TCP SMTP), writes server.pid into the directory, and updates ip.quality in <overrides json>.
set -eu

fail() {
    echo "smoke-ip: $*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "usage: scripts/smoke-ip.sh <directory> <overrides json>"
mkdir -p "$1"
dir=$(cd "$1" && pwd)
overrides=$2
http_port=18081
dns_port=18053
smtp_port=18025

cat > "$dir/server.py" <<'PY'
import http.server
import socketserver
import sys
import threading

HTTP_PORT = int(sys.argv[1])
DNS_PORT = int(sys.argv[2])
SMTP_PORT = int(sys.argv[3])

PAYLOADS = {
    "ipapi": (
        200,
        b'{"rir":"ARIN","is_proxy":false,"is_vpn":false,"is_tor":false,'
        b'"is_datacenter":true,"is_abuser":false,"is_crawler":false,'
        b'"location":{"country_code":"US"},'
        b'"company":{"name":"Smoke Host","type":"hosting","abuser_score":"0.0010 (Low)"},'
        b'"asn":{"asn":64500,"org":"Smoke Host","country":"US","type":"hosting"}}',
    ),
    "scamalytics": (200, b'<pre>{"score":"5","risk":"low"}</pre>'),
    "proxycheck": (
        200,
        b'{"status":"ok","target":{"asn":"AS64500","provider":"Smoke Host",'
        b'"isocode":"US","proxy":"no","type":"Hosting","risk":5}}',
    ),
    "ripestat": (
        200,
        b'{"data":{"authorities":["arin"],"records":[[{"key":"country","value":"US"}]]}}',
    ),
    "greynoise": (404, b'{"message":"not observed"}'),
    "shodan": (200, b'{"ports":[22,443],"tags":["cloud"],"vulns":[]}'),
    "ipinfo": (200, b'{"country":"US","org":"AS64500 Smoke Host"}'),
    "dbip": (200, b'{"countryCode":"US"}'),
    "ipwhois": (
        200,
        b'{"success":true,"country_code":"US","connection":{"asn":64500,"org":"Smoke Host"}}',
    ),
}


class HttpHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        parts = self.path.strip("/").split("/")
        key = parts[0] if parts else ""
        status, body = PAYLOADS.get(key, (200, b"{}"))
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        return


class DnsHandler(socketserver.BaseRequestHandler):
    def handle(self):
        data, sock = self.request
        if len(data) < 12:
            return
        # QR=1, RD=1, RA=1, RCODE=3 (NXDOMAIN -> clean in DNSBL).
        reply = data[:2] + b"\x81\x83" + data[4:6] + b"\x00\x00\x00\x00\x00\x00" + data[12:]
        sock.sendto(reply, self.client_address)


class SmtpHandler(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            self.request.sendall(b"220 127.0.0.1 ESMTP smoke\r\n")
            self.request.recv(64)
        except OSError:
            pass


class ReuseTcpServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class ReuseUdpServer(socketserver.ThreadingUDPServer):
    allow_reuse_address = True
    daemon_threads = True


http_srv = ReuseTcpServer(("127.0.0.1", HTTP_PORT), HttpHandler)
dns_srv = ReuseUdpServer(("127.0.0.1", DNS_PORT), DnsHandler)
smtp_srv = ReuseTcpServer(("127.0.0.1", SMTP_PORT), SmtpHandler)

threading.Thread(target=dns_srv.serve_forever, daemon=True).start()
threading.Thread(target=smtp_srv.serve_forever, daemon=True).start()
http_srv.serve_forever()
PY

python3 "$dir/server.py" "$http_port" "$dns_port" "$smtp_port" >/dev/null 2>&1 &
echo "$!" > "$dir/server.pid"

tries=0
while ! curl --fail --silent "http://127.0.0.1:$http_port/dbip/127.0.0.1" >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -le 30 ] || fail "local IP quality server did not start on 127.0.0.1:$http_port"
    sleep 0.1
done

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
jq --arg base "http://127.0.0.1:$http_port" \
    --arg dns "127.0.0.1:$dns_port" \
    --arg smtp "127.0.0.1:$smtp_port" \
    '. + {"ip.quality": {base_url: $base, dns_server: $dns, smtp_server: $smtp}}' \
    "$overrides" > "$tmp"
mv "$tmp" "$overrides"
echo "smoke-ip: responders listen on 127.0.0.1:$http_port/$dns_port/$smtp_port; $overrides updated"
