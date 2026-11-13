#!/usr/bin/env bash
# Extensions rocm and rocm-sdk (extensions/rocm*, docs/rocm.md). The virtual
# machine has no AMD GPU: nothing runs on one.
#   1. rocm is downloaded and turned on (2.8 GB; on a larger disk)
#   2. ROCm is there: /opt/rocm, its libraries, rocminfo, rocm-smi
#   3. rocm-sdk (about 12 GB, too large to download inside the slow
#      virtual machine) is checked here, mounted: hipcc, the math
#      libraries, the headers for compiling, its compatibility level
source "$(dirname "$0")/common.sh"
export DISK_SIZE=40G
fresh_disk
system=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output "$system/SHA256SUMS.gpg" "$system/SHA256SUMS"

echo "::group::1. Turn rocm on"
tools/boot-test.sh --http "$system" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --run "/bin/sh -c 'systemctl start network-online.target && xctl ext enable rocm --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring'" \
    --expect "Extension rocm: on, from the next restart" \
    out/disk.raw out/serial-rocm-1.log
echo "::endgroup::"

echo "::group::2. ROCm is there"
cat > out/rocm-test.sh <<'SCRIPT'
readlink -f /opt/rocm | sed s,^,OPT-,
ldconfig -p | grep -q libamdhip64 && echo LIB-amdhip64
ldconfig -p | grep -q libOpenCL && echo LIB-opencl
ls /opt/rocm/bin | tr '\n' ' ' | sed s,^,BIN-,; echo
command -v rocminfo | sed s,^,ROCMINFO-,
command -v rocm-smi | sed s,^,SMI-,
rocminfo 2>&1 | head -n 3 | sed s,^,RUN-,
xctl status --check
SCRIPT
tools/boot-test.sh --credential rocm-test=out/rocm-test.sh \
    --run "/bin/sh /run/credentials/@system/rocm-test" \
    --expect "OPT-/usr/lib/xeeonos/opt/rocm" \
    --expect "LIB-amdhip64" \
    --expect "LIB-opencl" \
    --expect "ROCMINFO-.*rocminfo" \
    --expect "SMI-.*rocm-smi" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-rocm-2.log
echo "::endgroup::"

echo "::group::3. rocm-sdk"
sdk=$(find "$system" -name '*.rocm-sdk.xext' -print -quit)
[[ -n $sdk ]] || { echo "no rocm-sdk.xext in $system" >&2; exit 1; }
ls -lh "$sdk"
mnt=$RUNNER_TEMP/rocm-sdk
mkdir -p "$mnt"
sudo mount -t erofs -o loop,ro "$sdk" "$mnt"
trap 'sudo umount "$mnt"' EXIT
check() { # <what> <path inside the extension, glob>
    if compgen -G "$mnt/$2" >/dev/null; then echo "ok: $1"; else echo "FAIL: $1 ($2 missing)" >&2; exit 1; fi
}
check hipcc 'usr/lib/xeeonos/opt/rocm/bin/hipcc'
check "hipcc in PATH" 'usr/bin/hipcc'
check "HIP headers" 'usr/lib/xeeonos/opt/rocm/include/hip/hip_runtime.h'
check rocBLAS 'usr/lib/xeeonos/opt/rocm/lib/librocblas.so*'
check rocFFT 'usr/lib/xeeonos/opt/rocm/lib/librocfft.so*'
check "C library headers" 'usr/include/stdio.h'
check "kernel API headers" 'usr/include/linux/types.h'
check gcc 'usr/bin/gcc'
cat "$mnt/usr/lib/extension-release.d/extension-release.rocm-sdk"
grep -q '^ID=xeeonos$' "$mnt/usr/lib/extension-release.d/extension-release.rocm-sdk"
echo "::endgroup::"
