#!/usr/bin/env bash
# What extensions need and cannot be on with (Requires=, Conflicts=,
# docs/extensions.md), with the CI's test extensions ci-hello, ci-needs
# (Requires=ci-hello) and ci-clash (Conflicts=ci-hello), from a signed source:
#   1. turning on ci-needs asks first (no terminal: refused), with --yes it
#      turns on ci-hello too; turning off ci-hello asks first, since ci-needs
#      needs it; turning on ci-clash turns off ci-needs and ci-hello
#   2. after a restart: ci-clash is on, ci-hello and ci-needs stay off
source "$(dirname "$0")/common.sh"
fresh_disk
system=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
for n in ci-hello ci-needs ci-clash; do
    [[ -e $system/xeeonos_$VERSION.$n.xext ]] || { echo "no $n.xext in $system" >&2; exit 1; }
done

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
    printf '%s\n' "$script" > "out/relations-$n.sh"
    tools/boot-test.sh --credential "ext-test=out/relations-$n.sh" \
        --run "/bin/sh /run/credentials/@system/ext-test" "$@" \
        out/disk.raw "out/serial-relations-$n.log"
    echo "::endgroup::"
}

src="--from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring"
step 1 "
systemctl start network-online.target
xctl extensions list $src | sed s,^,REMOTE-,
xctl extensions enable ci-needs $src </dev/null 2>&1 | sed s,^,NEEDS-ASK-,
xctl extensions list --offline | grep -Eq 'ci-(needs|hello) +on' || echo NEEDS-REFUSED
xctl extensions enable ci-needs --yes $src
xctl extensions list --offline | sed s,^,ON-,
xctl extensions disable ci-hello </dev/null 2>&1 | sed s,^,DISABLE-ASK-,
xctl extensions enable ci-clash --yes $src | sed s,^,CLASH-,
xctl extensions list --offline | sed s,^,AFTER-,
" --http "$system" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --expect "REMOTE- +ci-needs +.*needs: ci-hello" \
    --expect "REMOTE- +ci-clash +.*not with: ci-hello" \
    --expect "NEEDS-ASK-(xctl: )?ci-needs needs: ci-hello\." \
    --expect "NEEDS-ASK-.*give --yes to turn them on" \
    --expect "NEEDS-REFUSED" \
    --expect "Extension ci-hello: on\." \
    --expect "Extension ci-needs: on\." \
    --expect "ON- +ci-hello +on " \
    --expect "ON- +ci-needs +on " \
    --expect "DISABLE-ASK-(xctl: )?ci-needs need ci-hello: they will be turned off too\." \
    --expect "CLASH-ci-clash cannot be on together with: .*ci-hello" \
    --expect "CLASH-Extension ci-needs: off\." \
    --expect "CLASH-Extension ci-hello: off\." \
    --expect "CLASH-Extension ci-clash: on\." \
    --expect "AFTER- +ci-clash +on " \
    --expect "AFTER- +ci-hello +off " \
    --expect "AFTER- +ci-needs +off "

step 2 "
xctl extensions list --offline | sed s,^,BOOT2-,
test -e /usr/share/xeeonos-ci/clash && echo CLASH-MERGED
test -e /usr/share/xeeonos-ci/needs || echo NEEDS-NOT-MERGED
command -v xeeonos-hello >/dev/null || echo HELLO-NOT-MERGED
xctl status --check
" \
    --expect "BOOT2- +ci-clash +on " \
    --expect "BOOT2- +ci-hello +off " \
    --expect "CLASH-MERGED" \
    --expect "NEEDS-NOT-MERGED" \
    --expect "HELLO-NOT-MERGED" \
    --expect "check: sealed system OK"
