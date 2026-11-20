# lowendaff-bench

**English** | [&#x7B80;&#x4F53;&#x4E2D;&#x6587; (Simplified Chinese)](README.zh-CN.md) | [&#x65E5;&#x672C;&#x8A9E; (Japanese)](README.ja.md)

`lowendaff-bench` is an all-in-one hardware benchmark, network speed test, and IP check tool for
Linux servers. It works out of the box on dedicated bare-metal servers, virtual machines (KVM, Xen,
cloud instances), and containers (Docker, LXC, OpenVZ).

### Key Highlights

- **Single file, zero setup, no config files.** Download one standalone binary and run it right
  away on `x86_64` or `aarch64` (ARM64) Linux. Everything is controlled with straightforward
  command-line options.
- **Fair, comparable numbers across every Linux distro.** Helper tools (`sysbench`, `fio`,
  `iperf3`, `NextTrace`, `Geekbench 7`, etc.) are automatically downloaded at pinned versions and
  verified before running, rather than relying on outdated packages installed on the host.
- **Adapts to your server and protects low-memory VPS instances.** Automatically skips tests that
  do not apply to your machine (for example, physical drive SMART checks on virtual machines or
  multi-socket NUMA tests on single-CPU hosts). Memory tests stay under half of free RAM, and
  Geekbench 7 automatically sets up temporary swap space when RAM is tight so small VPS instances
  do not crash.
- **Privacy-friendly reports ready to share.** View clean tables directly in your terminal, or save
  the report as **Markdown** or **JSON**. By default, your server's public IP addresses, reverse
  DNS hostnames, and hardware serial numbers are masked so you can share results without manual
  redaction.

For deep-dive guides on how each test calculates and interprets its numbers, see
[`docs/`](docs/README.md) (in Chinese).

---

## Quick Start in Three Steps

Grab the latest binary for your CPU (`lowendaff-bench-x86_64` for standard Intel/AMD servers, or
`lowendaff-bench-aarch64` for ARM64 servers) from
[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases), make it executable, and
run it with `sudo` (or as `root`):

```sh
# Example for x86_64 (replace with lowendaff-bench-aarch64 on ARM64 servers)
curl -fLO https://github.com/Lowendaff/lowendaff_bench/releases/latest/download/lowendaff-bench-x86_64
chmod +x lowendaff-bench-x86_64
sudo ./lowendaff-bench-x86_64
```

> While tests are running, live progress is shown on screen. Once finished, the complete formatted
> report prints at the end. Press `Ctrl-C` once at any time to stop early and still get a report of
> everything completed so far.

---

## Everyday Usage Examples

Assuming the binary is named `lowendaff-bench`:

```sh
# 1. See all 13 available test items and what they require (no root needed)
./lowendaff-bench --list

# 2. Run the full suite (CPU, memory, disk, network speed/routes/BGP, IP reputation, media unlock)
sudo ./lowendaff-bench

# 3. Quick server check: skip Geekbench 7 (long & uploads public scores) and route tracing
sudo ./lowendaff-bench --skip cpu.geekbench,network.trace

# 4. Check IP reputation & streaming unlock, or draw the server's BGP upstream topology graph
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only network.bgp

# 5. Pick any mix of categories and individual test items
sudo ./lowendaff-bench --only cpu.sysbench,memory.stream,disk.fio

# 6. Run a category while excluding one item (e.g. IP reputation without streaming checks)
sudo ./lowendaff-bench --only ip --skip ip.media

# 7. Save the report directly as Markdown (great for forums/issues) or JSON (great for scripts)
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 8. Test disk speed on another mounted drive (avoid /tmp, which is often a RAM disk)
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 9. Filter by region: test only Japan & US speed servers, China routes, or Asian streaming apps
sudo ./lowendaff-bench --only network.iperf3 --countries JP,US
sudo ./lowendaff-bench --only network.trace --trace-groups china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 10. Check IP reputation without testing outbound mail port 25 (useful on strict cloud providers)
sudo ./lowendaff-bench --only ip.quality --no-mail

# 11. Use a proxy to speed up tool downloads, and show unmasked public IPs in the report
#     (Put proxy variables after sudo; IP, speed, and streaming checks still connect directly)
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16 --show-ip
```

---

## Command-Line Options & Allowed Values

### All Options

| Option | Syntax | Default | What it does |
|---|---|---|---|
| `--only` | `<ITEM,...>` | All items | Run only the listed categories or test IDs (comma-separated or repeated) |
| `--skip` | `<ITEM,...>` | None | Skip the listed categories or test IDs (can be combined with `--only`) |
| `-o, --output` | `<FILE>` | Terminal | Save the final report to `<FILE>` instead of printing it to standard output |
| `-f, --format` | `<FORMAT>` | `text` | Choose report format: `text`, `markdown` (or `md`), or `json` |
| `--work-dir` | `<DIR>` | `.` | Directory used for temporary test files, cleaned up on exit (point this at the disk you want to test; avoid `/tmp`) |
| `--countries` | `<CODE,...>` | Default locations | Limit `network.iperf3` to two-letter country codes (such as `JP,US,DE`) or `all` |
| `--trace-groups` | `<GROUP,...>` | Caches + defaults | Choose which target groups `network.trace` traces routes to |
| `--media-groups` | `<GROUP,...>` | `all` | Choose which regional groups `ip.media` checks for streaming availability |
| `--no-dnsbl` | *(flag)* | Off | Skip the 25 spam/abuse blacklist (DNSBL) lookups inside `ip.quality` |
| `--no-mail` | *(flag)* | Off | Skip outbound port-25 mail server connectivity checks inside `ip.quality` |
| `--download-connections` | `<1..=256>` | `32` | Max parallel connections when downloading benchmark tools |
| `--show-ip` | *(flag)* | Off (masked) | Show your server's full public IPv4/IPv6 addresses and hostnames in the report |
| `-v, --verbose` | *(flag)* | Off | Print detailed download progress and underlying tool commands for troubleshooting |
| `--list` | *(flag)* | — | List all available test items, supported environments, and tool versions, then exit |
| `-h, --help` | *(flag)* | — | Show help message and exit |
| `-V, --version` | *(flag)* | — | Show version number and exit |

### Allowed Values Reference

| Option | Allowed Values | Description |
|---|---|---|
| `--only` / `--skip` *(categories)* | `cpu` | Both CPU tests: `cpu.sysbench`, `cpu.geekbench` |
| | `memory` | All 4 memory tests: `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa` |
| | `disk` | Both disk tests: `disk.health`, `disk.fio` |
| | `network` | All 3 network tests: `network.iperf3`, `network.trace`, `network.bgp` |
| | `ip` | Both IP tests: `ip.quality`, `ip.media` |
| `--only` / `--skip` *(single items)* | `disk.health`, `cpu.sysbench`, `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa`, `disk.fio`, `network.iperf3`, `network.trace`, `network.bgp`, `ip.quality`, `ip.media`, `cpu.geekbench` | Include or exclude any individual test item |
| `-f, --format` | `text` | Aligned plain-text tables formatted for terminal viewing |
| | `markdown` *(or `md`)* | GitHub-Flavored Markdown ready to paste into posts, wikis, or issues |
| | `json` | Structured JSON output for scripts, dashboards, or automated archiving |
| `--trace-groups` | `services` | Nearest YouTube and Netflix video cache servers only |
| | `china` | Mainland China carrier routes (Telecom 163/CN2, Unicom 169/9929, Mobile CMNET/CMIN2) |
| | `international` | Global Tier-1 backbones, Telegram data centers, AWS, and Google Cloud nodes |
| | `all` | Every route target above (73 targets in total) |
| `--media-groups` | `global` | Global platforms (Netflix, YouTube Premium, Disney+, Prime Video, Spotify, TikTok, etc.) |
| | `asia` | Asian platforms (Bilibili HK/MO/TW, Bahamut, Abema, U-NEXT, TVer, Viu, etc.) |
| | `europe` | European platforms (BBC iPlayer, ITVX, Channel 4, Canal+, ZDF, ARTE, RAI Play, etc.) |
| | `americas` | North & Latin American platforms (Hulu, Max, Peacock, Paramount+, Starz, Crave, etc.) |
| | `oceania` | Australian & New Zealand platforms (Stan, 9Now, 7plus, ABC iview, TVNZ+, etc.) |
| | `all` | All 68 platforms across all five regions (default) |
| `--countries` | Two-letter country code *(e.g. `US`, `JP`, `DE`, `SG`, `GB`)* | Test all public `iperf3` servers in the selected countries (case-insensitive) |
| | `all` | Test every public server in the server list |

---

## What Each Test Item Does

Before running benchmarks, `lowendaff-bench` gathers a quick **System Overview (`[System]`)**:
whether the host is bare metal, a VM, or a container, CPU model and cache sizes, RAM and swap, OS,
kernel, uptime, TCP congestion control (such as BBR), disks, network link speeds (flagging degraded
PCIe slots), and IPv4/IPv6 connectivity. On dedicated servers, it also lists populated RAM slots,
ECC memory error counts, IPMI/BMC controllers, and hardware RAID cards (see
[`docs/system.md`](docs/system.md)).

Then it runs up to 13 test items in order:

| Category | Item ID & Name | Where it runs | What it tells you | Deep dive |
|---|---|---|---|---|
| `disk` | `disk.health`<br>Disk health | Dedicated servers | Reads drive SMART data (including drives behind common hardware RAID cards) to check overall health, power-on hours, temperature, SSD wear, total writes, and bad sectors | [`docs/disk.md`](docs/disk.md) |
| `cpu` | `cpu.sysbench`<br>CPU speed | All servers | Runs a 20-second prime calculation on 1 core and on all cores to measure single-core responsiveness and multi-core scaling | [`docs/cpu.md`](docs/cpu.md) |
| `memory` | `memory.sysbench`<br>Sequential memory | All servers | Measures single-thread and multi-thread sequential memory read/write speed | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream`<br>Sustained bandwidth | All servers | Uses the industry-standard STREAM benchmark to measure sustained real-world RAM bandwidth (`Copy`, `Scale`, `Add`, `Triad`) | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency`<br>Cache & RAM latency | All servers | Measures random access latency in nanoseconds across CPU L1/L2/L3 caches and main memory | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa`<br>Multi-socket NUMA | Multi-NUMA servers | On dual-socket or multi-node machines, measures bandwidth and latency between local RAM and cross-socket RAM | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio`<br>Disk read/write | All servers | Runs YABS-compatible 50/50 random read/write tests at `4K`, `64K`, `512K`, and `1M` block sizes to show both database IOPS (`4K`) and large-file throughput (`512K`/`1M`) | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3`<br>Network speed | Servers with internet | Tests upload, download, and latency over IPv4 and IPv6 against public servers (default: Los Angeles, New York, Virginia, Tokyo, Frankfurt) | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace`<br>Route tracing | Servers with raw sockets | Traces hop-by-hop network paths to nearest YouTube/Netflix caches, Chinese carriers, and global backbones so you can spot direct vs detoured routing | [`docs/network.md`](docs/network.md) |
| `network` | `network.bgp`<br>BGP paths | Servers with a public IP | Queries global RIS route collectors for your announced prefix, RPKI status, and upstream carriers, drawing an AS-path diagram in the terminal, a Mermaid flowchart in Markdown, and standalone SVG markup in JSON | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality`<br>IP reputation | Servers with a public IP | Cross-checks 9 IP databases to show whether your IP is **native** or **broadcast**, datacenter vs residential, fraud/risk indicators, open ports, 25 spam blacklists, and port-25 mail reachability | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media`<br>Streaming unlock | Servers with internet | Tests direct IPv4/IPv6 access across 68 streaming and online services in 5 world regions (Netflix, Disney+, YouTube, BBC iPlayer, Bahamut, Abema, etc.) and reports the unlocked region | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.geekbench`<br>Geekbench 7 | `glibc >= 2.35` + IPv4 | Runs official Geekbench 7.1.0 for widely comparable single-core and multi-core scores plus an online result link | [`docs/cpu.md`](docs/cpu.md) |

---

## Building from Source

Most users can download prebuilt binaries directly from
[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases). To compile a standalone
static binary yourself on Linux, install the Rust toolchain (pinned in
[`rust-toolchain.toml`](rust-toolchain.toml)) and `musl-tools` (`musl-gcc`):

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

The resulting binary will be at `target/x86_64-unknown-linux-musl/release/lowendaff-bench`. For
project structure, third-party tool builds, and contributor guidelines, see
[`CONTRIBUTING.md`](CONTRIBUTING.md).

---

## Acknowledgments

`lowendaff-bench` builds upon the ideas, benchmarks, and open datasets of many great open-source
projects:

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
- **Upstream benchmark tools & open data sources** —
  [`sysbench`](https://github.com/akopytov/sysbench) (Alexey Kopytov),
  [`STREAM`](https://www.cs.virginia.edu/stream/) (John D. McCalpin),
  [`multichase`](https://github.com/google/multichase) (Google),
  [`smartmontools`](https://www.smartmontools.org/) (`smartctl`),
  [`fio`](https://github.com/axboe/fio) (Jens Axboe),
  [`iperf3`](https://github.com/esnet/iperf) (ESnet),
  [`Geekbench`](https://www.geekbench.com/) (Primate Labs),
  [`RIPEstat`](https://stat.ripe.net/) (RIPE NCC),
  [`iperf3serverlist.net`](https://iperf3serverlist.net/), and
  [`PeeringDB`](https://www.peeringdb.com/).
