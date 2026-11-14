#!/usr/bin/env bash
# Extension flatpak (extensions/flatpak, included): Flatpak with Flathub set up for the
# whole system, visible to the user of the setup, and GNOME Software with its
# Flatpak support; the default applications of GNOME (preinstall list,
# service, the setup's "no", xctl status).
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
# The default applications (desktop list, extension service): the CI
# answers "no" at setup (tools/boot-test.sh), so nothing is downloaded.
grep -c '^\[Flatpak Preinstall ' /usr/share/flatpak/preinstall.d/gnome.preinstall | sed s,^,LIST-,
test -L /usr/lib/systemd/system/timers.target.wants/xeeonos-flatpak-preinstall.timer && echo TIMER-link
flatpak preinstall --help >/dev/null && echo PREINSTALL-supported
test -e /var/lib/xeeonos/flatpak-preinstall-off && echo OFF-chosen
/usr/lib/xeeonos/flatpak-preinstall && echo SCRIPT-quiet
xctl status | grep '^Apps:' | sed s,^,STATUS-,
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
    --expect "LIST-15" \
    --expect "TIMER-link" \
    --expect "PREINSTALL-supported" \
    --expect "OFF-chosen" \
    --expect "SCRIPT-quiet" \
    --expect "STATUS-Apps: +not installed \(chosen at the first start\)" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-flatpak.log
