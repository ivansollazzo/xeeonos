#!/usr/bin/env bash
# Install next to another system. The disk has "another system": a small EFI
# System Partition (100 MiB, too small for the kernels) and a data partition
# with a marker, then free space. XeeonOS goes into the free space with its
# own boot partition (XBOOTLDR); the other partitions must stay as they were.
source "$(dirname "$0")/common.sh"
pack_iso
export DISK_SIZE=30G

truncate --size=30G out/other.raw
printf 'label: gpt\nstart=1MiB, size=100MiB, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\nsize=2GiB, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4\n' \
    | sfdisk --quiet out/other.raw
truncate --size=100M out/esp.img && mkfs.vfat -F 32 out/esp.img
dd if=out/esp.img of=out/other.raw bs=1M seek=1 conv=notrunc status=none
printf 'OTHER-SYSTEM-MARKER' | dd of=out/other.raw bs=1M seek=101 conv=notrunc status=none
rm out/esp.img

tools/boot-test.sh \
    --disk out/other.raw \
    --run "/bin/sh -c '/usr/bin/xctl install-system --disk /dev/vda --free-space --yes || { echo DIAG; systemd-repart --version; sfdisk -d /dev/vda; SYSTEMD_LOG_LEVEL=debug /usr/bin/xctl install-system --disk /dev/vda --free-space --yes; }; lsblk -o NAME,SIZE,PARTTYPENAME,LABEL /dev/vda'" \
    --expect "XeeonOS $VERSION is installed on /dev/vda" \
    "out/xeeonos-$VERSION.iso" out/serial-install-free.log
tools/boot-test.sh \
    --run "/bin/sh -c 'bootctl --print-boot-path | sed s,^,BOOT-PATH-,; head -c 19 /dev/vda2; echo; lsblk -o NAME,SIZE,PARTTYPENAME,LABEL /dev/vda; xctl status --check'" \
    --expect "BOOT-PATH-/boot" \
    --expect "OTHER-SYSTEM-MARKER" \
    --expect "check: sealed system OK" \
    out/other.raw out/serial-installed-free.log
rm out/other.raw
