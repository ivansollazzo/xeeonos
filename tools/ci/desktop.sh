#!/usr/bin/env bash
# KDE Plasma (mkosi.profiles/plasma): after the first-boot setup, the login
# screen (SDDM on KWin, Wayland) starts, with the keyboard chosen at setup;
# then the user's Plasma session starts (automatic login, set by the test)
# with sound (PipeWire). No GPU in the VM: KWin draws with Mesa's software
# renderer.
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
# The test command replaces the normal boot target (systemd.run=): start the
# graphical one, as a normal boot does.
systemctl start --no-block graphical.target
wait_for sddm kwin_wayland
pgrep -u sddm -l -x kwin_wayland | sed s,^,GREETER-,
# If the login screen is not there, say why in the log.
if ! pgrep -u sddm -x kwin_wayland >/dev/null; then
    {
        ps -eo user:12,pid,comm | grep -E 'sddm|kwin|USER'
        loginctl list-sessions --no-pager
        getent passwd sddm
        ls -ld /var/lib/sddm /run/user/*
        systemctl status --no-pager -n 0 "user@$(id -u sddm).service" sddm.service
        journalctl -b --no-pager -n 120 -u sddm.service -u "user@$(id -u sddm).service" \
            -t sddm -t sddm-helper -t sddm-greeter-qt6 -t kwin_wayland -t kwin_wayland_wrapper
    } 2>&1 | cut -c1-300 | sed s,^,DIAG-,
fi
systemctl is-active display-manager.service | sed s,^,SDDM-,
localectl status | grep 'X11 Layout' | sed s,^,KB-,
sed s,^,KXKB-, /home/test/.config/kxkbrc
mkdir -p /etc/sddm.conf.d
printf '[Autologin]\nUser=test\nSession=plasma\n' > /etc/sddm.conf.d/autologin.conf
systemctl restart display-manager.service
wait_for test plasmashell
pgrep -u test -l -x plasmashell | sed s,^,DESKTOP-,
sleep 10
if test -S "/run/user/$(id -u test)/pipewire-0"; then echo SOUND-ready; fi
SCRIPT
TIMEOUT=900 tools/boot-test.sh \
    --setup out/setup.txt \
    --credential desktop-test=out/desktop-test.sh \
    --run "/bin/sh /run/credentials/@system/desktop-test" \
    --expect "GREETER-[0-9]+ kwin_wayland" \
    --expect "SDDM-active" \
    --expect "KB-.*X11 Layout: it" \
    --expect "KXKB-LayoutList=it" \
    --expect "DESKTOP-[0-9]+ plasmashell" \
    --expect "SOUND-ready" \
    out/disk.raw out/serial-desktop.log
