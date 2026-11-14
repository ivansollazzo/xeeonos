#!/usr/bin/env bash
# Boot a XeeonOS image in QEMU (UEFI) and check that it works.
#
#   *.iso  live ISO: pass when systemd reaches multi-user.target, or, with
#          --run, run a command after boot and power off.
#   *.raw  installed-system disk: the disk is grown to DISK_SIZE (the first
#          boot then creates slot B and the data partition), a command runs
#          after boot (default: `xctl status --check`) and the VM powers off.
#          The image file is modified, so it can be booted again to test the
#          next step (e.g. after an update).
#
# Usage: tools/boot-test.sh [options] <image.iso|image.raw> [serial.log]
# Options:
#   --run <command>         command to run once booted (via systemd.run=);
#                           disk images default to `xctl status --check`
#   --expect <regex>        pattern that must appear on the console; may be
#                           given several times (all must appear)
#   --credential <name>=<file>
#                           pass <file> to the VM as system credential <name>
#                           (/run/credentials/@system/<name>)
#   --http <dir>            serve <dir> at http://10.0.2.2:8080/ in the VM
#   --disk <file>           attach <file> as an extra, blank virtio disk
#                           (/dev/vda in the live system), created with
#                           DISK_SIZE if missing: the target of an install
#   --setup <file>          run the first-boot setup unattended, with the
#                           answers in <file> (credential xeeonos.setup);
#                           by default the setup is skipped
#   --cmdline <args>        extra kernel command line arguments
#   --efivars <file>        keep the UEFI variables in <file> (created if
#                           missing), so that the next test boot sees them
#                           (one-shot boot menu choice, factory reset request)
#   --allow-reboot          let the VM restart (to test the fallback after
#                           failed boots); by default a restart ends the test
#   --secure-boot           firmware with Secure Boot (OVMF_CODE_4M.secboot);
#                           new UEFI variables start in Setup Mode, without
#                           keys, so systemd-boot enrolls the XeeonOS keys
#                           (and restarts: use --allow-reboot)
#   --tpm <dir>             a TPM 2.0 chip (swtpm), whose state is kept in
#                           <dir> (created if missing) for the next boots
# Env: TIMEOUT (seconds, default 300), MEMORY (MiB, default 6144),
#      DISK_SIZE (default 24G), OVMF_CODE / OVMF_VARS (Ubuntu defaults).
set -euo pipefail

run=""
disk=""
setup=""
extra_cmdline=""
expect=()
credentials=()
http_dir=""
efivars=""
secure_boot=no
tpm_dir=""
no_reboot=(-no-reboot)
while (( $# )); do
    case $1 in
        --run) run=$2; shift 2 ;;
        --expect) expect+=("$2"); shift 2 ;;
        --credential) credentials+=("$2"); shift 2 ;;
        --http) http_dir=$2; shift 2 ;;
        --disk) disk=$2; shift 2 ;;
        --setup) setup=$2; shift 2 ;;
        --cmdline) extra_cmdline=$2; shift 2 ;;
        --efivars) efivars=$2; shift 2 ;;
        --allow-reboot) no_reboot=(); shift ;;
        --secure-boot) secure_boot=yes; shift ;;
        --tpm) tpm_dir=$2; shift 2 ;;
        -*) echo "unknown option: $1" >&2; exit 2 ;;
        *) break ;;
    esac
done

image=${1:?usage: boot-test.sh [options] <image.iso|image.raw> [serial.log]}
log=${2:-serial.log}
timeout=${TIMEOUT:-300}
memory=${MEMORY:-6144}
ovmf_code=${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
ovmf_vars=${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
machine=()
if [[ $secure_boot == yes ]]; then
    # The Secure Boot firmware needs SMM and protected variable storage.
    ovmf_code=${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.secboot.fd}
    machine=(-global driver=cfi.pflash01,property=secure,value=on)
fi

work=$(mktemp -d)
if [[ -n $efivars ]]; then
    [[ -e $efivars ]] || cp "$ovmf_vars" "$efivars"
    vars=$efivars
else
    vars=$work/vars.fd
    cp "$ovmf_vars" "$vars"
fi
: > "$log"

accel=tcg
[[ -w /dev/kvm ]] && accel=kvm

# Extra kernel command line for this test only, passed to systemd-stub
# through SMBIOS (the UKI itself stays untouched): serial console, no
# interactive first boot, run the command and power off.
# systemd.run= replaces the normal boot target, so the step that marks a new
# version as good (systemd-bless-boot, normally pulled in by basic.target) is
# run explicitly first, when boot counting is active.
run_media() {
    local bless cmdline c name file
    bless="test -e /sys/firmware/efi/efivars/LoaderBootCountPath-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f"
    bless+=" || exit 0; systemctl start systemd-bless-boot.service"
    cmdline="console=ttyS0,115200"
    if [[ -n $setup ]]; then
        # The CI never downloads the default applications from Flathub
        # (several GB): "no" unless the test answers itself.
        grep -q '^apps=' "$setup" || echo apps=no >> "$setup"
        credentials+=("xeeonos.setup=$setup")
    else
        cmdline+=" systemd.firstboot=no"
    fi
    [[ -z $extra_cmdline ]] || cmdline+=" $extra_cmdline"
    cmdline+=" systemd.run=\"/bin/sh -c '$bless'\""
    cmdline+=" systemd.run=\"$run\""
    cmdline+=" systemd.run_success_action=poweroff systemd.run_failure_action=poweroff"
    # (QEMU option values escape "," as ",,".)
    media+=(-smbios "type=11,value=io.systemd.stub.kernel-cmdline-extra=${cmdline//,/,,}")
    for c in "${credentials[@]}"; do
        name=${c%%=*} file=${c#*=}
        media+=(-smbios "type=11,value=io.systemd.credential.binary:$name=$(base64 -w0 "$file")")
    done
}

case $image in
    *.iso)
        media=(-cdrom "$image")
        if [[ -n $run ]]; then
            run_media
        else
            (( ${#expect[@]} )) || expect=("Reached target.*Multi-User System")
        fi
        ;;
    *.raw)
        # Growing to the same size again is a no-op, so repeated boots are fine.
        truncate --size="${DISK_SIZE:-24G}" "$image"
        media=(-drive "if=virtio,format=raw,file=$image")
        [[ -n $run ]] || run="/usr/bin/xctl status --check"
        (( ${#expect[@]} )) || expect=("check: sealed system OK")
        run_media
        ;;
    *)
        echo "Unknown image type: $image" >&2
        exit 2
        ;;
esac

(( ${#expect[@]} )) || { echo "--run on a live ISO needs --expect" >&2; exit 2; }

if [[ -n $disk ]]; then
    [[ -e $disk ]] || truncate --size="${DISK_SIZE:-24G}" "$disk"
    # detect-zeroes: blocks of zeros written by the installer stay sparse.
    media+=(-drive "if=virtio,format=raw,discard=unmap,detect-zeroes=unmap,file=$disk")
fi

pids=()
if [[ -n $tpm_dir ]]; then
    mkdir -p "$tpm_dir"
    swtpm socket --tpm2 --tpmstate dir="$tpm_dir" \
        --ctrl type=unixio,path="$work/swtpm.sock" --terminate &
    pids+=($!)
    for (( i = 0; i < 50; i++ )); do
        [[ -S $work/swtpm.sock ]] && break
        sleep 0.1
    done
    media+=(-chardev "socket,id=chrtpm,path=$work/swtpm.sock"
            -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0)
fi
if [[ -n $http_dir ]]; then
    python3 -m http.server --directory "$http_dir" --bind 127.0.0.1 8080 >"$work/http.log" 2>&1 &
    pids+=($!)
fi

echo "Booting $image (accel=$accel, ${memory} MiB, timeout ${timeout}s)"
# QEMU's user-mode network makes the host reachable as 10.0.2.2.
smm=""
[[ $secure_boot == no ]] || smm=",smm=on"
qemu-system-x86_64 \
    -machine q35,accel="$accel$smm" "${machine[@]}" -cpu max -smp 2 -m "$memory" \
    -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
    -drive if=pflash,format=raw,file="$vars" \
    -nic user,model=virtio-net-pci \
    "${media[@]}" \
    -display none -serial file:"$log" "${no_reboot[@]}" &
qemu=$!
pids+=("$qemu")

# Mirror the serial console into this script's output as it arrives, so a
# stuck boot can be diagnosed from the CI log alone.
tail -n +1 -f "$log" &
pids+=($!)
trap 'kill "${pids[@]}" 2>/dev/null || true; rm -rf "$work"' EXIT

all_found() {
    local p
    for p in "${expect[@]}"; do
        grep -qE -- "$p" "$log" || return 1
    done
}

for (( waited = 0; waited < timeout; waited += 5 )); do
    if all_found; then
        echo "PASS: all expected output seen after ~${waited}s"
        # Let the VM finish powering off (or hibernating), so the disk is
        # consistent for the next boot.
        for (( i = 0; i < 180; i++ )); do
            kill -0 "$qemu" 2>/dev/null || break
            sleep 1
        done
        exit 0
    fi
    if ! kill -0 "$qemu" 2>/dev/null; then
        all_found && { echo "PASS: all expected output seen"; exit 0; }
        echo "FAIL: QEMU exited before the expected output appeared:"
        for p in "${expect[@]}"; do
            grep -qE -- "$p" "$log" || echo "  missing: $p"
        done
        exit 1
    fi
    sleep 5
done

echo "FAIL: timed out after ${timeout}s"
exit 1
