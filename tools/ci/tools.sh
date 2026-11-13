#!/usr/bin/env bash
# Module tools (mkosi.conf.d/tools): containers without root for the user
# created by the setup (podman, distrobox), the SSH client, and the SSH
# server installed but off.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/tools-test.sh <<'SCRIPT'
grep -c '^test:' /etc/subuid | sed s,^,SUBUID-,
grep -c '^test:' /etc/subgid | sed s,^,SUBGID-,
test -e /etc/containers/policy.json && echo POLICY-present
# podman as the user, without root (no login session here: no systemd).
su - test -c 'podman --cgroup-manager=cgroupfs info --format "{{.Host.Security.Rootless}} {{.Store.GraphDriverName}}"' 2>&1 |
    sed s,^,PODMAN-,
distrobox --version | sed s,^,DISTROBOX-,
ssh -V 2>&1 | sed s,^,SSH-,
systemctl is-enabled sshd.service | sed s,^,SSHD-,
systemctl is-active sshd.service | sed s,^,SSHD-,
test -e /etc/ssh/sshd_config && echo SSHD-CONFIG-present
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential tools-test=out/tools-test.sh \
    --run "/bin/sh /run/credentials/@system/tools-test" \
    --expect "SUBUID-1" \
    --expect "SUBGID-1" \
    --expect "POLICY-present" \
    --expect "PODMAN-true " \
    --expect "DISTROBOX-distrobox: [0-9]" \
    --expect "SSH-OpenSSH_" \
    --expect "SSHD-disabled" \
    --expect "SSHD-inactive" \
    --expect "SSHD-CONFIG-present" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-tools.log
