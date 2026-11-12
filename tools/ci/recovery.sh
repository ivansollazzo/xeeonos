#!/usr/bin/env bash
# Recovery: restart into it, then erase everything.
#
# Four boots sharing the UEFI variables: the setup creates a user; the running
# system asks for Recovery at the next boot only (one-shot boot menu choice);
# in Recovery, "erase" asks for a factory reset; at the boot after that the
# data partition is new: the user is gone.
# (While the test command runs, xeeonos-recovery.target is still being
# started: it waits for the units it pulls in, the command included. So the
# test looks for its start job, not for "active".)
# (The commands write "xeeonos[.]recovery": the command itself is on the
# kernel command line, and must not contain the Recovery word.)
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 user=test password=test-password admin=yes > out/setup.txt
vars=(--efivars out/vars.fd)

tools/boot-test.sh "${vars[@]}" --setup out/setup.txt \
    --run "/bin/sh -c 'id test && echo USER-CREATED'" \
    --expect "USER-CREATED" \
    out/disk.raw out/serial-recovery-0-setup.log
tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'grep -qw xeeonos[.]recovery /proc/cmdline && echo WRONG-BOOT; xctl recovery --yes'" \
    --expect "Restarting into Recovery \(xeeonos_${VERSION}\.efi@recovery\)" \
    out/disk.raw out/serial-recovery-1-restart.log
tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'grep -qw xeeonos[.]recovery /proc/cmdline && echo IN-RECOVERY; systemctl list-jobs --no-legend | grep xeeonos-recovery.target | sed s,^,TARGET-JOB-,; command -v nmtui && echo HAS-NMTUI; xctl recovery erase --yes'" \
    --expect "IN-RECOVERY" \
    --expect "TARGET-JOB-.*xeeonos-recovery.target +start" \
    --expect "HAS-NMTUI" \
    --expect "All data will be erased at the restart" \
    out/disk.raw out/serial-recovery-2-erase.log
tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'grep -qw xeeonos[.]recovery /proc/cmdline && echo WRONG-BOOT; id test || echo USER-GONE; xctl recovery < /dev/null; xctl status --check'" \
    --expect "USER-GONE" \
    --expect "Restart into Recovery now" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-recovery-3-erased.log
! grep -q WRONG-BOOT out/serial-recovery-1-restart.log out/serial-recovery-3-erased.log
