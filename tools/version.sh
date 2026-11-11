#!/usr/bin/env bash
# Print the full XeeonOS version for this build.
#
# The base version lives in mkosi.version (e.g. 0.1.0). The channel is the
# branch name: `experimental`, or the last part of a version branch such as
# `1.x/beta`. Every other branch (pull requests, work branches, local builds)
# is `dev`. The CI run number makes every build unique and ordered:
#
#   experimental -> 0.1.0-experimental.42
#   1.x/beta     -> 1.0.0-beta.57
#   1.x/stable   -> 1.0.0
#   anything     -> 0.1.0-dev.0
#
# With --channel, print the channel tag instead (the registry tag that
# `xctl update` follows): experimental, <N>.x-alpha|beta|stable, or nothing
# for development builds.
set -euo pipefail
cd "$(dirname "$0")/.."

base=$(< mkosi.version)
branch=${GITHUB_HEAD_REF:-${GITHUB_REF_NAME:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo dev)}}
build=${GITHUB_RUN_NUMBER:-0}

case $branch in
    experimental) channel=experimental ;;
    [0-9]*.x/alpha|[0-9]*.x/beta|[0-9]*.x/stable) channel=${branch#*/} ;;
    *) channel=dev ;;
esac

if [[ ${1:-} == --channel ]]; then
    case $channel in
        experimental) echo experimental ;;
        dev) ;;
        *) echo "${branch%%/*}-$channel" ;;
    esac
    exit 0
fi

if [[ $channel == stable ]]; then
    echo "$base"
else
    echo "$base-$channel.$build"
fi
