# Network Plugin Test Captures _(fixtures)_

> Scrubbed `iperf3`, `NextTrace`, cache-lookup, and target-list fixtures for `plugin-network`.

This directory (`plugins/network/tests/fixtures/`) stores recorded tool outputs, cache-discovery
responses, and validation lists for `plugin-network` inside `lowendaff_bench`.

## Security

All captured hostnames, private/public client addresses, and local resolver IPs are replaced with
neutral placeholders or RFC 5737 / RFC 5398 documentation addresses (`192.0.2.0/24`,
`198.51.100.0/24`, `203.0.113.0/24`, `AS64496`), and non-ASCII tool messages are translated to
English to satisfy `scripts/check-docs.sh`.

## Background

Fixtures exercise JSON/text parsers and list validators across `network.iperf3` and `network.trace`,
staying synchronized with the schema validation scripts in
[`Lowendaff/bench_data`](https://github.com/Lowendaff/bench_data).

## Fixture Sets

### `iperf3/`

Loopback `iperf3 -J` outputs from `iperf3-3.22-1` (`scripts/build-iperf3.sh`, 2026-11-17 in
`debian:13-slim`, `system_info` host replaced with `capture`):

| File | Run |
|---|---|
| `upload.json` | `-4 -c 127.0.0.1 -p 5201 -P 8 -t 2 -J --connect-timeout 5000` |
| `download.json` | Same with `-R` (server sends; streams report no sender RTT) |
| `upload6.json` | `-6 -c ::1 -p 5201 -P 2 -t 1 -J --connect-timeout 5000` |
| `capped.json` | `-4 -c 127.0.0.1 -p 5201 -P 2 -t 2 -b 50M -J --connect-timeout 5000` |
| `busy.json` | Upload while another test held the server (`the server is busy`) |
| `refused.json` | Upload to closed port 5299 (`Connection refused`) |
| `silent.json` | Upload to `192.0.2.1` with `--connect-timeout 2000` |

### `trace/`

NextTrace 1.7.6 (`nexttrace-1.7.6-1`) outputs with `--traceroute -j -M -C -g en -q 3 -m 30
--timeout 1000 -4` (captured 2026-11-18):

| File | Trace |
|---|---|
| `reached.json` | 13-hop trace to Beijing CN2 with hops 1..7 scrubbed to RFC 5737 / AS64496 and hops 8..13 translated to English |
| `load-balanced.json` | Derived from `reached.json` with hop 10 probe 2 set to `203.0.113.10` |
| `not-reached.json` | GitHub `ubuntu-24.04` runner trace where all 30 ICMP hops time out |
| `loopback.json` | `127.0.0.1` with `-d disable-geoip` |
| `warned.txt` | Unprivileged `nobody` with `CAP_NET_RAW`, emitting three `※` warning lines before JSON |
| `unresolved.txt` | `does-not-exist.invalid` as `nobody` with resolver scrubbed to `192.0.2.53` |
| `no-capability.txt` | `127.0.0.1` as `nobody` without `CAP_NET_RAW`, ending after `—` in English |

### `lookups/`, `target-lists/`, `exchange-lists/`, `server-lists/`, and `taier-lists/`

- `lookups/`: Scrubbed YouTube (`report_mapping?di=no`: `report-mapping-google.txt`,
  `report-mapping-isp.txt`, `report-mapping-u.txt`) and Netflix (`fast.json`) cache responses.
- `target-lists/`, `exchange-lists/`, `server-lists/`, `taier-lists/`: Valid (`good-*`) and
  malformed (`bad-*`) JSON catalogs shared with `Lowendaff/bench_data` schema test suites.

## Contributing

Questions and pull requests are welcome via
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues); scrub all network identifiers
and follow [`CONTRIBUTING.md`](../../../../CONTRIBUTING.md).

## License

[MIT](../../../../LICENSE) (c) 2026 Lowendaff
