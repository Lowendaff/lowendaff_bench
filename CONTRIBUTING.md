# Contributing

This guide holds the conventions that every change follows; CI enforces most of them (README
"Gates"). The decisions behind them are recorded in design documents outside the repository. The
plugin model is being reworked, one pull request at a time; the crate layout and the steps to add
a plugin join this guide when that work ends.

## Languages

- Code, identifiers, comments, documents and commit messages are English.
- Text that the program shows its users is Chinese; messages for developers, such as the
  registry checks, are English.
- Pull request bodies are English, in the sections of the template.

## Code

Rust follows the [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html),
formatted by `rustfmt` with its default settings (the guide's nightly-only options are not
used), with these rules of this repository:

- Call free functions through their module; import types unqualified, except common names such
  as `io::Error` and `toml::Value`; import a trait inside the function that uses it, unless the
  whole file does; one item per `use` (ast-grep rules `call-functions-through-module` and
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
- No system temp directory in `src/` (rule `no-temp-dir`): Debian keeps `/tmp` in memory, and
  hardened hosts mount it `noexec`.
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
  are the reporter's too: `bench-host`, `bench-tool` and `plugin-cpu` hold no Chinese string
  (rule `no-chinese-outside-the-reporter`), except in progress lines, and each other plugin crate
  joins the rule once its text has moved.
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

Every commit also builds and passes the gates on its own, with `--locked`;
`scripts/verify-commits.sh` checks a branch for this (README "Gates").

## Pull requests

- Work on a branch; `main` takes pull requests only, merged by squash.
- The title becomes the header of the squash commit on `main`, so it follows the header rule, and
  with ` (#N)` appended it fits in 100 columns. The squash message lists the commit messages.
- The body follows `.github/pull_request_template.md`, in the sections Background, Changes,
  Verification and Remaining issues, with lines of at most 100 columns.
- A pull request is merged only with the owner's approval and with every check green on its
  exact head; its branch is then deleted.
- Once CI has passed, add further fixes as new commits on top instead of rewriting the branch.
  CI reuses an earlier full validation only for commits that descend from the validated one
  (README "Gates"), so a rewritten branch runs the whole suite again.
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
