# Shared by the CI test scripts (sourced).
set -euo pipefail
mkdir -p out

# A fresh copy of the installed-system disk image in out/disk.raw.
# (Found wherever it is under build/: newer versions of download-artifact put
# a single artifact directly in build/, older ones in build/disk-image/.)
fresh_disk() {
    local image
    image=$(find build -name "xeeonos_$VERSION.raw.zst" -print -quit)
    [[ -n $image ]] || { echo "xeeonos_$VERSION.raw.zst not found in build/" >&2; exit 1; }
    zstd -d --sparse --quiet --force "$image" -o out/disk.raw
}

# The live ISO, carrying the system to install, in out/.
pack_iso() {
    tools/mkiso.sh "build/live/xeeonos_$VERSION.efi" "out/xeeonos-$VERSION.iso" build/update-system
}
