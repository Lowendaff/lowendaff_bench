#!/bin/sh
# Checks commit messages against CONTRIBUTING.md "Commits": every
# commit of <base>..<head> and, when PR_TITLE is set, the pull request's title, which becomes the
# header of its squash commit on main. PR_NUMBER, when set, counts in the title's width, since
# GitHub appends " (#N)" to it.
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

count=0
for commit in $(git rev-list --reverse "$base..$head"); do
    count=$((count + 1))
    what=$(git log -1 --format='%h' "$commit")
    if [ "$(git rev-list --parents -n 1 "$commit" | wc -w)" -gt 2 ]; then
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

if [ -n "${PR_TITLE:-}" ]; then
    check_header "the title" "$PR_TITLE"
    check_text "the title" "$PR_TITLE${PR_NUMBER:+ (#$PR_NUMBER)}" 100
fi

if [ "$problems" -gt 0 ]; then
    echo "check-commits: $problems problem(s); CONTRIBUTING.md \"Commits\" has the rules" >&2
    exit 1
fi
echo "check-commits: $count commit(s)${PR_TITLE:+ and the title} follow the conventions"
