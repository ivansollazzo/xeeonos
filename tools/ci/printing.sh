#!/usr/bin/env bash
# Module printing (mkosi.profiles/printing): CUPS starts on demand and can
# add a printer, network printers can be found (Avahi, .local names), and
# Plasma has its Printers page.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/printing-test.sh <<'SCRIPT'
systemctl is-enabled cups.socket | sed s,^,SOCKET-,
# CUPS writes its printers in /etc/cups: a real directory, not the link.
test ! -L /etc/cups && test -w /etc/cups/cupsd.conf && echo ETC-writable
lpstat -r | sed s,^,CUPS-,
lpadmin -p ci -E -v socket://127.0.0.1:9100 -m raw 2>&1 | sed s,^,LPADMIN-,
lpstat -p ci | sed s,^,PRINTER-,
systemctl is-active avahi-daemon.service | sed s,^,AVAHI-,
grep '^hosts:' /etc/nsswitch.conf | sed s,^,NSS-,
find /usr/lib/qt6/plugins -name '*kcm_printer_manager*' | sed s,^,KCM-,
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --credential printing-test=out/printing-test.sh \
    --run "/bin/sh /run/credentials/@system/printing-test" \
    --expect "SOCKET-enabled" \
    --expect "ETC-writable" \
    --expect "CUPS-scheduler is running" \
    --expect "PRINTER-printer ci is idle" \
    --expect "AVAHI-active" \
    --expect "NSS-hosts:.*mdns_minimal" \
    --expect "KCM-.*kcm_printer_manager" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-printing.log
