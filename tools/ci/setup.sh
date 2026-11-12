#!/usr/bin/env bash
# First-boot setup (unattended), on a fresh disk; no Arch tools in the image.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=it_IT.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
tools/boot-test.sh \
    --setup out/setup.txt \
    --run "/bin/sh -c 'id -nG test; cat /etc/locale.conf /etc/vconsole.conf; test -e /usr/bin/pacman || echo NO-PACMAN; test -e /etc/arch-release || test -e /usr/share/factory/etc/arch-release || echo NO-ARCH-RELEASE; locale -a | grep -q de_DE.utf8 && echo HAS-GERMAN; xctl packages systemd-libs; xctl install-system --disk /dev/vda --yes; xctl setup < /dev/null 2>&1 | sed s,^,AGAIN-,; xctl status --check'" \
    --expect "XeeonOS setup: created user test \(administrator\)" \
    --expect "test wheel" \
    --expect "LANG=it_IT.UTF-8" \
    --expect "KEYMAP=it" \
    --expect "NO-PACMAN" \
    --expect "NO-ARCH-RELEASE" \
    --expect "HAS-GERMAN" \
    --expect "systemd-libs [0-9]" \
    --expect "already installed and running here" \
    --expect "AGAIN-.*already set up" \
    --expect "Based on: +Arch Linux \(snapshot " \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-setup.log
