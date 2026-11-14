#!/usr/bin/env bash
# GNOME (mkosi.profiles/gnome): after the first-boot setup, the login screen
# (GDM: GNOME Shell of its own greeter user, Wayland) starts, with the keyboard chosen
# at setup; then the user's GNOME session starts (automatic login, set by the
# test) with that keyboard as its input source and with sound (PipeWire). No
# GPU in the VM: GNOME Shell draws with Mesa's software renderer.
source "$(dirname "$0")/common.sh"
fresh_disk
printf '%s\n' language=it_IT.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/desktop-test.sh <<'SCRIPT'
wait_for() { # <user> <process>: up to 3 minutes
    for _ in $(seq 180); do
        pgrep -u "$1" -x "$2" >/dev/null && return
        sleep 1
    done
}
# The login screen runs as a user of its own, made by GDM when it starts
# (gdm-greeter, gdm-greeter-2, ...): its name is known only then.
greeter() {
    ps -eo user:32=,comm= | awk '$1 ~ /^gdm-greeter/ && $2 == "gnome-shell" { print $1; exit }'
}
wait_for_greeter() { # up to 3 minutes
    for _ in $(seq 180); do
        test -n "$(greeter)" && return
        sleep 1
    done
}
diag() { # <user>: why their GNOME Shell is not there, in the log
    {
        ps -eo user:12,pid,comm | grep -E 'gdm|gnome|USER'
        loginctl list-sessions --no-pager
        getent passwd "$1"
        systemctl status --no-pager -n 0 "user@$(id -u "$1").service" gdm.service
        journalctl -b --no-pager -n 150 -u gdm.service -u "user@$(id -u "$1").service" \
            -t gdm -t gdm-session-worker -t gnome-shell -t gnome-session-binary
    } 2>&1 | cut -c1-300 | sed s,^,DIAG-,
}
# The test command replaces the normal boot target (systemd.run=): start the
# graphical one, as a normal boot does.
systemctl start --no-block graphical.target
wait_for_greeter
user=$(greeter)
if test -n "$user"; then
    pgrep -u "$user" -l -x gnome-shell | sed s,^,GREETER-,
else
    diag gdm-greeter
fi
systemctl is-active display-manager.service | sed s,^,GDM-,
localectl status | grep 'X11 Layout' | sed s,^,KB-,
# Automatic login (a copy of the default file instead of the link).
rm /etc/gdm
mkdir /etc/gdm
printf '[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=test\n' > /etc/gdm/custom.conf
systemctl restart display-manager.service
wait_for test gnome-shell
pgrep -u test -l -x gnome-shell | sed s,^,DESKTOP-,
pgrep -u test -x gnome-shell >/dev/null || diag test
sleep 15
uid=$(id -u test)
runuser -u test -- env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
    gsettings get org.gnome.desktop.input-sources sources | sed s,^,SOURCES-,
if test -S "/run/user/$uid/pipewire-0"; then echo SOUND-ready; fi
# The GNOME services and helpers of the module are there (also Users and
# Online Accounts of Settings: accountsservice, gnome-online-accounts).
missing=""
for f in /usr/bin/gnome-disks /usr/lib/fprintd /usr/bin/boltctl \
    /usr/bin/monitor-sensor /usr/lib/gvfsd-mtp /usr/lib/gvfsd-gphoto2 \
    /usr/lib/gvfsd-afc /usr/lib/gvfsd-smb /usr/lib/gvfsd-nfs \
    /usr/lib/bluetooth/obexd /usr/lib/NetworkManager/VPN/nm-openvpn-service.name \
    /usr/bin/xdg-open /usr/bin/glxinfo /usr/bin/vulkaninfo \
    /usr/lib/accounts-daemon /usr/lib/goa-daemon; do
    test -e "$f" || missing="$missing $f"
done
echo "GNOME-missing:${missing:- none}"
systemctl is-active switcheroo-control.service | sed s,^,SWITCHEROO-,
# The system's own partitions are not shown as disks (Files, through UDisks).
for d in /dev/mapper/root /dev/mapper/usr /dev/disk/by-partlabel/xeeonos-root; do
    udevadm info -q property "$d" | grep -q '^UDISKS_IGNORE=1' && echo "HIDDEN-$(basename "$d")"
done
# Nor are the images of the system extensions (loop devices).
extensions=0 shown=""
for l in /sys/block/loop*; do
    backing=$(cat "$l/loop/backing_file" 2>/dev/null) || continue
    case $backing in
        /usr/lib/xeeonos/extensions/* | /var/lib/xeeonos/extensions/* | /run/extensions/* | *.xext) ;;
        *) continue ;;
    esac
    extensions=$((extensions + 1))
    udevadm info -q property "/dev/${l##*/}" | grep -q '^UDISKS_IGNORE=1' || shown="$shown ${l##*/}:$backing"
done
echo "EXT-LOOPS-$extensions shown:${shown:- none}"
# No Arch Linux logo on the login screen; "Log Out" with a single user.
runuser -u test -- env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
    gsettings get org.gnome.login-screen logo | sed s,^,LOGO-,
runuser -u test -- env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
    gsettings get org.gnome.shell always-show-log-out | sed s,^,LOGOUT-,
SCRIPT
TIMEOUT=900 tools/boot-test.sh \
    --setup out/setup.txt \
    --credential desktop-test=out/desktop-test.sh \
    --run "/bin/sh /run/credentials/@system/desktop-test" \
    --expect "GREETER-[0-9]+ gnome-shell" \
    --expect "GDM-active" \
    --expect "KB-.*X11 Layout: it" \
    --expect "DESKTOP-[0-9]+ gnome-shell" \
    --expect "SOURCES-.*'xkb', 'it'" \
    --expect "SOUND-ready" \
    --expect "GNOME-missing: none" \
    --expect "SWITCHEROO-active" \
    --expect "HIDDEN-root" \
    --expect "HIDDEN-usr" \
    --expect "HIDDEN-xeeonos-root" \
    --expect "EXT-LOOPS-[1-9][0-9]* shown: none" \
    --expect "LOGO-''" \
    --expect "LOGOUT-true" \
    out/disk.raw out/serial-desktop.log
