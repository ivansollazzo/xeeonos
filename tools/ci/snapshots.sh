#!/usr/bin/env bash
# Snapshots of the data: take one, change some files, restore it from
# Recovery, and the files are back as they were.
# Three boots sharing the UEFI variables (Recovery is a one-shot boot choice).
# (The commands write "xeeonos[.]recovery": the command itself is on the
# kernel command line, and must not contain the Recovery word.)
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 user=test password=test-password admin=yes > out/setup.txt
vars=(--efivars out/vars.fd)

tools/boot-test.sh "${vars[@]}" --setup out/setup.txt \
    --run "/bin/sh -c 'echo first > /home/test/kept; xctl snapshots create; rm /home/test/kept; echo second > /home/test/added; xctl snapshots | sed s,^,LIST-,; xctl recovery --yes'" \
    --expect "Snapshot .*-manual taken" \
    --expect "LIST-.*-manual" \
    --expect "Restarting into Recovery" \
    out/disk.raw out/serial-snapshots-1-take.log
tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'grep -qw xeeonos[.]recovery /proc/cmdline && echo IN-RECOVERY; xctl snapshots restore \$(xctl snapshots | grep manual) --yes'" \
    --expect "IN-RECOVERY" \
    --expect "Restored settings and files from .*-manual" \
    out/disk.raw out/serial-snapshots-2-restore.log
tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'cat /home/test/kept | sed s,^,KEPT-,; test -e /home/test/added || echo ADDED-GONE; id test && echo USER-OK; xctl snapshots | grep -c before-restore | sed s,^,SAVED-,; xctl status --check'" \
    --expect "KEPT-first" \
    --expect "ADDED-GONE" \
    --expect "USER-OK" \
    --expect "SAVED-1" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-snapshots-3-after.log
