#!/usr/bin/env bash
# Make an .xapp application from a recipe (docs/xapp.md): a directory with a
# Containerfile and an xapp.conf. The image is built with docker (or podman),
# saved into the .xapp with zstd, and the icon is taken from it.
#
# Usage: tools/mkxapp.sh <recipe dir> <output dir> [Arch snapshot YYYY/MM/DD]
#   (the snapshot defaults to the system's, from mkosi.conf)
# Output: <output dir>/<Name>.xapp (directory) and <Name>.xapp.tar
# Install: copy <Name>.xapp into ~/Applications, or
#          xctl app install <Name>.xapp.tar
set -euo pipefail
recipe=${1:?usage: mkxapp.sh <recipe dir> <output dir> [snapshot]}
out=${2:?usage: mkxapp.sh <recipe dir> <output dir> [snapshot]}
snapshot=${3:-$(sed -n 's/^Snapshot=//p' "$(dirname "$0")/../mkosi.conf")}
engine=${XAPP_ENGINE:-docker}

conf=$recipe/xapp.conf
get() { sed -n "s/^$1=//p" "$conf" | head -n1; }
name=$(get Name)
id=$(get Id)
icon=$(get Icon)
icon_from=$(get IconFrom)
version_from=$(get VersionFrom)
[[ -n $name && -n $id ]] || { echo "$conf: Name= and Id= needed" >&2; exit 1; }
image=localhost/xapp/${id,,}:current

"$engine" build --build-arg "SNAPSHOT=$snapshot" --tag "$image" \
    --file "$recipe/Containerfile" "$recipe"

app=$out/$name.xapp
rm -rf "$app"
mkdir -p "$app"
version=""
if [[ -n $version_from ]]; then
    version=$("$engine" run --rm "$image" pacman -Q "$version_from" | cut -d' ' -f2)
fi
# The manifest without the build-only keys, with the version.
{
    grep -v -e '^IconFrom=' -e '^VersionFrom=' -e '^Version=' -e '^# For tools/mkxapp.sh' \
        -e '^# application' "$conf"
    [[ -z $version ]] || echo "Version=$version"
} > "$app/xapp.conf"
if [[ -n $icon && -n $icon_from ]]; then
    cid=$("$engine" create "$image")
    "$engine" cp "$cid:$icon_from" "$app/$icon"
    "$engine" rm "$cid" >/dev/null
fi
"$engine" save "$image" | zstd -T0 -10 --quiet -o "$app/image.tar.zst"
tar -C "$out" -cf "$out/$name.xapp.tar" "$name.xapp"
ls -lh "$app" "$out/$name.xapp.tar"
echo "$name ${version:-}: $app"
