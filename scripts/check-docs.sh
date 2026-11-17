#!/bin/sh
# Checks the rules of the documentation (README "Gates", Rules) in the repository it runs in:
#
# - no tracked file holds CJK characters: the code, its tests and their data, the scripts and the
#   documents are English. Only a catalog of another language, crates/bench/src/locale/<language>/,
#   holds that language's words; the English catalog, en/, is checked like every other file;
# - no tracked file points into the design documents, which are kept outside the repository: no
#   path under docs/design, no section sign, no decision id such as CP-07 and no parenthetical
#   that starts with "design". Fixture data is recorded output and is left out; the READMEs and
#   scripts beside it are checked.
#
# A sentence such as "as the design says" cannot be told from other uses of the word without
# false alarms, so it is not checked. This script and its cases define and test the patterns, so
# they are left out of the last check.
#
# Usage: scripts/check-docs.sh
set -eu

cd "$(git rev-parse --show-toplevel)"
problems=0
fail() {
    echo "check-docs: $*" >&2
    problems=$((problems + 1))
}

# grep_found <git grep arguments>: 0 when git grep found a line, 1 when it found none. Any other
# status is an error, such as a git without Perl-compatible patterns, and stops the check.
grep_found() {
    status=0
    LC_ALL=C git grep "$@" || status=$?
    case $status in
        0 | 1) return "$status" ;;
        *)
            echo "check-docs: git grep failed with status $status" >&2
            exit 2
            ;;
    esac
}

cjk='(*UTF)[\x{3000}-\x{30ff}\x{3400}-\x{4dbf}\x{4e00}-\x{9fff}\x{ac00}-\x{d7af}'
cjk="$cjk\\x{f900}-\\x{faff}\\x{ff00}-\\x{ffef}]"
if grep_found -n -I -P "$cjk" -- . ':(exclude,glob)crates/bench/src/locale/*/**' >&2 ||
    grep_found -n -I -P "$cjk" -- crates/bench/src/locale/en >&2
then
    fail "the lines above hold CJK characters; only a catalog of another language may"
fi

references='(*UTF)docs/design|\x{a7}|\b(D|P|S|DK|DH|CP|ME|R|H|NW)-[0-9]{2}\b|\(design\b'
if grep_found -n -I -P "$references" -- . ':(exclude,glob)**/tests/fixtures/**' \
    ':(exclude)scripts/check-docs.sh' ':(exclude)scripts/check-docs-cases.sh' >&2 \
    || grep_found -n -I -P "$references" -- ':(glob)**/tests/fixtures/**/README.md' \
        ':(glob)**/tests/fixtures/**/*.sh' >&2
then
    fail "the lines above point into the design documents; explain the reason in place instead"
fi

if [ "$problems" -gt 0 ]; then
    echo "check-docs: $problems problem(s)" >&2
    exit 1
fi
echo "check-docs: the documents follow the rules"
