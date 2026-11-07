#!/bin/sh
# Unpacks a capture stream from capture.sh into a fixture directory: the captured files
# under `root/`, and the output of the host's own tools under `truth/` for cross-checks.
#
#   ./unpack.sh host.capture <fixture directory>
set -eu

[ $# -eq 2 ] || {
    echo "usage: unpack.sh <capture> <fixture directory>" >&2
    exit 1
}
out=$2
mkdir -p "$out/root" "$out/truth"
while read -r marker kind path data; do
    [ "$marker" = "@@@" ] || continue
    case $kind in
        DIR) mkdir -p "$out/root/$path" ;;
        EMPTY)
            mkdir -p "$(dirname "$out/root/$path")"
            : > "$out/root/$path"
            ;;
        FILE)
            mkdir -p "$(dirname "$out/root/$path")"
            printf '%s' "${data:-}" | base64 -d > "$out/root/$path"
            ;;
        LINK)
            mkdir -p "$(dirname "$out/root/$path")"
            ln -sfn "${data:-}" "$out/root/$path"
            ;;
        CMD) printf '%s' "${data:-}" | base64 -d > "$out/truth/$path.txt" ;;
    esac
done < "$1"
