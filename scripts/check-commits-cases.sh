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
# check <accept|reject> <name> <base> <head> [<title> <number> [<author type>]]
check() {
    cases=$((cases + 1))
    if PR_TITLE=${5:-} PR_NUMBER=${6:-} PR_AUTHOR_TYPE=${7:-} "$checker" "$3" "$4" \
        > /dev/null 2>&1; then
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

# A bot's pull request changes the generated lists, servers/iperf3.json and
# servers/netflix-ix.json, and nothing else.
# files <message> [<path> <content>]...: a commit with these files on top of the base.
files() {
    git checkout -q --detach "$base"
    message=$1
    shift
    while [ $# -ge 2 ]; do
        mkdir -p "$(dirname "$1")"
        printf '%s\n' "$2" > "$1"
        git add "$1"
        shift 2
    done
    git commit -q -m "$message"
    git rev-parse HEAD
}
title="build: update the iperf3 server list"
list=$(files "$title

los-angeles: Clouvider to Leaseweb" servers/iperf3.json '{"version": 1}')
check accept "a bot's pull request of the list" "$base" "$list" "$title" 16 Bot
check accept "a person's pull request of the list" "$base" "$list" "$title" 16 User
both=$(files "$title

los-angeles: Clouvider to Leaseweb" servers/iperf3.json '{}' scripts/run.sh 'echo run')
check reject "a bot's pull request of the list and a script" "$base" "$both" "$title" 16 Bot
check accept "a person's pull request of the list and a script" "$base" "$both" "$title" 16 User
check accept "a pull request of the list and a script, without an author" "$base" "$both" \
    "$title" 16
candidates=$(files "$title

tokyo: Leaseweb, added to the candidates" servers/iperf3-candidates.json '{"version": 1}')
check reject "a bot's pull request of the candidates" "$base" "$candidates" "$title" 16 Bot
ports="build: update Netflix's exchange ports"
exchanges=$(files "$ports

206.223.116.133: Equinix San Jose, added" servers/netflix-ix.json '{"version": 1}')
check accept "a bot's pull request of the exchange ports" "$base" "$exchanges" "$ports" 16 Bot
generated=$(files "$ports

Both generated lists in one pull request." servers/iperf3.json '{}' \
    servers/netflix-ix.json '{}')
check accept "a bot's pull request of both generated lists" "$base" "$generated" "$ports" 16 Bot
targets=$(files "build: change route tracing's targets

pek-4809: another name" servers/traceroute.json '{"version": 1}')
check reject "a bot's pull request of the target list" "$base" "$targets" \
    "build: change route tracing's targets" 16 Bot
check accept "a person's pull request of the target list" "$base" "$targets" \
    "build: change route tracing's targets" 16 User
git checkout -q --detach "$list"
git mv servers/iperf3.json servers/iperf3-old.json
git commit -q -m "$title

los-angeles: the list under another name"
renamed=$(git rev-parse HEAD)
check reject "a bot's pull request that renames the list" "$list" "$renamed" "$title" 16 Bot
check accept "a person's pull request that renames the list" "$list" "$renamed" "$title" 16 User
script=$(files "test: add a script

The script stands for any file that is not the server list." scripts/run.sh 'echo run')
git checkout -q --detach "$script"
mkdir -p servers
git mv scripts/run.sh servers/iperf3.json
git commit -q -m "$title

los-angeles: a script under the list's name"
check reject "a bot's pull request that renames a script to the list" "$script" \
    "$(git rev-parse HEAD)" "$title" 16 Bot

if [ "$failures" -gt 0 ]; then
    echo "check-commits-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "check-commits-cases: all $cases cases passed"
