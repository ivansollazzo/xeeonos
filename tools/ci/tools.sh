#!/usr/bin/env bash
# Module tools (mkosi.conf.d/tools): the command line tools and Python
# (with pip in a venv, and pipx), the SSH client, the SSH server
# installed but off (and turned on and off with xctl ssh-server), and the user ID ranges for containers without root
# (extensions podman, distrobox) given to the user created by the setup.
# Also the module drivers (file system tools, firmware), in every system.
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
# The command line tools: every command is there.
missing=""
for c in which tree rsync zip unzip 7z ncdu lsof bc jq vim man tmux screen \
    lsusb dmidecode nvme sensors powertop htop strace wget dig traceroute mtr \
    iw ethtool hostname nc whois git python pip pipx; do
    command -v "$c" >/dev/null || missing="$missing $c"
done
echo "COMMANDS-missing:${missing:- none}"
# Module drivers: file system tools, firmware and the Wi-Fi regulatory database.
missing=""
for c in mkfs.fat mkfs.exfat ntfsfix mkfs.ext4 usb_modeswitch; do
    command -v "$c" >/dev/null || missing="$missing $c"
done
for f in /usr/lib/firmware/regulatory.db /usr/lib/firmware/intel/sof \
    /usr/share/alsa/ucm2 /usr/lib/firmware/cirrus /usr/lib/firmware/brcm; do
    ls -d "$f"* >/dev/null 2>&1 || missing="$missing $f"
done
echo "DRIVERS-missing:${missing:- none}"
man -w ls | sed s,^,MAN-,
test -e /usr/share/bash-completion/completions/systemctl && echo COMPLETION-present
# Python: libraries go in a venv (pip inside it), never into /usr.
python -m venv /tmp/venv && /tmp/venv/bin/pip --version | sed s,^,VENV-,
pip install --dry-run requests 2>&1 | grep -q externally-managed && echo PIP-system-protected
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential tools-test=out/tools-test.sh \
    --run "/bin/sh /run/credentials/@system/tools-test" \
    --expect "COMMANDS-missing: none" \
    --expect "DRIVERS-missing: none" \
    --expect "MAN-/usr/share/man/man1/ls.1" \
    --expect "COMPLETION-present" \
    --expect "VENV-pip .* from /tmp/venv/" \
    --expect "PIP-system-protected" \
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
