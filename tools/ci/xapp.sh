#!/usr/bin/env bash
# Extension xapp (extensions/xapp, included, special): XeeonOS applications
# (docs/xapp.md). On the runner: xapp is built, the test application
# (tools/ci/xapp-hello) is made with `xapp build` from its recipe and signed
# with a test key; two more files signed with other keys are made from a
# tiny tree (`--root`). In the VM, inside a GNOME session:
#   - the monitor puts the application dropped into ~/Applications in the
#     menu; .xapp files have their file type and handler (double click);
#   - in the sandbox the application sees its own image and not the system,
#     not the home (only its data), a folder it was given and not one it was
#     refused, no network (refused), no user namespaces (seccomp), a
#     read-only /usr, Wayland, the system's graphics drivers, a filtered
#     session bus, and a file it was asked to open;
#   - a changed file, another author's copy of the same application (pinning),
#     a revoked application are refused; an unknown author is asked about
#     (text dialog through a terminal), and without anybody to ask it does
#     not open;
#   - thumbnail, list, info; remove takes it out of the menu, its data stays.
source "$(dirname "$0")/common.sh"

# 1. On the runner.
sudo apt-get update -qq && sudo apt-get install -y -qq erofs-utils >/dev/null
(cd xapp && cargo build --release --locked --quiet)
xapp=$PWD/xapp/target/release/xapp
http=out/xapp-http
mkdir -p "$http"
"$xapp" build tools/ci/xapp-hello --key out/xapp-ci.key --output "$http/Hello.xapp" \
    --work "$RUNNER_TEMP/xapp-work" --no-install
"$xapp" verify "$http/Hello.xapp"
"$xapp" info "$http/Hello.xapp" | tee out/xapp-info.txt
# The public key of the test key, trusted by the VM's administrator.
python3 - "$http/Hello.xapp" > "$http/ci.pub" <<'PY'
import json, struct, sys
data = open(sys.argv[1], 'rb').read()
image, mlen = struct.unpack('<QI', data[-24:-12])
print(json.loads(data[image:image + mlen])['signer'])
PY
# Two more files, from a tiny tree: same id with another key (a fake
# update), and an unknown author.
tiny=$RUNNER_TEMP/xapp-tiny
mkdir -p "$tiny/root/usr/bin" "$tiny/fake" "$tiny/unknown"
printf '#!/bin/sh\necho tiny\n' > "$tiny/root/usr/bin/tiny"
chmod +x "$tiny/root/usr/bin/tiny"
cp tools/ci/xapp-hello/icon.svg "$tiny/fake/" && cp tools/ci/xapp-hello/icon.svg "$tiny/unknown/"
for kind in fake unknown; do
    id=org.xeeonos.Hello
    [[ $kind == unknown ]] && id=org.xeeonos.Unknown
    printf '{"app_id":"%s","name":"%s","version":"9.0","snapshot":"2026/11/13","exec":["/usr/bin/tiny"],"icon":"icon.svg","gpu":"bundled"}\n' \
        "$id" "$kind" > "$tiny/$kind/recipe.json"
    "$xapp" build "$tiny/$kind" --root "$tiny/root" --key "out/xapp-$kind.key" \
        --output "$http/$kind.xapp" --work "$RUNNER_TEMP/xapp-work-$kind" --no-install
done
ls -lh "$http"

# 2. In the VM.
fresh_disk
printf '%s\n' language=en_US.UTF-8 hostname=xeeonos-ci user=test \
    "fullname=Test User" password=test-password admin=yes > out/setup.txt
cat > out/xapp-test.sh <<'SCRIPT'
url=http://10.0.2.2:8080
xctl extensions list --offline | grep '^ *xapp ' | sed 's,^ *,EXT-,'
command -v xapp >/dev/null && echo XAPP-present
# A GNOME session for the user test (automatic login), as desktop.sh does.
systemctl start --no-block graphical.target
rm /etc/gdm
mkdir /etc/gdm
printf '[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=test\n' > /etc/gdm/custom.conf
systemctl restart display-manager.service
for _ in $(seq 240); do pgrep -u test -x gnome-shell >/dev/null && break; sleep 1; done
pgrep -u test -x gnome-shell >/dev/null && echo SESSION-up
sleep 10
uid=$(id -u test)
session_env=$(runuser -u test -- env XDG_RUNTIME_DIR="/run/user/$uid" systemctl --user show-environment |
    grep -E '^(DBUS_SESSION_BUS_ADDRESS|XDG_RUNTIME_DIR|WAYLAND_DISPLAY|XDG_DATA_DIRS|XDG_CURRENT_DESKTOP|LANG)=')
as_test() { runuser -u test -- env $session_env HOME=/home/test USER=test XAPP_TERMINAL=none "$@"; }
home=/home/test
# The administrator trusts the test key.
mkdir -p /etc/xapp/trusted.d
curl -fsS "$url/ci.pub" -o /etc/xapp/trusted.d/ci.pub

# The monitor: a file dropped into ~/Applications appears in the menu.
test -d $home/Applications && echo APPLICATIONS-folder
as_test curl -fsS "$url/Hello.xapp" -o $home/Applications/Hello.xapp
for _ in $(seq 60); do test -e $home/.local/share/applications/org.xeeonos.Hello.desktop && break; sleep 1; done
grep '^Exec=' $home/.local/share/applications/org.xeeonos.Hello.desktop | sed s,^,MENU-,
as_test xdg-mime query filetype $home/Applications/Hello.xapp | sed s,^,MIME-,
as_test cp $home/Applications/Hello.xapp /tmp/renamed
as_test gio info -a standard::content-type /tmp/renamed | grep content-type | sed 's,^ *,MAGIC-,'
as_test xdg-mime query default application/x-xapp | sed s,^,HANDLER-,

# The sandbox.
as_test mkdir -p $home/Documents $home/Pictures
as_test sh -c "echo secret > $home/secret.txt; echo doc > $home/Documents/doc.txt; echo pic > $home/Pictures/pic.txt; echo note > /tmp/note.txt"
as_test xapp permissions org.xeeonos.Hello documents yes
as_test xapp permissions org.xeeonos.Hello pictures no
as_test xapp permissions org.xeeonos.Hello network no
as_test xapp permissions org.xeeonos.Hello audio no
as_test xapp run org.xeeonos.Hello /tmp/note.txt 2>&1 | sed s,^,RUN-,
as_test xapp list | sed s,^,LIST-,
as_test xapp info org.xeeonos.Hello | sed s,^,INFO-,

# Refused: a changed file, another author's copy, a revoked application.
as_test cp $home/Applications/Hello.xapp /tmp/changed.xapp
printf 'X' | as_test dd of=/tmp/changed.xapp bs=1 seek=200000 conv=notrunc status=none
as_test xapp verify /tmp/changed.xapp 2>&1 | sed s,^,CHANGED-,
as_test curl -fsS "$url/fake.xapp" -o /tmp/fake.xapp
as_test xapp verify /tmp/fake.xapp 2>&1 | sed s,^,FAKE-,
echo "app org.xeeonos.Hello" > /etc/xapp/revoked
as_test xapp run org.xeeonos.Hello 2>&1 | sed s,^,REVOKED-,
rm /etc/xapp/revoked

# An unknown author: nobody to ask -> it does not open; asked in a
# terminal (text dialog): open this once, then "Cancel" at the author.
as_test curl -fsS "$url/unknown.xapp" -o /tmp/unknown.xapp
as_test xapp run /tmp/unknown.xapp 2>&1 | sed s,^,NOBODY-,
printf '1\n2\n' | as_test env XAPP_DIALOG=text script -qec "xapp run /tmp/unknown.xapp" /dev/null 2>&1 |
    grep -E 'unknown author|Open anyway' | head -2 | sed s,^,ASKED-,

# Thumbnail, removal.
as_test xapp thumbnail $home/Applications/Hello.xapp /tmp/thumb.png 128 && head -c 4 /tmp/thumb.png | grep -q PNG && echo THUMB-png
as_test xapp remove org.xeeonos.Hello | sed s,^,REMOVE-,
test -e $home/.local/share/applications/org.xeeonos.Hello.desktop || echo MENU-gone
test -d $home/.var/xapps/org.xeeonos.Hello && echo DATA-kept
xctl status --check
SCRIPT
TIMEOUT=900 tools/boot-test.sh \
    --setup out/setup.txt \
    --http "$http" \
    --credential xapp-test=out/xapp-test.sh \
    --run "/bin/sh /run/credentials/@system/xapp-test" \
    --expect "EXT-xapp +on +.*\(included, special\)" \
    --expect "XAPP-present" \
    --expect "SESSION-up" \
    --expect "APPLICATIONS-folder" \
    --expect "MENU-Exec=xapp run org.xeeonos.Hello %U" \
    --expect "MIME-application/x-xapp" \
    --expect "MAGIC-standard::content-type: application/x-xapp" \
    --expect "HANDLER-xapp-run.desktop" \
    --expect "IN-HOME-/home/test" \
    --expect "IN-ROOTFS-app" \
    --expect "IN-HOST-hidden" \
    --expect "IN-SECRET-hidden" \
    --expect "IN-DOCS-visible" \
    --expect "IN-PICS-hidden" \
    --expect "IN-PID1-bwrap" \
    --expect "IN-NET-off" \
    --expect "IN-USERNS-blocked" \
    --expect "IN-USR-readonly" \
    --expect "IN-ARG-readable-/run/xapp/docs/0/note.txt" \
    --expect "IN-WAYLAND-ok" \
    --expect "IN-GPU-.*llvmpipe" \
    --expect "IN-BUS-portal" \
    --expect "IN-BUS-shell-hidden" \
    --expect "LIST-Hello +org.xeeonos.Hello +1.0 +trusted author" \
    --expect "INFO-Author: .*\(trusted\)" \
    --expect "INFO-Based on: +Arch Linux snapshot 2026-11-13" \
    --expect "CHANGED-xapp: the image does not match its manifest" \
    --expect "FAKE-xapp: org.xeeonos.Hello is signed by a different author" \
    --expect "REVOKED-.*revoked" \
    --expect "NOBODY-xapp: unknown needs an answer before it opens" \
    --expect "ASKED-.*unknown author" \
    --expect "THUMB-png" \
    --expect "REMOVE-Removed /home/test/Applications/Hello.xapp" \
    --expect "MENU-gone" \
    --expect "DATA-kept" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-xapp.log
