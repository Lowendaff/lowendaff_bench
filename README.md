# lowendaff-bench

A comprehensive test tool for Linux servers, virtual or dedicated, written in Rust:

- Each test item is a plugin compiled into one static binary.
- An orchestrator selects the plugins, downloads the pinned tools they need and runs them in
  order with timeouts. It logs its progress on stderr, and prints one report on stdout when every
  plugin is done.
- It must run as root.

The program prints in English. Every word it prints comes from one message catalog in
`crates/bench/src/locale/`, so another language needs only a catalog of its own.

**Status:** nine test items, each a plugin with a result of its own type: disk health, CPU
performance, four items of memory performance, disk performance, network speed and Geekbench 7.
[`CONTRIBUTING.md`](CONTRIBUTING.md) holds the layout of the code, the steps to add a test item
and the conventions for code, commits and pull requests.

The tool first reads its host: where it runs (a dedicated server, a virtual machine or a
container), its CPU, memory, disks and system, and whether it reaches the Internet over IPv4 and
IPv6, with the public address of each. The report starts with this system information: CPU,
memory and swap, disks and filesystems, distribution, kernel, uptime, load, TCP congestion
control, and the two address families; on dedicated servers also the machine, board and
firmware, the BMC, ECC, the memory modules and RAID cards. Only the test items made for the
platform run, and the test between NUMA nodes only where two of them can be used; `--list` shows
the items by group, and where each applies.

| Test item | Shows |
|-----------|-------|
| Disk health (`disk.health`) | Dedicated servers only: each physical disk's SMART health as smartctl judges it, with power-on time, temperature, endurance used, data written, bad blocks and interface errors. Disks behind MegaRAID and 3SNIC RAID cards are included |
| CPU performance (`cpu.sysbench`) | sysbench's CPU test (primes up to 10000), 20 seconds with one thread and 20 seconds with every usable thread: events per second, and how many times the single-thread rate the multi-thread one is |
| Memory sysbench (`memory.sysbench`) | sysbench's sequential read and write bandwidth, each with one thread and with every usable thread |
| Memory STREAM (`memory.stream`) | STREAM's Copy, Scale, Add and Triad, each with one thread and with every usable thread |
| Memory latency (`memory.latency`) | multichase's latency in each cache level and in memory |
| Memory NUMA (`memory.numa`) | Machines with two usable NUMA nodes or more: the bandwidth (multiload's stream-triad) and the latency from each node's CPUs to each node's memory |
| Disk performance (`disk.fio`) | Speed and IOPS of the test directory's filesystem, measured with fio exactly as YABS does: mixed random reads and writes (50/50) at 4K, 64K, 512K and 1M. Skipped on a RAM disk or without enough free space |
| Network speed (`network.iperf3`) | iperf3's TCP throughput each way, and the latency, between the host and public iperf3 servers, as YABS measures them: over IPv4, then over IPv6 when the host has it, an upload and a download of 10 seconds over 8 streams to each server, by default those of Los Angeles, New York, Virginia, Tokyo and Frankfurt. About 5 GB of traffic per server at 1 Gbit/s ("Network speed" below); `--skip network` leaves it out |
| Geekbench 7 (`cpu.geekbench`) | Geekbench 7's single- and multi-core scores and the public result link. The free version uploads every result to the Geekbench Browser; `--skip cpu.geekbench` leaves it out. Needs IPv4 and glibc 2.35 or later, and adds temporary swap when memory is short |

The four memory items make up the report's memory section. Each reads the available memory when
it starts, and every run fits in half of it.

## Usage

```sh
lowendaff-bench --list                         # list the test items by group
sudo lowendaff-bench                           # run all of them
sudo lowendaff-bench --only cpu,disk.fio       # run a subset by group or id; --skip excludes
sudo lowendaff-bench --config bench.toml       # per-item settings: [plugins."<id>"]
sudo lowendaff-bench -v                        # log more detail: each download and each tool run
sudo lowendaff-bench --show-ip                 # show the public addresses in full
sudo lowendaff-bench --download-connections 8  # at most 8 connections per download (default 32)
sudo lowendaff-bench --countries JP,DE         # the network test's servers in these countries
```

- **Output.** The progress log on stderr has one line per event, after the time since the start:
  the host and its network, the tools' downloads, and each item's start, steps and end; `-v` adds
  each download's address, its number of byte ranges and its retries, and each tool's command and
  exit code, with its last lines when it fails, except for Geekbench and smartctl, whose output
  holds a result's claim link or the disks' serial numbers, and iperf3, whose output names the
  host's own address. The report follows on stdout once every item is done: the system
  information, a section for each group (CPU, memory, disks, network) and a summary of how each
  item ended. On a terminal, its tables wrap to the terminal's width. After
  Ctrl-C the report holds what finished, and the rest is marked interrupted.
- **Public addresses.** Each address family that reaches the Internet asks Cloudflare, ipify and
  icanhazip in turn, without a proxy, for the address they see; the first valid answer counts.
  Behind NAT that is the shared address. The report masks it, keeping two octets of IPv4 and the
  first 32 bits of IPv6, unless `--show-ip` is given; the progress log masks it always.

- **Run directory.** The tool creates its run directory under the current directory, or under
  `--work-dir`, and removes it at exit. Avoid `/tmp`: it may be RAM-backed or mounted `noexec`.
- **Exit codes:**
  - `0`: the run finished;
  - `2`: usage or config error, or not root;
  - `130`: interrupted.
- **Downloads.** Before the first item runs, the program downloads the tools that the items need,
  and the files that they ask for, such as the network test's server list.
  A file of 2 MiB or more comes in byte ranges over up to 32 connections at once
  (`--download-connections` sets the number, from 1 to 256), when its server serves ranges: a slow
  route often limits each connection rather than the host. Each range must come back exactly as
  asked for, and the whole file must match its pinned size and SHA-256 before it is used. A
  download that fails in a way that may pass (no answer, a server answering 408, 429 or 5xx, or a
  body that breaks off, stalls for 30 seconds or ends short) gets up to four attempts, after waits
  of 1, 2 and 4 seconds. A byte range goes on from its first missing byte, and one that brought
  1 MiB or more before it broke off starts its count afresh; a smaller file, or one from a server
  without ranges, starts over. Ctrl-C stops the downloads too.
- **Proxies.** Tool downloads go through the proxy that `HTTPS_PROXY` or `ALL_PROXY` names
  (lowercase names work too); `NO_PROXY` exempts hosts. HTTP and SOCKS5 proxies work, and with
  `socks5h://` the proxy resolves the names. On a server that reaches GitHub poorly, for
  example, `ssh -D 1080 <a host abroad>` opens a SOCKS5 proxy, and
  `sudo ALL_PROXY=socks5h://127.0.0.1:1080 lowendaff-bench` uses it: put the variable after
  `sudo`, which drops most of the caller's environment. A proxy cannot change a tool, since each
  is pinned by SHA-256 and size. A download that fails on the network names the proxy variable
  that is set, or suggests setting one. The network checks at start connect directly.

## Network speed

The network test (`network.iperf3`) runs iperf3 as YABS does. Over IPv4, then over IPv6 when the
host reaches the Internet over it, each server gets an upload of 10 seconds over 8 streams, a
second's pause, then a download: `iperf3 -4|-6 -c <server> -p <port> -P 8 -t 10 -J`, with `-R`
for the download. A run that fails, such as on a server that is busy with another client, is
tried again 2 seconds later on another port of the server, up to three times; a server that no
port answers gets no download. The upload shows the speed that the server received, the download
the speed that the host received, and the latency is the smallest round-trip time that the
host's kernel saw during the upload. iperf3 runs as the unprivileged user 65534 (`nobody`), since
the servers are third parties.

- **Traffic.** Each server moves about 5 GB at 1 Gbit/s, over both families and directions: about
  25 GB for the five default servers, ten times as much on a 10 Gbit/s host. On a host that pays
  for its traffic, choose fewer servers, or leave the test out with `--skip network`.
- **The servers.** They are listed in `servers/iperf3.json` in this repository, which the program
  downloads at each run from
  `https://raw.githubusercontent.com/Lowendaff/lowendaff_bench/main/servers/iperf3.json`, so that a
  server that disappears is replaced without a new release. The list holds, for each location, the
  first of its reviewed candidates in `servers/iperf3-candidates.json` that
  [iperf3serverlist.net](https://iperf3serverlist.net), which checks its servers every 15 minutes,
  still lists; `scripts/iperf3-servers.sh update` writes it, and no one edits it by hand. A
  location without such a candidate is left out. The program checks the list's rules before it
  uses it (`scripts/iperf3-servers.sh` names them), and CI checks them in every pull request.
- **Choosing servers.** By default, the servers that the list marks as defaults. `--countries`
  takes ISO 3166 codes in any case, separated by commas, or `all`: every server of those
  countries, instead of the defaults. A code without a server in the list gets a note.
- **Settings.** The table `[plugins."network.iperf3"]` of the config file may set:

  ```toml
  [plugins."network.iperf3"]
  countries = ["US", "JP"]   # as --countries, which wins over it
  servers = ["tokyo"]        # by the list's ids; wins over the countries
  list = "my-servers.json"   # another list in the same format: a path, or an https address
  seconds = 10               # each way
  streams = 8
  rate = 100                 # Mbit/s that a stream sends at most; no limit when left out
  ```

## Build

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml). Release binaries are
static musl builds for `x86_64` and `aarch64`. The `ring` crate compiles C code for the musl
target, so the build needs `musl-gcc` (Debian/Ubuntu package `musl-tools`).

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

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
| Commit messages | `scripts/check-commits.sh origin/main HEAD`; CI adds the title | any host |
| Rule tests | `ast-grep test` (cases in `ast-grep/tests/`) | any host |
| Rules | `ast-grep scan` (rules in `ast-grep/rules/`, config in `sgconfig.yml`), then `scripts/check-docs-cases.sh` and `scripts/check-docs.sh`: no tracked file holds CJK characters, except the catalog of another language in `crates/bench/src/locale/<language>/`, and no tracked file outside the fixture data points into the design documents | any host |
| Server list | `scripts/iperf3-servers-cases.sh`, then `scripts/iperf3-servers.sh check`: the network test's server list and its candidates follow their rules, the list holds candidates only, and nothing builds from it; the cases include the fixtures that the program's reader reads too | any host with jq |
| Static build | `cargo build --release --locked --target <arch>-unknown-linux-musl --bin lowendaff-bench` | Linux |
| Swap round trip | `cargo build --release --locked --target <arch>-unknown-linux-musl -p swap-file --example roundtrip`, then `sudo target/<arch>-unknown-linux-musl/release/examples/roundtrip <dir>`: turns a 64 MiB swap file in `<dir>` on and off | Linux, with sudo, on a filesystem that holds swap files, such as ext4 |
| Smoke | `scripts/smoke-iperf3.sh <dir>`, which starts an iperf3 server of the pinned build on 127.0.0.1 and writes a server list naming it and a config file, then `scripts/smoke.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench --skip cpu.geekbench --config <dir>/config.toml`: a full run without Geekbench 7, which would publish a result, in which no test item is skipped, fails or times out, with the network test against that server only, the report on stdout and the progress log on stderr, then a short run that must find the public IPv4 address without the proxy that the environment names | Linux, as root or with sudo, with network access (it downloads sysbench, STREAM, multichase, fio and iperf3) |
| Smoke mutants | `scripts/smoke-mutants.sh <arch>-unknown-linux-musl`: smoke must reject deliberately broken builds; these runs skip the groups cpu, memory, disk and network (`--skip cpu,memory,disk,network`) | Linux, as root or with sudo |
| Distributions | `scripts/distros.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench ghcr.io/lowendaff/mirror`: the system information of a run with every test item skipped, in the images of `scripts/distros.txt`, 12 distributions (13 on x86_64), pulled from their copies on GHCR, with the uptime and load marked as the host's, and Geekbench 7 there without a network, which must skip for the image's glibc; without the last argument, from the original registries | Linux or macOS with Docker; the copies need `docker login ghcr.io` |
| Network speed, not required | `scripts/network-check.sh <build> servers/iperf3.json`: the network test against one or two servers of the list, chosen at random, 3 seconds over 2 streams of at most 100 Mbit/s each (300 MB at most); it fails only when the program misbehaves, and a server that is busy or cannot be reached shows in the job's summary. CI runs it in the job `network speed against public servers` with the x86_64 build of the test job, which no merge waits for | Linux, as root or with sudo, with network access |

`<arch>` is `x86_64` or `aarch64`.

The two `test, static build and smoke` jobs first run `scripts/ci-scope.sh`, which decides whether
the rest of the job runs. A job skips its other steps, and still passes, in two cases:

- an earlier run validated an ancestor of the commit in full, on the same base and runner image,
  and only Markdown or `LICENSE` changed since;
- the whole change (a pull request against its base, or a push against the commit before it)
  touches only Markdown or `LICENSE`.

Anything else, or any doubt, runs every step. A job that ran every step successfully uploads a
validation record (`validation-pr<N>-<arch>` or `validation-main-<arch>`, kept 90 days), and the
decision with its evidence appears in the job summary. The workflow audit job, which runs "Shell
scripts", also runs this decision's cases: `scripts/ci-scope-cases.sh`.

`GH_TOKEN=$(gh auth token) scripts/verify-commits.sh` runs the gates on every commit of a branch,
each commit on its own, then checks the commit messages. It leaves to CI the gates that need Linux
or root, and Tool downloads.

CI pins these tool versions; install the same versions locally:

- cargo-nextest 0.9.151;
- cargo-deny 0.20.2;
- zizmor 1.32.0, for example `uvx zizmor@1.32.0 --persona pedantic .`;
- ast-grep 0.51.1;
- shellcheck 0.11.0;
- actionlint 1.7.12.

The `cargo deny check` and `zizmor` gates need network access: one fetches the RustSec advisory
database, the other runs online audits (zizmor needs `GH_TOKEN` for those).

CI also uses sccache (backed by the GitHub Actions cache) and the mold linker. They change build
speed, not results, so they are optional locally.

Every Monday, the `latest-deps` workflow updates a scratch checkout to the newest dependency
versions that `Cargo.toml` allows. It then runs the tests and the dependency policy. A failure
means an upstream release would break the build. The committed `Cargo.lock` is not touched.

`scripts/distros.txt` lists container images pinned by digest. The `mirror` workflow copies them
to `ghcr.io/lowendaff/mirror` under the same digests with `scripts/mirror.sh`, which checks each
copy, so that CI can pull them from there: AWS ECR Public allows one anonymous pull per second
per address, and GitHub's runners share addresses. The workflow publishes with `GITHUB_TOKEN`, so
the packages are linked to this repository and CI can pull them with its own token. After
changing `scripts/distros.txt`, run `gh workflow run mirror.yml --ref <branch>`; images that are
already copied are skipped.

Dependabot (`.github/dependabot.yml`) proposes weekly update PRs for cargo and for the pinned
GitHub Actions. It waits until a release is at least 7 days old.

## Third-party tools

Some test items run external tools. The program downloads each tool as a pinned build and checks
its SHA-256 and size before use; it never uses a tool already installed on the host. This
repository builds the tools and publishes them as immutable GitHub releases, except Geekbench.

| Tool | Version | License | Used by | Release |
|------|---------|---------|---------|---------|
| fio, with libaio 0.3.113 linked statically | 3.43 | GPL-2.0; libaio LGPL-2.1 | Disk performance (`disk.fio`) | `fio-3.43-2` |
| iperf3, without OpenSSL, so without its authentication code, and without SCTP | 3.22 | BSD-3-Clause, with the notices of the code it bundles | Network speed (`network.iperf3`) | `iperf3-3.22-1` |
| smartctl, from smartmontools | 7.5 | GPL-2.0-or-later | Disk health (`disk.health`) | `smartctl-7.5-1` |
| sysbench, with its bundled LuaJIT and Concurrency Kit | 1.0.20 | GPL-2.0-or-later; LuaJIT MIT; Concurrency Kit BSD-2-Clause | CPU performance (`cpu.sysbench`), memory sysbench (`memory.sysbench`) | `sysbench-1.0.20-1` |
| STREAM, built for six array sizes | 5.10 | STREAM's own: free to use and change, but published results must follow its run rules or say that they do not | Memory STREAM (`memory.stream`) | `stream-5.10-1` |
| multichase and multiload, with glibc 2.41 linked statically | commit `8b68e918` of 2026-10-01 | Apache-2.0; glibc LGPL-2.1 | Memory latency (`memory.latency`), memory NUMA (`memory.numa`) | `multichase-2026.10.01-1` |

Each release holds the binaries for x86_64 and aarch64, the exact sources they were built from,
the build script, the license texts (also that of the C library linked statically: musl's, which
`fio-3.43-1` lacked, or glibc's for multichase) and `SHA256SUMS`. The workflow `tools` builds
each tool with `scripts/build-<tool>.sh` in the image that the script names on its `IMAGE` and
`IMAGE_DIGEST` lines: Alpine 3.22, or Debian 13 for multichase, which needs glibc. A pull request
that changes a build runs every build without publishing anything. To publish a new build,
update the script's `RELEASE` (the tool's version and the build number) and, for a new version,
the sources and their SHA-256; merge, run `gh workflow run tools.yml --ref main -f tool=<tool>`,
and then update the tool's pin in `crates/bench-tool`. The builds are reproducible bit for bit
while the images' packages stay the same.

Geekbench 7.1.0, for Geekbench 7 (`cpu.geekbench`), is the exception. It is closed source and
its license forbids redistribution, so the program downloads Primate Labs' own archive from
`cdn.geekbench.com`, pinned by SHA-256 and size, and unpacks it. The free license allows
personal, non-commercial use on one's own devices only, requires every result to be uploaded to
the Geekbench Browser, and forbids automated use; the owner accepted that risk.
