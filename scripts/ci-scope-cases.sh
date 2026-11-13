#!/bin/sh
# Cases for scripts/ci-scope.sh: a scratch history, canned API answers, and the decision that a
# leg must take in each case (run=true runs the full suite, run=false skips it), with the reason
# that its job summary must give.
#
# Usage: scripts/ci-scope-cases.sh
set -eu

script=$(cd "$(dirname "$0")" && pwd)/ci-scope.sh
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
git -c init.defaultBranch=main init -q "$scratch/repo"
cd "$scratch/repo"
git config user.name ci-scope
git config user.email ci-scope@example.invalid
git config commit.gpgsign false

# commit <message> [<path> <content>]...: writes the files, commits, and prints the commit.
commit() {
    message=$1
    shift
    while [ $# -ge 2 ]; do
        mkdir -p "$(dirname "$1")"
        printf '%s\n' "$2" > "$1"
        git add "$1"
        shift 2
    done
    git commit -q --allow-empty -m "$message"
    git rev-parse HEAD
}

# The history: a base, a pull request validated at V, and what can follow V.
base=$(commit base src/lib.rs 'fn a() {}' README.md '# readme' LICENSE 'MIT')
validated=$(commit code src/lib.rs 'fn b() {}')
markdown=$(commit docs README.md '# readme, again' notes/plan.md '# plan')
script_change=$(commit script scripts/run.sh 'echo run')
git checkout -q --detach "$validated"
license=$(commit license LICENSE 'Apache-2.0')
git checkout -q --detach "$validated"
git mv src/lib.rs src/lib.md
renamed=$(commit rename)
git checkout -q --detach "$base"
rewritten=$(commit 'code, rewritten' src/lib.rs 'fn b() {}')
rewritten=$(commit docs README.md '# readme, again')
git checkout -q --detach "$base"
moved=$(commit 'the base moves' src/other.rs 'fn c() {}')
git checkout -q --detach "$base"
docs_pull_request=$(commit 'docs only' README.md '# readme, in other words')
git checkout -q --detach "$base"
git mv src/lib.rs src/lib.md
renamed_pull_request=$(commit 'rename only')

# The canned API answers of each group of cases live in a directory of their own.
group=0
fresh() {
    group=$((group + 1))
    answers=$scratch/answers-$group
    mkdir -p "$answers"
}

# artifact <scope> <arch> <artifact id> <run id> <head> [expired]
artifact() {
    printf '{"artifacts": [{"id": %s, "expired": %s, "created_at": "2026-11-13T10:00:00Z",
        "workflow_run": {"id": %s, "head_sha": "%s"}}]}\n' "$3" "${6:-false}" "$4" "$5" \
        > "$answers/repos_o_r_actions_artifacts_per_page_100_name_validation-$1-$2.json"
}

# record <artifact id> <scope> <arch> <head> <base> <image version> <run id>
record() {
    jq -n --arg scope "$2" --arg arch "$3" --arg head "$4" --arg base "$5" --arg version "$6" \
        --argjson run "$7" '{scope: $scope, arch: $arch, head: $head, base: $base,
            image_os: "ubuntu24", image_version: $version, run_id: $run, run_attempt: 1}' \
        > "$answers/artifact-$1.json"
}

# jobs <run id> <arch> [<job conclusion> [<step> <its conclusion, or absent>]]
jobs() {
    jq -n --arg arch "$2" --arg job "${3:-success}" --arg step "${4:-}" --arg mark "${5:-}" '
        ["Set up job", "Run actions/checkout", "Scope", "Install the pinned toolchain", "Test",
            "Tool downloads", "Static build", "Swap round trip", "Smoke", "Smoke mutants",
            "Log in to ghcr.io", "Distributions", "Validation record",
            "Upload the validation record", "Complete job"]
        | map(select(. != $step or $mark != "absent"))
        | map({name: ., status: "completed",
            conclusion: (if . == $step then $mark else "success" end)})
        | {jobs: [{name: "test, static build and smoke (\($arch))", status: "completed",
            conclusion: $job, steps: .},
            {name: "fmt and clippy", status: "completed", conclusion: "success", steps: []}]}' \
        > "$answers/repos_o_r_actions_runs_$1_attempts_1_jobs_per_page_100.json"
}

# validated_at <scope> <head> <base>: full, successful runs of both legs at that head.
validated_at() {
    artifact "$1" x86_64 11 501 "$2"
    artifact "$1" aarch64 12 502 "$2"
    record 11 "$1" x86_64 "$2" "$3" 20261101.360.1 501
    record 12 "$1" aarch64 "$2" "$3" 20261111.182.1 502
    jobs 501 x86_64
    jobs 502 aarch64
}

failures=0
cases=0
# expect <true|false> <name> <reason> <variable=value>...: runs one leg's decision in that
# environment; its summary must contain the reason.
expect() {
    cases=$((cases + 1))
    expected=$1
    name=$2
    reason=$3
    shift 3
    : > "$scratch/output"
    : > "$scratch/summary"
    env -i PATH="$PATH" HOME="$HOME" CI_SCOPE_API="$answers" GITHUB_OUTPUT="$scratch/output" \
        GITHUB_STEP_SUMMARY="$scratch/summary" REPOSITORY=o/r ARCH=x86_64 ImageOS=ubuntu24 \
        ImageVersion=20261101.360.1 "$@" "$script" decide > "$scratch/log" 2>&1 || true
    got=$(sed -n 's/^run=//p' "$scratch/output")
    if [ "$got" = "$expected" ] && grep -qF -- "$reason" "$scratch/summary"; then
        echo "ok: $name"
    else
        echo "FAIL: $name: expected run=$expected because \"$reason\"; got run=${got:-nothing}" >&2
        sed 's/^/    /' "$scratch/summary" >&2
        failures=$((failures + 1))
    fi
}

reused="reusing an earlier validation"
skipped="skipped: only Markdown or LICENSE changed"
since="files other than Markdown or LICENSE changed since"
unconfirmed="GitHub's job records do not show full, successful runs"
pr="EVENT=pull_request PR_NUMBER=7"

fresh
validated_at pr7 "$validated" "$base"
# shellcheck disable=SC2086 # $pr holds two assignments
{
    expect false "reuse after a Markdown change" "$reused" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown"
    expect false "reuse after a LICENSE change" "$reused" \
        $pr BASE_SHA="$base" HEAD_SHA="$license"
    expect false "reuse for the validated head itself" "$reused" \
        $pr BASE_SHA="$base" HEAD_SHA="$validated"
    expect false "the aarch64 leg reuses on its own image" "$reused" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown" ARCH=aarch64 ImageVersion=20261111.182.1
    expect true "a script changed since the validated head" "$since" \
        $pr BASE_SHA="$base" HEAD_SHA="$script_change"
    expect true "a code file renamed into Markdown" "$since" \
        $pr BASE_SHA="$base" HEAD_SHA="$renamed"
    expect true "the validated head is not an ancestor" "is not an ancestor of" \
        $pr BASE_SHA="$base" HEAD_SHA="$rewritten"
    expect true "the base moved" "the base moved from" \
        $pr BASE_SHA="$moved" HEAD_SHA="$markdown"
    expect true "the runner image changed" "the runner image changed from" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown" ImageVersion=20261108.120.1
    expect true "the runner names no image" "does not name its image" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown" ImageVersion=
    expect true "the aarch64 leg on the x86_64 image" "the runner image changed from" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown" ARCH=aarch64
}

fresh
artifact pr7 x86_64 11 501 "$validated"
record 11 pr7 x86_64 "$validated" "$base" 20261101.360.1 501
jobs 501 x86_64
listing=repos_o_r_actions_artifacts_per_page_100_name_validation-pr7-aarch64.json
echo '{"artifacts": []}' > "$answers/$listing"
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "no record of the other architecture" "no validation record of aarch64 for" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
artifact pr7 aarch64 12 502 "$script_change"
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "the other record is at another head" "no validation record of aarch64 for" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
jobs 501 x86_64 success Smoke skipped
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a reused run cannot be a baseline" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
jobs 502 aarch64 success Distributions failure
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a failed step of the other leg" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
jobs 501 x86_64 success Distributions absent
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a required step is missing" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
jobs 501 x86_64 failure
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "the job did not succeed" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
# Two jobs of the leg's name, the second one failed: the leg is not confirmed.
jobs_file=$answers/repos_o_r_actions_runs_501_attempts_1_jobs_per_page_100.json
jq '.jobs += [.jobs[0] | .conclusion = "failure"]' "$jobs_file" > "$scratch/jobs.json"
mv "$scratch/jobs.json" "$jobs_file"
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "two jobs of the leg's name" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
# A record whose attempt is not a number, which would steer the lookup of its jobs elsewhere:
# the answer at the steered path is a success, and must not be consulted.
jq '.run_attempt = "1/../2"' "$answers/artifact-11.json" > "$scratch/record.json"
mv "$scratch/record.json" "$answers/artifact-11.json"
jobs_file=$answers/repos_o_r_actions_runs_501_attempts_1_jobs_per_page_100.json
cp "$jobs_file" "$answers/repos_o_r_actions_runs_501_attempts_1_.._2_jobs_per_page_100.json"
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a record whose attempt is not a number" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
record 11 pr7 x86_64 "$validated" "$base" 20261101.360.1 999
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a record that names another run" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
record 11 pr7 x86_64 "$script_change" "$base" 20261101.360.1 501
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a record at another head than its run" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
record 11 main x86_64 "$validated" "$base" 20261101.360.1 501
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a record of another scope" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
record 12 pr7 aarch64 "$validated" "$moved" 20261111.182.1 502
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "the records name different bases" "name different bases" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
artifact pr7 x86_64 11 501 "$validated" true
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "an expired record" "no validation record of x86_64 for" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
# An older full run at the base, and the newer one at V: the newer one counts.
cat > "$answers/repos_o_r_actions_artifacts_per_page_100_name_validation-pr7-x86_64.json" <<EOF
{"artifacts": [
    {"id": 11, "expired": false, "created_at": "2026-11-13T10:00:00Z",
        "workflow_run": {"id": 501, "head_sha": "$validated"}},
    {"id": 9, "expired": false, "created_at": "2026-11-12T10:00:00Z",
        "workflow_run": {"id": 401, "head_sha": "$base"}}]}
EOF
record 9 pr7 x86_64 "$base" "$base" 20261101.360.1 401
jobs 401 x86_64
# shellcheck disable=SC2086 # $pr holds two assignments
expect false "the newest of two validated runs" "$reused" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
validated_at pr7 "$validated" "$base"
record 11 pr7 aarch64 "$validated" "$base" 20261101.360.1 501
# shellcheck disable=SC2086 # $pr holds two assignments
expect true "a record of the other architecture under this one's name" "$unconfirmed" \
    $pr BASE_SHA="$base" HEAD_SHA="$markdown"

fresh
# shellcheck disable=SC2086 # $pr holds two assignments
{
    expect true "the API answers nothing" "cannot be listed" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown"
    expect true "a pull request with code, without records" \
        "changes files other than Markdown or LICENSE" \
        $pr BASE_SHA="$base" HEAD_SHA="$markdown"
    expect false "a pull request of Markdown only, without records" "$skipped" \
        $pr BASE_SHA="$base" HEAD_SHA="$docs_pull_request"
    expect false "a Markdown-only pull request on an unnamed runner image" "$skipped" \
        $pr BASE_SHA="$base" HEAD_SHA="$docs_pull_request" ImageVersion=
    expect true "a pull request without a base" "the pull request names no base" \
        $pr BASE_SHA= HEAD_SHA="$docs_pull_request"
    expect true "a pull request without a number" "the event does not say what to compare" \
        EVENT=pull_request BASE_SHA="$base" HEAD_SHA="$docs_pull_request"
    expect true "an unknown event" "the event does not say what to compare" \
        EVENT=workflow_dispatch HEAD_SHA="$docs_pull_request"
    expect false "a Markdown-only pull request after its base moved" "$skipped" \
        $pr BASE_SHA="$moved" HEAD_SHA="$docs_pull_request"
    expect true "a pull request that renames a code file into Markdown" \
        "changes files other than Markdown or LICENSE" \
        $pr BASE_SHA="$base" HEAD_SHA="$renamed_pull_request"
    expect true "no architecture" "the event does not say what to compare" \
        $pr BASE_SHA="$base" HEAD_SHA="$docs_pull_request" ARCH=
    expect true "no repository" "the event does not say what to compare" \
        $pr BASE_SHA="$base" HEAD_SHA="$docs_pull_request" REPOSITORY=
    expect true "no head" "the event does not say what to compare" \
        $pr BASE_SHA="$base" HEAD_SHA=
}

fresh
validated_at main "$validated" ""
expect false "a push of Markdown after a validated main commit" "$reused" \
    EVENT=push BEFORE_SHA="$validated" HEAD_SHA="$markdown"
expect true "a push of a script" "changes files other than Markdown or LICENSE" \
    EVENT=push BEFORE_SHA="$markdown" HEAD_SHA="$script_change"

fresh
expect false "a push of Markdown only, without records" "$skipped" \
    EVENT=push BEFORE_SHA="$validated" HEAD_SHA="$markdown"
expect true "a push of two commits, code in the first" \
    "changes files other than Markdown or LICENSE" \
    EVENT=push BEFORE_SHA="$base" HEAD_SHA="$markdown"
expect true "a push that names no commit before it" "the push names no commit before it" \
    EVENT=push BEFORE_SHA=0000000000000000000000000000000000000000 HEAD_SHA="$markdown"
expect true "a push that changes no file" "changes no file" \
    EVENT=push BEFORE_SHA="$markdown" HEAD_SHA="$markdown"

# A full run's record names its scope, leg, head, base, runner image and run.
cases=$((cases + 1))
recorded=$(env -i PATH="$PATH" EVENT=pull_request PR_NUMBER=7 ARCH=aarch64 \
    HEAD_SHA="$validated" BASE_SHA="$base" ImageOS=ubuntu24 ImageVersion=20261111.182.1 \
    GITHUB_RUN_ID=502 GITHUB_RUN_ATTEMPT=2 "$script" record | jq -c .)
wanted=$(jq -nc --arg head "$validated" --arg base "$base" '{scope: "pr7", arch: "aarch64",
    head: $head, base: $base, image_os: "ubuntu24", image_version: "20261111.182.1",
    run_id: 502, run_attempt: 2}')
if [ "$recorded" = "$wanted" ]; then
    echo "ok: a full run's record"
else
    echo "FAIL: a full run's record: $recorded" >&2
    failures=$((failures + 1))
fi

if [ "$failures" -gt 0 ]; then
    echo "ci-scope-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "ci-scope-cases: all $cases cases passed"
