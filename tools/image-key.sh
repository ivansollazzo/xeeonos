#!/usr/bin/env bash
# A fingerprint of everything that goes into the images: the hash of every
# file in the repository except the tests and the documentation (the build
# workflow counts: it says how the images are built). Two commits with the
# same key build the same images, so CI can reuse the images of a failed run
# instead of building them again (see .github/workflows/build.yml).
set -euo pipefail
cd "$(dirname "$0")/.."
# git's own hash of every file ("<mode> <hash> <stage>\t<path>"): it covers
# symbolic links (their target) and the executable bit too.
git ls-files --stage -z |
    grep -zEv $'\t(docs/|tools/ci/|\\.github/dependabot\\.yml$|\\.github/workflows/snapshot\\.yml$|tools/(boot-test\\.sh|mkiso\\.sh|snapshot-report\\.py)$)|\\.md$' |
    sort -z |
    sha256sum |
    cut -d' ' -f1
