#!/usr/bin/env bash
# Hibernation: the VM hibernates into the swap file and powers off; at the
# next start the initrd unlocks the data partition and resumes the session,
# so the command of the first boot simply goes on (the one of the second
# boot runs only if the resume failed). A file in /dev/shm (memory only)
# proves that the session survived.
# Two boots sharing the UEFI variables (systemd notes where the image is in
# HibernateLocation, and the next start in LoaderEntryOneShot).
source "$(dirname "$0")/common.sh"
fresh_disk
vars=(--efivars out/vars.fd)

tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'echo alive > /dev/shm/session; echo HIBERNATING; systemctl hibernate; sleep 5; cat /dev/shm/session | sed s,^,RESUMED-,; xctl status --check'" \
    --expect "HIBERNATING" \
    out/disk.raw out/serial-hibernate-1-sleep.log
tools/boot-test.sh "${vars[@]}" \
    --run "/bin/sh -c 'echo NOT-RESUMED'" \
    --expect "RESUMED-alive" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-hibernate-2-resume.log
