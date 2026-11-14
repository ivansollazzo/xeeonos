#!/usr/bin/env bash
# The live ISO boots, installs XeeonOS on a blank disk, and the installed
# system boots; the network ISO (live only) downloads the system, installs
# it, and that boots too.
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

echo "::group::4. The network ISO downloads the system, installs it, and it boots"
# The network ISO carries only the live system (it is the one on GitHub
# Releases): the system is downloaded, here from a local web directory,
# signed with a throwaway key as in the update test.
tools/mkiso.sh "build/live/xeeonos_$VERSION.efi" "out/xeeonos-$VERSION-net.iso"
export GNUPGHOME=$RUNNER_TEMP/net-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/net-keyring.gpg"
mkdir -p out/net-source
cp -l build/update-system/* out/net-source/ 2>/dev/null || cp build/update-system/* out/net-source/
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output out/net-source/SHA256SUMS.gpg out/net-source/SHA256SUMS
tools/boot-test.sh \
    --disk out/target.raw \
    --http out/net-source \
    --credential "xctl.keyring=$RUNNER_TEMP/net-keyring.gpg" \
    --run "/bin/sh -c 'systemctl start network-online.target && /usr/bin/xctl install-system --disk /dev/vda --yes --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring'" \
    --expect "XeeonOS $VERSION is installed on /dev/vda" \
    "out/xeeonos-$VERSION-net.iso" out/serial-install-net.log
rm -r out/net-source
tools/boot-test.sh \
    --expect "XeeonOS $VERSION \(build" \
    --expect "check: sealed system OK" \
    out/target.raw out/serial-installed-net.log
rm out/target.raw
echo "::endgroup::"
