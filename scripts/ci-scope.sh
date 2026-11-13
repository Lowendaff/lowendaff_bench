#!/bin/sh
# Decides whether a leg of the "test, static build and smoke" job runs its steps or reuses an
# earlier validation, and prints the leg's validation record after a full run.
#
# A leg reuses an earlier validation only when it can verify it independently:
#   1. validation records of both architectures exist for the same head V; a leg writes its
#      record only after a full run;
#   2. GitHub's own job records show that, in the runs that wrote them, both legs and every one
#      of their steps succeeded, none skipped;
#   3. V is an ancestor of the commit under test, a pull request's base is the one V was tested
#      on, and this leg runs on the runner image that V's leg ran on;
#   4. every file changed since V is Markdown or LICENSE, listed without rename detection, so
#      that a renamed file shows under both of its names.
# Otherwise the leg looks at the whole change: a pull request's diff against its base, or a
# push's diff against the commit before it; only Markdown or LICENSE lets it skip. Any doubt or
# error runs the full suite: a redundant run is acceptable, a wrongly skipped test is not.
#
# Usage: scripts/ci-scope.sh decide   writes run=true|false to $GITHUB_OUTPUT and the decision
#                                     with its evidence to $GITHUB_STEP_SUMMARY
#        scripts/ci-scope.sh record   prints the leg's validation record as JSON
#
# Environment: EVENT (pull_request or push), ARCH (x86_64 or aarch64), REPOSITORY, HEAD_SHA,
# BASE_SHA and PR_NUMBER for a pull request, BEFORE_SHA for a push, GITHUB_RUN_ID,
# GITHUB_RUN_ATTEMPT, ImageOS and ImageVersion (set by GitHub's runner images) and GH_TOKEN.
# CI_SCOPE_API, if set, names a directory of canned API answers, which the cases use instead of
# GitHub.
set -u

# The steps that a validated leg must have run; every step of the leg must have succeeded.
required_steps='["Test", "Tool downloads", "Static build", "Swap round trip", "Smoke",
    "Smoke mutants", "Distributions", "Validation record", "Upload the validation record"]'

log() {
    printf 'ci-scope: %s\n' "$*" >&2
}

# The name that this leg's validation records carry: pr<number> or main.
scope() {
    case ${EVENT:-} in
        pull_request) [ -n "${PR_NUMBER:-}" ] && printf 'pr%s' "$PR_NUMBER" ;;
        push) printf 'main' ;;
        *) return 1 ;;
    esac
}

# api <path>: a GET from GitHub's API, or its canned answer.
api() {
    if [ -n "${CI_SCOPE_API:-}" ]; then
        cat "$CI_SCOPE_API/$(printf '%s' "$1" | tr '/?&=' '____').json"
    else
        gh api "$1"
    fi
}

# record_of <artifact id>: the validation record inside an artifact.
record_of() {
    if [ -n "${CI_SCOPE_API:-}" ]; then
        cat "$CI_SCOPE_API/artifact-$1.json"
        return
    fi
    archive=$(mktemp) || return 1
    if gh api "repos/$REPOSITORY/actions/artifacts/$1/zip" > "$archive"; then
        unzip -p "$archive" validation.json
        status=$?
    else
        status=1
    fi
    rm -f "$archive"
    return "$status"
}

# newest [head]: "<artifact id> <run id> <head>" of the newest unexpired artifact in the list on
# standard input, at the given head if one is given.
newest() {
    jq -r --arg head "${1:-}" '
        [.artifacts[] | select(.expired == false
            and ($head == "" or .workflow_run.head_sha == $head))]
        | sort_by(.created_at) | last // empty
        | "\(.id) \(.workflow_run.id) \(.workflow_run.head_sha)"'
}

# verify_leg <record> <arch> <head> <run id>: the record belongs to that leg, head and run, and
# GitHub's job records show that the leg ran every step, each of them successfully.
verify_leg() {
    printf '%s' "$1" | jq -e --arg scope "$scope_name" --arg arch "$2" --arg head "$3" \
        --arg run "$4" '.scope == $scope and .arch == $arch and .head == $head
            and (.run_id | tostring) == $run' > /dev/null || return 1
    attempt=$(printf '%s' "$1" | jq -r '.run_attempt') || return 1
    case $attempt in
        '' | *[!0-9]*) return 1 ;;
    esac
    api "repos/$REPOSITORY/actions/runs/$4/attempts/$attempt/jobs?per_page=100" |
        jq -e --arg name "test, static build and smoke ($2)" --argjson required "$required_steps" '
            [.jobs[] | select(.name == $name)] as $jobs
            | ($jobs | length) == 1
            and $jobs[0].status == "completed" and $jobs[0].conclusion == "success"
            and all($jobs[0].steps[]; .status == "completed" and .conclusion == "success")
            and ([$jobs[0].steps[].name] as $names
                | all($required[]; . as $step | any($names[]; . == $step)))' > /dev/null
}

# docs_only: true when every path on standard input is Markdown or LICENSE.
docs_only() {
    while IFS= read -r path; do
        case $path in
            '') ;;
            *.md | LICENSE) ;;
            *) return 1 ;;
        esac
    done
}

# try_reuse: true when this leg may reuse an earlier validation; sets why or evidence.
try_reuse() {
    if [ -z "${ImageOS:-}" ] || [ -z "${ImageVersion:-}" ]; then
        why="this runner does not name its image (ImageOS, ImageVersion)"
        return 1
    fi
    case $ARCH in
        x86_64) other=aarch64 ;;
        aarch64) other=x86_64 ;;
        *) why="unknown architecture $ARCH"; return 1 ;;
    esac
    records="repos/$REPOSITORY/actions/artifacts?per_page=100&name=validation-$scope_name"
    if ! listing=$(api "$records-$ARCH"); then
        why="the validation records of $ARCH cannot be listed"
        return 1
    fi
    found=$(printf '%s' "$listing" | newest) || found=
    # shellcheck disable=SC2086 # three words: artifact id, run id, head
    set -- $found
    if [ $# -ne 3 ]; then
        why="no validation record of $ARCH for $scope_name"
        return 1
    fi
    mine=$1 mine_run=$2 validated=$3
    if ! listing=$(api "$records-$other"); then
        why="the validation records of $other cannot be listed"
        return 1
    fi
    found=$(printf '%s' "$listing" | newest "$validated") || found=
    # shellcheck disable=SC2086 # three words: artifact id, run id, head
    set -- $found
    if [ $# -ne 3 ]; then
        why="no validation record of $other for $validated"
        return 1
    fi
    theirs=$1 their_run=$2
    if ! my_record=$(record_of "$mine") || ! their_record=$(record_of "$theirs"); then
        why="a validation record of $validated cannot be read"
        return 1
    fi
    if ! verify_leg "$my_record" "$ARCH" "$validated" "$mine_run" ||
        ! verify_leg "$their_record" "$other" "$validated" "$their_run"; then
        why="GitHub's job records do not show full, successful runs of both legs at $validated"
        return 1
    fi
    if ! my_base=$(printf '%s' "$my_record" | jq -r '.base') ||
        ! their_base=$(printf '%s' "$their_record" | jq -r '.base'); then
        why="the bases of the validation records cannot be read"
        return 1
    fi
    if [ "$my_base" != "$their_base" ]; then
        why="the two validation records name different bases"
        return 1
    fi
    if [ "$EVENT" = pull_request ] && [ "$my_base" != "${BASE_SHA:-}" ]; then
        why="the base moved from $my_base to ${BASE_SHA:-} since $validated was validated"
        return 1
    fi
    image="$ImageOS $ImageVersion"
    recorded=$(printf '%s' "$my_record" | jq -r '"\(.image_os) \(.image_version)"') || recorded=
    if [ "$recorded" != "$image" ]; then
        why="the runner image changed from $recorded to $image"
        return 1
    fi
    if ! git merge-base --is-ancestor "$validated" "$HEAD_SHA" 2> /dev/null; then
        why="$validated is not an ancestor of $HEAD_SHA"
        return 1
    fi
    if ! changed=$(git diff --no-renames --name-only "$validated" "$HEAD_SHA"); then
        why="the changes since $validated cannot be listed"
        return 1
    fi
    if ! printf '%s\n' "$changed" | docs_only; then
        why="files other than Markdown or LICENSE changed since $validated"
        return 1
    fi
    evidence="validated head $validated, by runs $mine_run ($ARCH) and $their_run ($other),"
    evidence="$evidence every step of both legs succeeded according to GitHub's job records;"
    evidence="$evidence base ${my_base:-none}; runner image $image; changed since:"
    evidence="$evidence $(printf '%s' "$changed" | tr '\n' ' ')"
}

# whole_change: true when the whole change is Markdown or LICENSE only; sets why or evidence.
whole_change() {
    case $EVENT in
        pull_request)
            if [ -z "${BASE_SHA:-}" ]; then
                why="the pull request names no base"
                return 1
            fi
            range="$BASE_SHA...$HEAD_SHA"
            ;;
        push)
            case ${BEFORE_SHA:-} in
                '' | 0000000000000000000000000000000000000000)
                    why="the push names no commit before it"
                    return 1
                    ;;
            esac
            range="$BEFORE_SHA..$HEAD_SHA"
            ;;
        *)
            why="unknown event $EVENT"
            return 1
            ;;
    esac
    if ! changed=$(git diff --no-renames --name-only "$range"); then
        why="the changes in $range cannot be listed"
        return 1
    fi
    if [ -z "$changed" ]; then
        why="$range changes no file"
        return 1
    fi
    if ! printf '%s\n' "$changed" | docs_only; then
        why="$range changes files other than Markdown or LICENSE"
        return 1
    fi
    evidence="$range changes only Markdown or LICENSE: $(printf '%s' "$changed" | tr '\n' ' ')"
}

# report <run> <title> <lines...>: the decision, to the step's outputs and summary.
report() {
    printf 'run=%s\n' "$1" >> "${GITHUB_OUTPUT:-/dev/null}"
    run=$1
    title=$2
    shift 2
    {
        printf '### test, static build and smoke (%s): %s\n\n' "${ARCH:-?}" "$title"
        for line in "$@"; do
            printf -- '- %s\n' "$line"
        done
    } >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
    log "run=$run: $title"
    for line in "$@"; do
        log "  $line"
    done
}

decide() {
    if ! scope_name=$(scope) || [ -z "${ARCH:-}" ] || [ -z "${REPOSITORY:-}" ] ||
        [ -z "${HEAD_SHA:-}" ]; then
        report true "running in full" "the event does not say what to compare"
        return
    fi
    if try_reuse; then
        report false "reusing an earlier validation" "$evidence"
        return
    fi
    reuse_why=$why
    if whole_change; then
        report false "skipped: only Markdown or LICENSE changed" \
            "no reuse: $reuse_why" "$evidence"
    else
        report true "running in full" "no reuse: $reuse_why" "whole change: $why"
    fi
}

record() {
    scope_name=$(scope) || return 1
    jq -n --arg scope "$scope_name" --arg arch "$ARCH" --arg head "$HEAD_SHA" \
        --arg base "${BASE_SHA:-}" --arg image_os "${ImageOS:-}" \
        --arg image_version "${ImageVersion:-}" --arg run_id "$GITHUB_RUN_ID" \
        --arg run_attempt "$GITHUB_RUN_ATTEMPT" '{scope: $scope, arch: $arch, head: $head,
            base: $base, image_os: $image_os, image_version: $image_version,
            run_id: ($run_id | tonumber), run_attempt: ($run_attempt | tonumber)}'
}

case ${1:-} in
    decide) decide ;;
    record) record ;;
    *)
        echo "usage: scripts/ci-scope.sh decide|record" >&2
        exit 2
        ;;
esac
