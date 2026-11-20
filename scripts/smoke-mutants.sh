#!/bin/sh
# Negative check for scripts/smoke.sh. Builds the release binary from a copy of the sources,
# first unchanged and then with one deliberate defect at a time. smoke.sh must accept the
# unchanged binary and reject each defective one for the expected reason. CI runs it;
# README "Gates" documents it.
#
# Usage: scripts/smoke-mutants.sh <target triple>
# Linux only. Run it from the repository root, as root or as a user with sudo; smoke.sh then
# runs the same way. The working tree is never modified.
set -eu

fail() {
    echo "smoke-mutants: $*" >&2
    exit 1
}

[ $# -eq 1 ] || fail "usage: scripts/smoke-mutants.sh <target triple>"
target=$1
repo=$PWD
scratch=$repo/target/smoke-mutants
built=$repo/target/$target/release/lowendaff-bench
bin=$scratch/bin/lowendaff-bench
if [ "$(id -u)" -eq 0 ]; then
    as_root=""
else
    as_root="sudo"
fi

# A defective binary can leave root-owned files in the run directory; if Static build already
# produced $built, restore it on exit so later steps see the untouched release binary.
cleanup() {
    if [ -f "$scratch/orig-lowendaff-bench" ]; then
        cp -p "$scratch/orig-lowendaff-bench" "$built" || true
    fi
    $as_root rm -rf "$scratch/run"
    rm -rf "$scratch"
}
trap cleanup EXIT

rm -rf "$scratch"
mkdir -p "$scratch/bin" "$scratch/src"
if [ -f "$built" ]; then
    cp -p "$built" "$scratch/orig-lowendaff-bench"
fi
# Preserve timestamps (-p) so Cargo treats unchanged workspace and dependency crates in
# $repo/target as fresh instead of recompiling 200+ dependencies from scratch.
cp -pR Cargo.toml Cargo.lock crates plugins "$scratch/src/"
dirty=""

# build [<file> <perl substitution>]: builds the binary from $scratch/src, restoring any file
# mutated by the previous round first (without -p so Cargo sees its mtime advance).
build() {
    if [ -n "$dirty" ]; then
        cp "$repo/$dirty" "$scratch/src/$dirty"
        dirty=""
    fi
    if [ $# -eq 2 ]; then
        [ -f "$1" ] || fail "no such file: $1"
        cp "$repo/$1" "$scratch/src/$1"
        perl -0pi -e "$2" "$scratch/src/$1"
        if cmp -s "$1" "$scratch/src/$1"; then
            fail "the substitution does not change $1: $2"
        fi
        dirty=$1
    fi
    # Unset RUSTC_WRAPPER: dependency crates are already compiled in $repo/target, and one-off
    # mutant binaries are never cacheable by sccache.
    (
        unset RUSTC_WRAPPER
        cargo build --quiet --release --locked --target "$target" --bin lowendaff-bench \
            --manifest-path "$scratch/src/Cargo.toml" --target-dir "$repo/target"
    )
    cp -p "$built" "$bin"
}

# smoke: runs smoke.sh on the built binary in a fresh directory and prints its output. The
# mutants concern the root check and the run directory, so the long tests are left out, and so is
# the network test, which would measure public servers.
smoke() {
    $as_root rm -rf "$scratch/run"
    mkdir "$scratch/run"
    (cd "$scratch/run" &&
        "$repo/scripts/smoke.sh" "$bin" --skip cpu,memory,disk,network,ip) 2>&1
}

# mutant <text smoke.sh must print> <file> <perl substitution>
mutant() {
    build "$2" "$3"
    if output=$(smoke); then
        fail "smoke.sh accepted a binary built with $3 in $2"
    fi
    case $output in
        *"$1"*) echo "smoke-mutants: $2 with $3 rejected: $(printf '%s\n' "$output" | tail -n 1)" ;;
        *) fail "smoke.sh rejected the mutant of $2 for another reason: $output" ;;
    esac
}

build
output=$(smoke) || fail "smoke.sh rejected the unchanged binary: $output"

# The non-root run must stop at the root check, not merely exit with 2.
mutant "non-root run" crates/bench/src/main.rs \
    's/if !rustix::process::geteuid\(\)\.is_root\(\) \{/if false {/'
mutant "run directory left behind" crates/bench/src/app.rs \
    's/\.tempdir_in\(&work_dir\)/.disable_cleanup(true).tempdir_in(&work_dir)/'
echo "smoke-mutants: ok"
