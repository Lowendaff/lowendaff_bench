# Contributing

This guide holds the layout of the code, the steps to add a test item and the conventions that
every change follows; CI enforces most of them ("CI checks" below). The decisions behind them are
recorded in design documents outside the repository.

## Languages

- Code, identifiers, comments, commit messages and English documents (`README.md`,
  `CONTRIBUTING.md`, `.github/pull_request_template.md`, `.github/workflows/`) are English;
  localized user documentation lives in `README.zh-CN.md`, `README.ja.md` and `docs/*.md`, and
  `.github/ISSUE_TEMPLATE/` holds bilingual English/Chinese issue forms.
- Text that the program shows its users is English, and comes from the message catalog in
  `crates/bench/src/locale/`: a trait per part of the program, which each language implements
  in a module of its own. Messages for developers, such as the registry checks, are English and
  written in place.
- Another language is one more module, `crates/bench/src/locale/<language>/`, that implements
  every trait, listed in `LOCALES`; the compiler names each text it still lacks. The locale comes
  from `LC_ALL`, `LC_MESSAGES` or `LANG`. The tests expect the English catalog, which
  `LC_ALL=C` selects.
- Outside localized Markdown documents (`README.zh-CN.md`, `README.ja.md`, `docs/*.md`),
  bilingual issue templates (`.github/ISSUE_TEMPLATE/`) and catalogs of other languages, no
  tracked file holds a CJK character, so that code, tests, fixtures, scripts and comments stay
  English (`scripts/check-docs.sh`).
- Pull request bodies are English, in the sections of the template.

## Layout

```text
crates/
  bench-host/  the host's facts, read from files, and the host profile that holds them
  bench-api/   the plugin contract: Plugin, Descriptor, Requirements, Context, Measurement, and
               the files that a plugin asks for
  bench-tool/  the pins of the external tools, and the code that runs a tool with a time limit,
               cancellation, retries, process groups, CPU placement and an unprivileged user
  bench/       the program: the command line, the registry, the orchestrator, the result enum,
               the report, the progress log, the network checks, the downloads of tools and
               files, and the message catalog
  swap-file/   a temporary swap file, with the only unsafe code
plugins/
  cpu/         cpu.bench (sysbench, 7zip, unixbench, hpl, passmark, ycruncher, prime95, geekbench)
  memory/      memory.sysbench, memory.stream, memory.latency, memory.numa
  disk/        disk.health, disk.fio
  network/     network.iperf3, network.trace, network.bgp
  ip/          ip.quality, ip.media
docs/          detailed module-by-module guides explaining how each test item measures and judges
servers/       the lists that the network items download at run time: network speed's servers,
               with their candidates, route tracing's targets and Netflix's exchange ports
```

Each test item is a plugin, whose id is `<group>.<name>`, in the crate of its group,
`plugins/<group>`. A plugin's module, `plugins/<group>/src/<name>/`, holds:

| File | Holds |
|------|-------|
| `mod.rs` | the plugin: its descriptor, and its `run`, which hands the tool to the measurement |
| `result.rs` | its `Output` and `Failure`: numbers and kinds of reasons, never words |
| `measure.rs` | the measurement, which runs the tool through an injected function |
| `parse.rs` | pure functions that read the tool's output |

Its tests are `plugins/<group>/tests/<name>_*.rs`, with the tool's recorded output in
`tests/fixtures/`. The plugins of a group share only the group's own helpers, such as the
memory plugins' working-set plan. Every plugin calls its types `Output` and `Failure`, written
with the module, as in `stream::Output`.

A run goes this way: the program reads its command-line arguments, checks for root, and enters
the interactive TUI when invoked with no options. Otherwise it builds the selected plugins in the
registry's order, reads the host profile, leaves out the plugins that do not apply to the host,
downloads the tools of the others, runs each plugin with its time limit while its progress goes
to stderr, and writes the report to stdout (plus an archive copy in `/tmp`, or to `--output`
alone) once every plugin is done.

## Adding a test item

A new test item starts with a design document (below). Then:

1. Write the plugin's module, as above. Its descriptor names its requirements (tools, network,
   platforms, glibc, NUMA nodes) and its time limit, set by its worst case. It publishes its
   result after each measurement, so that a timeout or an interrupt keeps the last one, and its
   progress lines tell the log what it is doing.
2. Add its variant to `Measured` in `crates/bench/src/results.rs`, and register it with that
   variant in `registry::all()`, where its place is its place in the run order.
3. Name it in the catalog's `item_name`, and write its part of the report in
   `crates/bench/src/report/terminal/<group>.rs` (which drives both `text` and `markdown`) and
   `crates/bench/src/report/json.rs`, with its words in the catalog; the registry's
   checks fail for a plugin without a name, and a variant without a report does not compile. A
   new group also gets its section in `SECTIONS`, which orders the report, and its title in the
   catalog's `group_title`.
4. Test the parsers with recorded output, the measurement with recorded runs, comparing each
   tool's whole command, and the words with insta snapshots in `crates/bench/tests/`.
5. Add the item to the README's table of items, and its tool to "Third-party tools" below;
   a new tool is built and released first, then pinned in `crates/bench-tool`.
6. A new group's crate depends on `bench-api` and `bench-tool` along the direction below: it
   joins their `wrappers` lists in `deny.toml` and gets an entry of its own, used by `bench`
   alone. The rule against printing covers `plugins/*/src` already.

## The network test's server list

`servers/iperf3-candidates.json` holds the reviewed candidates of each location, in order of
preference: a location's id, country and whether it is a default, then for each candidate its place,
provider, host, ports, families and link. `servers/iperf3.json`, which the program downloads, is
written from it by `scripts/iperf3-servers.sh update <export>`, with the export of
iperf3serverlist.net (`https://export.iperf3serverlist.net/listed_iperf3_servers.json`): for each
location, the first candidate that the export lists with the same host and ports and with downloads.
To add, remove or reorder servers, change the candidates in a pull request, run `update` with a
fresh export, and `scripts/iperf3-servers.sh check`. The list is never edited by hand: `check`
refuses a server that is no candidate. From then on, the `data` workflow keeps the list current by
itself: twice a day, it runs `update` with a fresh export and, when the list changes, opens a pull
request as the GitHub App `lowendaff-bench-data`, which merges itself once the required checks pass;
`scripts/check-commits.sh` lets a bot's pull request change the generated lists, this one and
`servers/netflix-ix.json`, and nothing else. Nothing builds from a list in `servers/`, so that a
change of the lists alone needs no build: `check` refuses a file that names one, other than
Markdown, the lists' scripts and their cases, ci-scope and its cases, check-commits and its cases,
and the workflows that test and update the lists, and Rust code that names one other than by the
address that the program downloads it from. A rule of the list changes in the program's reader
(`plugins/network/src/iperf3/servers.rs`) and in the script together, with a fixture in
`plugins/network/tests/fixtures/server-lists/` that both read. The same workflow keeps route
tracing's list of Netflix's exchange ports, `servers/netflix-ix.json`, current from PeeringDB with
`scripts/netflix-ix.sh update`.

## Code

Rust follows the [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html),
formatted by `rustfmt` with its default settings (the guide's nightly-only options are not
used), with these rules of this repository:

- Call free functions through their module; import types unqualified, except common names such
  as `io::Error` and `serde_json::Value`; import a trait inside the function that uses it, unless
  the whole file does; one item per `use` (ast-grep rules `call-functions-through-module` and
  `use-one-item`).
- Name constructors `new` or `new_*`, or `try_new` when they return a `Result`. Implement
  `Display` instead of writing label helpers. Name a module after its primary type, without
  abbreviations.
- No combinator chains in `for` headers; prefer `let`-`else`; `expect("why")` instead of
  `unwrap()`, which only tests may use; an assertion's message says what was expected.
- Public enums are not `#[non_exhaustive]` (rule `no-non-exhaustive`): every consumer lives in
  the workspace, so a new variant is a compile error instead of a silent default.
- `unsafe` is forbidden, except in `crates/swap-file`, whose two calls turn swap on and off.
- Lines are at most 100 columns; a CJK character counts as two.
- Library crates and plugins never print (rule `no-print-in-plugins`); they report through the
  plugin context.
- No system temp directory in `src/` (rule `no-temp-dir`, except `crates/bench/src/report_path.rs`
  for finished report copies): Debian keeps `/tmp` in memory, and hardened hosts mount it `noexec`.
- No program from the host (rule `no-host-tools`): external tools are pinned builds, so that
  results do not depend on what a server has installed. Reading the host's files, such as
  `/proc`, is fine.
- Code that reads the environment takes it as a parameter, so that tests need no
  `std::env::set_var`, which is `unsafe` in Rust 2024.
- Crates depend in one direction: `bench-host` ← `bench-api` ← `bench-tool` ← the plugin crates
  ← `bench`, and a plugin crate is used by `bench` alone. `deny.toml` names the crates that may
  depend on each of them; a new dependency along the direction is added there.
- Plugins and the shared crates return data, and the reporter in `bench` formats it: `deny.toml`
  allows comfy-table, which draws the report's tables, in `bench` only. The words the user reads
  are the catalog's too: a plugin returns numbers and kinds of reasons, with the tools' and the
  system's own messages, and names the step it starts with a `Step`, which holds no words.
- Only `bench` connects anywhere: it downloads the tools and asks for the public addresses, and
  `deny.toml` allows reqwest there only. Host facts are read from files.
- Besides clippy's defaults, the lint gate checks `must_use_candidate`, `needless_for_each`,
  `unwrap_used` (allowed in tests by `clippy.toml`) and `wildcard_imports`; `bench-api`,
  `bench-host`, `bench-tool` and `swap-file` document every public item (`missing_docs`).
- A plugin's panic is contained by its task and reported as a failure: defense in depth, never
  control flow.

## Tests and fixtures

- Parsers are tested against recorded tool output in `tests/fixtures/`, and host facts against
  captures of real hosts (`crates/bench-host/tests/fixtures/README.md`).
- The scripts that check the lists in `servers/` take the rules that the lists share, of an IPv4
  address, an IPv6 address, a DNS name and a text, from `scripts/list-rules.jq`. The lists'
  fixtures, which the scripts and the program's readers both read, hold every edge of those
  rules, so that a script and its reader agree on each.
- Whole reports are compared with insta snapshots in `crates/bench/tests/snapshots/`. A change to
  the report shows in its snapshots, which the pull request holds for review:
  `INSTA_UPDATE=always` rewrites them locally, and CI writes none, so a difference fails there.
- Captures and fixtures never hold hostnames, IP addresses, MAC addresses, serials, WWNs, UUIDs,
  asset tags or BMC GUIDs, nor a device tree's serial number or boot arguments. Geekbench result
  links with a claim key never appear anywhere.
- Key logic gets a mutation round: each mutant must make a test fail, or the change must be shown
  not to alter behaviour. Linux-only code is tested and mutated in a Linux container.
- Numbers measured on a busy development machine are not reported as results.

## Commits

`scripts/check-commits.sh` checks these rules on every pull request:

- The header is `<type>(<scope>): <summary>`:
  - the type is build, ci, docs, feat, fix, perf, refactor or test;
  - the scope is the name of a package of the workspace, or is left out;
  - the summary is imperative and lowercase, without a final period.
- Every commit except `docs` has a body of at least 20 characters that says why.
- Lines are at most 98 columns, so that the squash commit's two-column bullet still fits in 100.
  ASCII only, no `Co-authored-by` trailers and no merge commits.

Every commit also builds and passes the checks on its own, with `--locked`;
`scripts/verify-commits.sh` checks a branch for this ("CI checks" below).

## Pull requests

- Work on a branch; `main` takes pull requests only, merged by squash.
- The title becomes the header of the squash commit on `main`, so it follows the header rule, and
  with ` (#N)` appended it fits in 100 columns. The squash message lists the commit messages.
- The body follows `.github/pull_request_template.md`, in the sections Background, Changes,
  Verification and Remaining issues, with lines of at most 100 columns.
- A pull request is merged only with the owner's approval and with every check green on its
  exact head; its branch is then deleted.
- Once CI has passed, add further fixes as new commits on top instead of rewriting the branch.
  CI reuses an earlier full validation only for commits that descend from the validated one, so a
  rewritten branch runs the whole suite again.
- A failed check is investigated with its log, not retried. Only a failure caused by an outage of
  GitHub itself is rerun, once GitHub has recovered, and at most three times.
- Dependabot's pull requests skip the commit check; their squash message is rewritten at merge,
  since their links exceed 100 columns.
- A ruleset on `main`, which nobody can bypass, enforces this: pull requests only, squash merges,
  every required check green on a branch that is up to date with `main`, and no force push or
  deletion. A job that is added to CI or renamed is updated in it as a required check.

## Design documents

A test item, or a change to the model, starts with a design document that holds:

- a status line: what was accepted, by whom and when;
- the decisions, each with an id and the alternatives that were turned down;
- the design, the risks that the owner accepted, and the plan;
- the changes made during implementation, in a section of their own.

A decision that a later one replaces stays in place, and the later document says so. Design
documents are kept outside the repository until they are published, so nothing in the repository
points into them: a comment or a document gives the reason itself.

## Third-party tools

External tools are built reproducibly by `.github/workflows/tools.yml` from `scripts/build-*.sh`
and published as immutable GitHub releases of this repository (with sources, licenses and
`SHA256SUMS`), except vendor-distributed suites (`Geekbench 7`, `PassMark PerformanceTest`,
`y-cruncher`, and `Prime95`), which are downloaded directly from their upstream distributions
under pinned SHA-256 and size:

- `7-Zip` (`7zz`) 26.06 without unRAR (`LGPL-2.1-or-later`; `BSD-3-Clause`) — `7zip-26.06-1`
  (`cpu.bench`)
- `fio` 3.43 with `libaio` 0.3.113 static (`GPL-2.0`; `LGPL-2.1`) — `fio-3.43-2` (`disk.fio`)
- `HPL` 2.3 with `OpenBLAS` 0.3.34 OpenMP static (`BSD-3-Clause`) — `hpl-2.3-1` (`cpu.bench`)
- `iperf3` 3.22 without OpenSSL or SCTP (`BSD-3-Clause`) — `iperf3-3.22-1` (`network.iperf3`)
- `passmark-libs` (`ncurses` 6.5 ABI 5 + `curl` 8.12.1 runtime, `MIT`; `curl`) —
  `passmark-libs-1.0-1` (`cpu.bench`)
- `smartctl` 7.5 from smartmontools (`GPL-2.0-or-later`) — `smartctl-7.5-1` (`disk.health`)
- `sysbench` 1.0.20 (`GPL-2.0-or-later`; `MIT`; `BSD-2-Clause`) — `sysbench-1.0.20-1`
  (`cpu.bench`, `memory.sysbench`)
- `STREAM` 5.10 for six array sizes (STREAM license) — `stream-5.10-1` (`memory.stream`)
- `Byte-UnixBench` 6.0.1 static (`GPL-2.0`) — `unixbench-6.0.1-1` (`cpu.bench`)
- `NextTrace` 1.7.6 tiny flavor (`GPL-3.0`; `BSD-3-Clause`) — `nexttrace-1.7.6-1`
  (`network.trace`)
- `multichase` and `multiload` `8b68e918` with `glibc` 2.41 static (`Apache-2.0`; `LGPL-2.1`) —
  `multichase-2026.10.01-1` (`memory.latency`, `memory.numa`)

## CI checks

`.github/workflows/ci.yml` and `.github/workflows/commits.yml` run these checks; run the local
ones before pushing:

- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo nextest run --workspace --locked --no-fail-fast` and
  `cargo nextest run -p bench --test tools --locked --run-ignored only`
- `cargo deny check`, `zizmor --persona pedantic .`, `actionlint`, and `shellcheck` across `*.sh`
- `scripts/check-commits-cases.sh` and `scripts/check-commits.sh origin/main HEAD`
- `ast-grep test`, `ast-grep scan`, `scripts/check-docs-cases.sh` and `scripts/check-docs.sh`
- `scripts/{iperf3-servers,traceroute-targets,netflix-ix}{-cases,}.sh` (`check` subcommand)
- On Linux (`x86_64` and `aarch64`): static `musl` release build, `swap-file` roundtrip example,
  `scripts/smoke.sh` with local loopback responders (`smoke-{iperf3,trace,ip,media,bgp}.sh`),
  `scripts/smoke-mutants.sh`, `scripts/distros.sh`, and non-blocking `scripts/network-check.sh`.

CI pins `cargo-nextest` `0.9.151`, `cargo-deny` `0.20.2`, `zizmor` `1.32.0`, `ast-grep` `0.51.1`,
`shellcheck` `0.11.0`, and `actionlint` `1.7.12`.
