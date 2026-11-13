#!/usr/bin/env bash
# .xapp applications (module xapp, experimental): Firefox made as a container
# (tools/mkxapp.sh, recipe tools/ci/xapp-firefox), installed with `xctl app
# install` by an administrator without sudo; the menu entry appears; the
# user's monitor imports the image and makes the container in the background,
# without network; Firefox starts in it; a copy into ~/Applications gets its
# own menu entry and container, its deletion removes both.
source "$(dirname "$0")/common.sh"
# Importing the image and the first start of the container are slow without
# KVM.
export TIMEOUT=3000
fresh_disk
tools/mkxapp.sh tools/ci/xapp-firefox out/xapp
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/xapp-test.sh <<'SCRIPT'
# systemd.run= replaces the boot target: start what a normal boot starts.
systemctl start network-online.target xeeonos-xapps.path
# The user's manager, as at login: ~/Applications (user tmpfiles) and the
# user's monitor of the applications.
uid=$(id -u test)
systemctl start user@$uid.service
as_test() {
    su - test -c "export XDG_RUNTIME_DIR=/run/user/$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus; $1"
}
menu=/var/lib/xeeonos/xapps/share/applications
readlink /home/test/Applications | sed s,^,LINK-,
as_test 'curl -sSf http://10.0.2.2:8080/Firefox.xapp.tar -o /tmp/Firefox.xapp.tar && xctl app install /tmp/Firefox.xapp.tar && rm /tmp/Firefox.xapp.tar'
for i in $(seq 60); do test -e $menu/xapp-org.mozilla.firefox.desktop && break; sleep 2; done
grep -h '^Exec=' $menu/xapp-org.mozilla.firefox.desktop | sed s,^,ENTRY-,
# No network from here on: the .xapp holds all it needs.
ip route del default 2>/dev/null; true
# The user's monitor prepares the container in the background.
for i in $(seq 1200); do as_test 'xctl app list' | grep -q ', ready)' && break; sleep 2; done
as_test 'xctl app list' | sed s,^,LIST-,
as_test 'podman ps --all --format "{{.Names}}"' | sed s,^,CONTAINER-,
as_test 'xctl app run Firefox --version' 2>&1 | sed s,^,RUN-,
# The monitors: a copy into ~/Applications, then its deletion.
# (Copied under a hidden name first, which the monitors ignore.)
as_test 'cp -r ~/Applications/Firefox.xapp ~/Applications/.Copy && sed -i "s/^Name=.*/Name=Firefox Copy/;s/^Id=.*/Id=org.example.copy/" ~/Applications/.Copy/xapp.conf && mv ~/Applications/.Copy ~/Applications/Copy.xapp'
for i in $(seq 1200); do as_test 'podman container exists xapp-org.example.copy' && test -e $menu/xapp-org.example.copy.desktop && break; sleep 2; done
test -e $menu/xapp-org.example.copy.desktop && echo MONITOR-menu-added
as_test 'podman container exists xapp-org.example.copy' && echo MONITOR-container-added
as_test 'rm -rf ~/Applications/Copy.xapp'
for i in $(seq 300); do as_test 'podman container exists xapp-org.example.copy' || break; sleep 2; done
test -e $menu/xapp-org.example.copy.desktop || echo MONITOR-menu-removed
as_test 'podman container exists xapp-org.example.copy' || echo MONITOR-container-removed
as_test 'podman container exists xapp-org.mozilla.firefox' && echo FIREFOX-kept
journalctl --user -M test@ -u xeeonos-xapps.service --no-pager 2>/dev/null | tail -n 30 | sed s,^,USERLOG-,
xctl status --check
SCRIPT
tools/boot-test.sh \
    --setup out/setup.txt \
    --http out/xapp \
    --credential xapp-test=out/xapp-test.sh \
    --run "/bin/sh /run/credentials/@system/xapp-test" \
    --expect "LINK-/var/lib/xapps" \
    --expect "Firefox installed in /var/lib/xapps/Firefox.xapp" \
    --expect "ENTRY-Exec=/usr/bin/xctl app run \"Firefox\" %U" \
    --expect "LIST-Firefox [0-9].*  \(Firefox.xapp, ready\)" \
    --expect "CONTAINER-xapp-org.mozilla.firefox" \
    --expect "RUN-Mozilla Firefox [0-9]" \
    --expect "MONITOR-menu-added" \
    --expect "MONITOR-container-added" \
    --expect "MONITOR-menu-removed" \
    --expect "MONITOR-container-removed" \
    --expect "FIREFOX-kept" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-xapp.log
