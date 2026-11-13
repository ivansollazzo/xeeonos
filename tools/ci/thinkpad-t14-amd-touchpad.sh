#!/usr/bin/env bash
# Extension thinkpad-t14-amd-touchpad: turned on, its patched modules are the
# ones in use (QEMU's PS/2 mouse loads psmouse). No ThinkPad here: whether
# the touchpad moves to SMBus is checked on the laptop.
source "$(dirname "$0")/common.sh"
fresh_disk
system=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output "$system/SHA256SUMS.gpg" "$system/SHA256SUMS"

echo "::group::1. Turn it on"
tools/boot-test.sh --http "$system" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --run "/bin/sh -c 'systemctl start network-online.target && xctl ext enable thinkpad-t14-amd-touchpad --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring'" \
    --expect "Extension thinkpad-t14-amd-touchpad: on, from the next restart" \
    out/disk.raw out/serial-t14-1.log
echo "::endgroup::"

echo "::group::2. The patched modules are in use"
cat > out/t14-test.sh <<'SCRIPT'
modinfo -n psmouse | sed s,^,PATH-,
modinfo -n i2c_piix4 | sed s,^,PATH-,
zstdcat "$(modinfo -n psmouse)" | grep -aq LEN2073 && echo ALLOWLIST-LEN2073
systemctl is-active xeeonos-t14-touchpad.service | sed s,^,RELOAD-,
grep -q O /sys/module/psmouse/taint && echo LOADED-patched-psmouse
command -v thinkpad-t14-amd-touchpad-check | sed s,^,CHECK-,
xctl status --check
SCRIPT
tools/boot-test.sh --credential t14-test=out/t14-test.sh \
    --run "/bin/sh /run/credentials/@system/t14-test" \
    --expect "PATH-/usr/lib/modules/.*/updates/psmouse.ko.zst" \
    --expect "PATH-/usr/lib/modules/.*/updates/i2c-piix4.ko.zst" \
    --expect "ALLOWLIST-LEN2073" \
    --expect "RELOAD-active" \
    --expect "LOADED-patched-psmouse" \
    --expect "CHECK-/usr/bin/thinkpad-t14-amd-touchpad-check" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-t14-2.log
echo "::endgroup::"
