# Plugin System Design

- Status:
  - **M0: Accepted** 2026-11-07. The owner decided P-01..P-09, P-11 and P-12 on 2026-11-07,
    P-10 on 2026-11-08 (§12), and P-13..P-16, on proxies, on 2026-11-10 (§9.1).
  - **Overhaul: Accepted** 2026-11-13. On 2026-11-11 the owner asked for the plugin model to be
    reworked (§1.1). The owner decided R-01..R-19 (§4.2) from 2026-11-11 to 2026-11-13, confirmed
    the design section by section and accepted the risks (§16) on 2026-11-13; §4.3 lists the
    earlier decisions it replaces. The migration runs in eight pull requests (§17). Until the last
    one, the code keeps the M0 model wherever §17 shows a step as pending.
- Scope: the plugin contract, the orchestrator, the host profile, the report and tool
  provisioning. Each test item has a document of its own.
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
  - **M5** adds pinned STREAM and multichase builds and the test item 内存性能 (`docs/design/memory.md`).
  - **The overhaul**, designed from 2026-11-11, reworks the plugin model in the steps of §17.

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

### 1.1 Why the overhaul

On 2026-11-11 the owner asked for the plugin model to be reworked:

- every function becomes a decoupled plugin, and the orchestrator composes them;
- plugins hand their results to the orchestrator instead of printing them; a log may show what
  is running;
- when all plugins are done, the orchestrator reports everything at once;
- the orchestrator detects the operating system, the virtualization and the command line, and
  decides which plugins run: an x86_64 KVM guest started without arguments runs every plugin that
  can run there.

M0 already kept plugins from printing, since they emitted records, and already chose plugins by
platform, glibc and network. Three things change: when the output appears (one report at the end,
R-01), how fine the plugins are (one per measurement process, R-02), and who formats the results
(the orchestrator, from typed results instead of text records, R-04 and R-06). In the end the
results are meant for lowendaff.com (R-05).

M0 turned down one typed result per test item (D-06), because in vmbench every new item touches
the code that prints (§1). The overhaul accepts that cost (R-07) and matches the results
exhaustively (R-08): each place that a new plugin must touch is a compile error until it is
written.

## 2. Goals and non-goals

Goals:

1. **Decoupled plugins.** One plugin per independent measurement process (R-02). Plugins never
   depend on one another, which the dependency policy enforces (§15).
2. **Typed results.** Each plugin returns a serializable result of its own type. Apart from
   progress text, plugins hold no user-facing text (R-06, R-07).
3. **One report.** Progress goes to stderr while the plugins run; the report goes to stdout once
   they are done (R-01, R-10).
4. **Honest results.** Partial results survive failures, timeouts and interrupts (R-11). A missing
   value carries its reason instead of a fake number.
5. **Safe defaults.** Every plugin gets timeouts, interruption, panic containment, tool
   provisioning and work-directory cleanup without writing code for them.
6. **Distribution.** The tool ships as one static binary that runs on a fresh server.
7. **No residue.** Downloaded tools and work files live in one run directory, removed at exit.

Non-goals:

- New test items while the migration runs (R-14).
- The lowendaff.com export; only its seam is built (R-05).
- A report file, or protection against a dropped session (R-01, P-06). Long runs belong in tmux
  or screen.
- Concurrent plugins (P-04), and plugins installed at run time (P-01).
- Running as non-root (P-09), or running test items on non-Linux hosts. The code stays portable,
  so that development works on macOS.
- M0's locality goal, "one new crate plus one registry line", is given up: a new plugin also adds
  a variant to the results and its formatting to the reporter (R-07, R-08).

## 3. Constraints (decided by the owner, §12)

- **C1. Compile-time plugins only (P-01).** Plugins are crates in this workspace. No plugin is added at run time.
- **C2. One static musl binary for x86_64 and aarch64 (P-02).** This rules out `dlopen`. HTTPS uses rustls with exactly one crypto provider.
- **C3. Missing external tools are downloaded (P-03)** as pinned, verified builds (§9).
- **C4. Plugins run sequentially (P-04).**
- **C5. User-facing text is Chinese (P-05).** Ids, identifiers, comments and documents are English.
- **C6. No report file (P-06).** The report goes to stdout, which can be redirected; progress
  goes to stderr (R-10).
- **C7. Names (P-07, R-13):** binary `lowendaff-bench` (package `bench`); core crates `bench-*`;
  one plugin crate per group, `plugin-<group>` (§15).
- **C8. Exit codes (P-08):** `0`, `2` and `130` (§8).
- **C9. Root is required (P-09).** The program refuses to start otherwise.
- **C10. Plugins share only the host profile** (§10) and their group's helpers, never each
  other's results.

## 4. Decision log

### 4.1 M0

The table keeps these decisions as they were made; §4.3 lists the ones that the overhaul replaces.

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

### 4.2 The overhaul

Decided by the owner from 2026-11-11 to 2026-11-13, one question at a time. Each entry names the
alternatives that were turned down.

- **R-01. One report, at the end.** Long runs belong in tmux; no result file guards against a
  dropped session. Turned down: printing results as the items finish, and a result file.
- **R-02. One plugin per independent measurement process;** host facts are a shared library, not
  plugins. Turned down: one plugin per data item.
- **R-03. The orchestrator fixes the run order.** Turned down: plugins declaring dependencies.
- **R-04. Change the output, the split and the data:** plugins hand over structured data, and the
  orchestrator formats it. Turned down: changing the output only; the output and the split only.
- **R-05. lowendaff.com is the final consumer.** This round does not design for its current
  templates; a second reporter will read the same data later.
- **R-06. Each plugin defines a typed, serializable result of its own,** and every result
  implements one trait. Turned down: one generic structure of values with units; generic values
  whose names live in the reporter.
- **R-07. The trait provides the common metadata only;** the reporter formats each result type,
  so every new plugin adds its formatting to the reporter. Turned down: a trait that also gives a
  generic view and the full data; results that format themselves.
- **R-08. An enum in the orchestrator holds the results,** one variant per plugin, and the
  reporter matches it exhaustively, so a missing format is a compile error. Turned down: trait
  objects and downcasting, where a missing format shows only at run time.
- **R-09. The orchestrator collects a host profile once, at the start,** to choose the plugins,
  to hand to them read-only and to report as 系统信息. Values that change during a run are read
  again by the plugins that need them. Turned down: system facts collected at the end, whose load
  would include the tests; every plugin reading its own facts.
- **R-10. Plugins report their steps through the context;** the orchestrator writes them, with a
  start and an end line per plugin, to stderr, and the report to stdout. Turned down: everything
  on stdout; start and end lines only.
- **R-11. Results may be partial.** A result type can say that one measurement did not happen
  and why. A plugin publishes its result after every measurement, and a plugin stopped at its
  deadline is reported with the last one. Turned down: all or nothing; partial results only when
  `run` returns.
- **R-12. Ids are `<group>.<name>`;** `--only` and `--skip` take ids or group names, and the
  report groups its sections. Turned down: full ids only; separate options for groups.
- **R-13. Code, commits, pull requests and the layout follow written, checked conventions**
  (§15). The owner's request.
- **R-14. The model evolves in place:** the contract, the orchestrator and the reporter come
  first, then the plugins move over one group at a time, with a temporary legacy variant in the
  results. No new test item until the migration ends. Turned down: one rewrite; a second program
  next to the first.
- **R-15. Mature libraries are welcome,** within the dependency policy (`deny.toml`) and the
  static musl build.
- **R-16. The public IPv4 and IPv6 addresses are host facts,** found through public services.
  Hostnames, MAC addresses, serials and UUIDs are still never read. This replaces the address
  part of S-02.
- **R-17. The address sources are an ordered list that can grow.** The first valid answer wins,
  later sources are not asked, and the report names the source.
- **R-18. The report masks the addresses by default:** the first two IPv4 octets and the first
  32 bits of IPv6 remain. `--show-ip` shows them in full. Inside the program they stay complete,
  for later test items. Turned down: always in full; in full unless `--mask-ip` is given.
- **R-19. The network module lives in `crates/bench` and uses reqwest directly,** with the
  downloader's TLS configuration and without its proxy. `bench-host` holds only the result type
  and has no network dependency. Turned down: the module in `bench-host`; a request function
  passed in by the orchestrator.

### 4.3 What the overhaul replaces

- **D-02** made the registry order the run order, and so the order on screen. The registry still
  sets the run order; the report orders its sections by group (R-03, §7).
- **CP-08, ME-09 and DH-08** chose the run order, which was also the order on screen. The run
  order stays; in the report, Geekbench 7 joins CPU 性能, and 硬盘健康 and 硬盘性能 share one
  section (§7).
- **D-05 and D-06** streamed text records (`fact`, `metric`, `table`) through `ctx.emit`, so that
  the renderer needed no change per item. Plugins now return typed results, publish partial ones
  while they run, and the reporter formats each type (R-06..R-08, R-11). D-04, a `run` that
  returns `impl Future` and is boxed once, stays.
- **D-10 and D-11:** plugins share the host profile instead of `Env` (R-09), and the layout gains
  `bench-tool` and one plugin crate per group (§5, §15).
- **D-16, P-12 and S-01** gave 系统基础信息 one plugin per platform kind. The system facts become
  the host profile, which is not a plugin (R-02, R-09). D-16's single detection and the plugins'
  platform requirements stay.
- **S-02** never read IP addresses. The public addresses are read now, and the report masks them
  by default (R-16..R-18).
- **ME-10** made 内存性能 one item of four chapters that needed all three tools and failed as a
  whole when one chapter failed. It becomes four plugins, still the four chapters of 内存性能 in
  the report: a missing tool skips only the plugins that use it, and each plugin completes or
  fails on its own (R-02, R-12).
- **The descriptor's `title` and `version`** go: the reporter names the plugins, and the program's
  version stands for the method's.

## 5. Architecture

The target; §17 tells which steps have landed.

```text
bench-host   host facts read from files; the platform and glibc types
   ↑
bench-api    the plugin contract: Plugin, Measurement, Context with the host profile
   ↑
bench-tool   the pinned tools, and the shared code that runs external tools
   ↑
plugins/*    one crate per group; plugins never depend on one another
   ↑
bench        orchestrator, reporter, network and downloads; the only binary
```

A crate depends only on the crates above it; `deny.toml` enforces the direction (§15).

```text
crates/
  bench-host/  host facts, and HostProfile (§10)
  bench-api/   Plugin, Measurement, Context, Descriptor, Requirements (§6)
  bench-tool/  a ToolSpec for sysbench, fio, smartctl, STREAM, multichase and Geekbench (§9);
               running a tool with a time limit, cancellation, retries and CPU placement
  bench/       command line, orchestrator, registry with the run order, the result enum,
               report/terminal, network, provisioning
  swap-file/   swapon and swapoff, the only unsafe code (CP-07)
plugins/
  cpu/         cpu.sysbench, cpu.geekbench
  memory/      memory.sysbench, memory.stream, memory.latency, memory.numa
  disk/        disk.health, disk.fio
```

Run flow:

1. **Check root**, then parse the command line (§14) and the config file. An unknown id or group,
   or an invalid config, exits with 2 before anything runs.
2. **Collect the host profile** (§10): the files through `bench-host`, then the network through
   the network module of `bench`. Fields that cannot be read stay empty, with the reason.
3. **Select** in the run order: apply `--only` and `--skip`, then each plugin's requirements
   (platform kinds, glibc, network). Plugins that do not apply, or whose requirements fail, do not
   run; the reasons go into the report.
4. **Provision** the tools of the plugins that will run (§9). A plugin whose tool fails is
   skipped, with the reason.
5. **Run** each plugin in order. Its start and end lines and its progress go to stderr. Each has
   a timeout; on a timeout or an interrupt the stopping sequence of §8 applies, and the plugin's
   last published result is kept.
6. **Report.** When every plugin is done, or the run is interrupted, the reporter writes the host
   profile and every plugin's record to stdout, ending with a summary (§7).
7. **Exit** with 0, 2 or 130 (§8), after removing the run directory.

For every plugin the orchestrator keeps
`Run { id, group, started, elapsed, status, result: Option<Measured> }`. The status is completed,
skipped, failed, timed out or cancelled, each with a typed reason.

The run order: 硬盘健康 (`disk.health`), CPU 性能 (`cpu.sysbench`), 内存性能 (`memory.sysbench`,
`memory.stream`, `memory.latency`, `memory.numa`), 硬盘性能 (`disk.fio`), Geekbench 7
(`cpu.geekbench`). SMART is read before fio writes, CPU and memory are measured before the disk,
and Geekbench runs last (CP-08, DH-08, ME-09).

## 6. Plugin contract (`bench-api`)

A sketch of the target contract; the names settle in step 3 of §17, which records any difference.

```rust
pub trait Plugin: DeserializeOwned + Default + Send + Sync + 'static {
    type Output: Measurement;        // the plugin's typed result
    type Failure: Serialize + Send;  // why it skipped or failed, typed too
    const DESCRIPTOR: Descriptor;
    fn run(&self, ctx: &Context<Self::Output>)
        -> impl Future<Output = Result<Self::Output, Stop<Self::Failure>>> + Send;
}

pub enum Stop<F> { Skipped(F), Failed(F) }

/// The common metadata of every result.
pub trait Measurement: Serialize + Clone + Send + 'static {
    /// The tools used, with the versions that the tools report.
    fn tools(&self) -> Vec<ToolVersion>;
}

pub struct Descriptor {
    pub id: &'static str,        // "<group>.<name>", such as "memory.stream"
    pub requires: Requirements,  // tools, network, platform kinds and glibc, as in M0
    pub timeout: Duration,       // hard limit for one run
}

impl<O: Measurement> Context<O> {
    pub fn host(&self) -> &HostProfile;               // collected once, read-only (§10)
    pub fn tool(&self, name: &str) -> Result<&Path, …>; // a declared, verified tool
    pub fn work_dir(&self) -> &Path;                  // removed after the run
    pub fn cancel_token(&self) -> CancellationToken;  // fires on timeout and on interrupt
    pub fn deadline(&self) -> Instant;
    pub fn progress(&self, text: impl Into<String>);  // one progress line on stderr
    pub fn publish(&self, output: &O);                // replaces the last partial result
}
```

Rules:

- **Results.** `run` returns the complete result, or a typed reason to skip or to fail. After a
  skip, a failure, a timeout or an interrupt, the report shows the last published result.
- **Metadata.** The orchestrator records what it knows: id, group, start, elapsed time, status,
  and its own reasons (not applicable, glibc, network, a tool that failed, timeout, interrupt).
  The result adds what only the plugin knows, such as the versions its tools report.
- **No text.** Results hold numbers, enums and kinds of reasons, such as an exit code and the
  last line of output; the reporter writes every Chinese word (§7). Progress text is the
  exception: it is a log, not part of the report, so plugins write it in Chinese.
- **Registration.** Each plugin is registered with its variant of the result enum, for example
  `entry::<memory::Stream>(Measured::MemoryStream)`.
- **Kept from M0:**
  - tools are called through `ctx.tool(name)`, never through `PATH`;
  - `run` must not block the executor: CPU-bound work goes to `spawn_blocking` and polls the
    cancel token, and child processes use `kill_on_drop(true)`;
  - `run` returns `impl Future` and is boxed once, behind an object-safe trait (D-04);
  - release profiles keep `panic = "unwind"`, because a panic is contained only when it unwinds;
  - the orchestrator runs on a multi-thread runtime with at least two workers;
  - `unsafe_code` is forbidden, except in `crates/swap-file` (`docs/design/cpu.md` §7.2, CP-07).

## 7. Results and the report

- **The result enum.** `Measured`, in `crates/bench`, has one variant per plugin. Each variant
  holds the plugin's `Output` type, which lives in the plugin's crate. Until step 8 of §17, a
  `Legacy` variant carries the records of the plugins that still use the M0 contract (R-14).
- **Reporters.** A reporter reads the host profile and every `Run`. The terminal reporter is the
  only one for now, in `crates/bench/src/report/terminal/`, with one file per group: `host.rs`,
  `cpu.rs`, `memory.rs`, `disk.rs` and `summary.rs`. Its entry point matches `Measured`
  exhaustively (R-08). A lowendaff.com reporter will read the same data; neither the plugins nor
  the orchestrator change for it (R-05).
- **Sections**, in an order of their own, apart from the run order (R-03, R-12):

  ```text
  【系统信息】  the host profile
  【CPU 性能】  sysbench, Geekbench 7
  【内存性能】  sysbench, STREAM, latency, NUMA
  【硬盘】      health (SMART), performance (fio)
  【汇总】      each plugin's status and time, the plugins that do not apply here, total time
  ```

- **A plugin's subsection:** a title line with its name, its tool and the tool's version, then
  the parameters, the tables and the notes. A measurement that did not happen shows its reason in
  the table instead of a blank. A plugin that skipped or failed shows its reason, and its partial
  result as far as it got.
- **Tables** come from comfy-table 8.0.1 (MIT) without its default `tty` feature, which leaves
  only `unicode-segmentation` and `unicode-width`. Columns line up by display width (D-15),
  numbers are right-aligned, and the borderless style stays. On a terminal the tables wrap to its
  width, which `rustix::termios::tcgetwinsize` reads; written to a file, they do not wrap.
- **Text.** Today's Chinese words stay as they are; they move from the plugins into the reporter.
- **Addresses** are masked when the reporter writes them (R-18, §14).

## 8. Orchestrator semantics

| Situation | Status | Result in the report |
|-----------|--------|----------------------|
| Does not apply to the platform | not run; named in the summary as not applicable | none |
| A glibc, network or tool requirement is not met | `skipped`, with the reason | none |
| `run` returns the output | `completed` | the output |
| `run` returns `Stop::Skipped` | `skipped`, with the plugin's reason | the last published |
| `run` returns `Stop::Failed` | `failed`, with the plugin's reason | the last published |
| `run` panics | `failed`: 插件崩溃：<message>; the next plugin still runs | the last published |
| The deadline passes | `timed_out` | the last published, also from the grace period |
| Interrupt (SIGINT) while running | `cancelled` | the last published |
| Not started because of an interrupt | `cancelled`, no elapsed time | none |

Stopping sequence, used for both timeout and interrupt:

1. Cancel the plugin's token.
2. Wait up to `grace` (default 5 s) for `run` to return.
3. Abort the task, which drops the future and releases its resources.
4. Wait up to `grace` again. If the task is still running, detach it: it is blocking a thread, which breaks the contract.

After the task ends, its progress and its published results are always drained. This is
needed even on normal completion, because `tokio::select!` polls its branches in random order
and can see the task finish before its last messages arrive.

Exit codes:

- `0`: the run finished and the results were printed. Plugin failures are results, not process errors.
- `2`: usage or config error, or not root; nothing ran.
- `130`: interrupted; the results so far were printed.

## 9. Tool provisioning (D-13)

- **Pins live in `crates/bench-tool`** (in the plugin crates until step 2 of §17), one
  `ToolSpec` const per tool: name, version, and one asset per architecture with an HTTPS URL,
  SHA-256 and exact size. The registry contract enforces:
  - both architectures present;
  - `https://` URLs;
  - 64 lowercase hex digits for the SHA-256;
  - a positive size;
  - identical declarations of the same tool across plugins.
- **When:** after planning and before the first test. Downloads run one after another into `<run dir>/tools/`, so no download overlaps a measurement.
- **Verification fails closed.** Each download streams into `<name>.part`. Reading stops as soon as the size exceeds the pin. Size and SHA-256 must then match exactly; only after that is the file made executable and renamed. Any failure deletes the partial file. Integrity comes from the pin, independent of TLS.
- **Failure handling.** A tool that cannot be provisioned skips the plugins that need it. Each
  tool's result goes to the progress log (§14), and a skipped plugin's reason in the report names
  the tool and why it failed, with the proxy hint of §9.1. Until step 3 of §17, the 【准备工具】
  section prints the tools' results instead, for example `fio 3.39  失败：SHA-256 不符…`.
- **Transport** (`crates/bench/src/fetcher.rs`).
  - reqwest 0.13 with `rustls-no-provider`, given a preconfigured rustls 0.23 config that uses the ring provider only (aws-lc-rs is not in the dependency tree) and the bundled `webpki-roots`.
  - Downloads work on images without `ca-certificates`; verified on Debian 13 slim (§11).
  - The proxy comes from the standard environment variables, SOCKS5 included (§9.1).
  - The core (`crates/bench/src/provision.rs`) takes the fetcher as a parameter, so tests run without a network.
- **Pinned builds only (P-11).** Tools already installed on the host are never used.
- **Source of the builds (P-10).** This repository builds the tools and publishes them. A manual workflow builds static binaries from checksum-pinned sources; since M4 one workflow builds every tool (`docs/design/cpu.md` §4). It publishes them, together with those sources, the build script and the license texts, as an immutable GitHub release whose tag carries a build number, such as `fio-3.43-1`. For fio, see `docs/design/disk.md` §4; for smartctl, `docs/design/disk-health.md` §4; for sysbench, `docs/design/cpu.md` §4; for STREAM and multichase, `docs/design/memory.md` §4. Geekbench is the one exception: Primate Labs' own builds, pinned and downloaded at run time (`docs/design/cpu.md`, CP-02).

### 9.1 Proxies (P-13..P-16)

Designed with the owner on 2026-11-10, as the third of the engineering follow-ups after M4.

**Understanding.** On servers that reach GitHub poorly, mostly in mainland China, the pinned tools often fail to download, and their items skip. Users who have a proxy should be able to use it; for users without one nothing changes. Integrity still comes from the pins alone. Out of scope: GitHub outages (P-14), the program's own distribution (decided at its first release), mirrors, and the reachability check at start, which still connects directly.

**Design.**
- reqwest takes the proxy from `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY` or `all_proxy` and honours `NO_PROXY`; that is its default. `HTTP_PROXY` applies to `http://` addresses only. Its `socks` feature, which adds no crate, makes `socks5://` and `socks5h://` work (P-15).
- A network error, when the request fails or the body breaks off, ends in a hint (P-16): `；本机设置了 <variable>，请确认这个代理可用` for the first of those four variables that is set and not empty, otherwise `；可设置环境变量 HTTPS_PROXY 或 ALL_PROXY 使用代理`. The hint names the variable, never its value, which may hold a password, and reqwest's errors do not show the proxy either. HTTP errors and mismatched sizes or SHA-256 get no hint, because the network worked. The hint follows each failed tool in 【准备工具】.
- Tests: the hint's choice of variable with a fake environment; against a local server, the hint after a refused connection and after a body that breaks off, and none after a 404; a download through a local SOCKS5 proxy, which fails without the `socks` feature. A mutation round killed all 10 mutants.

**Checked by hand** on 2026-11-10: a static aarch64 build ran `--only cpu.sysbench` in `debian:13-slim` under Docker Desktop.
- **`HTTPS_PROXY` (an HTTP proxy) and `ALL_PROXY=socks5h://…`:** each downloaded sysbench from GitHub and completed the item. The proxies, running on the host, saw `github.com:443` and then `release-assets.githubusercontent.com:443`, as names.
- **A dead proxy with a password in `HTTPS_PROXY`:** sysbench's line ended in `；本机设置了 HTTPS_PROXY，请确认这个代理可用`, and the password appeared nowhere in the output.
- **No network and no proxy:** the line ended in `；可设置环境变量 HTTPS_PROXY 或 ALL_PROXY 使用代理`.

**Decision log.**

| Question | Decision | Alternatives |
|----------|----------|--------------|
| What should a second download source solve? | Networks that reach GitHub poorly. GitHub outages were in at first, then left out (P-14) | Outages only; nothing |
| Scope | The tools only | The tools and the program |
| Where the downloads come from | First public GitHub acceleration proxies, then the same day the user's own proxy, which reqwest already reads (P-13) | A copy in Cloudflare R2, on the site's server, or on the owner's host in Hong Kong |
| GitHub outages | Not covered (P-14) | A second source outside GitHub |
| SOCKS5 | Supported (P-15) | HTTP proxies only |
| How users learn about proxies | The README, and a hint after each failed download (P-16) | The README only; one hint line under 【准备工具】 |

The dropped direction tried each GitHub address directly, then through a built-in list of public acceleration proxies and a `--github-proxy` option. Of four such proxies checked from Singapore on 2026-11-10, two failed and the other two differed 17 times in speed, though both served the exact file. They relay without caching, so they fail along with GitHub.

**Risks**, accepted on 2026-11-10:
1. Users without a proxy still cannot download from such networks.
2. GitHub outages are not covered.
3. A proxy sees the server's address and which files it downloads. It cannot change a file unnoticed.
4. SOCKS5 with a password, `socks4` and `socks4a` are untested; reqwest implements them.
5. The hint says a variable is set even when `NO_PROXY` exempts the host or the value is wrong.
6. Reading the variables is reqwest's default and is not tested here. Recheck it by hand when reqwest's major version changes.
7. The variables also apply to Geekbench's download. Whether Geekbench's own upload uses them is up to Geekbench.
8. Failure lines grow longer and wrap on narrow terminals.
9. A server that is online only through a proxy shows IPv4 and IPv6 as 不可用, and Geekbench 7 skips for want of IPv4.

## 10. Host profile and network

`HostProfile`, in `bench-host`, is a typed structure that the orchestrator builds before any
plugin runs, in two steps:

1. `HostProfile::collect(root, work_dir)` reads the files. Its sections are read on threads of
   their own, and the orchestrator waits for them at most 30 s, as long as the 系统基础信息 item
   could run. A section that has not finished by then shows 不可用（超时）, the plugins run
   anyway, and the stuck thread is left behind, as in D-09.
2. The network module of `bench` probes reachability and asks for the public addresses (R-19),
   and fills in the profile's network part.

Contents:

- **Platform:** a dedicated server, a virtual machine and its hypervisor, or a container and its
  kind; the architecture; glibc.
- **CPU:** model, logical CPUs, sockets and cores, frequency, AES, virtualization support; the
  caches and the CPUs that share each; the usable threads; the CPUs this process may use.
- **Memory:** total, available and swap, and a container's cgroup limits; the NUMA nodes, and the
  nodes this process may allocate on.
- **Storage:** the disks, the filesystems, and the device that holds the work directory.
- **System:** distribution, kernel, uptime and load (in a container, whether they are the host's,
  S-22), TCP congestion control and queueing discipline.
- **Network:** IPv4 and IPv6 reachability, with each target's result; the public addresses and
  their sources.
- **Dedicated hardware,** on dedicated servers only: machine, board, firmware, BMC, ECC, memory
  modules, RAID cards.

Rules:

- **Missing is not fatal.** A field that cannot be read stays empty, with the reason; the report
  shows 不可用 and the reason, and the run goes on.
- **Read only.** The profile runs no program and writes no file. Only the network module connects
  anywhere, and it lives in `bench`.
- **No identifiers.** Hostnames, MAC addresses, serials and UUIDs are never read (S-02); the
  public addresses are the one exception (R-16).
- **Changing values.** Available memory, free space and load are the values at the start. A
  plugin that needs current values reads them again through `bench-host`.
- **Modules.** The existing modules of `bench-host` stay, and `HostProfile` combines them. The
  combining that `plugins/system` does today, such as taking the smaller of a cgroup limit and
  `/proc`, or telling whether uptime and load are the host's, moves into `bench-host`; its text
  moves into the reporter.

### 10.1 Reachability

Unchanged: a TCP handshake with port 443 of any target counts, 3 s per target, the targets of a
family one after another, the two families at once.

| Family | Targets |
|--------|---------|
| IPv4 | `1.1.1.1`, `223.5.5.5` |
| IPv6 | `2606:4700:4700::1111`, `2400:3200::1` |

Each target's result is kept: connected, timed out, refused or no route.

### 10.2 Public addresses (R-16, R-17)

For each reachable family the module asks the sources in order, until one answers with a valid
public address of that family:

1. **Cloudflare:** `https://1.1.1.1/cdn-cgi/trace` and
   `https://[2606:4700:4700::1111]/cdn-cgi/trace`. The address is the `ip=` line; connecting to
   an address needs no DNS.
2. **ipify:** `https://api.ipify.org` and `https://api6.ipify.org`. The whole body is the
   address.
3. **icanhazip:** `https://ipv4.icanhazip.com` and `https://ipv6.icanhazip.com`. The whole body
   is the address.

- **The list grows by one entry per source:** `Source { name, ipv4, ipv6, read }`, where a source
  may serve one family only and `read` parses its answer. The two formats above have readers.
- **One client per family:** the downloader's TLS configuration (§9), no proxy, since a proxy
  would report its own address, and `local_address(0.0.0.0)` or `local_address(::)`. With a local
  address of one family only, hyper-util connects to addresses of that family only, also to an
  address written in the URL (Appendix B).
- **Limits:** 5 s per source, and at most 4 KiB of the body.
- **The first valid answer wins;** later sources are not asked. The profile records the source's
  name. When every source fails, the address is 不可用, with each source's reason: a timeout, an
  HTTP status, an answer that is not an address of this family, or no address of this family for
  the source's name. A reason never quotes the body, which may hold the address.
- **Full addresses inside.** The profile keeps the addresses complete, and plugins see them
  through `ctx.host()`; only the output masks them (R-18, §14).
- **Meaning.** The services see the default route's address: of several public addresses only
  one shows, and behind NAT the shared address shows. A host that reaches the Internet only
  through a proxy shows the addresses as 不可用.

## 11. Testing strategy

### 11.1 The overhaul

The tests must show three things: the measurements did not change, the new model works as
designed, and the report is right without leaking addresses.

- **Plugins.** An injected runner replays recorded tool output; every existing fixture stays.
  Tests assert typed results instead of text, that a result is published after each
  measurement, and that a failure, a cancellation or a timeout leaves the last one.
- **Host profile.** A profile from each of the six captured hosts (KVM, dedicated, ARM board,
  Docker, LXC with limits, Proxmox VE LXC), compared with a snapshot.
- **Network.** A local HTTP server on 127.0.0.1 stands in for the sources, reached through the
  real reqwest client with the source list pointed at it. Cases: a timeout, HTTP 503, an answer
  that is not an address, a private address, a name without an address of the family; the first
  valid answer stops the search and names its source; when all fail, each reason is listed; a
  body over 4 KiB is cut. A forced IPv4 client must reach the server, a forced IPv6 client must
  not. IPv6 parsing and checks are pure functions with tests of their own.
- **Reporter.** Reports built from constructed results, compared with snapshots: completed,
  partial, skipped, failed, timed-out and interrupted plugins, masked addresses and `--show-ip`.
- **Orchestrator.** Test plugins cover ids and group names in `--only` and `--skip`, unknown ids
  (exit 2), the run order and the report order, the last result after a timeout, an interrupt,
  and that stdout holds only the report and stderr only progress.
- **No leaked address.** With `-v`, the network module logs only masked addresses. A full run
  over a profile with a known address shows it neither on stdout nor on stderr, unless
  `--show-ip` is given.
- **End to end.** CI's smoke run on x86_64 and aarch64, and the distribution matrix (README
  "Gates").

A forgotten format is a compile error (R-08) and needs no test.

**The measurements do not change.** Migrated plugins keep their recorded fixtures and their
expected numbers, such as 42552.53 MiB/s for a sysbench memory read; only the assertions move from
text to typed results. Tool arguments stay compared verbatim: those of sysbench, multichase,
multiload, fio and smartctl, and STREAM's program and `OMP_*` variables. Geekbench's arguments
(`--cpu --upload`) have no such test yet; step 5 of §17 adds one.

**Snapshots** come from insta 1.49.0 (Apache-2.0), a dev-dependency without its default `colors`
feature, which leaves `once_cell`, `similar` and `tempfile`. Snapshot files live in each crate's
`tests/snapshots/` and are reviewed in the pull request. On CI insta writes no snapshot, and a
difference fails.

**CI scripts:**

- `smoke.sh` still finds the summary on stdout. It also checks that stderr holds progress lines,
  and that the IPv4 line shows a masked public address and its source; the runners have no IPv6.
- `distros.sh` runs `--skip cpu,memory,disk`, since the system facts are no longer a plugin. A
  run whose plugins are all skipped reports the system facts and the summary, which is not an
  error. It checks what it checks today: the platform, the 系统 line, the host's marks, and
  Geekbench 7's skip for glibc.
- `smoke-mutants.sh` skips by group, with `--skip cpu,memory,disk`.
- These checks add no CI job.

**Mutation rounds** use cargo-mutants 27.1.0 (MIT), a local tool and no dependency, instead of
hand-written mutation scripts. They cover the key logic: selection by group name, the source
fallback, masking, keeping partial results and the result after a timeout, and the reporter's
branches for each status. Linux-only code is mutated in a container. A surviving mutant gets a
test, or a written reason why it changes nothing.

**Real hosts.** While the network module is written, curl checks the three sources over IPv4 and
IPv6 on the owner's three hosts, read-only. After step 8, one full run per host checks the
report. It writes on the hosts (the program, its test directory, fio's 2 GiB file) and uploads a
Geekbench result, so its scope is approved separately (§16, risk 17).

### 11.2 M0, as recorded on 2026-11-07

Kept as it was when M0 was accepted; later milestones record their tests in their own documents.

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
| ignore tool preflight | `unmet_requirements_skip_without_running_glibc_first_then_network_then_tools` |
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
| P-13 | How should tools download on networks that reach GitHub poorly? | **Through the user's own proxy**, which the standard environment variables name (§9.1). No mirrors, no public GitHub proxies, no built-in proxy list. The alternatives were a copy in Cloudflare R2, on the site's server or on the owner's host in Hong Kong, and public GitHub acceleration proxies, which the owner chose first and dropped the same day. Decided 2026-11-10. |
| P-14 | Should downloads survive a GitHub outage? | **No.** A tool that cannot be downloaded skips its items with the reason, as before. The alternative was a second source outside GitHub. Decided 2026-11-10. |
| P-15 | Should SOCKS5 proxies work? | **Yes,** through reqwest's `socks` feature, so that a proxy from `ssh -D` serves. The alternative was HTTP proxies only. Decided 2026-11-10. |
| P-16 | How do users learn that a proxy helps? | **The README, and a hint after each download that fails on the network** (§9.1), naming the variable that is set but never its value. The alternatives were the README only, and one hint line under 【准备工具】. Decided 2026-11-10. |

## 13. Deferred extensions (designed seams, not built)

1. **Out-of-process plugins.** Not needed while P-01 stands. Build only if a first-party item needs a hard kill.
   - An `ExternalPlugin` adapter implements `DynPlugin`.
   - It runs the plugin as a self-exec subcommand: `lowendaff-bench __plugin <id>`.
   - Each run gets its own process group.
   - Results come back as JSON Lines on stdout.
2. **Concurrency.** Exclusive resource tags in `Descriptor`, plus bounded parallelism for untagged probes.
3. **Report file or lowendaff.com.** Another reporter over the same data (R-05); neither the
   plugins nor the orchestrator change.
4. **Route traces.** A typed result of their own plugin (R-06), such as hops of
   `{ ttl, address, asn, location, operator, latency }`; no new record kind is needed.
5. **Profiles, per-run timeout overrides, English labels.**
6. **Scripting or WASM.** Only as a single plugin that interprets data-driven rules (for example media-unlock definitions), never as the plugin mechanism.

## 14. Command line and progress

Options, parsed with clap:

- `--only`, `--skip`: comma-separated ids or group names (R-12);
- `--config <file>`: TOML, one `[plugins."<id>"]` table per plugin (D-03);
- `--work-dir <dir>`: where the run directory goes (D-14);
- `--list`: the plugins by group, with id, name, platforms and tools;
- `--show-ip`: addresses in full (R-18);
- `-v`, `--verbose`: every tool run in the progress log.

**Progress** (R-10) goes through tracing and tracing-subscriber (MIT), with one formatter that
writes to stderr. `ctx.progress(text)` becomes an event that carries the plugin's id:

```text
[00:00:01] 主机概况：独服，x86_64
[00:00:04] 网络：IPv4 可用，公网地址 203.0.*.*（来自 Cloudflare）；IPv6 不可用：本机没有 IPv6 路由
[00:00:09] 准备工具：sysbench 1.0.20、STREAM 5.10、multichase 2026.10.01 就绪
[00:00:10] memory.stream 开始
[00:00:11] memory.stream 单线程测试
[00:00:19] memory.stream 多线程测试（64 线程）
[00:00:31] memory.stream 完成，用时 21.3 秒
[00:02:40] memory.numa 失败：线程在节点 0、内存在节点 1 的带宽测试：……（共试了 4 次）
```

- **Time** counts from the start. A static binary cannot read the local time zone reliably once
  threads run, and in a long run the elapsed time says more.
- **`-v`** adds each tool run's command and exit code, and the last lines of its output when it
  fails, and each address source's answer.
- **End lines** use the reporter's words for statuses and reasons.

**Addresses on screen** (R-18):

- 系统信息 ends with one line per family, after TCP 拥塞控制, where M0 printed one
  `IPv4 / IPv6` line:

  ```text
  TCP 拥塞控制      bbr（队列 fq）
  IPv4              可用；公网地址 203.0.*.*（来自 Cloudflare）
  IPv6              可用；公网地址 2001:db8:*（来自 Cloudflare）
  ```

- With `--show-ip`: `可用；公网地址 203.0.113.7（来自 Cloudflare）`.
- When an earlier source failed: `可用；公网地址 203.0.*.*（来自 ipify）`.
- When every source failed:
  `可用；公网地址不可用：Cloudflare 超时，ipify 返回 HTTP 503，icanhazip 返回的不是 IPv4 地址`.
- When the family is unreachable, and no source is asked: `不可用：本机没有 IPv6 路由`.
- The progress log masks addresses the same way.
- **Masking:** IPv4 keeps two octets (`203.0.113.7` becomes `203.0.*.*`); IPv6 keeps its first two
  groups, 32 bits (`2001:db8:85a3::8a2e:370:7334` becomes `2001:db8:*`, `2a01::1` becomes
  `2a01:0:*`). One function serves the report and the log.
- **Guards:** a reason never includes an answer's body; plugins never write the host's address in
  progress text; the tests of §11.1 look for full addresses on stdout and stderr.

## 15. Engineering conventions

R-13 asks for code, commits, pull requests and the layout to follow written, checked conventions.
`CONTRIBUTING.md` holds them for contributors; this section records the design.

**Layering** (§5): `bench-host` ← `bench-api` ← `bench-tool` ← `plugins/*` ← `bench`. Today
`bench-host` depends on `bench-api` for six types only (`Platform`, `PlatformKind`, `Hypervisor`,
`ContainerKind`, `Libc`, `GlibcVersion`); they move to `bench-host`. `swap-file` stays a leaf
that the CPU group uses.

**A plugin's module**, in `plugins/<group>/src/<name>/`:

| File | Holds |
|------|-------|
| `mod.rs` | the plugin's declaration and `run` |
| `result.rs` | `Output` and `Failure` |
| `measure.rs` | the measurement flow, with an injectable runner |
| `parse.rs` | pure functions that read tool output |

Its tests are `tests/<name>_*.rs`, its fixtures `tests/fixtures/<name>/`, its snapshots
`tests/snapshots/`. The plugins of one group share only the group's helpers, such as the
working-set plan of the memory plugins. A plugin module holds no formatting.

**Names.** Core crates are `bench-*`; plugin crates `plugin-<group>`, in `plugins/<group>`;
plugin ids `<group>.<name>`. Every plugin module calls its types `Output` and `Failure`, written
with the module, as in `stream::Output`, since the names are common.

**Automated checks**, enforced by CI:

- **Dependency direction.** `deny.toml` lists, with cargo-deny's `bans.deny` and `wrappers`, the
  crates that may depend on `bench-api`, on `bench-tool` and on each plugin crate; plugin crates
  only on `bench`. Checked on 2026-11-13: limiting `plugin-cpu` to `bench` fails today's
  `plugin-memory` → `plugin-cpu` with `error[banned]`.
- **Network and tables only in `bench`.** reqwest and comfy-table are allowed for `bench` only.
- **Text only in the reporter.** An ast-grep rule rejects string literals with CJK characters in
  `plugins/`, `bench-api`, `bench-host` and `bench-tool`, except inside `progress(...)`; checked
  on 2026-11-13 on sample code, `format!` inside `progress(...)` included. Those crates hold 333
  such strings today; the rule covers each crate once its text has moved (§17).
- **The existing ast-grep rules** stay: no printing in plugins, no system temp directory in
  `src/`, no host programs, one item per `use`, free functions through their module, no
  `#[non_exhaustive]`.
- **Commit messages and pull request titles.** `scripts/check-commits.sh`, in the workflow
  `.github/workflows/commits.yml`, as the required check `commit messages`:
  - the header is `<type>(<scope>): <summary>`: the type is build, ci, docs, feat, fix, perf,
    refactor or test, the scope a package name or none, and the summary starts lowercase and does
    not end in a period;
  - every commit except `docs` has a body of at least 20 characters that says why;
  - lines are at most 98 columns, since the squash merge adds a two-column bullet; ASCII only; no
    `Co-authored-by`; no merge commits;
  - the title follows the header rule, and with ` (#N)` appended it fits in 100 columns.

  The workflow is separate because it also runs when a title is edited, which must not rerun the
  build. Dependabot's pull requests skip it, since their links exceed 100 columns; their squash
  message is rewritten at merge, and a skipped required check counts as passed.
- **Shell scripts and workflows.** shellcheck 0.11.0 over every tracked `*.sh`, and actionlint
  1.7.12, in the `workflow audit` job. taiki-e/install-action provides shellcheck but not
  actionlint, which is downloaded and checked against its SHA-256.

**Documents and scripts:**

- `CONTRIBUTING.md`, in English: the conventions that apply now; the layout and the steps to add
  a plugin join it in step 8 of §17.
- `.github/pull_request_template.md`, in Chinese: 背景, 改动, 验证, 遗留问题.
- `scripts/verify-commits.sh` runs the gates that need neither Linux nor root on every commit of
  a branch, in a worktree, and then checks the messages.

**Adding a plugin**, from step 8 of §17 on:

1. Write the plugin module (above), with its `Output`, its `Failure` and its requirements.
2. Add a `Measured` variant, and register the plugin with it in the run order.
3. Place it in the report order, and write its formatting in `report/terminal/<group>.rs`.
4. Add fixtures, tests and snapshots.
5. Add the plugin to the README's table of items.
6. A new group's crate joins the `wrappers` lists of `deny.toml` and the text rule.

## 16. Risks

The owner accepted these on 2026-11-13.

Running and output:

1. **One report at the end (R-01).** A dropped SSH session or a killed program, for instance by
   the OOM killer, loses the whole report; the longest run takes about 40 minutes (Geekbench on
   the ARM board). README advises tmux or screen; an interrupt or a timeout still reports what
   finished.
2. **A stuck read in the host profile.** The 系统基础信息 item had a 30 s timeout; the profile is
   read before every plugin, so a read that hangs, such as `statvfs` on a dead network mount,
   would stop the program at the start. The sections are read on threads of their own within
   30 s; unfinished ones show 不可用（超时）, and the plugins run (§10). The profile is now read
   on every run, also when one plugin is selected.
3. **The look changes in details.** comfy-table's spacing differs a little from today's; tables
   wrap to the terminal's width, but not in a file; the progress log has elapsed time only, which
   is harder to match with system logs. The words stay, checked by snapshots.

Public addresses:

4. **Third-party services** may change their format, limit requests, fail, or be unreachable in
   some regions. When all three fail, the addresses show 不可用; no plugin fails for it. Mainland
   China is untested.
5. **Privacy.** Every run contacts these services, which see the server's address. The report
   masks the addresses, but the provider's range still shows (two IPv4 octets, 32 bits of IPv6);
   `--show-ip` shows them in full. README says so.
6. **What the address means.** It is the default route's address: one of several public
   addresses, the shared address behind NAT, and 不可用 on a host online only through a proxy.
7. **Forcing the family relies on hyper-util's behaviour,** not on reqwest's documentation: a
   local address of one family makes it connect to that family only. A test pins it, so an
   upgrade that changes it fails.
8. **CI cannot test real IPv6 requests:** GitHub's runners have no IPv6. Only a host with IPv6
   can.
9. **Up to about 21 s more at the start**, when every target times out: 2 × 3 s of reachability
   and 3 × 5 s of sources, the two families at once.

The migration:

10. **A long transition.** Eight pull requests, each approved on its own. Two contracts live side
    by side until step 8 (the legacy variant), so migrated and unmigrated plugins look different
    meanwhile. This document describes the target, and §17 tells what has landed. No new test
    item meanwhile (R-14).
11. **333 Chinese strings move** from the plugins to the reporter, and may be copied or matched
    wrong. The reporter's snapshots are checked word for word against the texts that today's
    tests assert.
12. **Shared code for running tools** may change details: timeouts, kills, process groups,
    Geekbench's `oom_score_adj`, the memory tests' CPU placement. The plugins' process tests
    stay, the Linux-only ones run in a container, and a mutation round follows.
13. **Four memory plugins instead of one item.** Each reads the available memory when it starts,
    so the buffer sizes of one run can differ a little from M5's single plan; each has its own
    timeout, so in the worst case the memory part can exceed the former 30 minutes; a missing
    tool skips only its plugins; the summary counts four items for memory.

Engineering and dependencies:

14. **More dependencies:** comfy-table, tracing and tracing-subscriber in the program, insta in
    the tests, cargo-mutants and actionlint as tools. cargo-deny and Dependabot watch the crates,
    but actionlint is a pinned download in a workflow, which Dependabot does not update.
15. **New gates can block wrongly.** The commit check is required; when it rejects a sound
    commit, such as a revert, the script must change before the merge. The `wrappers` lists must
    follow every new plugin group (§15).
16. **The typed results have no consumer yet.** Their serialization, kept for lowendaff.com, is
    unused, so their field names may change then (R-05).

Verification:

17. **Real runs write on the hosts:** the test directory, fio's 2 GiB file (on the ARM board's
    storage too), and a Geekbench result that is uploaded publicly; on the VM, Geekbench runs out
    of memory. Their scope is approved separately.

## 17. Plan

Every step is one pull request that leaves the program complete and CI green. Every commit builds
and passes on its own. The measurements stay the same, checked with the same fixtures and numbers.
Each pull request is merged only with the owner's approval. The program has no release yet, so
changed ids and output need no compatibility.

The order: the gates first, so that every later pull request is checked; then the dependency
direction; then the framework; then the plugins, from simple to complex.

1. **Design and gates.** This document; `CONTRIBUTING.md`; the pull request template; the commit
   check; shellcheck and actionlint in CI; `scripts/verify-commits.sh`; README "Gates". Users see
   no change. New checks: commit messages, added to the ruleset after the merge; shellcheck;
   actionlint. *In progress.*
2. **Dependency direction.** `crates/bench-tool` with the six tools' pins, so that the memory
   plugin no longer depends on the CPU plugin; the platform and glibc types move to
   `bench-host`. Users see no change. New checks: the dependency direction; plugin crates only
   in `bench`.
3. **Contract, orchestrator, reporter.** The new contract; the orchestrator; comfy-table tables
   and insta snapshots; tracing progress and `-v`; group names in `--only` and `--skip`. Every
   plugin first goes through the legacy variant, with today's content. Users see progress on
   stderr, one report on stdout at the end, and sections by group, with Geekbench 7 in CPU 性能.
   New check: comfy-table only in `bench`.
4. **Host profile and addresses.** `HostProfile`, with the combining of `plugins/system`; the
   network module, with sources, fallback and masking; 系统信息 from the reporter;
   `plugins/system` removed; `--show-ip`; curl checks of the sources on the owner's hosts. Users
   see 系统信息 as the report's first section, with masked public addresses, and the ids
   `system.dedicated`, `system.vm` and `system.container` go. New checks: reqwest only in
   `bench`; the text rule over `bench-host` and `bench-tool`.
5. **CPU.** `cpu.sysbench` and `cpu.geekbench` return typed results; `plugin-geekbench` joins
   `plugin-cpu`; the shared code that runs tools (time limit, cancellation, retries, process
   groups) enters `bench-tool`; Geekbench's arguments get a test. Users see the new layout, with
   the same words. New check: the text rule over `plugins/cpu`.
6. **Disk.** `disk.health` and `disk.fio`; `plugin-disk-health` joins `plugin-disk`; smartctl and
   fio run through the shared code. New check: the text rule over `plugins/disk`.
7. **Memory.** `memory.sysbench`, `memory.stream`, `memory.latency` and `memory.numa`, sharing
   the group's working-set plan; the shared code gains CPU placement; the 30-minute timeout
   becomes one per plugin, by each one's worst case. Users still see the four chapters of
   内存性能; a failed chapter affects only itself, and `--only memory` still selects all four.
   New check: the text rule over `plugins/memory`.
8. **Cleanup.** The legacy variant and the M0 record API (`Record`, `Table`, `emit`, the old
   `Plugin`) go; `CONTRIBUTING.md` gains the layout and the steps to add a plugin; this
   document's status becomes implemented; the README is finished. New check: the text rule over
   `bench-api`, so that it covers every crate it should.

Each step is verified as §11.1 describes, with a mutation round on its key logic. After step 8,
real runs on the owner's three hosts check the report, with a scope approved at the time.

## Appendix A. Spike

The interface was first validated in a throwaway crate at `/tmp/plugin-spike`: 12 tests, and 8 of 8 mutations killed. It was then ported into `crates/bench-api` and `crates/bench`, with the owner's decisions applied:

- no per-plugin root requirement;
- pinned tools with `ctx.tool`;
- no `artifact` record and no raw text in `Measurement`, since there is no report file;
- work directories under the run root;
- Chinese messages.

The spike is superseded and can be deleted.

## Appendix B. Evidence and references

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

Checked for the overhaul on 2026-11-11 (comfy-table) and on 2026-11-13 (the rest):

- comfy-table 8.0.1: MIT, Rust 1.88. Without the default `tty` feature it depends on
  `unicode-segmentation` and `unicode-width` only (`cargo info`), and `Table::set_width` works
  without `tty`.
- insta 1.49.0: Apache-2.0. Without the default `colors` feature it depends on `once_cell`,
  `similar` (Apache-2.0) and `tempfile`.
- cargo-mutants 27.1.0: MIT.
- rustix 1.1.6 has `termios::isatty` and `termios::tcgetwinsize`, behind its `termios` feature.
- reqwest 0.13.5 has `ClientBuilder::no_proxy`, `local_address` and `timeout`, and passes the
  local address to hyper-util 0.1.21. There `HttpConnector::set_local_address` sets one family,
  and `SocketAddrs::split_by_preference` then keeps only that family's addresses
  (`client/legacy/connect/dns.rs`). Addresses written in the URL take the same path
  (`client/legacy/connect/http.rs`: `try_parse`, then `ConnectingTcp::new`). When no address is
  left, hyper-util fails with an error of its own: `NotConnected`, "Network unreachable".
- A scratch program with the project's versions, built like the downloader plus `no_proxy()`, ran
  from Singapore, where there is no IPv6. Forced to IPv4, Cloudflare (0.3 s), ipify (0.7 s),
  icanhazip (0.4 s) and `api64.ipify.org`, a name with both families, each gave a valid public
  IPv4 address. Forced to IPv6, each failed at once for want of a route; `api64.ipify.org` did
  not fall back to IPv4, and `https://1.1.1.1/` failed with hyper-util's own error. The program
  printed only whether each answer was a valid address, never the address.
- cargo-deny 0.20.2: `bans.deny = [{ crate = "plugin-cpu", wrappers = ["bench"] }]` reports
  `error[banned]` for today's `plugin-memory` → `plugin-cpu`, after a warning that the direct
  parent is not a wrapper.
- ast-grep 0.51.1: a rule for `string_literal` and `raw_string_literal` nodes that match
  `\p{Han}` outside `$CTX.progress($$$ARGS)` flagged the three Chinese literals of a sample outside
  `progress(...)`, and none inside it, `format!` included. Over today's code it finds 21 in
  `bench-api`, 3 in `bench-host`, 129 in `plugins/system`, 23 in `plugins/cpu`, 31 in
  `plugins/geekbench`, 66 in `plugins/memory`, 26 in `plugins/disk` and 34 in
  `plugins/disk-health`.
- taiki-e/install-action v2.90.2 has a manifest for shellcheck, whose latest is 0.11.0, and none
  for actionlint. actionlint 1.7.12's `actionlint_1.7.12_linux_amd64.tar.gz`, 2353908 bytes, has
  the same SHA-256 in its checksums file, in GitHub's record of the asset, and when downloaded:
  `8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8`.
- The workflows of `main` at `f49962f` pass actionlint 1.7.12. shellcheck 0.11.0 passes every
  tracked script but for two intended `SC2016` notes in
  `crates/bench-host/tests/fixtures/capture.sh`, which step 1 marks.
