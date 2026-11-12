# Shared by the CI test scripts (sourced).
set -euo pipefail
mkdir -p out

# A fresh copy of the installed-system disk image in out/disk.raw.
fresh_disk() {
    zstd -d --sparse --quiet --force "build/disk-image/xeeonos_$VERSION.raw.zst" -o out/disk.raw
}

# The live ISO, carrying the system to install, in out/.
pack_iso() {
    tools/mkiso.sh "build/live/xeeonos_$VERSION.efi" "out/xeeonos-$VERSION.iso" build/update-system
}
