#!/bin/bash
# Prints what YABS's own format_speed and format_iops make of the values that
# tests/disk.rs checks, one tab-separated line each: kind, input, output.
# yabs-format.txt holds the output; see README.md for how it was made.
#
# Usage: yabs-format.sh <yabs.sh>
# yabs.sh must be the file of YABS commit 21f62408ecde9b23d4eec878f46a8292f1944aa0
# (v2026-10-31). Its functions call `awk`, so the awk on PATH decides the result.
set -eu

[ $# -eq 1 ] || { echo "usage: yabs-format.sh <yabs.sh>" >&2; exit 2; }
echo "dd6414ad37460cb6c872a15c79e09dd52cf7fc7f417f664dfba6473b775debf1  $1" |
    sha256sum -c - >/dev/null
# The two functions, unchanged.
eval "$(awk '/^function format_speed/,/^}/; /^function format_iops/,/^}/' "$1")"

# Boundaries of the units and of the truncation, then pseudo-random values of every
# magnitude from a fixed linear congruential generator.
speeds="0 1 2 3 9 10 99 100 975 976 977 978 999 1000 1023 1024 9765 9766 97656 97657
976561 976562 976563 976564 1048575 1048576 2097152 9765625 30000000 97656250"
iops="0 1 9 10 99 100 999 1000 1001 1049 1050 1099 1100 9999 10000 10049 10050 99999
100000 100049 100050 999999 1000000 1000050 9999999"
x=12345
step() {
    x=$(( (1103515245 * x + 12345) % 2147483648 ))
}
for i in $(seq 1 600); do
    # The generator's low bits repeat quickly, so each value joins the high bits of two steps.
    step
    high=$(( x >> 8 ))
    step
    value=$(( high * 8388608 + (x >> 8) ))
    magnitude=$(( 10 ** (i % 9 + 1) ))
    if [ $(( i % 2 )) -eq 0 ]; then
        speeds="$speeds $(( value % magnitude ))"
    else
        iops="$iops $(( value % (magnitude / 10 + 1) ))"
    fi
done

for value in $speeds; do
    printf 'speed\t%s\t%s\n' "$value" "$(format_speed "$value")"
done
for value in $iops; do
    printf 'iops\t%s\t%s\n' "$value" "$(format_iops "$value")"
done
