#!/usr/bin/env bash
# .xapp applications (module xapp, experimental): Firefox made from
# Mozilla's build, installed with `xctl app install` by an administrator
# without sudo; the menu entry appears; Firefox starts; a copy into
# ~/Applications is picked up by the monitor, a deletion too.
source "$(dirname "$0")/common.sh"
fresh_disk
tools/mkxapp-firefox.sh out/xapp
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/xapp-test.sh <<'SCRIPT'
# systemd.run= replaces the boot target: start what a normal boot starts.
systemctl start network-online.target xeeonos-xapps.path
readlink /home/test/Applications | sed s,^,LINK-,
su - test -c 'curl -sSf http://10.0.2.2:8080/Firefox.xapp.tar.zst -o /tmp/Firefox.xapp.tar.zst && xctl app install /tmp/Firefox.xapp.tar.zst'
su - test -c 'xctl app list' | sed s,^,LIST-,
sleep 5
grep -h '^Exec=' /var/lib/xeeonos/xapps/share/applications/xapp-org.mozilla.firefox.desktop | sed s,^,ENTRY-,
su - test -c 'xctl app run Firefox --version' 2>&1 | sed s,^,RUN-,
# The monitor: a copy into ~/Applications, then a deletion.
su - test -c 'cp -r ~/Applications/Firefox.xapp /tmp/Copy && sed -i "s/^Name=.*/Name=Firefox Copy/;s/^Id=.*/Id=org.example.copy/" /tmp/Copy/xapp.conf && mv /tmp/Copy ~/Applications/Copy.xapp'
for i in $(seq 60); do test -e /var/lib/xeeonos/xapps/share/applications/xapp-org.example.copy.desktop && break; sleep 1; done
test -e /var/lib/xeeonos/xapps/share/applications/xapp-org.example.copy.desktop && echo MONITOR-added
su - test -c 'rm -rf ~/Applications/Copy.xapp'
for i in $(seq 60); do test -e /var/lib/xeeonos/xapps/share/applications/xapp-org.example.copy.desktop || break; sleep 1; done
test -e /var/lib/xeeonos/xapps/share/applications/xapp-org.example.copy.desktop || echo MONITOR-removed
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --http out/xapp \
    --credential xapp-test=out/xapp-test.sh \
    --run "/bin/sh /run/credentials/@system/xapp-test" \
    --expect "LINK-/var/lib/xapps" \
    --expect "Firefox installed in /var/lib/xapps/Firefox.xapp" \
    --expect "LIST-Firefox  \(Firefox.xapp, host\)" \
    --expect "ENTRY-Exec=/usr/bin/xctl app run \"Firefox\" %U" \
    --expect "RUN-Mozilla Firefox [0-9]" \
    --expect "MONITOR-added" \
    --expect "MONITOR-removed" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-xapp.log
