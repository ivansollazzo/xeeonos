#!/usr/bin/env bash
# From the stable version people have today to this build (pull requests to
# stable, and the publishing of every stable version: docs/updates.md). One
# story on one disk, made from the files published as
# ghcr.io/ivansollazzo/xeeonos:stable, as the installer writes them:
#   1. the old version starts; some settings and data are written, and the
#      CI's optional test extension (ci-hello, published with it and signed
#      with the XeeonOS key) is downloaded and turned on
#   2. still on the old version, its own xctl updates to this build over HTTP
#      (a throwaway key): the extension turned on comes along, for the new
#      version, from the same source
#   3. this build starts from the other slot, the data are there, the
#      extension works, the seal is intact
# Needs tools/mkdisk.sh of the repository xeeonos-extensions, in
# $EXTENSIONS_TOOLS (its stable branch, checked out by the CI).
source "$(dirname "$0")/common.sh"
tools=${EXTENSIONS_TOOLS:?EXTENSIONS_TOOLS: the tools/ directory of xeeonos-extensions}
repo=${XEEONOS_REPOSITORY:-ivansollazzo/xeeonos}
new=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
[[ -e $new/xeeonos_$VERSION.efi ]] || { echo "no update files of $VERSION in build/" >&2; exit 1; }

echo "::group::0. The stable version published today"
old_dir=out/old-stable
mkdir -p "$old_dir"
token=$(curl -sSf "https://ghcr.io/token?scope=repository:$repo:pull" | jq -r .token)
auth=(-H "Authorization: Bearer $token")
manifest=$(curl -sSfL "${auth[@]}" -H "Accept: application/vnd.oci.image.manifest.v1+json" \
    "https://ghcr.io/v2/$repo/manifests/stable")
jq -r '.layers[] | "\(.digest) \(.annotations["org.opencontainers.image.title"])"' <<< "$manifest" |
    while read -r digest file; do
        [[ $file =~ ^[A-Za-z0-9._-]+$ ]] || { echo "odd file name: $file" >&2; exit 1; }
        curl -sSfL "${auth[@]}" "https://ghcr.io/v2/$repo/blobs/$digest" -o "$old_dir/$file"
        [[ $(sha256sum "$old_dir/$file" | cut -d' ' -f1) == "${digest#sha256:}" ]] ||
            { echo "$file: wrong checksum" >&2; exit 1; }
    done
old=$(find "$old_dir" -name 'xeeonos_*.efi' -printf '%f\n' | sed 's/^xeeonos_//; s/\.efi$//')
echo "Stable today: XeeonOS $old; this build: XeeonOS $VERSION"
[[ -e $old_dir/xeeonos_$old.ci-hello.xext ]] || { echo "no ci-hello in $old" >&2; exit 1; }
WORK=$RUNNER_TEMP "$tools/mkdisk.sh" "$old_dir" "$old" out/disk.raw
echo "::endgroup::"

# The new version's files, signed with a throwaway key for step 2.
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output "$new/SHA256SUMS.gpg" "$new/SHA256SUMS"

step() { # <number> <title> <script> <boot-test options...>
    local n=$1 title=$2 script=$3
    shift 3
    echo "::group::$n. $title"
    printf '%s\n' "$script" > "out/stable-update-$n.sh"
    tools/boot-test.sh --credential "step=out/stable-update-$n.sh" \
        --run "/bin/sh /run/credentials/@system/step" "$@" \
        out/disk.raw "out/serial-stable-update-$n.log"
    echo "::endgroup::"
}

step 1 "XeeonOS $old: data and an extension" "
xctl status
mkdir -p /var/lib/xeeonos-ci && echo kept > /var/lib/xeeonos-ci/mark
echo kept > /etc/xeeonos-ci.conf
systemctl start network-online.target
xctl extensions enable ci-hello --from http://10.0.2.2:8080
xeeonos-hello | sed s,^,HELLO-,
" --http "$old_dir" \
    --expect "XeeonOS $old \(build" \
    --expect "Signature: valid" \
    --expect "Extension ci-hello: on" \
    --expect "HELLO-Hello from the ci-hello extension"

step 2 "XeeonOS $old updates to $VERSION" "
systemctl start network-online.target
xctl update --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring
" --http "$new" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --expect "XeeonOS $VERSION is installed"

step 3 "XeeonOS $VERSION starts" "
xctl status
cat /var/lib/xeeonos-ci/mark | sed s,^,VAR-,
cat /etc/xeeonos-ci.conf | sed s,^,ETC-,
xeeonos-hello | sed s,^,HELLO-,
xctl extensions list --offline
xctl status --check
" \
    --expect "XeeonOS $VERSION \(build" \
    --expect "$VERSION +good, running" \
    --expect "VAR-kept" \
    --expect "ETC-kept" \
    --expect "HELLO-Hello from the ci-hello extension" \
    --expect "ci-hello +on " \
    --expect "check: sealed system OK"
