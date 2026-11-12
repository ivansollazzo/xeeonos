#!/usr/bin/env bash
# First boot, A/B update over HTTP, rollback, and the automatic fallback after
# a broken update. These steps form one story on one disk: each step starts
# from the disk the previous one left.
source "$(dirname "$0")/common.sh"
BROKEN=$VERSION.2   # a broken version after the next one

# A throwaway signing key for the test updates.
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output build/update-next/SHA256SUMS.gpg build/update-next/SHA256SUMS
keyring=(--credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg")
# systemd.run= replaces the boot target, so the normal services (network
# included) are not started: start the network first.
update="/bin/sh -c 'systemctl start network-online.target && /usr/bin/xctl update --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring'"

echo "::group::1. First boot, sealed /usr"
fresh_disk
tools/boot-test.sh out/disk.raw out/serial-1-firstboot.log
echo "::endgroup::"

echo "::group::2. Update to the next version over HTTP"
tools/boot-test.sh --http build/update-next "${keyring[@]}" --run "$update" \
    --expect "XeeonOS $NEXT is installed" \
    out/disk.raw out/serial-2-update.log
echo "::endgroup::"

echo "::group::3. The next version boots from slot B"
tools/boot-test.sh \
    --expect "XeeonOS $NEXT \(build" \
    --expect "xeeonos_$NEXT  \(running\)" \
    --expect "xeeonos_${VERSION}[[:space:]]*\$" \
    --expect "$NEXT +good, running" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-3-after-update.log
echo "::endgroup::"

echo "::group::4. xctl rollback"
tools/boot-test.sh --run "/usr/bin/xctl rollback" \
    --expect "XeeonOS $NEXT is disabled. Restart to go back to XeeonOS $VERSION\." \
    out/disk.raw out/serial-4-rollback.log
echo "::endgroup::"

echo "::group::5. The previous version boots again"
tools/boot-test.sh \
    --expect "XeeonOS $VERSION \(build" \
    --expect "$VERSION +good, running" \
    --expect "$NEXT +bad" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-5-after-rollback.log
echo "::endgroup::"

# The next version under a newer number, with the start of its /usr
# overwritten: it installs fine (the files match the signed manifest) but
# cannot start, because dm-verity refuses the damaged blocks.
echo "::group::6. Install a broken version"
src=out/broken-src
mkdir -p "$src"
cp "build/update-next/xeeonos_$NEXT.efi" "$src/xeeonos_$BROKEN.efi"
for f in build/update-next/xeeonos_"$NEXT".usr-*.raw.zst; do
    name=$(basename "$f" .zst)
    zstd -d --sparse --quiet "$f" -o "$src/xeeonos_$BROKEN${name#xeeonos_$NEXT}"
done
dd if=/dev/urandom of="$(echo "$src"/xeeonos_"$BROKEN".usr-x86-64.*.raw)" bs=1M count=1 conv=notrunc
SIGNING_KEY="XeeonOS CI test key" tools/mkupdate.sh "$src" "$BROKEN" out/update-broken
rm -rf "$src"
tools/boot-test.sh --http out/update-broken "${keyring[@]}" --run "$update" \
    --expect "XeeonOS $BROKEN is installed" \
    out/disk.raw out/serial-6-update-broken.log
echo "::endgroup::"

echo "::group::7. After 3 failed attempts, the previous version starts"
TIMEOUT=900 tools/boot-test.sh --allow-reboot \
    --expect "XeeonOS: this version failed to start; restarting" \
    --expect "XeeonOS $VERSION \(build" \
    --expect "$VERSION +good, running" \
    --expect "$BROKEN +bad" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-7-fallback.log
echo "::endgroup::"
