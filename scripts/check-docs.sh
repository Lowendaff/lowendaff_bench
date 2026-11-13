#!/bin/sh
# Checks the rules of the documentation (README "Gates", Rules) in the repository it runs in:
#
# - every Chinese term of glossary.md still appears, character for character, in the program's
#   source (crates/*/src and plugins/*/src, comment lines left out), so that the glossary follows
#   what the program prints;
# - README.md, CONTRIBUTING.md and the pull request template hold no CJK characters;
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

header='| Chinese | English | Where it appears |'
# The table's lines: the header, then the rows; the separator line is left out.
rows=$(sed -n '/^|/p' glossary.md | grep -v -E '^\|[-:| ]+\|$' || true)
if [ "$(printf '%s\n' "$rows" | sed -n 1p)" != "$header" ]; then
    fail "glossary.md: the table must start with the header \"$header\""
fi
terms=0
while IFS= read -r row; do
    [ -n "$row" ] || continue
    term=${row#| }
    term=${term%% |*}
    if [ -z "$term" ]; then
        fail "glossary.md: a row without a term: $row"
        continue
    fi
    terms=$((terms + 1))
    status=0
    source=$(LC_ALL=C git grep -h -F -e "$term" -- ':(glob)crates/*/src/**' \
        ':(glob)plugins/*/src/**') || status=$?
    if [ "$status" -gt 1 ]; then
        echo "check-docs: git grep failed with status $status" >&2
        exit 2
    fi
    if ! printf '%s\n' "$source" | grep -v -q -E '^[[:space:]]*(//|$)'; then
        fail "glossary.md: \"$term\" no longer appears in crates/*/src or plugins/*/src"
    fi
done <<EOF
$(printf '%s\n' "$rows" | sed 1d)
EOF
if [ "$terms" -eq 0 ]; then
    fail "glossary.md: the table has no terms"
fi

cjk='(*UTF)[\x{3000}-\x{30ff}\x{3400}-\x{4dbf}\x{4e00}-\x{9fff}\x{ac00}-\x{d7af}'
cjk="$cjk\\x{f900}-\\x{faff}\\x{ff00}-\\x{ffef}]"
if grep_found -n -P "$cjk" -- README.md CONTRIBUTING.md .github/pull_request_template.md >&2
then
    fail "the lines above hold CJK characters; these documents are English"
fi

references='(*UTF)docs/design|\x{a7}|\b(D|P|S|DK|DH|CP|ME|R|H)-[0-9]{2}\b|\(design\b'
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
echo "check-docs: $terms glossary terms appear in the source, and the documents follow the rules"
