# Contributing

This guide holds the conventions that every change follows; CI enforces most of them (README
"Gates"). The decisions behind them are in `docs/design/`. The plugin model is being reworked,
one pull request at a time; the crate layout and the steps to add a plugin join this guide when
that work ends.

## Languages

- Code, identifiers, comments, documents and commit messages are English.
- Text that the program shows its users is Chinese; messages for developers, such as the
  registry checks, are English.
- Pull request bodies are Chinese, in the sections of the template.

## Code

Rust follows the Google Rust Style Guide, with these rules of this repository:

- Call free functions through their module; import types unqualified, except common names such
  as `io::Error`; import a trait inside the function that uses it; one item per `use` (ast-grep
  rules `call-functions-through-module` and `use-one-item`).
- Name a constructor that returns `Result` `try_new`. Implement `Display` instead of writing label
  helpers. Name a module after its primary type, without abbreviations.
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

## Tests and fixtures

- Parsers are tested against recorded tool output in `tests/fixtures/`, and host facts against
  captures of real hosts (`crates/bench-host/tests/fixtures/README.md`).
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
- The body, in Chinese, follows `.github/pull_request_template.md`, with lines of at most 100
  columns.
- A pull request is merged only with the owner's approval, given for that pull request, and with
  every check green on its exact head.
- Once CI has passed, add further fixes as new commits on top instead of rewriting the branch.
  CI reuses an earlier full validation only for commits that descend from the validated one
  (README "Gates"), so a rewritten branch runs the whole suite again.
- A failed check is investigated with its log, not retried. Only a failure caused by an outage of
  GitHub itself is rerun, once GitHub has recovered, and at most three times.
- Dependabot's pull requests skip the commit check; their squash message is rewritten at merge,
  since their links exceed 100 columns.
- A job that is added to CI or renamed is updated in the repository's ruleset as a required check.

## Design documents

A test item, or a change to the model, starts with a document in `docs/design/` that holds:

- a status line: what was accepted, by whom and when;
- the decisions, each with an id and the alternatives that were turned down;
- the design, the risks that the owner accepted, and the plan;
- the changes made during implementation, in a section of their own.

A decision that a later one replaces stays in place, and the later document says so.
