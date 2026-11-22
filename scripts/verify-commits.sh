#!/bin/sh
# Runs the gates of README "Gates" on every commit of a branch, each commit checked out on its
# own, so that every commit builds and passes by itself (CONTRIBUTING.md "Commits"). Then checks
# the messages of the same commits. It fails when git cannot list the commits, or there are none.
# Left to CI: the gates that need Linux or root (static build, swap round trip, smoke, smoke
# mutants, distributions) and the download of every pinned tool.
#
# Usage: GH_TOKEN=<token> scripts/verify-commits.sh [<base> [<head>]]   (main and HEAD)
# The dependency policy and zizmor's online audits need the network; zizmor needs GH_TOKEN.
set -eu

base=${1:-main}
head=${2:-HEAD}
repo=$(git rev-parse --show-toplevel)
cd "$repo"

if [ -z "${GH_TOKEN:-}" ]; then
    echo "verify-commits: set GH_TOKEN for zizmor's online audits, as CI does" >&2
    exit 2
fi
missing=
for tool in cargo cargo-nextest cargo-deny ast-grep actionlint shellcheck jq; do
    command -v "$tool" > /dev/null 2>&1 || missing="$missing $tool"
done
if ! command -v zizmor > /dev/null 2>&1 && ! command -v uvx > /dev/null 2>&1; then
    missing="$missing zizmor"
fi
if [ -n "$missing" ]; then
    echo "verify-commits: missing tools:$missing (versions in README \"Gates\")" >&2
    exit 2
fi

# zizmor at the version CI pins, through uvx when it is not installed.
zizmor_version=$(sed -n 's/.*tool: zizmor@\([0-9.]*\).*/\1/p' .github/workflows/ci.yml)
run_zizmor() {
    if command -v zizmor > /dev/null 2>&1; then
        zizmor "$@"
    else
        uvx "zizmor@$zizmor_version" "$@"
    fi
}

# Listed apart from the loop, which would pass over git's failure: set -e does not stop at a failed
# substitution in a for list, and a branch without commits would pass with nothing verified.
if ! commits=$(git rev-list --reverse "$base..$head"); then
    echo "verify-commits: git cannot list the commits of $base..$head" >&2
    exit 1
fi
if [ -z "$commits" ]; then
    echo "verify-commits: $base..$head holds no commit" >&2
    exit 1
fi

# One worktree and one target directory for all commits, so that builds stay incremental.
work=$repo/target/verify-commits
tree=$work/tree
mkdir -p "$work"
git worktree remove --force "$tree" 2> /dev/null || true
rm -rf "$tree"
git worktree prune
git worktree add --quiet --detach "$tree" "$base"
trap 'git -C "$repo" worktree remove --force "$tree" 2> /dev/null || true' EXIT
export CARGO_TARGET_DIR="$work/target"

verified=0
for commit in $commits; do
    echo "verify-commits: $(git log -1 --format='%h %s' "$commit")"
    git -C "$tree" checkout --quiet --detach "$commit"
    git -C "$tree" clean -dffxq
    (
        cd "$tree"
        cargo fmt --all --check
        cargo clippy --workspace --all-targets --locked -- -D warnings
        cargo nextest run --workspace --locked --no-fail-fast
        cargo deny check
        ast-grep test
        ast-grep scan
        actionlint
        git ls-files -z '*.sh' | xargs -0 shellcheck
        # The cases of the CI scripts and the rules of the documents, in the commits that have
        # them.
        for script in scripts/ci-scope-cases.sh scripts/check-commits-cases.sh \
            scripts/check-docs-cases.sh scripts/check-docs.sh; do
            if [ -x "$script" ]; then
                "$script" > /dev/null
            fi
        done
        run_zizmor --persona pedantic .
    )
    verified=$((verified + 1))
done
# The messages of the commits verified above: check-commits fails when the range holds another
# number of commits, as it does when the branch moves during the run.
PR_COMMITS=$verified scripts/check-commits.sh "$base" "$head"
echo "verify-commits: every commit of $base..$head passes"
