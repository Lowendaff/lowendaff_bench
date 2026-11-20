# lowendaff-bench

**English** | [&#x7B80;&#x4F53;&#x4E2D;&#x6587; (Simplified Chinese)](README.zh-CN.md) | [&#x65E5;&#x672C;&#x8A9E; (Japanese)](README.ja.md)

A comprehensive, single-binary benchmark and diagnostics suite for Linux servers (dedicated bare
metal, virtual machines and containers), written in Rust:

- **One static binary, twelve plugins.** Every test item is compiled into `lowendaff-bench` (`musl`
  static builds for `x86_64` and `aarch64`) with strongly typed results and isolated failure/timeout
  handling.
- **Zero host-tool drift.** External benchmark binaries (`sysbench`, `STREAM`, `multichase`,
  `smartctl`, `fio`, `iperf3`, `NextTrace`, `Geekbench 7`) are downloaded on demand and verified
  against pinned sizes and `SHA-256` digests before execution; host-installed binaries are never
  used.
- **Platform-aware & memory-safe.** Hardware-only items (`disk.health`) and multi-socket items
  (`memory.numa`) are automatically filtered by host capability; memory tests size themselves to at
  most half of available RAM while exceeding `4x` last-level CPU cache.
- **Multi-format output & privacy by default.** Live progress streams to `stderr`; the final report
  renders as aligned terminal `text`, GitHub-Flavored `markdown` (`md`), or structured `json` on
  `stdout` or via `-o` / `--output`, masking the server's public IPv4/IPv6 addresses and PTR
  hostnames unless `--show-ip` is passed.

Detailed test-methodology guides (in Chinese) live under [`docs/`](docs/README.md); architecture
and contributor conventions live in [`CONTRIBUTING.md`](CONTRIBUTING.md).

---

## Quick Start & Usage Examples

Download the latest static binary for your architecture from
[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) (or build from source; see
[Build](#build)), verify `SHA256SUMS`, and run as `root`:

```sh
# 1. Inspect all 12 registered test items, their groups, platform rules and pinned tool versions
./lowendaff-bench --list

# 2. Run the full suite (prints live progress on stderr and the text report on stdout)
sudo ./lowendaff-bench

# 3. Fast daily check: skip Geekbench 7 (uploads public scores) and 73-target route tracing
sudo ./lowendaff-bench --skip cpu.geekbench,network.trace

# 4. Run only specific groups or individual plugins (mix group names and plugin IDs freely)
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only cpu.sysbench,memory.stream,disk.fio

# 5. Run an entire group except one plugin (e.g. IP reputation without streaming unlock)
sudo ./lowendaff-bench --only ip --skip ip.media

# 6. Save a GitHub-Flavored Markdown or JSON report to a file (stdout stays quiet)
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 7. Benchmark a specific mounted disk/filesystem instead of the current directory (avoid /tmp)
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/nvme

# 8. Test network speed only against servers in Japan and Germany (saves bandwidth)
sudo ./lowendaff-bench --only network.iperf3 --countries JP,DE

# 9. Accelerate tool downloads via a SOCKS5/HTTP proxy on restricted networks
#    (Note: put proxy variables after sudo; IP/media/network probes still connect directly)
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16

# 10. Customize per-plugin settings via TOML and reveal full public IP addresses in the report
sudo ./lowendaff-bench --config bench.toml --show-ip -v
```

### Example `bench.toml` Configuration

Every configurable plugin reads its own `[plugins."<id>"]` table:

```toml
[plugins."network.iperf3"]
countries = ["US", "JP"]            # ISO 3166 country codes or ["all"]; --countries overrides this
servers = ["tokyo"]                 # explicit server IDs from servers/iperf3.json; overrides countries
seconds = 10                        # duration per direction (upload, then download)
streams = 8                         # parallel TCP streams
rate = 100                          # per-stream cap in Mbit/s (omit for unlimited)

[plugins."network.trace"]
groups = ["china"]                  # services, china, international, or all
targets = ["pek-4809", "youtube"]   # explicit target IDs; overrides groups
geo = true                          # set false to disable hop ASN/geolocation lookups

[plugins."ip.quality"]
dnsbl = true                        # set false to skip 25-zone DNSBL blacklist lookups
mail = true                         # set false to skip outbound TCP port-25 MX banner checks

[plugins."ip.media"]
groups = ["global", "asia"]         # global, asia, europe, americas, oceania, or all
services = ["netflix", "bahamut"]   # explicit platform IDs; overrides groups
```

---

## What Each Section & Test Item Does

Before running any plugin, `lowendaff-bench` collects a read-only **System Profile (`[System]`)**
from kernel and firmware interfaces within a 30-second budget: virtualization/container type, CPU
model (including ARM implementer/part decoding) and caches, RAM/swap and container cgroup quotas,
OS/kernel/uptime/load/TCP congestion control, physical disks/filesystems, NICs with PCIe link-width
degradation warnings, direct IPv4/IPv6 reachability, and on dedicated servers DMI/SMBIOS memory
slots, ECC/EDAC error counters, BMC/IPMI and hardware RAID controllers. See
[`docs/system.md`](docs/system.md).

| Group | Plugin ID | Title | Applies to | What it measures | Detailed guide |
|---|---|---|---|---|---|
| `disk` | `disk.health` | Disk health | Dedicated servers | Read-only `smartctl` SMART status across NVMe/SATA/SAS disks and behind MegaRAID / 3SNIC RAID cards: health verdict, power-on hours, temperature, wear, data written, bad blocks and link errors (serials/WWNs are never kept) | [`docs/disk.md`](docs/disk.md) |
| `cpu` | `cpu.sysbench` | CPU performance | All platforms | `sysbench` prime test (`primes <= 10000`, 20 s) on 1 thread and all usable threads, events/s and multi-to-single scaling ratio | [`docs/cpu.md`](docs/cpu.md) |
| `memory` | `memory.sysbench` | Memory sysbench | All platforms | Sequential read and write throughput (`MiB/s`) on 1 thread and all usable threads with cache-exceeding buffers (`>= 4x LLC`, `<= 50%` free RAM) | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream` | Memory STREAM | All platforms | Sustained `Copy`, `Scale`, `Add` and `Triad` bandwidth (`MB/s`) across six pre-built array sizes (`2M`..`64M` doubles), auto-fitted to cache and available RAM | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency` | Memory latency | All platforms | Random pointer-chasing access latency (`ns`) in `L1d`, `L2`, `L3` caches (`1/2` cache size) and main memory (`>= 4x LLC`) via `multichase` | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa` | Memory NUMA | `>= 2` usable NUMA nodes | Node-to-node `stream-triad` bandwidth (`multiload`) and random latency (`multichase`) matrix across every CPU node and memory node | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio` | Disk performance | All platforms | YABS-compatible `libaio` + `O_DIRECT` 50/50 random read/write speed and IOPS at `4K`, `64K`, `512K` and `1M` (`2 GiB` file, `512 MiB` on ARM/RISC-V; refuses `tmpfs`/`ramfs`) | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3` | Network speed | All (needs network) | Bidirectional 8-stream, 10 s TCP throughput and minimum RTT over IPv4 and IPv6 against reviewed public servers (default: LA, NY, Virginia, Tokyo, Frankfurt; ~5 GB/server at 1 Gbit/s) | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace` | Route tracing | All (needs `CAP_NET_RAW`) | Unprivileged `NextTrace` ICMP hop-by-hop route tracing (with `cap_net_raw+ep`) to nearest YouTube/Netflix caches and 71 Chinese/international carrier targets, with PeeringDB Netflix IX port matching | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality` | IP reputation | All (needs public IP) | Dual-stack evaluation across 9 intelligence APIs (ASN, RIR `native` vs `broadcast` origin, usage type, fraud scores, Shodan ports/CVEs, 7 risk factors), 25 DNSBL zones (filtering RFC 5782 resolver refusals) and 8 mail providers on TCP port 25 | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media` | Media unlock | All (needs network) | Direct dual-stack HTTPS availability checks across 68 streaming/OTT services in 5 regions (`global`, `asia`, `europe`, `americas`, `oceania`), reporting unlock region, originals-only, geo-block or WAF challenge | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.geekbench` | Geekbench 7 | `glibc >= 2.35` + IPv4 | Official Geekbench 7.1.0 single- and multi-core scores + browser link, with automatic temporary swap provisioning (up to 8 GiB) and OOM protection on low-RAM hosts | [`docs/cpu.md`](docs/cpu.md) |

---

## Output, Privacy & Runtime Behavior

- **CLI flags summary:**
  - `--only <LIST>` / `--skip <LIST>`: comma-separated (or repeated) group names (`cpu`, `memory`,
    `disk`, `network`, `ip`) or plugin IDs (`cpu.geekbench,ip.media`).
  - `-o, --output <PATH>`: open `<PATH>` before running benchmarks and write the report there
    instead of `stdout` (fails immediately with exit code `2` if `<PATH>` cannot be created).
  - `-f, --format <FORMAT>`: `text` (default), `markdown` (alias `md`), or `json`. Terminal width
    wrapping applies only to `text` on a TTY; files, `markdown` and `json` never wrap.
  - `--config <PATH>`: TOML file with `[plugins."<id>"]` tables.
  - `--work-dir <DIR>`: parent directory for the temporary run directory (removed at exit; avoid
    `/tmp`, which is often `tmpfs` or mounted `noexec`).
  - `--countries <CODES>`: comma-separated ISO 3166 codes (or `all`) for `network.iperf3`.
  - `--download-connections <1..=256>`: parallel HTTP range connections per tool download (default
    `32` for files `>= 2 MiB`, with automatic range resume and 4 retries).
  - `--show-ip`: show full public IPv4/IPv6 addresses and reverse-DNS hostnames in the report.
  - `-v, --verbose`: log download URLs/ranges/retries and tool commands/exit codes (redacting
    `geekbench` claim links, `smartctl` serial numbers and `iperf3` own-IP output).
- **Public address privacy.** Public IP discovery, `ip.quality`, `ip.media` and `network.trace`
  cache lookups always connect directly (ignoring `HTTPS_PROXY` / `ALL_PROXY`). Without
  `--show-ip`, every report format (`text`, `markdown`, `json`) masks own IPv4 addresses to two
  octets (`203.0.*.*`), masks own IPv6 addresses to 32 bits (`2001:db8:*`), and omits PTR
  hostnames; `stderr` logs always mask addresses.
- **Interrupts & exit codes:**
  - `0`: run completed (individual skipped/failed items are recorded inside the report);
  - `2`: invalid CLI usage, bad config, unwritable `--output`, or not running as `root`;
  - `130`: interrupted via `Ctrl-C` (partial results collected so far are still written to the
    report; a second `Ctrl-C` exits immediately).
- **Dynamic server & target lists.** `network.iperf3` and `network.trace` fetch
  `servers/iperf3.json`, `servers/traceroute.json` and `servers/netflix-ix.json` from `main` at run
  time and validate them against strict schema and size limits; the `data` workflow refreshes
  `servers/iperf3.json` (against `iperf3serverlist.net` from reviewed candidates in
  `servers/iperf3-candidates.json`) and `servers/netflix-ix.json` (from PeeringDB) twice daily.

---

## Build

The Rust toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml). Release binaries are
static `musl` builds for `x86_64` and `aarch64` (`musl-gcc` from `musl-tools` compiles `ring`'s C
sources):

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

Pushing a version tag (`v*`, matching `[workspace.package] version` in `Cargo.toml`) triggers
`.github/workflows/release.yml`, which builds `lowendaff-bench-x86_64` and `lowendaff-bench-aarch64`
and publishes them with `SHA256SUMS` as a GitHub Release.

---

## Gates

CI (`.github/workflows/ci.yml`, and `commits.yml` for the commit messages) runs exactly these
commands. Run them locally before pushing.

| Gate | Command | Runs on |
|------|---------|---------|
| Format | `cargo fmt --all --check` | any host |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | any host |
| Test | `cargo nextest run --workspace --locked --no-fail-fast` | any host; CI uses Linux x86_64 and aarch64 |
| Tool downloads | `cargo nextest run -p bench --test tools --locked --run-ignored only`: downloads every pinned tool for the host's architecture, checks its size and SHA-256 and unpacks archives, also the tools of items that do not apply or do not run on the CI runners | any host with network access; CI uses Linux x86_64 and aarch64 |
| Dependencies | `cargo deny check` (policy in `deny.toml`) | any host |
| Workflows | `zizmor --persona pedantic .` (config in `.github/zizmor.yml`) | any host |
| Workflow lint | `actionlint`, which runs shellcheck over the `run:` scripts too | any host |
| Shell scripts | `git ls-files -z '*.sh' \| xargs -0 shellcheck` | any host |
| Checker cases | `scripts/check-commits-cases.sh` | any host |
| Commit messages | `scripts/check-commits.sh origin/main HEAD`, which fails when git cannot list the commits or the range holds none; CI adds the title, the pull request's number of commits, which the range must hold, and the author's kind: a bot's pull request may change the generated lists, `servers/iperf3.json` and `servers/netflix-ix.json`, only | any host |
| Rule tests | `ast-grep test` (cases in `ast-grep/tests/`) | any host |
| Rules | `ast-grep scan` (rules in `ast-grep/rules/`, config in `sgconfig.yml`), then `scripts/check-docs-cases.sh` and `scripts/check-docs.sh`: no tracked file outside localized Markdown (`README.zh-CN.md`, `README.ja.md`, `docs/*.md`) and non-English catalogs (`crates/bench/src/locale/<language>/`) holds CJK characters, and no tracked file outside fixture data points into external design documents | any host |
| Server list | `scripts/iperf3-servers-cases.sh`, then `scripts/iperf3-servers.sh check`: the network test's server list and its candidates follow their rules, the list holds candidates only, and nothing builds from a list in `servers/`; the cases include the fixtures that the program's reader reads too | any host with jq |
| Target list | `scripts/traceroute-targets-cases.sh`, then `scripts/traceroute-targets.sh check`: route tracing's target list follows its rules; the cases include the fixtures that the program's reader reads too | any host with jq |
| Exchange list | `scripts/netflix-ix-cases.sh`, then `scripts/netflix-ix.sh check`: the list of Netflix's exchange ports follows its rules; the cases include the fixtures that the program's reader reads too, and the safety stops of `update` | any host with jq |
| Static build | `cargo build --release --locked --target <arch>-unknown-linux-musl --bin lowendaff-bench` | Linux |
| Swap round trip | `cargo build --release --locked --target <arch>-unknown-linux-musl -p swap-file --example roundtrip`, then `sudo target/<arch>-unknown-linux-musl/release/examples/roundtrip <dir>`: turns a 64 MiB swap file in `<dir>` on and off | Linux, with sudo, on a filesystem that holds swap files, such as ext4 |
| Smoke | `scripts/smoke-iperf3.sh <dir>`, `scripts/smoke-trace.sh <dir2> <dir>/config.toml`, `scripts/smoke-ip.sh <dir3> <dir>/config.toml` and `scripts/smoke-media.sh <dir4> <dir>/config.toml` start local loopback responders, then `scripts/smoke.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench --skip cpu.geekbench --config <dir>/config.toml` runs a full pass in which no item skips, fails or times out, followed by a proxy-bypass check | Linux, as root or with sudo, with network access |
| Smoke mutants | `scripts/smoke-mutants.sh <arch>-unknown-linux-musl`: smoke must reject deliberately broken builds (`--skip cpu,memory,disk,network,ip`) | Linux, as root or with sudo |
| Distributions | `scripts/distros.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench ghcr.io/lowendaff/mirror`: system profile and glibc skip checks across the pinned container images in `scripts/distros.txt` | Linux or macOS with Docker |
| Network speed, not required | `scripts/network-check.sh <build> servers/iperf3.json`: short 3 s / 2-stream / 100 Mbit/s check against one or two random public servers; non-blocking | Linux, as root or with sudo, with network access |

`<arch>` is `x86_64` or `aarch64`. The `linux` CI jobs run `scripts/ci-scope.sh` first to skip
rebuilds when only Markdown, `LICENSE`, or `servers/*.json` changed since a validated ancestor.
`GH_TOKEN=$(gh auth token) scripts/verify-commits.sh` validates every commit on a branch locally.

CI pins cargo-nextest `0.9.151`, cargo-deny `0.20.2`, zizmor `1.32.0`, ast-grep `0.51.1`,
shellcheck `0.11.0`, and actionlint `1.7.12`. Weekly `latest-deps` checks newest allowed crate
versions, `mirror` mirrors `scripts/distros.txt` digests to GHCR, and Dependabot proposes updates
for releases at least 7 days old.

---

## Third-party tools

External tools are built reproducibly by `.github/workflows/tools.yml` from `scripts/build-*.sh`
and published as immutable GitHub releases of this repository (with sources, licenses and
`SHA256SUMS`), except Geekbench 7, whose license forbids redistribution and which is downloaded
directly from `cdn.geekbench.com` under its pinned SHA-256 and size.

| Tool | Version | License | Used by | Release |
|------|---------|---------|---------|---------|
| fio, with libaio 0.3.113 linked statically | 3.43 | GPL-2.0; libaio LGPL-2.1 | Disk performance (`disk.fio`) | `fio-3.43-2` |
| iperf3, without OpenSSL or SCTP | 3.22 | BSD-3-Clause | Network speed (`network.iperf3`) | `iperf3-3.22-1` |
| smartctl, from smartmontools | 7.5 | GPL-2.0-or-later | Disk health (`disk.health`) | `smartctl-7.5-1` |
| sysbench, with bundled LuaJIT and Concurrency Kit | 1.0.20 | GPL-2.0-or-later; MIT; BSD-2-Clause | CPU (`cpu.sysbench`), memory (`memory.sysbench`) | `sysbench-1.0.20-1` |
| STREAM, built for six array sizes | 5.10 | STREAM license | Memory STREAM (`memory.stream`) | `stream-5.10-1` |
| NextTrace, tiny flavor with Go fallback roots | 1.7.6 | GPL-3.0; BSD-3-Clause | Route tracing (`network.trace`) | `nexttrace-1.7.6-1` |
| multichase and multiload, with glibc 2.41 static | commit `8b68e918` (2026-10-01) | Apache-2.0; glibc LGPL-2.1 | Memory latency (`memory.latency`), NUMA (`memory.numa`) | `multichase-2026.10.01-1` |
