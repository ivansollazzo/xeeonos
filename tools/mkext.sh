#!/usr/bin/env bash
# Build the system extensions (extensions/<name>/, docs/extensions.md) for
# one XeeonOS version.
#
# Each extension is the set of files its packages add to the system: they
# are installed with pacman into an empty directory that only holds the
# package database of the system image (written by the system build as
# xeeonos_<version>.pkgdb.tar), so pacman installs exactly what the system
# does not already have, from the same Arch snapshot. pacman runs in an Arch
# Linux container (tools/mkext-install.sh); the result is packed here as a
# read-only erofs image for systemd-sysext, a .xext file.
#
#   Default=yes   "included" extension: <bundled dir>/<name>.xext, which the
#                 system build then copies into the image
#                 (/usr/lib/xeeonos/extensions). It fits whatever version it
#                 is inside of.
#   Default=no    "optional" extension: <update dir>/xeeonos_<version>.<name>.xext
#                 and <update dir>/<name>.conf (tools/mkupdate.sh adds them
#                 to the signed manifest). It fits only <version>. Here only
#                 the CI's test extension (ci-hello): the real optional
#                 extensions are built for every version, the same way, in
#                 the repository xeeonos-extensions.
#
# An included extension's extension.conf is also copied to
# <bundled dir>/<name>.conf: that is how the system knows it.
#
# Usage: tools/mkext.sh <pkgdb.tar> <version> <bundled dir> <update dir>
# Needs: docker, mkfs.erofs (erofs-utils); root for packing (sudo).
set -euo pipefail
cd "$(dirname "$0")/.."

pkgdb=$(realpath "${1:?usage: mkext.sh <pkgdb.tar> <version> <bundled dir> <update dir>}")
version=${2:?version missing}
bundled=${3:?bundled dir missing}
update=${4:?update dir missing}
mkdir -p "$bundled" "$update"

snapshot=$(sed -n 's/^Snapshot=//p' mkosi.conf)
work=${MKEXT_WORK:-$PWD/mkext.work}
mkdir -p "$work/cache"
cp "$pkgdb" "$work/pkgdb.tar"

# 1. Install: one tree per extension in $work/tree/<name>.
# (apparmor=unconfined: the CI runner has no AppArmor tools, which docker
# would need to load its default profile.)
docker run --rm --security-opt apparmor=unconfined \
    --volume "$PWD:/src:ro" --volume "$work:/work" \
    "${MKEXT_IMAGE:-archlinux:base}" \
    /bin/bash /src/tools/mkext-install.sh "$snapshot"

# 2. Pack.
for conf in extensions/*/extension.conf; do
    name=$(basename "$(dirname "$conf")")
    tree=$work/tree/$name
    Default=no
    # shellcheck disable=SC1090
    source "$conf"
    release=$tree/usr/lib/extension-release.d/extension-release.$name
    sudo mkdir -p "$(dirname "$release")"
    if [[ $Default == yes ]]; then
        cp "$conf" "$bundled/$name.conf"
        # Inside the system image: no version of its own. Not compressed:
        # the system image around it is.
        printf 'ID=_any\n' | sudo tee "$release" >/dev/null
        sudo mkfs.erofs --quiet "$bundled/$name.xext" "$tree"
        sudo chown "$(id -u):$(id -g)" "$bundled/$name.xext"
        ls -lh "$bundled/$name.xext"
    else
        printf 'ID=xeeonos\nSYSEXT_LEVEL=%s\n' "$version" | sudo tee "$release" >/dev/null
        xext=$update/xeeonos_$version.$name.xext
        cp "$conf" "$update/$name.conf"
        sudo mkfs.erofs --quiet -zlz4hc "$xext" "$tree"
        sudo chown "$(id -u):$(id -g)" "$xext"
        ls -lh "$xext"
    fi
done
sudo rm -rf "$work/tree"
