#!/bin/sh
# Checks commit messages against CONTRIBUTING.md "Commits": every
# commit of <base>..<head> and, when PR_TITLE is set, the pull request's title, which becomes the
# header of its squash commit on main. PR_NUMBER, when set, counts in the title's width, since
# GitHub appends " (#N)" to it. PR_AUTHOR_TYPE, when it is Bot, allows the pull request to change
# the generated lists, servers/iperf3.json, servers/taier.json and servers/netflix-ix.json, and
# nothing else: such pull requests come from the data workflow, which a bot runs, and auto-merge
# merges them unread (README "Network speed", "China carrier speed" and "Route tracing").
# PR_COMMITS, when set, is the number of commits that the pull request has, as GitHub counts them:
# <base>..<head> must hold as many, so that a history cut short, or a wrong base, fails instead of
# passing with commits unchecked. A range without commits fails too, and so does a git that cannot
# list them.
#
# Usage: scripts/check-commits.sh <base> <head>
set -eu

if [ $# -ne 2 ]; then
    echo "usage: scripts/check-commits.sh <base> <head>" >&2
    exit 2
fi
base=$1
head=$2

problems=0
problem() {
    printf 'check-commits: %s\n' "$*" >&2
    problems=$((problems + 1))
}

# A scope is the name of a package of the workspace.
scopes=$(awk '
    /^\[package\]/ { package = 1; next }
    /^\[/ { package = 0 }
    package && $1 == "name" { gsub(/"/, "", $3); print $3 }
' crates/*/Cargo.toml plugins/*/Cargo.toml | paste -s -d '|' -)
header="^(build|ci|docs|feat|fix|perf|refactor|test)(\(($scopes)\))?: [a-z0-9]"

# check_header <what> <header>
check_header() {
    if ! printf '%s\n' "$2" | grep -Eq "$header"; then
        problem "$1: not \"<type>(<scope>): <summary>\" with a known type, a package as" \
            "scope and a lowercase summary: $2"
    fi
    case $2 in
        *.) problem "$1: the header ends with a period: $2" ;;
    esac
}

# check_text <what> <text> <columns>: printable ASCII only, and no line wider than <columns>.
check_text() {
    if printf '%s\n' "$2" | LC_ALL=C grep -q '[^ -~]'; then
        problem "$1: only printable ASCII is allowed, without tabs"
    fi
    if ! printf '%s\n' "$2" | awk -v what="$1" -v columns="$3" '
        length($0) > columns {
            printf "check-commits: %s: line %d has %d columns, more than %d\n",
                what, NR, length($0), columns > "/dev/stderr"
            wide = 1
        }
        END { exit wide }'; then
        problems=$((problems + 1))
    fi
}

# The commits are listed, and each commit's parents asked for, apart from the loop and the test:
# set -e stops at neither a failed substitution in a for list nor one inside a test, so a git that
# fails would leave commits unchecked, and the check would pass.
if ! commits=$(git rev-list --reverse "$base..$head"); then
    echo "check-commits: git cannot list the commits of $base..$head" >&2
    exit 1
fi
count=0
for commit in $commits; do
    count=$((count + 1))
    what=$(git log -1 --format='%h' "$commit")
    parents=$(git rev-list --parents -n 1 "$commit")
    if [ "$(printf '%s\n' "$parents" | wc -w)" -gt 2 ]; then
        problem "$what: a merge commit; rebase the branch instead"
        continue
    fi
    message=$(git log -1 --format='%B' "$commit")
    subject=$(printf '%s\n' "$message" | sed -n 1p)
    check_header "$what" "$subject"
    # The squash merge lists each commit's message behind a two-column bullet.
    check_text "$what" "$message" 98
    if [ -n "$(printf '%s\n' "$message" | sed -n 2p)" ]; then
        problem "$what: the second line must be empty"
    fi
    case $subject in
        docs:* | docs\(*) ;;
        *)
            characters=$(printf '%s\n' "$message" | sed -n '3,$p' | tr -d '[:space:]' | wc -c)
            if [ $((characters)) -lt 20 ]; then
                problem "$what: only docs commits may lack a body of 20 characters that says why"
            fi
            ;;
    esac
    if printf '%s\n' "$message" | grep -qi '^co-authored-by:'; then
        problem "$what: no Co-authored-by trailers"
    fi
done

# A range without commits, or with another number than the pull request has, would pass with the
# pull request's commits unchecked.
if [ "$count" -eq 0 ]; then
    problem "$base..$head holds no commit"
elif [ -n "${PR_COMMITS:-}" ] && [ "$count" != "$PR_COMMITS" ]; then
    problem "$base..$head holds $count commit(s), but the pull request has $PR_COMMITS"
fi

if [ -n "${PR_TITLE:-}" ]; then
    check_header "the title" "$PR_TITLE"
    check_text "the title" "$PR_TITLE${PR_NUMBER:+ (#$PR_NUMBER)}" 100
fi

# A bot's pull request changes the generated lists only, so that no fault of the workflow that
# opens it passes anything else as an update of a list; route tracing's targets are not generated.
# It goes by the author's kind, not by name, so that a renamed App stays covered; Dependabot's pull
# requests skip this check altogether.
if [ "${PR_AUTHOR_TYPE:-}" = Bot ]; then
    if ! changed=$(git diff --no-renames --name-only "$base...$head"); then
        problem "the files that the pull request changes cannot be listed"
    else
        others=$(printf '%s\n' "$changed" |
            grep -v -x -F -e servers/iperf3.json -e servers/taier.json -e servers/netflix-ix.json \
                -e '' |
            tr '\n' ' ' || true)
        if [ -n "$others" ]; then
            problem "a bot's pull request may change servers/iperf3.json, servers/taier.json and" \
                "servers/netflix-ix.json only, not: ${others% }"
        fi
    fi
fi

if [ "$problems" -gt 0 ]; then
    echo "check-commits: $problems problem(s); CONTRIBUTING.md \"Commits\" has the rules" >&2
    exit 1
fi
echo "check-commits: $count commit(s)${PR_TITLE:+ and the title} follow the conventions"
