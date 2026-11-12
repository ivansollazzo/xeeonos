#!/usr/bin/env bash
# Print the full XeeonOS version for this build.
#
# The base version lives in mkosi.version (e.g. 0.1.0). The channel is the
# branch name: `experimental`, or a release branch `<major>.<minor>-<stage>`
# (`1.0-stable`, `1.1-beta`, `1.2-alpha`): the same name for the branch, the
# channel and the registry tag. Every other branch (pull requests, work
# branches, local builds) is `dev`. The CI run number makes every build unique
# and ordered:
#
#   experimental -> 0.1.0-experimental.42
#   1.1-beta     -> 1.1.0-beta.57
#   1.0-stable   -> 1.0.0 (then 1.0.1, ...: set in mkosi.version)
#   anything     -> 0.1.0-dev.0
#
# On a release branch, mkosi.version must belong to it (1.0.x on 1.0-*).
#
# With --channel, print the channel instead (the registry tag that
# `xctl update` follows): the branch name, or nothing for development builds.
set -euo pipefail
cd "$(dirname "$0")/.."

base=$(< mkosi.version)
branch=${GITHUB_HEAD_REF:-${GITHUB_REF_NAME:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo dev)}}
build=${GITHUB_RUN_NUMBER:-0}

stage=dev
if [[ $branch == experimental ]]; then
    stage=experimental
elif [[ $branch =~ ^([0-9]+\.[0-9]+)-(alpha|beta|stable)$ ]]; then
    stage=${BASH_REMATCH[2]}
    if [[ $base != "${BASH_REMATCH[1]}".* ]]; then
        echo "mkosi.version ($base) does not belong to the branch $branch" >&2
        exit 1
    fi
fi

if [[ ${1:-} == --channel ]]; then
    case $stage in
        dev) ;;
        *) echo "$branch" ;;
    esac
    exit 0
fi

if [[ $stage == stable ]]; then
    echo "$base"
else
    echo "$base-$stage.$build"
fi
