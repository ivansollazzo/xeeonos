#!/usr/bin/env bash
# Pack a XeeonOS UKI into a hybrid, UEFI-bootable ISO.
#
# The ISO holds an EFI system partition image (appended as a GPT partition and
# referenced by El Torito) with the UKI as the default EFI boot loader
# (EFI/BOOT/BOOTX64.EFI). The same file boots from a CD/DVD, from a USB stick
# written with dd/Etcher/Ventoy, and in virtual machines.
#
# With a system directory (made by tools/mkupdate.sh), the ISO also carries
# the system that `xctl install-system` installs, in xeeonos/ on the ISO file
# system (full ISO, installs offline). Without it, it is a network ISO:
# `xctl install-system` downloads the system from the channel.
#
# Usage: tools/mkiso.sh <uki.efi> <output.iso> [system dir]
# Needs: mkfs.fat (dosfstools), mmd/mcopy (mtools), xorriso.
set -euo pipefail

uki=${1:?usage: mkiso.sh <uki.efi> <output.iso>}
out=${2:?usage: mkiso.sh <uki.efi> <output.iso>}
system=${3:-}
label=${ISO_LABEL:-XEEONOS}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# ESP image: UKI size + 5% + 4 MiB headroom, never below the FAT32 minimum.
uki_kib=$(( ($(stat -c %s "$uki") + 1023) / 1024 ))
esp_kib=$(( uki_kib + uki_kib / 20 + 4096 ))
(( esp_kib < 34816 )) && esp_kib=34816

mkfs.fat -C -F 32 -n XEEONOS_EFI "$work/efiboot.img" "$esp_kib" >/dev/null
mmd -i "$work/efiboot.img" ::/EFI ::/EFI/BOOT
mcopy -i "$work/efiboot.img" "$uki" ::/EFI/BOOT/BOOTX64.EFI

mkdir -p "$work/root"
# The UKI lives only in the ESP image, so the ISO is not twice its size.
if [[ -n $system ]]; then
    printf 'XeeonOS live ISO. Boot it in UEFI mode.\n' > "$work/root/README.txt"
else
    printf 'XeeonOS network ISO. Boot it in UEFI mode; installing downloads XeeonOS (needs the Internet).\n' > "$work/root/README.txt"
fi
contents=("/=$work/root")
# The system's files, without the optional extensions (.xext): those are
# downloaded where they are turned on (xctl extensions), not installed.
if [[ -n $system ]]; then
    for file in "$system"/*; do
        [[ $file == *.xext ]] || contents+=("/xeeonos/$(basename "$file")=$file")
    done
fi

xorriso -as mkisofs \
    -iso-level 3 -full-iso9660-filenames -joliet -joliet-long -rational-rock \
    -volid "$label" -appid "XeeonOS live" -publisher "XeeonOS" \
    -partition_offset 16 \
    -append_partition 2 C12A7328-F81F-11D2-BA4B-00A0C93EC93B "$work/efiboot.img" \
    -appended_part_as_gpt \
    -e --interval:appended_partition_2:all:: -no-emul-boot \
    -output "$out" \
    -graft-points "${contents[@]}"

echo "ISO written to $out"
