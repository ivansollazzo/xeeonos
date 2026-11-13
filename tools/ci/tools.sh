#!/usr/bin/env bash
# Module tools (mkosi.conf.d/tools): the SSH client, the SSH server
# installed but off (and turned on and off with xctl ssh-server), and the user ID ranges for containers without root
# (extensions podman, distrobox) given to the user created by the setup.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/tools-test.sh <<'SCRIPT'
grep -c '^test:' /etc/subuid | sed s,^,SUBUID-,
grep -c '^test:' /etc/subgid | sed s,^,SUBGID-,
ssh -V 2>&1 | sed s,^,SSH-,
systemctl is-enabled sshd.service | sed s,^,SSHD-,
systemctl is-active sshd.service | sed s,^,SSHD-,
test -e /etc/ssh/sshd_config && echo SSHD-CONFIG-present
# AppImage: the FUSE 2 library and fusermount (setuid, for users).
test -e /usr/lib/libfuse.so.2 && echo FUSE2-present
test -u /usr/bin/fusermount && echo FUSERMOUNT-setuid
# xctl ssh-server: the service and the firewall port together.
xctl ssh-server | sed s,^,BEFORE-,
xctl ssh-server on
systemctl is-active sshd.service | sed s,^,ON-,
xctl firewall | sed s,^,ON-,
xctl ssh-server | sed s,^,ON-,
xctl ssh-server off
systemctl is-active sshd.service | sed s,^,OFF-,
xctl firewall | sed s,^,OFF-,
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential tools-test=out/tools-test.sh \
    --run "/bin/sh /run/credentials/@system/tools-test" \
    --expect "SUBUID-1" \
    --expect "SUBGID-1" \
    --expect "SSH-OpenSSH_" \
    --expect "SSHD-disabled" \
    --expect "SSHD-inactive" \
    --expect "SSHD-CONFIG-present" \
    --expect "FUSE2-present" \
    --expect "FUSERMOUNT-setuid" \
    --expect "BEFORE-SSH server: +off" \
    --expect "SSH server: on \(port 22/tcp open\)" \
    --expect "ON-active" \
    --expect "ON-Open ports: +22/tcp" \
    --expect "ON-SSH server: +on" \
    --expect "SSH server: off \(port 22/tcp closed\)" \
    --expect "OFF-inactive" \
    --expect "OFF-Open ports: +none" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-tools.log
