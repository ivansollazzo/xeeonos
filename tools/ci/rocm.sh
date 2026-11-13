#!/usr/bin/env bash
# Extension rocm-sdk (extensions/rocm-sdk): downloaded and turned on, ROCm is
# in /opt/rocm, its libraries are found, hipcc compiles a program. The
# virtual machine has no AMD GPU: nothing runs on one.
source "$(dirname "$0")/common.sh"
fresh_disk
system=$(dirname "$(find build -name SHA256SUMS -path '*update-system*' -print -quit)")
export GNUPGHOME=$RUNNER_TEMP/test-gnupg
mkdir -m 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-gen-key "XeeonOS CI test key" ed25519 sign 1d
gpg --export > "$RUNNER_TEMP/test-keyring.gpg"
gpg --batch --yes --local-user "XeeonOS CI test key" --detach-sign \
    --output "$system/SHA256SUMS.gpg" "$system/SHA256SUMS"

echo "::group::1. Turn rocm-sdk on"
tools/boot-test.sh --http "$system" --credential "xctl.keyring=$RUNNER_TEMP/test-keyring.gpg" \
    --run "/bin/sh -c 'systemctl start network-online.target && xctl ext enable rocm-sdk --from http://10.0.2.2:8080 --keyring /run/credentials/@system/xctl.keyring'" \
    --expect "Extension rocm-sdk: on, from the next restart" \
    out/disk.raw out/serial-rocm-1.log
echo "::endgroup::"

echo "::group::2. ROCm is there"
cat > out/rocm-test.sh <<'SCRIPT'
readlink -f /opt/rocm | sed s,^,OPT-,
ldconfig -p | grep -q libamdhip64 && echo LIB-amdhip64
/opt/rocm/bin/hipcc --version | grep -m1 HIP | sed s,^,HIPCC-,
cat > /tmp/add.hip <<'HIP'
#include <hip/hip_runtime.h>
__global__ void add(int *x) { x[threadIdx.x] += 1; }
int main() { return 0; }
HIP
/opt/rocm/bin/hipcc --offload-arch=gfx1100 -o /tmp/add /tmp/add.hip && echo COMPILED-ok
command -v rocminfo | sed s,^,ROCMINFO-,
xctl status --check
SCRIPT
tools/boot-test.sh --credential rocm-test=out/rocm-test.sh \
    --run "/bin/sh /run/credentials/@system/rocm-test" \
    --expect "OPT-/usr/lib/xeeonos/opt/rocm" \
    --expect "LIB-amdhip64" \
    --expect "HIPCC-HIP version" \
    --expect "COMPILED-ok" \
    --expect "ROCMINFO-.*rocminfo" \
    --expect "check: sealed system OK" \
    out/disk.raw out/serial-rocm-2.log
echo "::endgroup::"
