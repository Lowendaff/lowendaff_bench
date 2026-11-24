# lowendaff-bench _(lowendaff_bench)_

[![standard-readme compliant](https://img.shields.io/badge/standard--readme-OK-green.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)
[![CI](https://img.shields.io/github/actions/workflow/status/Lowendaff/lowendaff_bench/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/Lowendaff/lowendaff_bench/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Lowendaff/lowendaff_bench?style=flat-square)](https://github.com/Lowendaff/lowendaff_bench/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-linux--x86__64%20%7C%20linux--aarch64-lightgrey.svg?style=flat-square)](https://github.com/Lowendaff/lowendaff_bench/releases)

> All-in-one Linux server hardware benchmark, network speed test, route tracer, and IP check tool.

**English** | [&#x7B80;&#x4F53;&#x4E2D;&#x6587; (Simplified Chinese)](README.zh-CN.md) | [&#x65E5;&#x672C;&#x8A9E; (Japanese)](README.ja.md)

`lowendaff-bench` evaluates CPU compute, cache and memory hierarchy, disk SMART health and mixed
random I/O, global and regional network throughput, hop-by-hop routes, BGP upstream topology, IP
reputation, and streaming platform availability on `x86_64` and `aarch64` Linux hosts. It runs out
of the box on dedicated bare-metal servers, virtual machines (KVM, Xen, cloud instances), and
containers (Docker, LXC, OpenVZ), offering both an interactive terminal UI (TUI) and a scriptable
CLI with aligned text, GitHub-Flavored Markdown, and JSON reports.

The GitHub repository and root folder are named `lowendaff_bench`, while the standalone Linux
binary is named `lowendaff-bench` and built from the `bench` workspace orchestrator crate.

## Table of Contents

- [Security](#security)
- [Background](#background)
- [Install](#install)
  - [Dependencies](#dependencies)
  - [Building from Source](#building-from-source)
  - [Updating](#updating)
- [Usage](#usage)
  - [CLI](#cli)
  - [Options](#options)
  - [Allowed Values](#allowed-values)
- [Test Suite](#test-suite)
- [Maintainers](#maintainers)
- [Acknowledgements](#acknowledgements)
- [Contributing](#contributing)
- [License](#license)

## Security

- **Pinned, verified helper binaries**: External benchmark binaries (`sysbench`, `7zz`,
  `UnixBench`, `HPL`, `PassMark`, `fio`, `iperf3`, `NextTrace`, `Geekbench 7`, `smartctl`,
  `STREAM`, `multichase`, `y-cruncher`, `mprime`) are fetched over HTTPS at exact pinned versions
  and verified against expected byte lengths and SHA-256 digests before execution. Pre-installed
  host binaries of the same name are never invoked.
- **Privilege separation & clean teardown**: Unprivileged external workloads drop privileges before
  running in isolated per-plugin working directories and process groups. Pressing `Ctrl-C` or
  receiving `SIGTERM`/`SIGHUP` stops child process groups, unmounts temporary swap files, restores
  terminal raw mode, and deletes temporary directories cleanly.
- **Strict download-proxy isolation**: `--socks5-proxy` and `--github-proxy` apply exclusively to
  downloading helper binaries and static target lists. Ambient proxy environment variables
  (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`) are ignored and stripped from child processes so IP
  detection, speed tests, route traces, BGP lookups, and streaming probes always reflect direct
  host connectivity.
- **Default privacy masking**: Public IPv4/IPv6 addresses, reverse DNS hostnames, MAC addresses,
  and hardware serial numbers/UUIDs are masked or omitted by default across text, Markdown, JSON,
  and uploaded BGP SVG topology diagrams unless `--show-ip` is explicitly passed.

To report a security vulnerability, please contact [@Lowendaff](https://github.com/Lowendaff) or
open a report on [GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues) without
exposing live host credentials or private endpoints.

## Background

Evaluating a new Linux server typically requires stitching together shell scripts, distro packages
compiled with varying compiler flags, and ad-hoc web checks that break on low-memory VPS instances
or leak public IP addresses when shared online. `lowendaff-bench` consolidates these checks into a
single static Rust orchestrator with deterministic rules:

- **Single file, zero setup, no config files**: One standalone static binary runs across Debian,
  Ubuntu, RHEL/Alma/Rocky, Alpine, and Arch on `x86_64` and `aarch64`. Running with no arguments
  launches the interactive TUI; passing any CLI flag runs non-interactively.
- **Comparable results across distributions**: Every external benchmark binary is pinned to an
  identical static or self-contained build (`crates/bench-tool`), eliminating version and compiler
  drift across Linux distributions.
- **Adaptive hardware & low-memory protection**: Automatically skips items inapplicable to the
  current host (such as physical SMART checks inside VMs/containers, multi-socket NUMA matrices on
  single-node hosts, or `x86_64`-only torture suites on ARM64). Memory working sets, 7-Zip
  dictionaries, and HPL matrix orders scale to available host/cgroup RAM, and Geekbench 7 sets up a
  temporary swap file automatically when RAM is tight.
- **Share-ready reports & `/tmp` archiving**: Renders aligned terminal tables, GitHub-Flavored
  Markdown, or structured JSON, and saves an unwrapped archive copy under
  `/tmp/lowendaff-bench-<timestamp>.<txt|md|json>` whenever `-o` / `--output` is omitted.

For module-by-module measurement formulas, parameter derivations, and verdict rules, see
[`docs/README.md`](docs/README.md) (in Chinese). Runtime server and target catalogs are maintained
in [`Lowendaff/bench_data`](https://github.com/Lowendaff/bench_data).

## Install

Download the latest prebuilt static binary for your architecture (`lowendaff-bench-x86_64` for
Intel/AMD servers, or `lowendaff-bench-aarch64` for ARM64 servers) from
[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) and make it executable:

```sh
# Example for x86_64 (replace with lowendaff-bench-aarch64 on ARM64 servers)
curl -fLO https://github.com/Lowendaff/lowendaff_bench/releases/latest/download/lowendaff-bench-x86_64
chmod +x lowendaff-bench-x86_64
```

### Dependencies

- **Operating system**: Linux on `x86_64` or `aarch64` (no host benchmark packages or runtime
  libraries required; `passmark` requires `glibc >= 2.28`, `prime95` requires `x86_64` +
  `glibc >= 2.18`, and `geekbench` requires `glibc >= 2.35` + IPv4, auto-skipping cleanly
  otherwise).
- **Privileges**: Run with `sudo` (or as `root`) for full hardware inspection (`SMBIOS`, `SMART`,
  `ICMP` raw sockets, and temporary swap management). Informational flags (`--list`,
  `--taier-list`, `--help`, `--version`) run without `root`.

### Building from Source

Install the Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml) and `musl-tools`
(`musl-gcc`), then build a static `musl` binary:

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

The compiled binary is written to `target/x86_64-unknown-linux-musl/release/lowendaff-bench` (use
`aarch64-unknown-linux-musl` on ARM64).

### Updating

Download the newest binary from
[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) over the existing file;
each release is a self-contained single binary with no persistent state or configuration files to
migrate.

## Usage

Launch the interactive TUI by running with no arguments, or pass `--all` (or `--only` / `--skip`)
for non-interactive execution:

```sh
sudo ./lowendaff-bench-x86_64        # Opens interactive TUI
sudo ./lowendaff-bench-x86_64 --all  # Runs all applicable items non-interactively
```

Every finished run without `-o` / `--output` also archives an unwrapped report copy to
`/tmp/lowendaff-bench-<timestamp>.<txt|md|json>`. Press `Ctrl-C` once at any time to stop early and
emit a report containing all items completed so far.

**Notes before running:**

- **Resource usage & provider policies**: Running benchmarks briefly saturates CPU cores and
  network bandwidth (as well as memory and disk I/O). Make sure running these tests complies with
  your hosting provider's Acceptable Use Policy (AUP) and Terms of Service (ToS).
- **Full-suite duration & background sessions**: A full benchmark run covers multiple CPU suites,
  multi-region speed tests, and route traces and takes a noticeable amount of time to finish.
  Running inside `tmux` or `screen` is recommended so execution continues safely in the background
  if your SSH connection drops.

### CLI

Assuming the binary is named `lowendaff-bench`:

```sh
# 1. See all 15 available test items and what they require (no root needed)
./lowendaff-bench --list

# 2. Launch the interactive TUI (default when run with no options: toggle items, edit params, view report)
sudo ./lowendaff-bench

# 2b. Run the full suite non-interactively in CLI mode (CPU, memory, disk, network, IP, streaming)
sudo ./lowendaff-bench --all

# 3. Quick server check: run fast CPU sub-suites (skip UnixBench & Geekbench) and skip route tracing
sudo ./lowendaff-bench --cpu-bench sysbench,7zip,hpl,passmark --skip network.trace

# 4. Check IP reputation & streaming unlock, or draw the server's BGP upstream topology graph
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only network.bgp

# 5. Pick any mix of items and opt-in CPU stress sub-suites (such as y-cruncher & Prime95)
sudo ./lowendaff-bench --only cpu.bench,memory.stream,disk.fio --cpu-bench sysbench,7zip,ycruncher,prime95

# 6. Run a category while excluding one item (e.g. IP reputation without streaming checks)
sudo ./lowendaff-bench --only ip --skip ip.media

# 7. Save the report directly as Markdown (great for forums/issues) or JSON (great for scripts)
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 8. Test disk speed on another mounted drive (avoid /tmp, which is often a RAM disk)
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 9. Filter by region: test only Japan & US speed servers, Chinese carrier nodes, Cloudflare/Apple CDN, China routes, or Asian streaming apps
./lowendaff-bench --taier-list guangdong
sudo ./lowendaff-bench --only network.iperf3 --countries JP,US
sudo ./lowendaff-bench --only network.taier --taier beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct
sudo ./lowendaff-bench --only network.cloudflare,network.inetspeed --inetspeed-threads 8
sudo ./lowendaff-bench --only network.trace --trace-groups china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 10. Check IP reputation without testing outbound mail port 25 (useful on strict cloud providers)
sudo ./lowendaff-bench --only ip.quality --no-mail

# 11. Speed up tool and server-list downloads via a GitHub mirror and/or SOCKS5 proxy
#     (Environment proxy variables are ignored; IP, speed, route, BGP, and streaming checks always connect directly)
sudo ./lowendaff-bench --all --github-proxy https://ghfast.top/
sudo ./lowendaff-bench --all --socks5-proxy socks5h://127.0.0.1:1080 --download-connections 16 --show-ip
```

### Options

| Option | Syntax | Default | What it does |
|---|---|---|---|
| *(no options)* | — | Interactive TUI | Running `sudo ./lowendaff-bench` with zero arguments launches the interactive TUI (toggle items with bidirectional sub-option sync, edit parameters, monitor progress, and browse the report) |
| `-a, --all` | *(flag)* | Off | Run every test item in non-interactive CLI mode (equivalent to `--only all`) |
| `--only` | `<ITEM,...>` | All items | Run only the listed categories, test IDs, or `all` in CLI mode (comma-separated or repeated) |
| `--skip` | `<ITEM,...>` | None | Skip the listed categories, test IDs, or `all` (can be combined with `--all` or `--only`) |
| `-o, --output` | `<FILE>` | Terminal + `/tmp` | Write the final report only to `<FILE>` (when omitted, prints to terminal/viewer and saves an unwrapped copy to `/tmp/lowendaff-bench-<timestamp>.<ext>`) |
| `-f, --format` | `<FORMAT>` | `text` | Choose report format: `text`, `markdown` (or `md`), or `json` |
| `--work-dir` | `<DIR>` | `.` | Directory used for temporary test files, cleaned up on exit (point this at the disk you want to test; avoid `/tmp`) |
| `--cpu-bench` | `<SUITE,...>` | 6 default suites | Choose which sub-suites `cpu.bench` runs (`sysbench`, `7zip`, `unixbench`, `hpl`, `passmark`, `ycruncher`, `prime95`, `geekbench`, or `all`) |
| `--countries` | `<CODE,...>` | Default locations | Limit `network.iperf3` to two-letter country codes (such as `JP,US,DE`) or `all` |
| `--taier` | `<SELECTOR,...>` | Beijing, Shanghai, Guangzhou | Select `network.taier` targets as `<place>[:<op>[+<op>...]]` (such as `beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct,all:cm`) |
| `--taier-list` | `[PROVINCE]` | — | List all `network.taier` regions (or every city in `PROVINCE` with its selector and carrier counts), then exit (no root needed) |
| `--taier-cities` | `<CITY,...>` | Beijing, Shanghai, Guangzhou | Limit `network.taier` to cities, provinces, location IDs (such as `Beijing,Jiangsu`), or `all` |
| `--taier-operators` | `<ISP,...>` | `telecom,unicom,mobile` | Choose which carriers `network.taier` tests (`telecom`, `unicom`, `mobile`, `cernet`, `cbn`, `other`, or `all`) |
| `--no-cloudflare-udp` | *(flag)* | Off | Skip the RFC 5389 STUN UDP packet-loss probe inside `network.cloudflare` |
| `--inetspeed-threads` | `<1..=64>` | `4` | Concurrent worker threads for multi-thread rounds inside `network.inetspeed` |
| `--inetspeed-endpoint` | `<IP>` | Auto (Dual DoH) | Pin a specific Apple CDN IPv4 or IPv6 address inside `network.inetspeed`, skipping DoH discovery |
| `--trace-groups` | `<GROUP,...>` | Caches + defaults | Choose which target groups `network.trace` traces routes to |
| `--media-groups` | `<GROUP,...>` | `all` | Choose which regional groups `ip.media` checks for streaming availability |
| `--no-dnsbl` | *(flag)* | Off | Skip the 25 spam/abuse blacklist (DNSBL) lookups inside `ip.quality` |
| `--no-mail` | *(flag)* | Off | Skip outbound port-25 mail server connectivity checks inside `ip.quality` |
| `--no-bgp-upload` | *(flag)* | Off | Skip uploading `network.bgp` topology SVG diagrams to `imageupload.app` in `text` mode |
| `--download-connections` | `<1..=256>` | `32` | Max parallel connections when downloading benchmark tools |
| `--socks5-proxy` | `<URL>` | None | SOCKS5 proxy (`socks5://` or `socks5h://`) used exclusively for tool and list downloads (never touches benchmark probes; env proxy vars are ignored) |
| `--github-proxy` | `<URL>` | None | GitHub URL prefix mirror (e.g. `https://ghfast.top/`) rewriting `https://github.com/...` and `https://raw.githubusercontent.com/...` downloads to `<URL>/https://...` |
| `--show-ip` | *(flag)* | Off (masked) | Show your server's full public IPv4/IPv6 addresses and hostnames in the report |
| `-v, --verbose` | *(flag)* | Off | Print detailed download progress and underlying tool commands for troubleshooting |
| `--list` | *(flag)* | — | List all available test items, supported environments, and tool versions, then exit |
| `-h, --help` | *(flag)* | — | Show help message and exit |
| `-V, --version` | *(flag)* | — | Show version number and exit |

### Allowed Values

| Option | Allowed Values | Description |
|---|---|---|
| `--only` / `--skip` *(categories)* | `cpu` | Unified CPU benchmark suite: `cpu.bench` |
| | `memory` | All 4 memory tests: `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa` |
| | `disk` | Both disk tests: `disk.health`, `disk.fio` |
| | `network` | All 6 network tests: `network.iperf3`, `network.taier`, `network.cloudflare`, `network.inetspeed`, `network.trace`, `network.bgp` |
| | `ip` | Both IP tests: `ip.quality`, `ip.media` |
| `--only` / `--skip` *(single items)* | `disk.health`, `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa`, `disk.fio`, `network.iperf3`, `network.taier`, `network.cloudflare`, `network.inetspeed`, `network.trace`, `network.bgp`, `ip.quality`, `ip.media`, `cpu.bench` | Include or exclude any individual test item |
| `-f, --format` | `text` | Aligned plain-text tables formatted for terminal viewing |
| | `markdown` *(or `md`)* | `lowendaff.com`-compatible GitHub-Flavored Markdown (`Item \| Value` tables, blockquotes, context-aware escaping, and Mermaid BGP / VM CPU diagrams) |
| | `json` | Structured JSON output for scripts, dashboards, or automated archiving |
| `--cpu-bench` | `sysbench` | 20-second prime sieve on 1 thread and all threads *(default)* |
| | `7zip` | 7-Zip LZMA2 compression & decompression MIPS on 1 thread and all threads *(default)* |
| | `unixbench` | Byte-UnixBench 6.0.1 full 12-item System Index at 1 copy and N copies *(default)* |
| | `hpl` | High-Performance Linpack (`OpenBLAS` FP64 LU) sustained GFLOPS *(default)* |
| | `passmark` | PassMark PerformanceTest Linux CPU Mark & 8 sub-tests *(default; `glibc >= 2.28`)* |
| | `ycruncher` | y-cruncher multi-threaded Pi computation *(opt-in; `x86_64` only)* |
| | `prime95` | Prime95 (`mprime`) 65-second Small-FFT torture stability test *(opt-in; `x86_64` only)* |
| | `geekbench` | Official Geekbench 7.1.0 single-core & multi-core scores + URL *(default; `glibc >= 2.35` + IPv4)* |
| | `all` | All 8 CPU sub-suites above (auto-skipping unsupported ones on the current host) |
| `--trace-groups` | `services` | Nearest YouTube and Netflix video cache servers only |
| | `china` | Mainland China carrier routes (Telecom 163/CN2, Unicom 169/9929, Mobile CMNET/CMIN2) |
| | `international` | Global Tier-1 backbones, Telegram data centers, AWS, and Google Cloud nodes |
| | `all` | Every route target above (73 targets in total) |
| `--media-groups` | `global` | Global platforms (Netflix, YouTube Premium, Disney+, Prime Video, Spotify, TikTok, etc.) |
| | `asia` | Asian platforms (Bilibili HK/MO/TW, Bahamut, Abema, U-NEXT, TVer, Viu, etc.) |
| | `europe` | European platforms (BBC iPlayer, ITVX, Channel 4, Canal+, ZDF, France TV, RAI Play, etc.) |
| | `americas` | North & Latin American platforms (Hulu, Max, Peacock, Paramount+, Starz, Crave, etc.) |
| | `oceania` | Australian & New Zealand platforms (Stan, 9Now, 7plus, ABC iview, TVNZ+, etc.) |
| | `all` | All 68 platforms across all five regions (default) |
| `--countries` | Two-letter country code *(e.g. `US`, `JP`, `DE`, `SG`, `GB`)* | Test all public `iperf3` servers in the selected countries (case-insensitive) |
| | `all` | Test every public server in the server list |
| `--taier` | `<place>[:<op>[+<op>...]]` *(e.g. `beijing:cu`, `guangdong/shenzhen:ct+cm`, `jiangsu:ct`, `all:cm`)* | Fine-grained per-city / per-province carrier selection (`ct`/`telecom`, `cu`/`unicom`, `cm`/`mobile`, `edu`/`cernet`, `cbn`, `other`, `all`) |
| `--taier-cities` | City, province, or ID *(e.g. `Beijing`, `Jiangsu`, `guangdong-shenzhen`)* | Test all matching CAICT GlobalSpeed locations in mainland China (case-insensitive) |
| | `all` | Test every location in the Taier node list |
| `--taier-operators` | `telecom`, `unicom`, `mobile`, `cernet`, `cbn`, `other`, or `all` | Filter CAICT GlobalSpeed carrier nodes (default: `telecom,unicom,mobile`) |
| `--socks5-proxy` | `socks5://[user:pass@]host[:port]` | Resolve download hostnames locally on this machine, then connect through the SOCKS5 proxy |
| | `socks5h://[user:pass@]host[:port]` | Resolve download hostnames remotely on the SOCKS5 proxy server |
| `--github-proxy` | `https://<mirror>/` *(or `http://<mirror>/`)* | Prefix mirror for GitHub tool and list downloads (for example `https://ghfast.top/` requests `https://ghfast.top/https://github.com/...` directly; can be combined with `--socks5-proxy`) |

## Test Suite

Before running benchmarks, `lowendaff-bench` collects a read-only **System Overview (`[System]`)**:
whether the host is bare metal, a VM, or a container, CPU model and cache hierarchy, RAM and swap,
OS, kernel, uptime, TCP congestion control (such as BBR), disks, network link speeds (flagging
degraded PCIe slots), and IPv4/IPv6 connectivity. On dedicated servers, it also lists populated RAM
slots, ECC memory error counts, IPMI/BMC controllers, and hardware RAID cards (see
[`docs/system.md`](docs/system.md)). On virtual machines, the orchestrator also samples
`/proc/stat` every 5 seconds from start to finish and appends a `CPU steal & softirq` section
reporting average, P95, and peak `%si` / `%st` alongside a 5-second sample table in `text` mode, a
Mermaid `xychart-beta` timeline in `markdown` mode, and `vm_cpu_telemetry` in `json` mode.

Then it executes up to 15 test items in deterministic registry order:

| Category | Item ID & Name | Where it runs | What it tells you | Deep dive |
|---|---|---|---|---|
| `disk` | `disk.health`<br>Disk health | Dedicated servers | Reads drive SMART data (including drives behind common hardware RAID cards) to check overall health, power-on hours, temperature, SSD wear, total writes, and bad sectors | [`docs/disk.md`](docs/disk.md) |
| `memory` | `memory.sysbench`<br>Sequential memory | All servers | Measures single-thread and multi-thread sequential memory read/write speed | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream`<br>Sustained bandwidth | All servers | Uses the industry-standard STREAM benchmark to measure sustained real-world RAM bandwidth (`Copy`, `Scale`, `Add`, `Triad`) | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency`<br>Cache & RAM latency | All servers | Measures random access latency in nanoseconds across CPU L1/L2/L3 caches and main memory | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa`<br>Multi-socket NUMA | Multi-NUMA servers | On dual-socket or multi-node machines, measures bandwidth and latency between local RAM and cross-socket RAM | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio`<br>Disk read/write | All servers | Runs YABS-compatible 50/50 random read/write tests at `4K`, `64K`, `512K`, and `1M` block sizes to show both database IOPS (`4K`) and large-file throughput (`512K`/`1M`) | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3`<br>Network speed | Servers with internet | Tests upload, download, and latency over IPv4 and IPv6 against public servers (default: Los Angeles, New York, Virginia, Tokyo, Frankfurt) | [`docs/network.md`](docs/network.md) |
| `network` | `network.taier`<br>China carrier speed | Servers with IPv4 | Pure-Rust multi-stream HTTP/1.1 download and upload speed test against CAICT GlobalSpeed carrier nodes (default: Beijing, Shanghai, Guangzhou across Telecom, Unicom, Mobile) | [`docs/network.md`](docs/network.md) |
| `network` | `network.cloudflare`<br>Cloudflare speed | Servers with internet | Pure-Rust dual-stack `speed.cloudflare.com` HTTPS download/upload throughput, idle & loaded latency, Waveform bufferbloat & CV stability grades, and STUN UDP packet-loss / MOS probing | [`docs/network.md`](docs/network.md) |
| `network` | `network.inetspeed`<br>Apple CDN speed | Servers with internet | Pure-Rust Apple CDN (`mensura.cdn-apple.com`) speed test with Dual DoH (`cloudflare-dns.com` + `dns.alidns.com`) optimal IP pinning, idle latency, and 1-thread & multi-thread download/upload rounds | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace`<br>Route tracing | Servers with raw sockets | Traces hop-by-hop ICMP routes (`Hop`, `Address`, `ASN`, `Network`, `Place`, `Latency`) to nearest YouTube/Netflix caches, Chinese carriers, and global backbones with IX tagging | [`docs/network.md`](docs/network.md) |
| `network` | `network.bgp`<br>BGP paths | Servers with a public IP | Queries global RIS route collectors for your announced prefix, RPKI status, and upstream carriers, providing an unlisted `imageupload.app` SVG topology link in `text` mode, a Mermaid flowchart in Markdown, and standalone SVG markup in JSON | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality`<br>IP reputation | Servers with a public IP | Cross-checks 9 IP databases to show whether your IP is **native** or **broadcast**, datacenter vs residential, fraud/risk indicators, open ports, 25 spam blacklists, and port-25 mail reachability | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media`<br>Streaming unlock | Servers with internet | Tests direct IPv4/IPv6 access across 68 streaming and online services in 5 world regions (Netflix, Disney+, YouTube, BBC iPlayer, Bahamut, Abema, etc.) and reports the unlocked region | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.bench`<br>CPU benchmarks | All servers *(per-suite checks)* | Runs selectable CPU sub-suites (`--cpu-bench`): default `sysbench`, `7zip`, `unixbench`, `hpl`, `passmark`, and `geekbench`, plus opt-in `x86_64` `ycruncher` and `prime95` torture checks | [`docs/cpu.md`](docs/cpu.md) |

## Maintainers

- [@Lowendaff](https://github.com/Lowendaff)

## Acknowledgements

`lowendaff-bench` builds upon the ideas, benchmarks, and open datasets of these open-source
projects and organizations:

- **[YABS (Yet Another Bench Script)](https://github.com/masonr/yet-another-bench-script)** —
  Inspiration and compatible parameters for `disk.fio` mixed random I/O tests and `network.iperf3`
  multi-stream speed tests.
- **[IPQuality (`xykt/IPQuality`)](https://github.com/xykt/IPQuality)** — Reference for
  `ip.quality` multi-source IP reputation checks, native vs broadcast origin detection, DNSBL
  lookups, and port-25 mail checks.
- **[RegionRestrictionCheck (`lmc999/RegionRestrictionCheck`)](https://github.com/lmc999/RegionRestrictionCheck)**
  and **[MediaUnlockTest (`HsukqiLee/MediaUnlockTest`)](https://github.com/HsukqiLee/MediaUnlockTest)** —
  Reference for `ip.media` regional streaming availability checks.
- **[NextTrace (`nxtrace/NTrace-core`)](https://github.com/nxtrace/NTrace-core)** — Fast ICMP route
  tracing engine and hop geolocation powering `network.trace`.
- **[peer.as (`Archeb/peer.as`)](https://github.com/Archeb/peer.as)** — Reference for
  `network.bgp` crossing-free layered AS-path layout, Tier-1 truncation, prepend folding, and SVG
  topology rendering.
- **[speedtaier (`transflo/speedtaier`)](https://github.com/transflo/speedtaier)** — Reference for
  `network.taier` CAICT GlobalSpeed (`dovalid`) handshake, multi-stream HTTP/1.1 throughput, and
  mainland China carrier node catalog.
- **[cloudflare-speed-cli (`kavehtehrani/cloudflare-speed-cli`)](https://github.com/kavehtehrani/cloudflare-speed-cli)** —
  Reference for `network.cloudflare` steady-state HTTPS throughput, loaded latency, Waveform
  bufferbloat / CV stability grading, and RFC 5389 STUN UDP packet-loss / E-model MOS probing.
- **[iNetSpeed-CLI (`nxtrace/iNetSpeed-CLI`)](https://github.com/nxtrace/iNetSpeed-CLI)** —
  Reference for `network.inetspeed` Dual DoH endpoint discovery, IP-pinned RTT selection, and
  `mensura.cdn-apple.com` single-thread & multi-thread throughput rounds.
- **Upstream benchmark tools & open data sources** —
  [`sysbench`](https://github.com/akopytov/sysbench) (Alexey Kopytov),
  [`7-Zip`](https://www.7-zip.org/) (Igor Pavlov),
  [`Byte-UnixBench`](https://github.com/kdlucas/byte-unixbench),
  [`HPL`](https://www.netlib.org/benchmark/hpl/) & [`OpenBLAS`](https://www.openblas.net/),
  [`PassMark PerformanceTest`](https://www.passmark.com/products/pt_linux/),
  [`y-cruncher`](http://www.numberworld.org/y-cruncher/) (Alexander J. Yee),
  [`Prime95 / GIMPS`](https://www.mersenne.org/),
  [`Geekbench`](https://www.geekbench.com/) (Primate Labs),
  [`STREAM`](https://www.cs.virginia.edu/stream/) (John D. McCalpin),
  [`multichase`](https://github.com/google/multichase) (Google),
  [`smartmontools`](https://www.smartmontools.org/) (`smartctl`),
  [`fio`](https://github.com/axboe/fio) (Jens Axboe),
  [`iperf3`](https://github.com/esnet/iperf) (ESnet),
  [`RIPEstat`](https://stat.ripe.net/) (RIPE NCC),
  [`iperf3serverlist.net`](https://iperf3serverlist.net/), and
  [`PeeringDB`](https://www.peeringdb.com/).

## Contributing

Questions, bug reports, and feature requests are welcome on
[GitHub Issues](https://github.com/Lowendaff/lowendaff_bench/issues). Pull requests are accepted;
before submitting a change, please review [`CONTRIBUTING.md`](CONTRIBUTING.md) for workspace
architecture, Rust style and `ast-grep` rules, commit message conventions, and required CI gates.

Small note: If editing the README, please conform to the
[standard-readme](https://github.com/RichardLitt/standard-readme) specification.

## License

[MIT](LICENSE) (c) 2026 Lowendaff
