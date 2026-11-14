#!/usr/bin/env bash
# Extension scanner (extensions/scanner, included): SANE with the driverless
# backend (sane-airscan) on, ipp-usb ready for USB devices, Avahi running to
# find network scanners, and Document Scanner. The VM has no scanner: the
# test checks that everything is in place and that SANE starts and looks.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/scanner-test.sh <<'SCRIPT'
scanimage --version | sed s,^,SANE-,
grep -qx airscan /etc/sane.d/dll.d/airscan && echo AIRSCAN-on
test -e /usr/lib/sane/libsane-airscan.so.1 && echo AIRSCAN-backend
# No scanner in the VM: SANE starts, looks (network included) and says so.
timeout 60 scanimage -L 2>&1 | sed s,^,LIST-,
systemctl cat ipp-usb.service >/dev/null && echo IPPUSB-present
systemctl is-active avahi-daemon.service | sed s,^,AVAHI-,
test -x /usr/bin/simple-scan && echo SIMPLESCAN-present
xctl extensions list --offline | grep scanner | sed s,^,EXT-,
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential scanner-test=out/scanner-test.sh \
    --run "/bin/sh /run/credentials/@system/scanner-test" \
    --expect "SANE-scanimage .*backend version" \
    --expect "AIRSCAN-on" \
    --expect "AIRSCAN-backend" \
    --expect "LIST-.*No scanners were identified" \
    --expect "IPPUSB-present" \
    --expect "AVAHI-active" \
    --expect "SIMPLESCAN-present" \
    --expect "EXT-.*scanner +on .*included, special" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-scanner.log
