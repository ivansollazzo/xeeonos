#!/bin/bash
# Build step of the extension thinkpad-t14-amd-touchpad (run by
# tools/mkext-install.sh in its Arch container).
#
# The project builds three in-tree kernel modules from pristine kernel files
# plus its patches, with DKMS. XeeonOS has no DKMS (its /usr is sealed), so
# the same build happens here, against the kernel of this XeeonOS version
# (same Arch snapshot): the modules go to /usr/lib/modules/<kernel>/updates,
# which the kernel prefers to the stock ones, with the module index made
# again for the whole kernel.
#
# Usage: build.sh <extension tree> <Arch snapshot>
set -euo pipefail
root=${1:?tree missing}
version=0.2.4
# The commit of the tag v0.2.4: what is built is checked against it (a tag
# can be moved, a commit cannot).
commit=3bb314ccb96cfb37df7b27a15434bc5198b8d6da
repo=https://github.com/orospakr/thinkpad-t14-amd-touchpad

# The kernel of the system (same snapshot, same version), its headers and
# the tools to build, in the container. The container's own packages are
# newer than the snapshot: -Syuu first brings all of them to the snapshot
# (downgrading), otherwise the snapshot's gcc or kernel cannot be installed
# next to them ("could not satisfy dependencies").
pacman --config /tmp/pacman.conf --cachedir /cache --noconfirm --needed -Syuu \
    linux linux-headers gcc make patch zstd kmod git >/dev/null
kver=$(ls /usr/lib/modules | grep -- '-arch' | head -n1)
[[ -d /usr/lib/modules/$kver/build ]] || { echo "no headers for $kver" >&2; exit 1; }
echo "Kernel $kver"

work=$(mktemp -d)
src=$work/thinkpad-t14-amd-touchpad-$version
git -c advice.detachedHead=false clone --quiet --depth 1 --branch "v$version" "$repo" "$src"
[[ $(git -C "$src" rev-parse HEAD) == "$commit" ]] ||
    { echo "v$version is not commit $commit" >&2; exit 1; }

# The patches, in the order of dkms.conf (PATCH[0], PATCH[1], ...).
sed -n 's/^PATCH\[[0-9]*\]="\(.*\)"$/\1/p' "$src/dkms.conf" | while read -r p; do
    echo "patch $p"
    patch -d "$src" -p1 --quiet < "$src/patches/$p"
done
make -C "/usr/lib/modules/$kver/build" M="$src" modules -j"$(nproc)" >/dev/null

updates=/usr/lib/modules/$kver/updates
mkdir -p "$updates"
for m in drivers/i2c/busses/i2c-piix4 drivers/input/rmi4/rmi_smbus drivers/input/mouse/psmouse; do
    zstd --quiet --force -19 "$src/$m.ko" -o "$updates/$(basename "$m").ko.zst"
done
depmod "$kver"

# Into the extension: the modules, and the module index (it replaces the
# system's, which does not know them).
mkdir -p "$root/usr/lib/modules/$kver"
cp -a "$updates" "$root/usr/lib/modules/$kver/"
cp -a /usr/lib/modules/"$kver"/modules.* "$root/usr/lib/modules/$kver/"
# The project's check tool and license.
install -Dm755 "$src/tools/check" "$root/usr/bin/thinkpad-t14-amd-touchpad-check"
install -Dm644 "$src/LICENSE" "$root/usr/share/licenses/thinkpad-t14-amd-touchpad/LICENSE"
install -Dm644 "$src/README.md" "$root/usr/share/doc/thinkpad-t14-amd-touchpad/README.md"
ls -l "$root/usr/lib/modules/$kver/updates"
rm -rf "$work"
