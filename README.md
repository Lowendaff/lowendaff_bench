# lowendaff-bench

A comprehensive test tool for Linux servers (VPS), written in Rust:

- Each test item is a plugin compiled into one static binary.
- An orchestrator selects the plugins, downloads the pinned tools they need, runs them in order with timeouts, and prints the results in Chinese.
- It must run as root.

**Status:** the plugin framework (M0) is in place, and the registry is still empty. Test items are added one at a time. The design is in [`docs/design/plugin-system.md`](docs/design/plugin-system.md).

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
| Static build | `cargo build --release --locked --target <arch>-unknown-linux-musl --bin lowendaff-bench` | Linux |
| Smoke | `scripts/smoke.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench` | Linux, as root or with sudo |

`<arch>` is `x86_64` or `aarch64`.

CI pins these tool versions; install the same versions locally:

- cargo-nextest 0.9.151;
- cargo-deny 0.20.2;
- zizmor 1.32.0, for example `uvx zizmor@1.32.0 --persona pedantic .`.

The `cargo deny check` and `zizmor` gates need network access: one fetches the RustSec advisory database, the other runs online audits (zizmor needs `GH_TOKEN` for those).

CI also uses sccache (backed by the GitHub Actions cache) and the mold linker. They change build speed, not results, so they are optional locally.

Every Monday, the `latest-deps` workflow updates a scratch checkout to the newest dependency versions that `Cargo.toml` allows. It then runs the tests and the dependency policy. A failure means an upstream release would break the build. The committed `Cargo.lock` is not touched.

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
