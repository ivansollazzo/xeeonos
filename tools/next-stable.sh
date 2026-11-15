#!/usr/bin/env bash
# The number the next stable version gets: YY.MM.S, the month of today and
# S one more than the last stable version of this month (the tags
# v<YY.MM>.<S> on GitHub, made when a stable version is published). Used by
# tools/version.sh (pull requests to stable: <next>-rc.<run>) and by the
# workflow that publishes stable (stable.yml).
# XEEONOS_MONTH (YY.MM) overrides the month, for tests.
set -euo pipefail
cd "$(dirname "$0")/.."
month=${XEEONOS_MONTH:-$(date -u +%y.%-m)}
# Asked to GitHub, not to the local clone: a CI checkout has no tags.
last=$(git ls-remote --tags --refs origin "v$month.*" |
    sed -n "s|.*refs/tags/v${month//./\\.}\\.\\([0-9][0-9]*\\)\$|\\1|p" |
    sort -n | tail -n1)
echo "$month.$(( ${last:-0} + 1 ))"
