
#!/usr/bin/env bash
# Print the full XeeonOS version for this build (docs/updates.md).
#
# Versions follow the month: YY.MM (26.11, then 27.1 in January). The branch
# is the channel and the registry tag:
#
#   testing      -> 26.11-testing.57        (every merge publishes one)
#   stable       -> 26.11.2                 only from the stable promotion
#                   workflow (release.yml), which sets XEEONOS_VERSION
#   anything     -> 26.11-dev.0             (pull requests, local builds)
#
# The number at the end is the CI run number: every build is unique and
# ordered. XEEONOS_MONTH (YY.MM) overrides the month, for tests.
#
# With --channel, print the channel instead (the registry tag that
# `xctl update` follows): testing or stable, or nothing for
# builds that are not published.
set -euo pipefail
cd "$(dirname "$0")/.."

month=${XEEONOS_MONTH:-$(date -u +%y.%-m)}
branch=${GITHUB_HEAD_REF:-${GITHUB_REF_NAME:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo dev)}}
build=${GITHUB_RUN_NUMBER:-0}
release=${XEEONOS_VERSION:-}

if [[ -n $release ]]; then
    if [[ ! $release =~ ^[0-9]{2}\.[0-9]{1,2}\.[0-9]+$ ]]; then
        echo "XEEONOS_VERSION ($release) is not YY.MM.S" >&2
        exit 1
    fi
    stage=release
else
    case $branch in
        testing) stage=testing ;;
        stable) stage=rc ;;
        *) stage=dev ;;
    esac
fi

if [[ ${1:-} == --channel ]]; then
    case $stage in
        testing) echo "$stage" ;;
        release) echo stable ;;
    esac
    exit 0
fi

if [[ $stage == release ]]; then
    echo "$release"
else
    echo "$month-$stage.$build"
fi
