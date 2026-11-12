#!/usr/bin/env bash
# Prepare UEFI Secure Boot signing for a build (see docs/secure-boot.md).
#
#   tools/secureboot.sh [--test] <dir>
#
# Writes into <dir>:
#   mkosi-args   the mkosi options that sign the build (empty: unsigned)
#   key.pem, cert.pem
#                the signing key and its certificate
#   tree/        files added to every image (mkosi --extra-tree):
#                  usr/share/xeeonos/secureboot/{PK,KEK,db}.auth and
#                  xeeonos.crt: the keys to enroll in the firmware (the
#                  installer copies them to the boot partition);
#                  efi/loader/keys/auto/: the same, on the boot partition of
#                  the disk image.
#
# The key:
#   - XEEONOS_SECUREBOOT_KEY set (channel builds): that key (PEM), with the
#     certificate in secureboot/xeeonos.crt;
#   - --test (pull requests): a throwaway key made now, valid for this build
#     only (the images of a pull request are never published);
#   - neither: no signing, as long as secureboot/xeeonos.crt does not exist.
#     Once it exists, every published version must be signed with its key:
#     the build stops.
#
# Enrolled keys: PK = KEK = db = the XeeonOS certificate, plus Microsoft's
# certificates in KEK and db (secureboot/microsoft/): without them, Windows
# (next to XeeonOS) and the firmware of graphics and network cards (option
# ROMs, signed by Microsoft) would no longer start.
set -euo pipefail
cd "$(dirname "$0")/.."

test_key=no
if [[ ${1:-} == --test ]]; then
    test_key=yes
    shift
fi
dir=${1:?usage: tools/secureboot.sh [--test] <dir>}
rm -rf "$dir"
mkdir -p "$dir"
dir=$(realpath "$dir")
: > "$dir/mkosi-args"

if [[ $test_key == yes ]]; then
    openssl req -new -x509 -newkey rsa:2048 -nodes -days 30 -sha256 \
        -subj "/CN=XeeonOS test key (pull request build, not trusted)/" \
        -keyout "$dir/key.pem" -out "$dir/cert.pem" 2>/dev/null
    echo "Secure Boot: signed with a throwaway test key."
elif [[ -n ${XEEONOS_SECUREBOOT_KEY:-} ]]; then
    [[ -f secureboot/xeeonos.crt ]] || {
        echo "XEEONOS_SECUREBOOT_KEY is set, but secureboot/xeeonos.crt is missing" >&2
        exit 1
    }
    (umask 077; printf '%s\n' "$XEEONOS_SECUREBOOT_KEY" > "$dir/key.pem")
    cp secureboot/xeeonos.crt "$dir/cert.pem"
    # The key must belong to the certificate.
    if [[ $(openssl pkey -in "$dir/key.pem" -pubout) != "$(openssl x509 -in "$dir/cert.pem" -noout -pubkey)" ]]; then
        echo "XEEONOS_SECUREBOOT_KEY does not match secureboot/xeeonos.crt" >&2
        exit 1
    fi
    echo "Secure Boot: signed with the XeeonOS key."
elif [[ -f secureboot/xeeonos.crt ]]; then
    echo "XEEONOS_SECUREBOOT_KEY is not set: this version cannot be signed," \
        "and computers with the XeeonOS keys enrolled would not start it." >&2
    exit 1
else
    echo "Secure Boot: no key yet (secureboot/xeeonos.crt), the build is not signed."
    exit 0
fi

# The key lists to enroll, signed with the XeeonOS key (the PK signs
# itself and the KEK, the KEK signs db; here they are the same key).
xeeonos_guid=c105bb0c-0132-42b0-b762-3998823c507d
microsoft_guid=77fa9abd-0359-4d32-bd60-28f4e78f784b
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
siglist() { # <owner guid> <pem> <output>
    openssl x509 -in "$2" -outform der -out "$work/cert.der"
    sbsiglist --owner "$1" --type x509 --output "$3" "$work/cert.der"
}
siglist "$xeeonos_guid" "$dir/cert.pem" "$work/PK.esl"
cp "$work/PK.esl" "$work/KEK.esl"
cp "$work/PK.esl" "$work/db.esl"
for pem in secureboot/microsoft/*.pem; do
    siglist "$microsoft_guid" "$pem" "$work/one.esl"
    case $(basename "$pem") in
        kek-*) cat "$work/one.esl" >> "$work/KEK.esl" ;;
        db-*) cat "$work/one.esl" >> "$work/db.esl" ;;
    esac
done

keys=$dir/tree/usr/share/xeeonos/secureboot
mkdir -p "$keys"
for var in PK KEK db; do
    # The attributes systemd-boot uses when it enrolls them.
    sbvarsign --key "$dir/key.pem" --cert "$dir/cert.pem" \
        --attr NON_VOLATILE,BOOTSERVICE_ACCESS,RUNTIME_ACCESS,TIME_BASED_AUTHENTICATED_WRITE_ACCESS \
        --output "$keys/$var.auth" "$var" "$work/$var.esl"
done
cp "$dir/cert.pem" "$keys/xeeonos.crt"
mkdir -p "$dir/tree/efi/loader/keys"
cp -r "$keys" "$dir/tree/efi/loader/keys/auto"
rm "$dir/tree/efi/loader/keys/auto/xeeonos.crt"

# Signing: the UKIs and systemd-boot. mkosi's own key enrollment is replaced
# by the files above (it would leave out Microsoft's certificates). No signed
# PCR policies: the TPM unlock is bound to PCR 7 (see xctl encryption).
cat > "$dir/mkosi-args" <<EOF
--secure-boot=yes
--secure-boot-key=$dir/key.pem
--secure-boot-certificate=$dir/cert.pem
--secure-boot-auto-enroll=no
--sign-expected-pcr=no
--extra-tree=$dir/tree
EOF
