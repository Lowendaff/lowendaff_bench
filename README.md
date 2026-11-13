# lowendaff-bench

A comprehensive test tool for Linux servers, virtual or dedicated, written in Rust:

- Each test item is a plugin compiled into one static binary.
- An orchestrator selects the plugins, downloads the pinned tools they need, runs them in order
  with timeouts, and prints the results.
- It must run as root.

The program prints its results in Chinese. [`glossary.md`](glossary.md) gives the English term
that this README, the code and the commit messages use for each word it prints often.

**Status:** the plugin framework is in place, with six test items so far: system information,
disk health, CPU performance, memory performance, disk performance and Geekbench 7. The plugin
model is being reworked, one pull request at a time; no test item is added meanwhile. The
conventions for code, commits and pull requests are in [`CONTRIBUTING.md`](CONTRIBUTING.md).

The tool first detects where it runs: a dedicated server, a virtual machine or a container. It
prints this on its environment line and runs only the test items made for that platform. For
example, system information has a variant for each platform (`system.dedicated`, `system.vm`,
`system.container`), and exactly one of them runs. `--list` shows which platforms each item
applies to.

| Test item | Shows |
|-----------|-------|
| System information | CPU, memory and swap, disks and filesystems, distribution, kernel, uptime, load, TCP congestion control, IPv4/IPv6 reachability; on dedicated servers also the machine, board and firmware, the BMC, ECC, the memory modules and RAID cards |
| Disk health | Dedicated servers only: each physical disk's SMART health as smartctl judges it, with power-on time, temperature, endurance used, data written, bad blocks and interface errors. Disks behind MegaRAID and 3SNIC RAID cards are included |
| CPU performance | sysbench's CPU test (primes up to 10000), 20 seconds with one thread and 20 seconds with every usable thread: events per second, and how many times the single-thread rate the multi-thread one is |
| Memory performance | sysbench's read and write bandwidth and STREAM's Copy, Scale, Add and Triad, each with one thread and with every usable thread; multichase's latency in each cache level and in memory; on machines with several NUMA nodes, the bandwidth (multiload's stream-triad) and latency from each node's CPUs to each node's memory. Every run fits in half of the available memory |
| Disk performance | Speed and IOPS of the test directory's filesystem, measured with fio exactly as YABS does: mixed random reads and writes (50/50) at 4K, 64K, 512K and 1M. Skipped on a RAM disk or without enough free space |
| Geekbench 7 | Geekbench 7's single- and multi-core scores and the public result link. The free version uploads every result to the Geekbench Browser; `--skip cpu.geekbench` leaves it out. Needs IPv4 and glibc 2.35 or later, and adds temporary swap when memory is short |

## Usage

```sh
lowendaff-bench --list                      # list the test items
sudo lowendaff-bench                        # run all of them
sudo lowendaff-bench --only <id>,<id>       # run a subset; --skip excludes items
sudo lowendaff-bench --config bench.toml    # per-item settings: [plugins."<id>"]
```

- **Run directory.** The tool creates its run directory under the current directory, or under
  `--work-dir`, and removes it at exit. Avoid `/tmp`: it may be RAM-backed or mounted `noexec`.
- **Exit codes:**
  - `0`: the run finished;
  - `2`: usage or config error, or not root;
  - `130`: interrupted.
- **Proxies.** Tool downloads go through the proxy that `HTTPS_PROXY` or `ALL_PROXY` names
  (lowercase names work too); `NO_PROXY` exempts hosts. HTTP and SOCKS5 proxies work, and with
  `socks5h://` the proxy resolves the names. On a server that reaches GitHub poorly, for
  example, `ssh -D 1080 <a host abroad>` opens a SOCKS5 proxy, and
  `sudo ALL_PROXY=socks5h://127.0.0.1:1080 lowendaff-bench` uses it: put the variable after
  `sudo`, which drops most of the caller's environment. A proxy cannot change a tool, since each
  is pinned by SHA-256 and size. A download that fails on the network names the proxy variable
  that is set, or suggests setting one. The IPv4/IPv6 check at start connects directly.

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
| Rules | `ast-grep scan` (rules in `ast-grep/rules/`, config in `sgconfig.yml`) | any host |
| Static build | `cargo build --release --locked --target <arch>-unknown-linux-musl --bin lowendaff-bench` | Linux |
| Swap round trip | `cargo build --release --locked --target <arch>-unknown-linux-musl -p swap-file --example roundtrip`, then `sudo target/<arch>-unknown-linux-musl/release/examples/roundtrip <dir>`: turns a 64 MiB swap file in `<dir>` on and off | Linux, with sudo, on a filesystem that holds swap files, such as ext4 |
| Smoke | `scripts/smoke.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench --skip cpu.geekbench`: a full run without Geekbench 7, which would publish a result, in which no test item is skipped, fails or times out | Linux, as root or with sudo, with network access (it downloads sysbench, STREAM, multichase and fio) |
| Smoke mutants | `scripts/smoke-mutants.sh <arch>-unknown-linux-musl`: smoke must reject deliberately broken builds; these runs skip CPU performance, memory performance, disk performance and Geekbench 7 | Linux, as root or with sudo |
| Distributions | `scripts/distros.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench ghcr.io/lowendaff/mirror`: system information in the images of `scripts/distros.txt`, 12 distributions (13 on x86_64), pulled from their copies on GHCR, with the uptime and load marked as the host's, and Geekbench 7 there without a network, which must skip for the image's glibc; without the last argument, from the original registries | Linux or macOS with Docker; the copies need `docker login ghcr.io` |

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
| smartctl, from smartmontools | 7.5 | GPL-2.0-or-later | Disk health (`disk.health`) | `smartctl-7.5-1` |
| sysbench, with its bundled LuaJIT and Concurrency Kit | 1.0.20 | GPL-2.0-or-later; LuaJIT MIT; Concurrency Kit BSD-2-Clause | CPU performance (`cpu.sysbench`), memory performance (`memory`) | `sysbench-1.0.20-1` |
| STREAM, built for six array sizes | 5.10 | STREAM's own: free to use and change, but published results must follow its run rules or say that they do not | Memory performance (`memory`) | `stream-5.10-1` |
| multichase and multiload, with glibc 2.41 linked statically | commit `8b68e918` of 2026-10-01 | Apache-2.0; glibc LGPL-2.1 | Memory performance (`memory`) | `multichase-2026.10.01-1` |

Each release holds the binaries for x86_64 and aarch64, the exact sources they were built from,
the build script, the license texts (also that of the C library linked statically: musl's, which
`fio-3.43-1` lacked, or glibc's for multichase) and `SHA256SUMS`. The workflow `tools` builds
each tool with `scripts/build-<tool>.sh` in the image that the script names on its `IMAGE` and
`IMAGE_DIGEST` lines: Alpine 3.22, or Debian 13 for multichase, which needs glibc. A pull request
that changes a build runs every build without publishing anything. To publish a new build,
update the script's `RELEASE` (the tool's version and the build number) and, for a new version,
the sources and their SHA-256; merge, run `gh workflow run tools.yml --ref main -f tool=<tool>`,
and then update the plugin's pins. The builds are reproducible bit for bit while the images'
packages stay the same.

Geekbench 7.1.0, for Geekbench 7 (`cpu.geekbench`), is the exception. It is closed source and
its license forbids redistribution, so the program downloads Primate Labs' own archive from
`cdn.geekbench.com`, pinned by SHA-256 and size, and unpacks it. The free license allows
personal, non-commercial use on one's own devices only, requires every result to be uploaded to
the Geekbench Browser, and forbids automated use; the owner accepted that risk.
