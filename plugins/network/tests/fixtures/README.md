# Fixtures for network speed

## `iperf3/`

Real output of iperf3 3.22 with `-J`, its standard output. iperf3 was the binary `iperf3-aarch64`
of the release `iperf3-3.22-1`, which `scripts/build-iperf3.sh` built. It ran on 2026-11-17 in
`debian:13-slim` under Docker Desktop on Apple silicon, over the loopback only, against a server
of the same build (`iperf3 -s -p 5201 -D`). The numbers measure no network; only the format
matters. The container's host name in `system_info` was replaced with `capture`.

| File | Run |
|------|-----|
| `upload.json` | `-4 -c 127.0.0.1 -p 5201 -P 8 -t 2 -J --connect-timeout 5000` |
| `download.json` | The same with `-R`: the server sends, so the streams report no round-trip time. |
| `upload6.json` | `-6 -c ::1 -p 5201 -P 2 -t 1 -J --connect-timeout 5000` |
| `capped.json` | `-4 -c 127.0.0.1 -p 5201 -P 2 -t 2 -b 50M -J --connect-timeout 5000` |
| `busy.json` | An upload while another client's test ran on the server: `the server is busy`. |
| `refused.json` | An upload to port 5299, where no server listened: `Connection refused`. |
| `silent.json` | An upload to 192.0.2.1, an address that never answers, with `--connect-timeout 2000`. |
