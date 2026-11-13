#!/usr/bin/env bash
# Does a pull request touch what a CI test depends on (tools/ci/affects.txt)?
#
# Usage: tools/ci/affected.sh <test> < changed-files
# Exit 0 (run the test) or 1 (not touched: skip it), with the reason.
set -euo pipefail
test=${1:?usage: affected.sh <test> < changed-files}
table=$(dirname "$0")/affects.txt

mine=() all=() every=()
while read -r name paths; do
    [[ -z $name || $name == \#* ]] && continue
    # shellcheck disable=SC2206
    list=($paths)
    every+=("${list[@]}")
    if [[ $name == "*" ]]; then
        all+=("${list[@]}")
    elif [[ $name == "$test" ]]; then
        mine+=("${list[@]}")
    fi
done < "$table"

if ! grep -q "^$test " "$table"; then
    echo "$test: no line in affects.txt, it always runs"
    exit 0
fi

starts() { # <file> <path>... (a path ending in / is a directory: anything
    # under it; any other path is that file only, so mkosi.conf does not
    # match mkosi.conf.d/...)
    local file=$1 p
    shift
    for p in "$@"; do
        if [[ $p == */ ]]; then
            [[ $file == "$p"* ]] && return 0
        else
            [[ $file == "$p" ]] && return 0
        fi
    done
    return 1
}

while read -r file; do
    [[ -z $file ]] && continue
    if starts "$file" "${all[@]}"; then
        echo "$test: $file changed (every test depends on it)"
        exit 0
    fi
    if starts "$file" "${mine[@]}"; then
        echo "$test: $file changed"
        exit 0
    fi
    # Documentation alone touches no test; anything else unknown, all.
    if [[ $file == docs/* || $file == *.md ]]; then
        continue
    fi
    if ! starts "$file" "${every[@]}"; then
        echo "$test: $file changed and affects.txt does not know it: every test runs"
        exit 0
    fi
done
echo "$test: not touched by this pull request"
exit 1
