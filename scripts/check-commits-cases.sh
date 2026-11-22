#!/bin/sh
# Cases for scripts/check-commits.sh: commits in a scratch repository, each of which the checker
# must accept or reject, and pull request titles likewise.
#
# Usage: scripts/check-commits-cases.sh
set -eu
# The checker reads these from the environment, which each case sets for itself.
unset PR_TITLE PR_NUMBER PR_AUTHOR_TYPE PR_COMMITS

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
# verdict <accept|reject> <name> <command>...: runs the command, the checker in the environment
# that the case needs, and counts a failure when it does not decide as expected.
verdict() {
    cases=$((cases + 1))
    expected=$1
    name=$2
    shift 2
    if "$@" > /dev/null 2>&1; then
        got=accept
    else
        got=reject
    fi
    if [ "$got" = "$expected" ]; then
        echo "ok: $name"
    else
        echo "FAIL: $name: expected $expected, got $got" >&2
        failures=$((failures + 1))
    fi
}

# check <accept|reject> <name> <base> <head> [<title> <number> [<author type>]]
check() {
    verdict "$1" "$2" env PR_TITLE="${5:-}" PR_NUMBER="${6:-}" PR_AUTHOR_TYPE="${7:-}" \
        "$checker" "$3" "$4"
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
merge=$(git rev-parse HEAD)
check reject "a merge commit" "$base" "$merge"

# The squash commit's header is the title with " (#N)" appended: at most 100 columns. The titles
# come with a commit that follows the rules, since a range without commits fails.
good=$(commit "feat(bench-api): add a thing

$why")
check accept "a title" "$base" "$good" "ci: check the commit messages" 16
check accept "a title of 100 columns with its number" "$base" "$good" \
    "ci: $(printf '%090d' 0)" 16
check reject "a title of 101 columns with its number" "$base" "$good" \
    "ci: $(printf '%091d' 0)" 16
check reject "a title that is not a header" "$base" "$good" "Check the commit messages" 16

# The checker passes only once it has checked every commit of the pull request: not when git
# cannot list the commits, or fails after listing them, when the range holds none, when it holds
# another number than the pull request has, as a history cut short or a wrong base gives, or when
# git cannot tell a commit's parents.
check reject "a base that git cannot resolve" no-such-base "$good"
verdict reject "no repository" env GIT_DIR="$scratch/no-repository" "$checker" "$base" "$good"
check reject "a range without commits" "$good" "$good"
git checkout -q --detach "$good"
git commit -q --allow-empty -m "test: cover the thing

$why"
two=$(git rev-parse HEAD)
verdict accept "as many commits as the pull request has" \
    env PR_COMMITS=2 "$checker" "$base" "$two"
verdict reject "fewer commits than the pull request has" \
    env PR_COMMITS=3 "$checker" "$base" "$two"
verdict reject "more commits than the pull request has" \
    env PR_COMMITS=1 "$checker" "$base" "$two"
# A clone one commit deep, as a checkout without the history makes, holds the last commit only.
git branch -q pull-request "$two"
git clone -q --depth 1 --no-single-branch "file://$scratch/repo" "$scratch/shallow"
verdict reject "a shallow history that holds one of the two commits" \
    env GIT_DIR="$scratch/shallow/.git" PR_COMMITS=2 "$checker" origin/main origin/pull-request
# Two gits that fail in the middle, as one whose repository goes away would: one after it lists
# the commits, the other when it is asked for a commit's parents.
real_git=$(command -v git)
mkdir "$scratch/listing-fails" "$scratch/parents-fail"
cat > "$scratch/listing-fails/git" << EOF
#!/bin/sh
case " \$* " in *" --reverse "*) "$real_git" "\$@"; exit 128 ;; esac
exec "$real_git" "\$@"
EOF
cat > "$scratch/parents-fail/git" << EOF
#!/bin/sh
case " \$* " in *" --parents "*) exit 128 ;; esac
exec "$real_git" "\$@"
EOF
chmod +x "$scratch/listing-fails/git" "$scratch/parents-fail/git"
verdict reject "a git that fails after it lists the commits" \
    env PATH="$scratch/listing-fails:$PATH" "$checker" "$base" "$two"
verdict reject "a git that cannot tell a merge commit's parents" \
    env PATH="$scratch/parents-fail:$PATH" "$checker" "$base" "$merge"

# Dependabot skips commits.yml; no other bot pull requests are accepted in this repository.
check reject "a bot's pull request" "$base" "$good" "ci: check the commit messages" 16 Bot
check accept "a person's pull request" "$base" "$good" "ci: check the commit messages" 16 User

if [ "$failures" -gt 0 ]; then
    echo "check-commits-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "check-commits-cases: all $cases cases passed"
