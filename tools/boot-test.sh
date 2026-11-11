#!/usr/bin/env bash
# Boot a XeeonOS image in QEMU (UEFI, no Secure Boot) and check that it works.
#
#   *.iso  live ISO: pass when systemd reaches multi-user.target.
#   *.raw  installed-system disk: the disk is grown to DISK_SIZE (first boot
#          then creates slot B and the data partition), and `xctl status
#          --check` must report a sealed system. The image file is modified.
#
# Usage: tools/boot-test.sh <image.iso|image.raw> [serial.log]
# Env:   TIMEOUT (seconds, default 300), MEMORY (MiB, default 6144),
#        DISK_SIZE (default 24G), OVMF_CODE / OVMF_VARS (Ubuntu defaults).
set -euo pipefail

image=${1:?usage: boot-test.sh <image.iso|image.raw> [serial.log]}
log=${2:-serial.log}
timeout=${TIMEOUT:-300}
memory=${MEMORY:-6144}
ovmf_code=${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
ovmf_vars=${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}

work=$(mktemp -d)
cp "$ovmf_vars" "$work/vars.fd"
: > "$log"

accel=tcg
[[ -w /dev/kvm ]] && accel=kvm

case $image in
    *.iso)
        media=(-cdrom "$image")
        pass="Reached target.*Multi-User System"
        ;;
    *.raw)
        truncate --size="${DISK_SIZE:-24G}" "$image"
        media=(-drive "if=virtio,format=raw,file=$image")
        pass="check: sealed system OK"
        # Extra kernel command line for this test only, passed to systemd-stub
        # through SMBIOS (the UKI itself stays untouched): serial console, no
        # interactive first boot, run the check and power off.
        cmdline="console=ttyS0,115200 systemd.firstboot=no"
        cmdline+=" systemd.run=\"/usr/bin/xctl status --check\""
        cmdline+=" systemd.run_success_action=poweroff systemd.run_failure_action=poweroff"
        # (QEMU option values escape "," as ",,".)
        media+=(-smbios "type=11,value=io.systemd.stub.kernel-cmdline-extra=${cmdline//,/,,}")
        ;;
    *)
        echo "Unknown image type: $image" >&2
        exit 2
        ;;
esac

echo "Booting $image (accel=$accel, ${memory} MiB, timeout ${timeout}s)"
qemu-system-x86_64 \
    -machine q35,accel="$accel" -cpu max -smp 2 -m "$memory" \
    -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
    -drive if=pflash,format=raw,file="$work/vars.fd" \
    "${media[@]}" \
    -display none -serial file:"$log" -no-reboot &
qemu=$!

# Mirror the serial console into this script's output as it arrives, so a
# stuck boot can be diagnosed from the CI log alone.
tail -n +1 -f "$log" &
tailer=$!
trap 'kill "$qemu" "$tailer" 2>/dev/null || true; rm -rf "$work"' EXIT

for (( waited = 0; waited < timeout; waited += 5 )); do
    if grep -q "$pass" "$log"; then
        echo "PASS: '$pass' after ~${waited}s"
        exit 0
    fi
    if ! kill -0 "$qemu" 2>/dev/null; then
        echo "FAIL: QEMU exited before the test passed"
        exit 1
    fi
    sleep 5
done

echo "FAIL: timed out after ${timeout}s"
exit 1
