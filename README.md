# lowendaff-bench

A comprehensive test tool for Linux servers (VPS), written in Rust:

- Each test item is a plugin compiled into one static binary.
- An orchestrator selects the plugins, downloads the pinned tools they need, runs them in order with timeouts, and prints the results in Chinese.
- It must run as root.

**Status:** the plugin framework (M0) is in place. Test items are added one at a time; the first is 系统基础信息 (system information). The design is in [`docs/design/plugin-system.md`](docs/design/plugin-system.md).

The tool first detects where it runs: a dedicated server, a virtual machine or a container. It prints this as 运行环境 and runs only the test items made for that platform. For example, 系统基础信息 has a variant for each platform (`system.dedicated`, `system.vm`, `system.container`), and exactly one of them runs. `--list` shows which platforms each item applies to.

| Test item | Shows | Design |
|-----------|-------|--------|
| 系统基础信息 | CPU, memory and swap, disks and filesystems, distribution, kernel, uptime, load, TCP congestion control, IPv4/IPv6 reachability | [`docs/design/system-info.md`](docs/design/system-info.md) |

## Usage

```sh
lowendaff-bench --list                      # list the test items
sudo lowendaff-bench                        # run all of them
sudo lowendaff-bench --only <id>,<id>       # run a subset; --skip excludes items
sudo lowendaff-bench --config bench.toml    # per-item settings: [plugins."<id>"]
```

- **Run directory.** The tool creates its run directory under the current directory, or under `--work-dir`, and removes it at exit. Avoid `/tmp`: it may be RAM-backed or mounted `noexec`.
- **Exit codes:**
  - `0`: the run finished;
  - `2`: usage or config error, or not root;
  - `130`: interrupted.

## Build

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml). Release binaries are static musl builds for `x86_64` and `aarch64`. The `ring` crate compiles C code for the musl target, so the build needs `musl-gcc` (Debian/Ubuntu package `musl-tools`).

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

## Gates

CI (`.github/workflows/ci.yml`) runs exactly these commands. Run them locally before pushing.

| Gate | Command | Runs on |
|------|---------|---------|
| Format | `cargo fmt --all --check` | any host |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | any host |
| Test | `cargo nextest run --workspace --locked --no-fail-fast` | any host; CI uses Linux x86_64 and aarch64 |
| Dependencies | `cargo deny check` (policy in `deny.toml`) | any host |
| Workflows | `zizmor --persona pedantic .` (config in `.github/zizmor.yml`) | any host |
| Rule tests | `ast-grep test` (cases in `ast-grep/tests/`) | any host |
| Rules | `ast-grep scan` (rules in `ast-grep/rules/`, config in `sgconfig.yml`) | any host |
| Static build | `cargo build --release --locked --target <arch>-unknown-linux-musl --bin lowendaff-bench` | Linux |
| Smoke | `scripts/smoke.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench` | Linux, as root or with sudo |
| Smoke mutants | `scripts/smoke-mutants.sh <arch>-unknown-linux-musl`: smoke must reject deliberately broken builds | Linux, as root or with sudo |
| Distributions | `scripts/distros.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench`: 系统基础信息 in 12 distributions' images (13 on x86_64) | Linux or macOS with Docker |

`<arch>` is `x86_64` or `aarch64`.

CI pins these tool versions; install the same versions locally:

- cargo-nextest 0.9.151;
- cargo-deny 0.20.2;
- zizmor 1.32.0, for example `uvx zizmor@1.32.0 --persona pedantic .`;
- ast-grep 0.51.1.

The `cargo deny check` and `zizmor` gates need network access: one fetches the RustSec advisory database, the other runs online audits (zizmor needs `GH_TOKEN` for those).

CI also uses sccache (backed by the GitHub Actions cache) and the mold linker. They change build speed, not results, so they are optional locally.

Every Monday, the `latest-deps` workflow updates a scratch checkout to the newest dependency versions that `Cargo.toml` allows. It then runs the tests and the dependency policy. A failure means an upstream release would break the build. The committed `Cargo.lock` is not touched.

`scripts/distros.txt` lists container images pinned by digest. The `mirror` workflow copies them to `ghcr.io/lowendaff/mirror` under the same digests with `scripts/mirror.sh`, which checks each copy, so that CI can pull them from there: AWS ECR Public allows one anonymous pull per second per address, and GitHub's runners share addresses. The workflow publishes with `GITHUB_TOKEN`, so the packages are linked to this repository and CI can pull them with its own token. After changing `scripts/distros.txt`, run `gh workflow run mirror.yml --ref <branch>`; images that are already copied are skipped.

Dependabot (`.github/dependabot.yml`) proposes weekly update PRs for cargo and for the pinned GitHub Actions. It waits until a release is at least 7 days old.

## Code style

Rust code follows the [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html).

**Formatting.** Use default `rustfmt` settings (100 columns). The guide's nightly-only options are not used. `use` statements import one item each, which satisfies "a `use` statement imports items from one module".

**Naming and calls:**

- Call free functions through their module, e.g. `orchestrator::plan(...)`.
- Import types and use them unqualified. Common names such as `io::Error` and `toml::Value` stay module-qualified.
- Import a trait inside the function that uses it, unless the whole file does.
- Name constructors `new()` or `new_*()`, or `try_new()` when they return a `Result`.

**Panics and assertions:**

- Use `expect("why")` instead of `unwrap()`.
- Put the explanation for an assertion in its message, not in a comment.

**Enforcement.** The lint gate checks these clippy lints:

- `must_use_candidate`
- `unwrap_used` (allowed in tests through `clippy.toml`)
- `wildcard_imports`
- `needless_for_each`
- `missing_docs` (only in `bench-api`)

`unsafe_code` is forbidden.

The ast-grep rules in `ast-grep/rules/` cover what rustc, clippy and rustfmt cannot check:

- one item per `use` statement, and free functions called through their module;
- no `#[non_exhaustive]` (see below);
- no files in /tmp or the system temp directory (design D-14);
- no programs started by name from the host instead of pinned tools (design D-13, P-11);
- no printing from the library crates (`bench-api`, `bench-host`) or plugins, which report through `ctx.emit` (design D-05, D-06).

**Deliberate interpretations:**

- Public enums are not `#[non_exhaustive]`, because every consumer is in this workspace (the guide's large-scale-change exception).
- A plugin panic is contained by its tokio task and reported as a failure. This is defense in depth, never control flow.

## Commits

Commit messages follow the [Angular convention](https://github.com/angular/angular/blob/main/contributing-docs/commit-message-guidelines.md):

- Header format: `<type>(<scope>): <summary>`.
- Types: `build`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `test`.
- The scope is the crate name (`bench-api`, `bench`, `plugin-<item>`).
- The summary is in the imperative and lowercase, with no final period.
- Every commit except `docs` has a body of at least 20 characters that explains why.
- No line is longer than 100 characters.

## Merging

Pull requests are merged with **squash merge** only; the repository allows no other merge method.

- The PR title becomes the commit header on `main`, so it must follow the Angular format above.
- The squash commit body lists the PR's commit messages.
- A PR is merged only after its exact head commit passed CI, and only with the owner's approval for that PR.
- The branch is deleted after the merge.

A repository ruleset on `main` enforces this, and nobody can bypass it:

- every change reaches `main` through a pull request, merged by squash;
- every job of the `ci` workflow must pass, on a branch that is up to date with `main`;
- `main` cannot be force-pushed or deleted.

When a CI job is added or renamed, update the ruleset's required checks to match.

Dependabot's commit messages contain release-note links longer than 100 characters. When squash-merging a Dependabot PR, replace the commit body with a short reason for the update.
