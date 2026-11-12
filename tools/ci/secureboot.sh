#!/usr/bin/env bash
# Secure Boot and TPM unlock, on one disk, with a firmware that has Secure
# Boot and starts without keys (Setup Mode) and a TPM 2.0 chip (swtpm):
#   1. systemd-boot enrolls the XeeonOS keys by itself (a virtual machine is
#      "safe") and restarts; XeeonOS then starts with Secure Boot on. The disk
#      password goes on, then the TPM unlock.
#   2. The next start needs no password: the TPM opens the disk.
#   3. An update to the next version (signed with the same key) still starts
#      with Secure Boot, and the TPM still opens the disk.
#   4. Another TPM (the disk moved to another computer): the password is asked.
# (The builds of a pull request are signed with a throwaway key, see
# tools/secureboot.sh.)
source "$(dirname "$0")/common.sh"
fresh_disk
sb=(--secure-boot --efivars out/vars.fd --tpm out/tpm)

# A throwaway signing key for the test update (as in update.sh).
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output build/update-next/SHA256SUMS.gpg build/update-next/SHA256SUMS
keyring=(--credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg")

echo "::group::1. Keys enrolled, Secure Boot on; password and TPM unlock on"
tools/boot-test.sh "${sb[@]}" --allow-reboot \
    --run "/bin/sh -c 'xctl status | grep Secure.Boot | sed s,^,SB-,; printf test-disk-pw | xctl encryption on --password-stdin; printf test-disk-pw | xctl encryption tpm on --password-stdin; xctl encryption status | sed s,^,STATUS-,'" \
    --expect "Custom Secure Boot keys successfully enrolled" \
    --expect "SB-Secure Boot: on" \
    --expect "Recovery key" \
    --expect "TPM unlock is on" \
    --expect "STATUS-tpm: on" \
    out/disk.raw out/serial-secureboot-1-enroll.log
echo "::endgroup::"

echo "::group::2. The TPM opens the disk: no password asked"
tools/boot-test.sh "${sb[@]}" \
    --run "/bin/sh -c 'xctl status; xctl status --check'" \
    --expect "Encrypted: +yes, password on, opened by the TPM" \
    --expect "Secure Boot: on" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-secureboot-2-tpm.log
echo "::endgroup::"

echo "::group::3. Update to the next version, which starts with Secure Boot and the TPM"
tools/boot-test.sh "${sb[@]}" --http build/update-next "${keyring[@]}" \
    --run "/bin/sh -c 'systemctl start network-online.target && /usr/bin/xctl update --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring'" \
    --expect "XeeonOS $NEXT is installed" \
    out/disk.raw out/serial-secureboot-3-update.log
tools/boot-test.sh "${sb[@]}" \
    --run "/bin/sh -c 'xctl status; xctl status --check'" \
    --expect "XeeonOS $NEXT \(build" \
    --expect "Encrypted: +yes, password on, opened by the TPM" \
    --expect "Secure Boot: on" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-secureboot-3-next.log
echo "::endgroup::"

echo "::group::4. Another TPM: the password is asked"
TIMEOUT=600 tools/boot-test.sh --secure-boot --efivars out/vars.fd --tpm out/other-tpm \
    --expect "Please enter passphrase for disk" \
    out/disk.raw out/serial-secureboot-4-other-tpm.log
echo "::endgroup::"
