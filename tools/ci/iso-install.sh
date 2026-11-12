#!/usr/bin/env bash
# The live ISO boots, installs XeeonOS on a blank disk, and the installed
# system boots.
source "$(dirname "$0")/common.sh"
pack_iso

echo "::group::1. The live system boots"
tools/boot-test.sh "out/xeeonos-$VERSION.iso" out/serial-live.log
echo "::endgroup::"

echo "::group::2. Install on a blank disk"
tools/boot-test.sh \
    --disk out/target.raw \
    --run "/bin/sh -c 'test -e /etc/arch-release || echo LIVE-NO-ARCH-RELEASE; /usr/bin/xctl install-system --disk /dev/vda --yes && exit 0; echo DIAG; ls -l /dev/disk/by-label; lsblk; ls /usr/lib/modules/*/kernel/drivers | head; journalctl -k -b --no-pager | grep -i -e sr0 -e sr_mod -e initramfs -e ahci -e ata; exit 1'" \
    --expect "LIVE-NO-ARCH-RELEASE" \
    --expect "XeeonOS $VERSION is installed on /dev/vda" \
    "out/xeeonos-$VERSION.iso" out/serial-install.log
echo "::endgroup::"

echo "::group::3. The installed system boots"
tools/boot-test.sh \
    --expect "XeeonOS $VERSION \(build" \
    --expect "check: sealed system OK" \
    out/target.raw out/serial-installed.log
rm out/target.raw
echo "::endgroup::"
