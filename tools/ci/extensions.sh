#!/usr/bin/env bash
# System extensions (xctl extensions, docs/extensions.md). One story on one disk:
#   1. the included ones (flatpak, printing, scanner) are on; a special one is not
#      turned off without asking; ci-hello (optional, the CI's test
#      extension, unknown to the system until then) is listed among those
#      that can be downloaded, then downloaded from a signed source and
#      turned on AT ONCE: it works and its hooks ran (Setup, Boot, Disable
#      masks a unit) in the same boot; flatpak is turned off at once
#   2. after a restart: all still so, Setup ran only once; an included
#      extension cannot be removed; ci-hello is removed at once (its
#      Cleanup runs, its program is gone, the masked unit is back), then
#      installed again from its file (not verified: with a warning) with
#      --no-apply, so only from the next restart
#   3. the extension from the file works, and its Setup ran again
# (The real optional extensions are built and checked in the repository
# xeeonos-extensions.)
source "$(dirname "$0")/common.sh"
fresh_disk
system=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
[[ -e $system/xeeonos_$VERSION.ci-hello.xext ]] || { echo "no ci-hello.xext in $system" >&2; exit 1; }

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
xctl extensions list --offline
xctl extensions list --offline | grep -q ci-hello || echo NOT-KNOWN-ci-hello
xctl extensions disable flatpak </dev/null 2>&1 | sed s,^,ASK-,
systemctl start network-online.target
xctl extensions list --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring | sed s,^,REMOTE-,
xctl extensions enable ci-hello --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring
xeeonos-hello | sed s,^,HELLO-,
cat /var/lib/ci-hello/setup | sed s,^,SETUP-,
cat /run/ci-hello-boot | sed s,^,BOOT-,
systemctl is-enabled systemd-tmpfiles-clean.timer | sed s,^,MASK-,
xctl extensions list --offline | sed s,^,AFTER-,
xctl extensions disable flatpak --yes
command -v flatpak >/dev/null || echo FLATPAK-gone
" --http "$system" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --expect "flatpak +on +.*\(included, special\)" \
    --expect "printing +on +.*\(included, special\)" \
    --expect "scanner +on +.*\(included, special\)" \
    --expect "NOT-KNOWN-ci-hello" \
    --expect "REMOTE-Can be downloaded for XeeonOS" \
    --expect "REMOTE- +ci-hello +Test extension of the CI.*\(optional\)" \
    --expect "ASK-.*special extension and there is no terminal" \
    --expect "Signature: valid" \
    --expect "Extension ci-hello: on\." \
    --expect "HELLO-Hello from the ci-hello extension" \
    --expect "SETUP-set up" \
    --expect "BOOT-ci-hello" \
    --expect "MASK-masked-runtime" \
    --expect "AFTER- +ci-hello +on +.*\(optional\)" \
    --expect "Warning: flatpak is a special extension" \
    --expect "Extension flatpak: off\." \
    --expect "FLATPAK-gone"

step 2 "
xeeonos-hello | sed s,^,HELLO-,
grep -c 'set up' /var/lib/ci-hello/setup | sed s,^,SETUP-TIMES-,
cat /run/ci-hello-boot | sed s,^,BOOT-,
command -v flatpak >/dev/null || echo FLATPAK-still-gone
command -v lpstat >/dev/null && echo PRINTING-there
xctl extensions remove printing 2>&1 | sed s,^,REMOVE-,
xctl status --check
cp /var/lib/xeeonos/extensions/$VERSION/ci-hello.xext /var/tmp/ci-hello.xext
xctl extensions remove ci-hello
cat /var/lib/ci-hello/cleanup | sed s,^,CLEANUP-,
test -e /var/lib/ci-hello/setup || echo SETUP-undone
command -v xeeonos-hello >/dev/null || echo HELLO-gone
systemctl is-enabled systemd-tmpfiles-clean.timer | sed s,^,UNMASKED-,
xctl extensions install /var/tmp/ci-hello.xext </dev/null 2>&1 | sed s,^,ASK-,
xctl extensions install /var/tmp/ci-hello.xext --yes --no-apply
command -v xeeonos-hello >/dev/null || echo HELLO-not-yet
xctl extensions list --offline
" \
    --expect "HELLO-Hello from the ci-hello extension" \
    --expect "SETUP-TIMES-1" \
    --expect "BOOT-ci-hello" \
    --expect "FLATPAK-still-gone" \
    --expect "PRINTING-there" \
    --expect "REMOVE-.*cannot be removed" \
    --expect "Extensions: +ci-hello, printing" \
    --expect "check: sealed system OK" \
    --expect "Extension ci-hello: cleaned up" \
    --expect "CLEANUP-cleaned up" \
    --expect "SETUP-undone" \
    --expect "Extension ci-hello: removed\." \
    --expect "HELLO-gone" \
    --expect "UNMASKED-(static|enabled)" \
    --expect "ASK-.*not verified and there is no terminal" \
    --expect "Warning: ci-hello .* comes from a file: it is not signed" \
    --expect "Extension ci-hello: installed; on, from the next restart" \
    --expect "HELLO-not-yet" \
    --expect "ci-hello +on \(from the next restart\) +.*\(installed from a file, not verified\)"

step 3 "
xeeonos-hello | sed s,^,HELLO-,
cat /var/lib/ci-hello/setup | sed s,^,SETUP-AGAIN-,
xctl extensions list --offline
xctl status --check
" \
    --expect "HELLO-Hello from the ci-hello extension" \
    --expect "SETUP-AGAIN-set up" \
    --expect "ci-hello +on +.*\(installed from a file, not verified\)" \
    --expect "flatpak +off " \
    --expect "Extensions: +ci-hello, printing" \
    --expect "check: sealed system OK"
