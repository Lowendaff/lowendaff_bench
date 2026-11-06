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
| Test | `cargo test --workspace --locked` | any host; CI uses Linux x86_64 and aarch64 |
| Static build | `cargo build --release --locked --target <arch>-unknown-linux-musl --bin lowendaff-bench` | Linux |
| Smoke | `scripts/smoke.sh target/<arch>-unknown-linux-musl/release/lowendaff-bench` | Linux, as root or with sudo |

`<arch>` is `x86_64` or `aarch64`.

## Commits

Commit messages follow the [Angular convention](https://github.com/angular/angular/blob/main/contributing-docs/commit-message-guidelines.md):

- Header format: `<type>(<scope>): <summary>`.
- Types: `build`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `test`.
- The scope is the crate name (`bench-api`, `bench`, `plugin-<item>`).
- The summary is in the imperative and lowercase, with no final period.
- Every commit except `docs` has a body of at least 20 characters that explains why.
- No line is longer than 100 characters.
