#!/bin/sh
# Cases for scripts/check-docs.sh: changes to a scratch repository, each of which the checker must
# accept, or reject with the expected message.
#
# Usage: scripts/check-docs-cases.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
checker=$here/check-docs.sh
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
git -c init.defaultBranch=main init -q "$scratch/repo"
cd "$scratch/repo"
git config user.name check-docs
git config user.email check-docs@example.invalid
git config commit.gpgsign false

# write <path> <text>: writes the text, with a final newline, and stages the file.
write() {
    mkdir -p "$(dirname "$1")"
    printf '%s\n' "$2" > "$1"
    git add "$1"
}

glossary_head='# Glossary

| Chinese | English | Where it appears |
|---------|---------|------------------|'
write glossary.md "$glossary_head
| 完成 | Completed | an item's last line |
| CPU 性能 | CPU performance | an item's title |"
write crates/bench/src/lib.rs 'pub const DONE: &str = "完成";'
write plugins/cpu/src/lib.rs 'pub const TITLE: &str = "CPU 性能";'
write README.md '# readme

Two × 3 at 25 °C — fine… These are not CJK. The design of the tool, and its designated runs.'
write CONTRIBUTING.md '# Contributing

## Design documents

- the design, the risks that the owner accepted, and the plan.'
write .github/pull_request_template.md '## Background'
write crates/bench-host/tests/fixtures/host/proc/cpuinfo 'model name : Xeon S-05 (§9) docs/design'
git commit -q -m 'the base of every case'
base=$(git rev-parse HEAD)

failures=0
cases=0
# check <accept|reject> <name> [<part of the expected message> [<text the output must not hold>]]
check() {
    cases=$((cases + 1))
    if output=$(PATH=${fake_path:-$PATH} "$checker" 2>&1); then
        got=accept
    else
        got=reject
    fi
    if [ "$got" != "$1" ]; then
        echo "FAIL: $2: expected $1, got $got: $output" >&2
        failures=$((failures + 1))
    elif [ -n "${3:-}" ] && ! printf '%s\n' "$output" | grep -q -F -e "$3"; then
        echo "FAIL: $2: the output lacks \"$3\": $output" >&2
        failures=$((failures + 1))
    elif [ -n "${4:-}" ] && printf '%s\n' "$output" | grep -q -F -e "$4"; then
        echo "FAIL: $2: the output holds \"$4\": $output" >&2
        failures=$((failures + 1))
    else
        echo "ok: $2"
    fi
    git reset -q --hard "$base"
    git clean -q -f -d -x
}

check accept 'the base, with other non-ASCII characters and the word design'

write glossary.md "$glossary_head
| 完成 | Completed | an item's last line |
| 未知 | Unknown | a value |
| CPU 性能 | CPU performance | an item's title |"
check reject 'a term that the source does not have' '"未知" no longer appears'

write glossary.md "$glossary_head
| 未知 | Unknown | a value |
| 完成 | Completed | an item's last line |"
check reject 'the first term, missing from the source' '"未知" no longer appears'

write glossary.md "$glossary_head
| --- | --- | --- |
| 完成 | Completed | an item's last line |"
check accept 'a second separator, written with spaces'

write glossary.md "$glossary_head
| 跳过 | Skipped | an item's last line |"
write crates/bench/src/lib.rs '// 跳过
/// 跳过
    //! 跳过'
check reject 'a term only in comments' '"跳过" no longer appears'

write glossary.md "$glossary_head
| 跳过 | Skipped | an item's last line |"
write crates/bench/tests/flow.rs 'const SKIPPED: &str = "跳过";'
write crates/bench/examples/run.rs 'const SKIPPED: &str = "跳过";'
check reject 'a term only in tests and examples' '"跳过" no longer appears'

write glossary.md "$glossary_head
| 跳过 | Skipped | an item's last line |"
write crates/bench-host/tests/fixtures/src/lib.rs 'const SKIPPED: &str = "跳过";'
check reject 'a term in a path that only looks like source' '"跳过" no longer appears'

write glossary.md "$glossary_head
| 跳过 | Skipped | an item's last line |"
write plugins/cpu/src/report/terminal.rs '    let skipped = "跳过：没有网络"; // a reason follows'
check accept 'a term in a module deeper in the source'

write glossary.md '# Glossary

| 中文 | English | Where |
|------|---------|-------|
| 完成 | Completed | an item'"'"'s last line |'
check reject 'a glossary with another header' 'must start with the header'

write glossary.md "$glossary_head"
check reject 'a glossary without terms' 'has no terms' 'a row without a term'

write glossary.md "$glossary_head
|  | Completed | an item's last line |"
check reject 'a row without a term' 'a row without a term'

for file in README.md CONTRIBUTING.md .github/pull_request_template.md; do
    write "$file" '运行环境 KVM 虚拟机'
    check reject "Chinese in $file" "$file:1:"
done
write README.md 'テスト'
check reject 'Japanese kana' 'README.md:1:'
write README.md '㐀'
check reject 'a rare ideograph' 'README.md:1:'
# U+F900, written as bytes: editors may normalize it to the ordinary ideograph U+8C48.
printf '\357\244\200\n' > README.md
git add README.md
check reject 'a compatibility ideograph' 'README.md:1:'
write README.md '한국어'
check reject 'Korean' 'README.md:1:'
write README.md 'ok，fine'
check reject 'fullwidth punctuation' 'README.md:1:'

write scripts/build.sh '# The flags follow docs/design/cpu.md.'
check reject 'a path to the design documents' 'scripts/build.sh:1:'
write crates/bench/src/lib.rs 'pub const DONE: &str = "完成"; // (§5.3)'
check reject 'a section sign' 'crates/bench/src/lib.rs:1:'
for id in D-16 P-11 S-22 DK-02 DH-08 CP-07 ME-09 R-19 H-01; do
    write README.md "Because of $id, it runs last."
    check reject "the decision id $id" 'README.md:1:'
done
write README.md 'A Xeon D-1541, the P-256 curve, SHA-256, x86-64, HDD-12,
S-1, S-123, CP-1 and ME-1.'
check accept 'text that only looks like a decision id'
write .github/workflows/ci.yml '# Smoke skips it (design cpu, 9.3).'
check reject 'a parenthetical that starts with design' '.github/workflows/ci.yml:1:'
write README.md 'Its runs are (designated) by name.'
check accept 'a parenthetical with a longer word'

write crates/bench-host/tests/fixtures/README.md 'Captures of real hosts (P-11).'
check reject 'a fixture README' 'crates/bench-host/tests/fixtures/README.md:1:'
write plugins/memory/tests/fixtures/owner/README.md 'Recorded on the owner'"'"'s server (§5.6).'
check reject 'a fixture README in a subdirectory' 'owner/README.md:1:'
write crates/bench-host/tests/fixtures/capture.sh '# Never the serial (S-06).'
check reject 'a fixture script' 'capture.sh:1:'
write plugins/disk/tests/fixtures/fio/output.txt 'P-11 § docs/design (design'
check accept 'fixture data'

printf 'CP-07\000\001' > tool.bin
git add tool.bin
check accept 'a binary file'
mkdir -p scripts
cp "$checker" scripts/check-docs.sh
cp "$here/check-docs-cases.sh" scripts/check-docs-cases.sh
git add scripts
check accept 'the checker and its cases, which hold the patterns'

# A git whose grep fails, as one without Perl-compatible patterns would, must stop the check:
# first only for the Perl-compatible searches, then only for the fixed-string ones.
mkdir "$scratch/bin"
real_git=$(command -v git)
for option in -P -F; do
    # shellcheck disable=SC2016 # $* and $@ belong to the fake git, which expands them
    printf '#!/bin/sh\ncase " $* " in *" grep "*" %s "*) exit 128 ;; esac\nexec %s "$@"\n' \
        "$option" "$real_git" > "$scratch/bin/git"
    chmod +x "$scratch/bin/git"
    fake_path="$scratch/bin:$PATH"
    check reject "a git grep $option that fails" 'git grep failed'
    fake_path=
done

write glossary.md "$glossary_head
| 完.成 | Completed | an item's last line |"
write crates/bench/src/lib.rs 'pub const DONE: &str = "完X成";'
check reject 'a term with a pattern character, matched literally' '"完.成" no longer appears'

if [ "$failures" -gt 0 ]; then
    echo "check-docs-cases: $failures of $cases cases failed" >&2
    exit 1
fi
echo "check-docs-cases: all $cases cases passed"
