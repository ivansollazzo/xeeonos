#!/usr/bin/env bash
# Extension flatpak (extensions/flatpak, included): Flatpak with Flathub set up for the
# whole system, visible to the user of the setup, and GNOME Software with its
# Flatpak support.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/flatpak-test.sh <<'SCRIPT'
flatpak --version | sed s,^,VERSION-,
flatpak remotes --system --columns=name,url | sed s,^,SYSTEM-,
su - test -c 'flatpak remotes --columns=name,options' | sed s,^,USER-,
# Installed applications appear in the menus through these directories.
su - test -c 'echo $XDG_DATA_DIRS' | sed s,^,DATADIRS-,
test -x /usr/bin/gnome-software && echo SOFTWARE-present
find /usr/lib/gnome-software -name '*flatpak*' | sed s,^,BACKEND-,
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential flatpak-test=out/flatpak-test.sh \
    --run "/bin/sh /run/credentials/@system/flatpak-test" \
    --expect "VERSION-Flatpak [0-9]" \
    --expect "SYSTEM-flathub.*https://dl.flathub.org/repo/" \
    --expect "USER-flathub.*system" \
    --expect "DATADIRS-.*/var/lib/flatpak/exports/share" \
    --expect "SOFTWARE-present" \
    --expect "BACKEND-.*flatpak" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-flatpak.log
