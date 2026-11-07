#!/bin/sh
# Copies each image in scripts/distros.txt to <prefix>/<name>:<tag> under the same digest, where
# <name> is the image's name without its registry, and checks the copy. The copies are for CI to
# pull: AWS ECR Public allows one anonymous pull per second per address, and GitHub's runners
# share addresses. The mirror workflow runs this script (README "Gates").
#
# Usage: scripts/mirror.sh <prefix>, for example scripts/mirror.sh ghcr.io/lowendaff/mirror
# Needs crane, logged in to the prefix's registry. Images that are already there are skipped, so
# a run that stopped partway, for example on a rate limit, can simply be started again.
set -eu

fail() {
    echo "mirror: $*" >&2
    exit 1
}

[ $# -eq 1 ] || fail "usage: scripts/mirror.sh <prefix>"
while read -r image _; do
    case $image in
    '' | '#'*) continue ;;
    */*:*@sha256:*) ;;
    *) fail "$image: expected <registry>/<name>:<tag>@<digest>" ;;
    esac
    digest=${image#*@}
    copy=$1/${image#*/}
    tagged=${copy%@*}
    # Fetching by digest makes crane check the manifest's hash against the digest.
    if crane manifest "${tagged%:*}@$digest" >/dev/null 2>&1; then
        echo "mirror: $tagged@$digest: already there"
        continue
    fi
    crane copy --jobs 1 "$image" "$tagged"
    [ "$(crane digest "$tagged")" = "$digest" ] || fail "$tagged does not point at $digest"
    echo "mirror: $tagged@$digest: copied"
done <"$(dirname "$0")/distros.txt"
echo "mirror: ok"
