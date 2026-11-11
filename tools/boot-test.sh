#!/usr/bin/env bash
# Boot a XeeonOS ISO in QEMU (UEFI, no Secure Boot) and wait until systemd
# reaches multi-user.target on the serial console.
#
# Usage: tools/boot-test.sh <image.iso> [serial.log]
# Env:   TIMEOUT (seconds, default 300), MEMORY (MiB, default 6144),
#        OVMF_CODE / OVMF_VARS (firmware paths, Ubuntu defaults).
set -euo pipefail

iso=${1:?usage: boot-test.sh <image.iso> [serial.log]}
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
echo "Booting $iso (accel=$accel, ${memory} MiB, timeout ${timeout}s)"

qemu-system-x86_64 \
    -machine q35,accel="$accel" -cpu max -smp 2 -m "$memory" \
    -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
    -drive if=pflash,format=raw,file="$work/vars.fd" \
    -cdrom "$iso" \
    -display none -serial file:"$log" -no-reboot &
qemu=$!

# Mirror the serial console into this script's output as it arrives, so a
# stuck boot can be diagnosed from the CI log alone.
tail -n +1 -f "$log" &
tailer=$!
trap 'kill "$qemu" "$tailer" 2>/dev/null || true; rm -rf "$work"' EXIT

for (( waited = 0; waited < timeout; waited += 5 )); do
    if grep -q "Reached target.*Multi-User System" "$log"; then
        echo "PASS: reached multi-user.target after ~${waited}s"
        exit 0
    fi
    if ! kill -0 "$qemu" 2>/dev/null; then
        echo "FAIL: QEMU exited before the system finished booting"
        exit 1
    fi
    sleep 5
done

echo "FAIL: timed out after ${timeout}s"
exit 1
