# Plugin System Design

- Status: **Accepted** 2026-11-07. The owner decided P-01..P-09, P-11 and P-12 on 2026-11-07, and P-10 on 2026-11-08 (§12).
- Scope: the plugin contract, the orchestrator and tool provisioning. Specific test items are out of scope; they are added one at a time on top of this design.
- Implementation:
  - **M0 is complete.** It covers:
    - `crates/bench-api` (contract);
    - in `crates/bench`: orchestrator, provisioning (core and HTTPS fetch), host probe, Chinese terminal renderer, CLI with root check and exit codes, and the registry with its contract test.
  - **M1** adds:
    - platform detection and per-platform plugins (D-16), with host facts in `crates/bench-host`;
    - the first test items, 系统基础信息 (`docs/design/system-info.md`).
  - **M2** adds pinned fio builds (P-10) and the 硬盘性能 test item (`docs/design/disk.md`).
  - **M3** adds pinned smartctl builds and the 硬盘健康 test item for dedicated servers (`docs/design/disk-health.md`). The dedicated additions to 系统基础信息 follow (DH-01).
  - **M4** adds one workflow for every pinned tool build, and the test items CPU 性能 (sysbench) and Geekbench 7 (`docs/design/cpu.md`).

## 1. Context

`lowendaff_bench` is a Rust program that runs a comprehensive test of a Linux server (VPS): system facts, CPU, memory, disk, network throughput, latency, route traces, media unlock, IP reputation, mail ports, and similar. Each test item is a **plugin**. An **orchestrator** owns the plugins: it selects and configures them, provisions the external tools they need, checks their preconditions, runs them with timeouts and cancellation, and prints the results.

Comparable tools and what they teach:

- **goecs (oneclickvirt/ecs, Go).** Each test item is a separate Go module (`basics`, `cputest`, `memorytest`, `disktest`, `UnlockTests`, `backtrace`, `nt3`, `speedtest`, `pingtest`, ...) compiled into one binary and imported by `goecs.go` (v0.1.112). Results are concatenated text.
- **vmbench (cloudapp3/vmbench, Go, MIT).** Studied with codebase-memory-mcp and ast-grep at commit `62811094` (2026-11-03):
  - Hardware benchmarks implement a small `Workload` interface (`bench/workload.go:22-28`).
  - Suite sections are hard-coded in `checkup.Run` (`checkup/run.go:13-112`).
  - Every section has its own typed result struct. The five section fields `Speed`, `Route`, `Media`, `Mail` and `IPQuality` are referenced **162 times across 17 non-test files**, so adding a test item touches most of them.
  - Timeouts are cooperative (`checkup/run.go:152-176`).
  - Tools are provisioned from pinned, SHA-256-checked static builds. Each pin records its license and source (`toolbin/toolbin.go:31-69`).
- **LowEndAFF benchmarks contract** (`viking://resources/projects/lowendaff-ui/stage-1-content-contracts.md`). It inspired the "missing value with a reason" vocabulary. Per P-06 the report is not published yet, so v1 neither follows that contract nor writes a report file.

## 2. Goals and non-goals

Goals:

1. **Locality.** Adding a test item means one new crate plus one registry line. No orchestrator or renderer change.
2. **Safe defaults.** Every plugin gets timeouts, interruption, panic containment, tool provisioning and work-directory cleanup without writing code for them.
3. **Honest results.** Partial results survive failures. Missing values carry a reason instead of a fake number. Every planned plugin appears in the output, so a partial run is visibly partial.
4. **Distribution.** The tool ships as one static binary that runs on a fresh VPS.
5. **No residue.** Downloaded tools and work files live in one run directory that is removed at exit.

Non-goals for v1:

- Specific test items, scoring or grades, upload and sharing, interactive menus, and any report file (P-06).
- Concurrent plugin execution and runtime-installable plugins (§13).
- Running as non-root (P-09), or running test items on non-Linux hosts. The API and the orchestrator stay portable so development works on macOS.

## 3. Constraints (decided by the owner, §12)

- **C1. Compile-time plugins only (P-01).** Plugins are crates in this workspace. No plugin is added at run time.
- **C2. One static musl binary for x86_64 and aarch64 (P-02).** This rules out `dlopen`. HTTPS uses rustls with exactly one crypto provider.
- **C3. Missing external tools are downloaded (P-03)** as pinned, verified builds (§9).
- **C4. Plugins run sequentially (P-04).**
- **C5. User-facing text is Chinese (P-05).** Ids, identifiers, comments and documents are English.
- **C6. No LowEndAFF contract and no report file in v1 (P-06).** Output goes to the terminal only.
- **C7. Names (P-07):** binary `lowendaff-bench` (package `bench`); crates `bench-api` and `bench`; plugin crates `plugin-<item>`.
- **C8. Exit codes (P-08):** `0`, `2` and `130` (§8).
- **C9. Root is required (P-09).** The program refuses to start otherwise.
- **C10. Plugins share only host facts** probed by the orchestrator, never each other's results.

## 4. Decision log

| ID | Decision | Alternatives considered | Why |
|----|----------|-------------------------|-----|
| D-01 | **Plugins are Rust crates compiled into the binary.** Each implements one trait. | **Dynamic libraries** (`libloading` 0.9.0, `abi_stable` 0.11.3, `stabby` 72.1.16): rejected. **WASM components** (`wasmtime` 50.0.1, `extism` 1.41.0): rejected for test items. **Embedded scripting** (`rhai` 1.27.0, `mlua` 0.12.2): not now. **Out-of-process executables**: deferred (§13). | **Dynamic libraries:** Rust has no stable ABI. `abi_stable` has had no release since 2023-10-12 and, per the Tremor write-up, has no `async` support. Loading is `unsafe`, and a crashing library takes the host down. Under C2 the question is moot: in a static musl binary `dlopen` is a stub that fails with "Dynamic loading not supported". **WASM:** WASI has no raw sockets (WASI issue #860, open), so ICMP traces, direct disk I/O and running `fio`/`iperf3` would all need custom host functions. A runtime between a CPU benchmark and the hardware also changes what is measured. |
| D-02 | **The registry is an explicit list** of `register::<P>()` entries in the binary crate. | `inventory` 0.3.25 or `linkme` 0.3.37 distributed registration. | The binary must list every plugin crate in `Cargo.toml` anyway. An explicit list is greppable and defines the default run order. |
| D-03 | **The plugin struct is its own configuration** (`Deserialize + Default`). The framework builds every selected plugin from its config table before anything runs. | A separate config type per plugin, or plugins reading untyped config inside `run`. | Config errors surface before a long run starts. The registry contract (§11) enforces `deny_unknown_fields` and that an empty table builds. The config file is TOML, with one `[plugins."<id>"]` table per plugin. |
| D-04 | **`run` is declared as `fn run(&self, ctx) -> impl Future + Send`**, so implementors write `async fn`. The framework boxes the future behind an object-safe `DynPlugin` trait. | The `async-trait` macro, or plugins writing `Pin<Box<dyn Future>>` themselves. | `async fn` in `dyn` traits is not stable (rust-lang/rust#133119 is an experimental nightly feature; the 2026 project goal plans only a nightly `dyn_box!` helper). Return-position `impl Trait` in traits is stable, so boxing happens once, at the seam. |
| D-05 | **Results are streamed** with `ctx.emit(record)`. | `run` returns all records at the end. | Emitted results survive a timeout, an interrupt, a failure or a panic (tests in §11). The terminal shows results while a long item is still running. |
| D-06 | **Records use generic primitives:** `fact`, `metric`, `table`. New kinds, such as a route `trace`, arrive with the first plugin that needs them. | One typed result struct per test item (the vmbench pattern). | The renderer stays plugin-agnostic (see the vmbench count in §1). A `Measurement` is a finite number with a unit, or a `Missing` reason shown as 繁忙, 超时 and so on, never a fake `0`. Values stay numeric, so a report file can be added later without touching plugins. |
| D-07 | **Preconditions are declared as data** (`tools`, `net`), and the orchestrator checks them before `run`. Conditions found only at run time use `PluginError::Skip`. | A `check()` method in every plugin. | The orchestrator knows the whole plan, including skips and their reasons, before running anything. Plugins write no checking code. |
| D-08 | **Sequential execution** (C4). | A scheduler with exclusive resource tags and bounded parallelism. | Overlapping benchmarks distort each other, and sequential output is deterministic. A plugin may still run concurrent work inside itself. |
| D-09 | **Stopping is cooperative first, then forced:** cancel token, grace period, abort, and finally detach. | Abort immediately; or wait indefinitely. | The grace period lets a plugin report how far it got. Abort drops the future, which releases child processes and sockets. In-process code that blocks a thread cannot be killed, so it is detached; a hard kill needs process isolation (§13). |
| D-10 | **Plugins share only `Env`** (C10). | Data dependencies between plugins, with a dependency graph. | Plugins stay independent, testable and reorderable. |
| D-11 | **Workspace layout:** `crates/bench-api` (the contract), `crates/bench` (orchestrator, provisioning, CLI, renderer; binary `lowendaff-bench`), `crates/bench-host` (host facts read from `/proc`, `/sys` and `/etc`, shared by the orchestrator and plugins), and `plugins/<item>` (package `plugin-<item>`). Plugins depend on `bench-api` and, for host facts, `bench-host`. A plugin crate may hold several variants of one test item (D-16). | A single crate; or a separate orchestrator crate. | The contract is the real seam, with N plugin adapters behind it. |
| D-12 | **Root is checked once, at startup** (C9). When the effective UID is not 0, the program exits with code 2 and a Chinese message. There is no per-plugin root requirement. | A per-plugin `root` requirement with skips. | This was the owner's decision. Plugins may assume root, which removes a whole class of skip paths. |
| D-13 | **Tools are provisioned from pinned builds before the first test** (§9). | Plugins install their own tools; use the host's package manager; use whatever is on `PATH`. | One verified mechanism instead of N ad-hoc ones. Pins give identical tool versions everywhere, so numbers stay comparable and parsers stay valid. Downloading before the first test keeps downloads out of measurements. |
| D-14 | **One run directory under the current working directory** holds `tools/` and the per-plugin work directories, and is removed at exit. `--work-dir` overrides the location. | The system temp directory. | Debian 13 mounts `/tmp` as a RAM-backed tmpfs by default, so a disk test there would measure memory. CIS benchmarks recommend mounting `/tmp` `noexec`, which would stop downloaded tools from running. |
| D-15 | **All user-facing text is Chinese** (C5): titles, labels, skip and failure reasons, summaries. Developer-facing registry-check messages stay English. The renderer aligns columns by display width (CJK takes two columns). | Plain character counts; bilingual output. | Owner's choice. Character counts misalign mixed CJK/ASCII tables. |
| D-16 | **The orchestrator detects the platform once** and stores it in `Env`: a dedicated server, a virtual machine and its hypervisor, or a container and its kind. Detection follows systemd-detect-virt's order without running it (`crates/bench-host/src/platform.rs`). **Plugins declare the platform kinds they apply to** in `Requirements::platforms`. On other platforms they do not run and get no tools. Variants of one test item share its title and must not overlap in platforms (a registry check). The header prints 运行环境 and one 本机不适用 line, which lists only titles that run here in no variant. | One environment-aware plugin per item; plugins detecting the platform themselves. | The owner chose one plugin per environment, with shared code extracted (P-12). Detecting once gives every plugin the same answer, and the plan is known before anything runs (as in D-07). |

## 5. Architecture

```text
               bench (package; binary lowendaff-bench)            plugins/<item> (N crates)
┌──────────────────────────────────────────────────────────┐     ┌─────────────────────────────┐
│ CLI + TOML config + root check                           │     │ struct Fio { config fields } │
│ registry: [register::<Fio>(), ...]  ─────────────────────┼───► │ impl Plugin for Fio         │
│ orchestrator: plan → probe → provision → run → summary   │     │ const FIO_TOOL: ToolSpec    │
│ renderer: terminal, live, Chinese                        │     └──────────────┬──────────────┘
└─────────────────────────────┬────────────────────────────┘                    │ depends on
                              │ depends on                                      ▼
                              └────────────────────────────────► bench-api: Plugin, Context,
                                                                 Descriptor, Record, ToolSpec
```

Run flow:

1. **Check root**, then parse the CLI and the config file.
2. **Plan.** Resolve the selection (`--only`/`--skip` over registry order) and build every selected plugin. All config errors are reported together, and nothing runs if there are any.
3. **Probe `Env`**: architecture, platform (D-16), IPv4 and IPv6 reachability. Print 运行环境 and the 本机不适用 line, and keep only the plugins that apply to the platform.
4. **Provision** every tool the remaining plan needs into the run directory, one after another, and print each tool's result.
5. **Run** each plugin in order:
   - Check preflight.
   - Create its work directory and spawn `run`.
   - Forward records live to the renderer.
   - Apply the timeout and interrupt rules (§8), then remove the work directory.
6. **Summarize** with a count per outcome. Remove the run directory and exit with the code from §8.

## 6. Plugin contract (`bench-api`)

These signatures are taken from `crates/bench-api/src/lib.rs`:

```rust
pub trait Plugin: DeserializeOwned + Default + Send + Sync + 'static {
    const DESCRIPTOR: Descriptor;
    /// Must not block the async executor: CPU-bound work goes to `spawn_blocking` and
    /// polls `ctx.cancel_token()`; child processes use `kill_on_drop(true)`.
    fn run(&self, ctx: &Context) -> impl Future<Output = Result<(), PluginError>> + Send;
}

pub struct Descriptor {
    pub id: &'static str,          // stable, lowercase dotted: "disk.fio"
    pub title: &'static str,       // Chinese section title
    pub version: &'static str,     // bump when the measurement method changes
    pub requires: Requirements,    // checked before `run`
    pub timeout: Duration,         // hard limit for one run
}

pub struct Requirements {
    pub tools: &'static [ToolSpec],
    pub net: Net,
    pub platforms: &'static [PlatformKind],   // empty: every platform (D-16)
}
pub enum Net { None, Any, Ipv4, Ipv6 }
pub enum PlatformKind { Dedicated, Vm, Container }
pub enum Platform { Dedicated, Vm(Hypervisor), Container(ContainerKind) }   // Display: 未检测到虚拟化, KVM 虚拟机, LXC 容器, …
pub struct ToolSpec { pub name: &'static str, pub version: &'static str, pub assets: &'static [ToolAsset] }
pub struct ToolAsset { pub arch: &'static str, pub url: &'static str, pub sha256: &'static str, pub size: u64 }

impl Context {
    pub fn env(&self) -> &Env;                                  // arch, platform, ipv4, ipv6, verified tools
    pub fn emit(&self, record: Record);                         // stream one result
    pub fn tool(&self, name: &str) -> Result<&Path, PluginError>; // path of a declared, verified tool
    pub fn cancel_token(&self) -> CancellationToken;            // fires on timeout and on interrupt
    pub fn deadline(&self) -> Instant;
    pub fn work_dir(&self) -> &Path;                            // removed after the run
}

pub enum PluginError { Skip(String), Fail(String) }   // `?` works on any std error; the cause chain is kept
pub const fn register<P: Plugin>() -> Registration;    // one line per plugin in the registry
pub fn validate_registry(registry: &[Registration]) -> Vec<String>;
```

A complete plugin, from `crates/bench/tests/provision.rs`:

```rust
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunsFio {}

impl Plugin for RunsFio {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "disk.fio",
        title: "磁盘（fio）",
        version: "0.1.0",
        requires: Requirements { tools: &[FIO], net: Net::None, platforms: &[] },
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        let output = tokio::process::Command::new(ctx.tool("fio")?).kill_on_drop(true).output().await?;
        ctx.emit(Record::fact("version", "版本", String::from_utf8_lossy(&output.stdout).trim()));
        Ok(())
    }
}
```

Rules that are part of the interface:

- Tools are called through `ctx.tool(name)`, never through `PATH`.
- Record ids are stable within a plugin's major version.
- Release profiles keep `panic = "unwind"`; the workspace sets it explicitly, because a panic is contained only when it unwinds.
- The orchestrator runs on the main thread over a multi-thread runtime with at least two workers.
- `unsafe_code` is forbidden workspace-wide.

## 7. Records and terminal output

| Kind | Fields | Invariant (enforced by constructor) |
|------|--------|-------------------------------------|
| `fact` | `id`, `label`, `value` | text, e.g. CPU model |
| `metric` | `id`, `label`, `result: Measurement` | finite value with a non-empty unit, or a `Missing` reason |
| `table` | `id`, `title`, `headers`, `rows` | every row has `headers.len()` cells |

`Missing` and `Measurement` implement `Display`. The `Missing` reasons show as: `Unavailable` 不可用, `Busy` 繁忙, `Timeout` 超时, `Unsupported` 不支持, `Error` 出错, `Unknown` 未知.

`Record` is not `#[non_exhaustive]`: every consumer lives in this workspace, so a new kind is a compile error in each renderer instead of a silently dropped record.

The renderer (`crates/bench/src/terminal.rs`) works as follows:

- Below the version line it prints 运行环境：<platform>. When a selected test item applies to this platform in no variant, it adds 本机不适用：<titles> (D-16).
- When a plugin starts, it prints the plugin's `title` as a section header.
- Each record prints as it arrives: `label：value` for facts and metrics, aligned tables for `table`.
- After the plugin, it prints the outcome line with a Chinese reason.
- Columns are aligned by display width, not by character count.

## 8. Orchestrator semantics

| Situation | Outcome | Records kept |
|-----------|---------|--------------|
| A declared tool is not verified | `skipped`: 工具 fio 不可用 | none |
| Required network is unreachable | `skipped`: 需要 IPv6 网络连接 | none |
| `run` returns `Ok` | `completed` | all |
| `run` returns `PluginError::Skip` | `skipped { reason }` | all emitted |
| `run` returns `PluginError::Fail`, or `?` on an error | `failed { error }` (cause chain kept) | all emitted |
| `run` panics | `failed`: 插件崩溃：<message>; the next plugin still runs | all emitted |
| The deadline passes | `timed_out` | all emitted, including during the grace period |
| Interrupt (SIGINT) while running | `cancelled` | all emitted |
| Not started because of an interrupt | `cancelled`, no elapsed time | none |
| Does not apply to the platform (D-16) | not run and not counted; its title is listed on the 本机不适用 line unless a variant runs | none |

Stopping sequence, used for both timeout and interrupt:

1. Cancel the plugin's token.
2. Wait up to `grace` (default 5 s) for `run` to return.
3. Abort the task, which drops the future and releases its resources.
4. Wait up to `grace` again. If the task is still running, detach it: it is blocking a thread, which breaks the contract.

After the task ends, the record channel is always drained. This is needed even on normal completion, because `tokio::select!` polls its branches in random order and can see the task finish before its last records arrive.

Exit codes:

- `0`: the run finished and the results were printed. Plugin failures are results, not process errors.
- `2`: usage or config error, or not root; nothing ran.
- `130`: interrupted; the results so far were printed.

## 9. Tool provisioning (D-13)

- **Pins live in the plugin crate** as a `ToolSpec` const: name, version, and one asset per architecture with an HTTPS URL, SHA-256 and exact size. Plugins that share a tool declare identical specs. The registry contract enforces:
  - both architectures present;
  - `https://` URLs;
  - 64 lowercase hex digits for the SHA-256;
  - a positive size;
  - identical declarations of the same tool across plugins.
- **When:** after planning and before the first test. Downloads run one after another into `<run dir>/tools/`, so no download overlaps a measurement.
- **Verification fails closed.** Each download streams into `<name>.part`. Reading stops as soon as the size exceeds the pin. Size and SHA-256 must then match exactly; only after that is the file made executable and renamed. Any failure deletes the partial file. Integrity comes from the pin, independent of TLS.
- **Failure handling.** A tool that cannot be provisioned skips the plugins that need it (工具 fio 不可用). The 【准备工具】 section prints each tool's result, for example `fio 3.39  失败：SHA-256 不符…` or `iperf3 3.19  失败：没有适用于 aarch64 的版本`.
- **Transport** (`crates/bench/src/fetcher.rs`).
  - reqwest 0.13 with `rustls-no-provider`, given a preconfigured rustls 0.23 config that uses the ring provider only (aws-lc-rs is not in the dependency tree) and the bundled `webpki-roots`.
  - Downloads work on images without `ca-certificates`; verified on Debian 13 slim (§11).
  - The core (`crates/bench/src/provision.rs`) takes the fetcher as a parameter, so tests run without a network.
- **Pinned builds only (P-11).** Tools already installed on the host are never used.
- **Source of the builds (P-10).** This repository builds the tools and publishes them. A manual workflow builds static binaries from checksum-pinned sources; since M4 one workflow builds every tool (`docs/design/cpu.md` §4). It publishes them, together with those sources, the build script and the license texts, as an immutable GitHub release whose tag carries a build number, such as `fio-3.43-1`. For fio, see `docs/design/disk.md` §4; for smartctl, `docs/design/disk-health.md` §4; for sysbench, `docs/design/cpu.md` §4. Geekbench is the one exception: Primate Labs' own builds, pinned and downloaded at run time (`docs/design/cpu.md`, CP-02).

## 10. Adding a test item (the step-by-step integration path)

1. Create `plugins/<item>` (package `plugin-<item>`) depending on `bench-api`, and on `bench-host` for host facts (plus the item's own crates).
2. Define the config struct with `#[serde(default, deny_unknown_fields)]` and a meaningful `Default`. Give it a `DESCRIPTOR` with id, Chinese title, version, requirements and timeout. An item that differs by platform gets one variant per platform kind in the same crate: same title, ids such as `system.vm`, and disjoint `platforms` (D-16).
3. If the item needs an external tool, declare its `ToolSpec` (§9) and call it through `ctx.tool(name)`.
4. Implement `run`:
   - Emit records with Chinese labels as results become available.
   - Return `PluginError::Skip` for conditions that can only be detected at run time.
   - Run CPU-bound loops in `spawn_blocking` and check the cancel token.
   - Spawn child processes with `kill_on_drop(true)`.
5. Keep parsing pure, as `fn parse(raw: &str) -> Result<Vec<Record>, _>`, and test it with captured tool output under `plugins/<item>/tests/fixtures/`.
6. Add the crate to `crates/bench/Cargo.toml` and add one `register::<Item>()` line to the registry.
7. Run the registry contract test, then run the item on a real Linux VM as root.

## 11. Testing strategy

Current state:

- macOS: `cargo nextest run --workspace` passes **90 tests**; `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- Linux: CI runs the same tests on x86_64 and aarch64 runners, plus one that needs Linux's `/dev/shm` (91).

| Suite | Tests | Covers |
|-------|-------|--------|
| `bench-api/tests/api.rs` | 10 | `Measurement` display and invariants, `Table` invariants, `?` cause chain, `ctx.tool`, registry contract (ids, duplicates, config strictness, tool pins, variants sharing a title), platform kinds and their Chinese names |
| `bench/tests/orchestrator.rs` | 11 | every row of §8, work-directory placement and removal, config errors before running |
| `bench/tests/provision.rs` | 7 | verified install, hash mismatch, size cap, truncation (nothing left behind in all three), per-tool failure reasons, de-duplication, a plugin running its provisioned tool |
| `bench/tests/app.rs` | 7 | argument parsing, `--only`/`--skip` in registry order with unknown-id errors, config file rules, a full run (sections, summary, run directory removed), an interrupted run (exit 130, every plugin listed), per-platform plugins (only applicable ones run and get tools; the 本机不适用 line), `--list` with platforms |
| `bench/tests/terminal.rs` | 4 | display-width alignment of mixed CJK/ASCII labels and tables, a section for plugins that never started, the tool section, the summary counts |
| `bench/tests/fetcher.rs` | 2 | streaming download, HTTP and connection errors as reasons, provisioning through the real client (local server) |
| `bench/tests/probe.rs` | 1 | reachability needs a completed handshake with any target |
| `bench/src/registry.rs` | 1 | the real registry passes `validate_registry` |
| `bench-host/tests/platform.rs` | 10 | platform detection on four captured hosts and on synthetic ones: containers (OpenVZ, WSL, container managers, PID 1, Podman and Docker files, PID namespaces), virtual machines (CPUID, DMI, Xen guests and dom0, Hyper-V enlightenments, EC2 metal, Google) and ARM device trees |
| `bench-host/tests/facts.rs` | 8 | CPU, memory, distribution and system facts against the hosts' own `lscpu`, `free` and `uname`; ARM part names; the `MemAvailable` fallback; cgroup v1 and v2 limits; os-release quoting and the older release files |
| `bench-host/tests/storage.rs` | 6 (7 on Linux) | disks and filesystems against `lsblk` and `df`; partitions, RAID, LVM, btrfs subvolumes, bind mounts, escaped mount points, swap and ZFS pools; a container's root; statvfs; RAM disks and ZFS from `statfs` magic numbers, and `/dev/shm` as a RAM disk on Linux |
| `plugins/system/tests/report.rs` | 4 | the exact 系统基础信息 lines for a KVM guest, a dedicated server and a container with cgroup limits; the three variants' descriptors |
| `plugins/disk/tests/fio.rs` | 9 | YABS's setup and test commands, escaped colons, terse fields 7, 8, 48 and 49 in real fio 3.43 output from both architectures, `note:` lines, ENOSPC and ramfs failures, the error line |
| `plugins/disk/tests/report.rs` | 10 | 655 speed and IOPS cases printed by YABS's own functions, impossible rates, the file size per architecture, RAM disks, the free-space threshold and its text, the ZFS note, the method line, both tables and their 说明 lines |

Mutation check on the real code: each mutation is applied to a copy, and `cargo test --workspace` must fail.

| Mutation | Killed by |
|----------|-----------|
| no size cap while reading | `install_stops_reading_past_the_declared_size` |
| no SHA-256 check | `install_rejects_a_hash_mismatch_and_leaves_nothing` |
| keep the partial file on error | the three `install_*` failure tests |
| no `chmod` | `install_makes_only_a_verified_file_executable`, `a_plugin_runs_the_tool_it_was_provisioned_with` |
| ignore tool preflight | `unmet_requirements_skip_without_running` |
| work directory in system temp | `work_dir_lives_under_the_run_root_and_is_removed` |
| drop the final drain | `completed_run…`, `cooperative_plugin…`, `cpu_bound_plugin…` |
| no abort after grace | `timeout_keeps_partial_records_and_releases_resources` |
| accept `http://` tool URLs | `registry_checks_catch_bad_tool_pins` |
| skip the tool-conflict check | `registry_checks_catch_bad_tool_pins` |
| pad by character count instead of display width | `records_line_up_by_display_width`, `a_run_prints_sections_and_summary…` |
| drop the section header for plugins that never started | `a_plugin_that_never_started…`, `an_interrupted_run_exits_130…` |
| ignore `--skip` | `selection_keeps_registry_order…`, `a_run_prints_sections_and_summary…` |
| accept unknown top-level config keys | `config_file_holds_plugin_tables_only` |
| keep the run directory | `a_run_prints_sections_and_summary…` |
| always exit 0 | `an_interrupted_run_exits_130…` |
| accept non-2xx downloads | `fetcher_streams_bodies_and_turns_failures_into_reasons` |
| count a timed-out connect as reachable | `reachable_needs_a_completed_handshake_with_any_target` |

All 18 mutations were killed (10 in the first round, 8 in the second). M1 added 50 more, all killed; they are listed by area in `docs/design/system-info.md` §7.

Linux smoke run of the static release binary on `debian:13-slim`, which has no CA store (`/etc/ssl/certs/ca-certificates.crt` absent):

- The binary is an aarch64 static executable, 6.1 MiB, and it starts on a glibc distribution.
- `--help` is Chinese, and `--list` prints 共 0 个测试项.
- As user `nobody`: 必须以 root 身份运行, exit 2.
- As root: header and 共 0 项 summary, exit 0, and no run directory left behind.
- `--only nope`: exit 2. A config file with a stray key: exit 2.
- `examples/fetch https://deb.debian.org/debian/README`: 1198 bytes, using the bundled roots.

CI runs the same smoke test on x86_64 and aarch64 (Ubuntu 24.04 runners, as a user with sudo), followed by `scripts/smoke-mutants.sh`. That script builds the release binary from a copy of the sources, unchanged and then with one defect at a time. smoke.sh must accept the unchanged binary and reject each mutant for the expected reason. Results in `rust:1.99.1-slim-trixie` (aarch64), in both of smoke.sh's modes:

| Mutant | As root | As a user with sudo |
|--------|---------|---------------------|
| root check removed (`if false`) | `non-root run exited with 2 but not at the root check: 无法在 . 创建运行目录：Permission denied (os error 13) …` | `non-root run exited with 0, expected 2` |
| run directory kept (`disable_cleanup(true)`) | `run directory left behind in …` | `run directory left behind in …` |

Before smoke.sh checked the refusal message, the root-check mutant passed as root. The non-root run uses `nobody`, which cannot write the current directory, so the mutant failed to create its run directory and exited with 2, the same code as the root refusal.

Not yet exercised: SIGINT against a real long-running plugin. The interrupt path itself is covered by the tests above.

Per plugin:

- Parser tests against fixtures run on any host.
- Tests that need the real tool run only on Linux CI, as root.
- The binary crate gets one test asserting that `validate_registry` on the real registry is empty.

## 12. Decisions and open questions

| ID | Question | Owner's decision |
|----|----------|------------------|
| P-01 | Must plugins be addable without rebuilding? | **No.** Compile-time plugins only. |
| P-02 | Distribution target? | **Static musl binary**, x86_64 and aarch64. |
| P-03 | Missing tools? | **Download them automatically** (§9). |
| P-04 | Sequential in v1? | **Yes.** |
| P-05 | Language of terminal text? | **Chinese.** |
| P-06 | Follow the LowEndAFF measurement contract? | **No.** The report is not shown for now, so there is no report file in v1 (confirmed). |
| P-07 | Names? | **As proposed** (C7). |
| P-08 | Exit codes? | **As proposed** (§8). |
| P-09 | Run as non-root? | **No; root is required** (D-12). |
| P-10 | Where do the pinned static builds come from, and where are they hosted? | **This repository builds and publishes them** (§9). A manual workflow per tool builds static binaries from checksum-pinned sources, in an image pinned by digest. It publishes them as an immutable release that also carries the sources, the build script and the license texts. Offering the source from the same place as the binary meets GPL-2.0 §3. The alternatives were a separate tools repository, and third-party builds such as YABS's. Decided 2026-11-08 with the disk test (`docs/design/disk.md`, DK-03). |
| P-11 | Use pinned builds only, and ignore tools already installed on the host? | **Yes**, pinned builds only. |
| P-12 | Environment-specific test items: one plugin that adapts, or one plugin per environment? | **One plugin per platform kind** (dedicated server, virtual machine, container), with the shared parts extracted as far as possible. Plugins for other platforms are summarized on one 本机不适用 line. The "environment" is the runtime platform, not the distribution. Decided 2026-11-07 (D-16). |

## 13. Deferred extensions (designed seams, not built)

1. **Out-of-process plugins.** Not needed while P-01 stands. Build only if a first-party item needs a hard kill.
   - An `ExternalPlugin` adapter implements `DynPlugin`.
   - It runs the plugin as a self-exec subcommand: `lowendaff-bench __plugin <id>`.
   - Each run gets its own process group.
   - Results come back as JSON Lines on stdout.
2. **Concurrency.** Exclusive resource tags in `Descriptor`, plus bounded parallelism for untagged probes.
3. **Report file.** A JSON report when a consumer exists (P-06). Records already hold numeric values, so plugins do not change.
4. **`trace` record**, with hops `{ ttl, address, asn, location, operator, latency: Measurement }`. Added with the first route-trace plugin.
5. **Profiles, per-run timeout overrides, English labels.**
6. **Scripting or WASM.** Only as a single plugin that interprets data-driven rules (for example media-unlock definitions), never as the plugin mechanism.

## 14. Evidence and references

Crate versions and release dates were taken from the crates.io API on 2026-11-07:

- `abi_stable` 0.11.3 (2023-10-12)
- `stabby` 72.1.16 (2026-07-20)
- `libloading` 0.9.0 (2025-11-05)
- `wasmtime` 50.0.1 (2026-10-29)
- `extism` 1.41.0 (2026-10-09)
- `inventory` 0.3.25 (2026-10-07)
- `linkme` 0.3.37 (2026-07-18)
- `rhai` 1.27.0 (2026-10-21)
- `mlua` 0.12.2 (2026-10-03)
- `reqwest` 0.13.5 (2026-09-08; its default TLS is rustls with aws-lc-rs)
- `sha2` 0.11.1 (2026-10-12)

Sources:

- Rust plugin trade-offs (Tremor):
  - https://nullderef.com/blog/plugin-tech/
  - https://nullderef.com/blog/plugin-dynload/
  - https://nullderef.com/blog/plugin-abi-stable/
- musl static `dlopen` stub: https://git.musl-libc.org/cgit/musl/tree/src/ldso/dlopen.c
- WASI sockets scope: https://github.com/WebAssembly/wasi-sockets
- Raw sockets request: https://github.com/WebAssembly/WASI/issues/860
- `async fn` in `dyn` trait:
  - https://github.com/rust-lang/rust/issues/133119
  - https://goals.rust-lang.org/2026/afidt-box.html
- Debian 13 `/tmp` on tmpfs (release notes §5.1.6): https://www.debian.org/releases/trixie/release-notes/issues
- CIS `noexec` on `/tmp`: https://www.tenable.com/audits/items/CIS_Red_Hat_EL7_v3.0.1_Server_L1.audit:042e08cd4c9027e00d3b361f83f79e27
- Nushell plugin protocol: https://www.nushell.sh/contributor-book/plugin_protocol_reference.html
- Telegraf `execd`: https://docs.influxdata.com/telegraf/v1/input-plugins/execd/
- Monitoring Plugins output: https://www.monitoring-plugins.org/doc/guidelines.html
- goecs: https://github.com/oneclickvirt/ecs
- vmbench: https://github.com/cloudapp3/vmbench (commit `62811094`)
- YABS: https://github.com/masonr/yet-another-bench-script

## Appendix A. Spike

The interface was first validated in a throwaway crate at `/tmp/plugin-spike`: 12 tests, and 8 of 8 mutations killed. It was then ported into `crates/bench-api` and `crates/bench`, with the owner's decisions applied:

- no per-plugin root requirement;
- pinned tools with `ctx.tool`;
- no `artifact` record and no raw text in `Measurement`, since there is no report file;
- work directories under the run root;
- Chinese messages.

The spike is superseded and can be deleted.
