#!/usr/bin/env bash
# System extensions (xctl ext, docs/extensions.md). One story on one disk:
#   1. the included ones (flatpak, printing) are on; a special one is not
#      turned off without asking; docker (optional) is downloaded from a
#      signed source
#   2. docker works; flatpak is turned off (with its warning); an included
#      extension cannot be removed
#   3. flatpak is gone, printing is still there; docker is removed, then
#      installed again from its file (not verified: with a warning)
#   4. the extension from the file works
source "$(dirname "$0")/common.sh"
fresh_disk
system=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
[[ -e $system/xeeonos_$VERSION.docker.xext ]] || { echo "no docker.xext in $system" >&2; exit 1; }

# A throwaway signing key, as for the update test.
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output "$system/SHA256SUMS.gpg" "$system/SHA256SUMS"

step() { # <number> <script> <boot-test options...>
    local n=$1 script=$2
    shift 2
    echo "::group::$n"
    printf '%s\n' "$script" > "out/extensions-$n.sh"
    tools/boot-test.sh --credential "ext-test=out/extensions-$n.sh" \
        --run "/bin/sh /run/credentials/@system/ext-test" "$@" \
        out/disk.raw "out/serial-extensions-$n.log"
    echo "::endgroup::"
}

step 1 "
xctl ext list
xctl ext disable flatpak </dev/null 2>&1 | sed s,^,ASK-,
systemctl start network-online.target
xctl ext enable docker --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring
xctl ext list | sed s,^,AFTER-,
" --http "$system" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --expect "flatpak +on +.*\(included, special\)" \
    --expect "printing +on +.*\(included, special\)" \
    --expect "docker +off +.*\(optional\)" \
    --expect "ASK-.*special extension and there is no terminal" \
    --expect "Signature: valid" \
    --expect "Extension docker: on, from the next restart" \
    --expect "AFTER-docker +on \(from the next restart\)"

step 2 "
docker --version | sed s,^,DOCKER-,
getent group docker | sed s,^,GROUP-,
systemctl is-active docker.socket | sed s,^,SOCKET-,
xctl ext list
xctl ext disable flatpak --yes
xctl ext remove printing 2>&1 | sed s,^,REMOVE-,
xctl status --check
" \
    --expect "DOCKER-Docker version" \
    --expect "GROUP-docker:" \
    --expect "SOCKET-active" \
    --expect "docker +on +.*\(optional\)" \
    --expect "Warning: flatpak is a special extension" \
    --expect "Extension flatpak: off, from the next restart" \
    --expect "REMOVE-.*cannot be removed" \
    --expect "Extensions: +docker, flatpak, printing" \
    --expect "check: sealed system OK"

step 3 "
command -v flatpak >/dev/null || echo FLATPAK-gone
command -v lpstat >/dev/null && echo PRINTING-there
cp /var/lib/xeeonos/extensions/$VERSION/docker.xext /var/tmp/docker.xext
xctl ext remove docker
xctl ext install /var/tmp/docker.xext </dev/null 2>&1 | sed s,^,ASK-,
xctl ext install /var/tmp/docker.xext --yes
xctl ext list
xctl status --check
" \
    --expect "FLATPAK-gone" \
    --expect "PRINTING-there" \
    --expect "Extension docker: removed" \
    --expect "ASK-.*not verified and there is no terminal" \
    --expect "Warning: docker .* comes from a file: it is not signed" \
    --expect "Extension docker: installed; on, from the next restart" \
    --expect "flatpak +off " \
    --expect "check: sealed system OK"

step 4 "
docker --version | sed s,^,DOCKER-,
xctl ext list
xctl status --check
" \
    --expect "DOCKER-Docker version" \
    --expect "docker +on +.*\(installed from a file, not verified\)" \
    --expect "Extensions: +docker, printing" \
    --expect "check: sealed system OK"
