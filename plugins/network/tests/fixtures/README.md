# Fixtures of the network test items

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

## `trace/`

Output of NextTrace 1.7.6, the release `nexttrace-1.7.6-1`, with the item's flags
(`--traceroute -j -M -C -g en -q 3 -m 30 --timeout 1000 -4`), captured on 2026-11-18 with the
owner's approval. What would tell where a host is was replaced, and NextTrace's Chinese text is in
English, since the repository holds no CJK text.

| File | Trace |
|------|-------|
| `reached.json` | A VM of the owner's to Beijing CN2 (`ipv4.pek-4809.endpoint.nxtrace.org`), as root: reached in 13 hops, 4 of them silent and 2 that answered one probe of three. Hops 1 to 7 are the VM's own network: their addresses are `10.0.0.1` and `192.0.2.<hop>` (RFC 5737), their names are removed, and their network is AS64496 `Example Net` (RFC 5398), without a place. Of hops 8 to 13, NextTrace's service names the networks and places in Chinese that are now in English, such as `China Telecom` and `Beijing`. |
| `load-balanced.json` | Made from `reached.json`, with the address of the second probe of hop 10 changed to `203.0.113.10`, so that the hop answers from two addresses, as a hop behind load balancing does; no capture showed one. |
| `not-reached.json` | A GitHub runner (ubuntu-24.04, x86_64) to Beijing CN2, as root. The runners receive no ICMP reply, so all 30 hops are silent and NextTrace stopped at `max_hops`. Unchanged. |
| `loopback.json` | 127.0.0.1 with `-d disable-geoip`, as root, in `debian:13-slim` under Docker Desktop on Apple silicon. Unchanged. |
| `warned.txt` | The same as nobody with CAP_NET_RAW alone, as the item runs NextTrace: it prints a warning of three lines before its JSON. The warning is in English here, each line starting with `※`, as each real line starts with a character outside ASCII. |
| `unresolved.txt` | A name that does not resolve, `does-not-exist.invalid`, as nobody: the warning, then the error that NextTrace printed on its standard error, with the resolver's address replaced by `192.0.2.53`. |
| `no-capability.txt` | 127.0.0.1 as nobody without the capability: the warning, then the error, whose Chinese end ("please run it as root") is in English after a `—` (U+2014), as the real end starts with a character outside ASCII. |

## `lookups/`

The answers that the item finds the nearest cache servers by, captured on 2026-11-18. The clients'
addresses are documentation addresses, and the cache servers' ids and sites are of documented
examples or neutral, so that no answer tells where a host is.

| File | Answer |
|------|--------|
| `report-mapping-google.txt` | Google's `report_mapping?di=no` where one of Google's own servers serves, as this repository's developer saw it: `192.0.2.1 => dub16s03 (192.0.2.0/26)`, with a documented id. |
| `report-mapping-isp.txt` | The same where an ISP's cache serves, from the VM, with the documented id `bellcanada-yyz16`. |
| `report-mapping-u.txt` | The same from the owner's dedicated server, which ends in ` [u]`, with an ISP cache's published id, `wavespeed-lca2`. |
| `fast.json` | fast.com's API with its token, from the VM. Its client is neutral, and its five targets are in Frankfurt (`ipv4-c001-fra002-ix` to `ipv4-c005-fra002-ix`), each address with a neutral query. |

## `target-lists/` and `exchange-lists/`

Lists in the item's two formats, written for the tests: a list named `good-*` follows every rule,
and one named `bad-*` breaks one. The program's readers (`trace_lists.rs`) and the lists' checking
scripts (`scripts/traceroute-targets-cases.sh`, `scripts/netflix-ix-cases.sh`) both read them, so
that they agree on each.
