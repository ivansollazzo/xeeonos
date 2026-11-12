#!/usr/bin/env bash
# Disk encryption, on a fresh disk. The data partition is always encrypted,
# with an empty password. Turn the disk password on (instant), then off again
# with the recovery key it printed; the next start must still need no password.
source "$(dirname "$0")/common.sh"
fresh_disk
tools/boot-test.sh \
    --run "/bin/sh -c 'xctl status | grep Encrypted; printf test-disk-pw | xctl encryption on --password-stdin | tee /run/on.txt; xctl encryption status | sed s,^,AFTER-ON-,; grep -oE [a-z-]{71} /run/on.txt | head -n1 | xctl encryption off --password-stdin; xctl encryption status | sed s,^,AFTER-OFF-,'" \
    --expect "Encrypted: +yes, password off" \
    --expect "Recovery key" \
    --expect "AFTER-ON-encryption: on" \
    --expect "Encryption is off" \
    --expect "AFTER-OFF-encryption: off" \
    out/disk.raw out/serial-encryption-1-on-off.log
tools/boot-test.sh out/disk.raw out/serial-encryption-2-after.log
