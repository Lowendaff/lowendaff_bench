#!/bin/sh
# Cases for scripts/check-commits.sh: commits in a scratch repository, each of which the checker
# must accept or reject, and pull request titles likewise.
#
# Usage: scripts/check-commits-cases.sh
set -eu

checker=$(cd "$(dirname "$0")" && pwd)/check-commits.sh
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
git -c init.defaultBranch=main init -q "$scratch/repo"
cd "$scratch/repo"
git config user.name check-commits
git config user.email check-commits@example.invalid
git config commit.gpgsign false
# Package names are scopes; the names of other tables are not.
mkdir -p crates/bench-api plugins/memory
printf '[package]\nname = "bench-api"\n\n[[bin]]\nname = "not-a-scope"\n' \
    > crates/bench-api/Cargo.toml
printf '[package]\nname = "plugin-memory"\n' > plugins/memory/Cargo.toml
git add .
git commit -q -m 'the base of every case'
base=$(git rev-parse HEAD)

failures=0
cases=0
# check <accept|reject> <name> <base> <head> [<title> <number>]
check() {
    cases=$((cases + 1))
    if PR_TITLE=${5:-} PR_NUMBER=${6:-} "$checker" "$3" "$4" > /dev/null 2>&1; then
        got=accept
    else
        got=reject
    fi
    if [ "$got" = "$1" ]; then
        echo "ok: $2"
    else
        echo "FAIL: $2: expected $1, got $got" >&2
        failures=$((failures + 1))
    fi
}

# commit <message>: commits on top of the base, verbatim, and prints the commit.
commit() {
    git checkout -q --detach "$base"
    printf '%s\n' "$1" > "$scratch/message"
    git commit -q --allow-empty --cleanup=verbatim -F "$scratch/message"
    git rev-parse HEAD
}

# message <accept|reject> <name> <message>
message() {
    check "$1" "$2" "$base" "$(commit "$3")"
}

why='The body says why the change is needed, in more than twenty characters.'
message accept "a type, a scope and a body" "feat(bench-api): add a thing

$why"
message accept "another package as the scope" "fix(plugin-memory): count the nodes

$why"
message accept "no scope" "refactor: move the thing

$why"
message accept "docs without a body" "docs: describe the thing"
message accept "a line of 98 columns" "test: cover the thing

$(printf '%098d' 0)"
message reject "a line of 99 columns" "test: cover the thing

$(printf '%099d' 0)"
message reject "an unknown type" "chore: tidy the thing

$why"
message reject "an unknown scope" "feat(nope): add a thing

$why"
message reject "the name of a table that is not a package" "feat(not-a-scope): add a thing

$why"
message reject "an uppercase summary" "feat: Add a thing

$why"
message reject "a final period" "feat: add a thing.

$why"
message reject "no body" "feat: add a thing"
message reject "a short body" "feat: add a thing

Too short."
message reject "no empty second line" "feat: add a thing
$why
$why"
message reject "a character outside ASCII" "feat: add a thing

$why $(printf 'caf\303\251')"
message reject "a tab" "feat: add a thing

$why$(printf '\tafter a tab')"
message reject "a co-author" "feat: add a thing

$why

Co-authored-by: Someone <someone@example.invalid>"

git checkout -q --detach "$base"
git commit -q --allow-empty -m "feat: add one side

$why"
side=$(git rev-parse HEAD)
git checkout -q --detach "$base"
git commit -q --allow-empty -m "feat: add the other side

$why"
git merge -q --no-ff -m "fix: join the two sides

$why" "$side"
check reject "a merge commit" "$base" "$(git rev-parse HEAD)"

# The squash commit's header is the title with " (#N)" appended: at most 100 columns.
check accept "a title" "$base" "$base" "ci: check the commit messages" 16
check accept "a title of 100 columns with its number" "$base" "$base" \
    "ci: $(printf '%090d' 0)" 16
check reject "a title of 101 columns with its number" "$base" "$base" \
    "ci: $(printf '%091d' 0)" 16
check reject "a title that is not a header" "$base" "$base" "Check the commit messages" 16

if [ "$failures" -gt 0 ]; then
    echo "check-commits-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "check-commits-cases: all $cases cases passed"
