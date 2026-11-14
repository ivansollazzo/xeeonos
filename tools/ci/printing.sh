#!/usr/bin/env bash
# Extension printing (extensions/printing, included): CUPS starts on demand and can
# add a printer, network printers can be found (Avahi, .local names), and
# the Printers page of GNOME Settings can manage printers (cups-pk-helper).
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/printing-test.sh <<'SCRIPT'
# Started by the extension at boot (xctl extensions activate), and pulled in by
# sockets.target through the link the extension carries in /usr.
systemctl is-active cups.socket | sed s,^,SOCKET-,
test -L /usr/lib/systemd/system/sockets.target.wants/cups.socket && echo WANTS-link
# CUPS writes its printers in /etc/cups: a real directory, not the link.
test ! -L /etc/cups && test -w /etc/cups/cupsd.conf && echo ETC-writable
lpstat -r | sed s,^,CUPS-,
lpadmin -p ci -E -v socket://127.0.0.1:9100 -m raw 2>&1 | sed s,^,LPADMIN-,
lpstat -p ci | sed s,^,PRINTER-,
systemctl is-active avahi-daemon.service | sed s,^,AVAHI-,
grep '^hosts:' /etc/nsswitch.conf | sed s,^,NSS-,
test -x /usr/lib/cups-pk-helper-mechanism && echo PKHELPER-present
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential printing-test=out/printing-test.sh \
    --run "/bin/sh /run/credentials/@system/printing-test" \
    --expect "SOCKET-active" \
    --expect "WANTS-link" \
    --expect "ETC-writable" \
    --expect "CUPS-scheduler is running" \
    --expect "PRINTER-printer ci is idle" \
    --expect "AVAHI-active" \
    --expect "NSS-hosts:.*mdns_minimal" \
    --expect "PKHELPER-present" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-printing.log
