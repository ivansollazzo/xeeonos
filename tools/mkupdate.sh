#!/usr/bin/env bash
# Assemble the update for one XeeonOS version: the files `xctl update` and
# systemd-sysupdate consume, plus a signed manifest.
#
#   xeeonos_<v>.efi                             kernel (UKI)
#   xeeonos_<v>.usr-x86-64.<uuid>.raw.zst        /usr partition
#   xeeonos_<v>.usr-x86-64-verity.<uuid>.raw.zst verity hash partition
#   xeeonos_<v>.manifest                         list of packages (if built)
#   SHA256SUMS, SHA256SUMS.gpg                   manifest + detached signature
#
# Usage: tools/mkupdate.sh <mkosi output dir> <version> <update dir>
# Env:   SIGNING_KEY  GnuPG key (id or fingerprint) used to sign SHA256SUMS,
#                     from the default GnuPG home (or GNUPGHOME). Without it
#                     the update is left unsigned and xctl will refuse it.
set -euo pipefail

src=${1:?usage: mkupdate.sh <mkosi output dir> <version> <update dir>}
version=${2:?version missing}
out=${3:?update dir missing}

mkdir -p "$out"
cp "$src/xeeonos_$version.efi" "$out/"
if [[ -e $src/xeeonos_$version.manifest ]]; then
    cp "$src/xeeonos_$version.manifest" "$out/"
fi
for part in "$src/xeeonos_$version".usr-x86-64.*.raw "$src/xeeonos_$version".usr-x86-64-verity.*.raw; do
    [[ -e $part ]] || { echo "missing partition file in $src: $part" >&2; exit 1; }
    zstd --quiet --force -T0 -19 --long=27 "$part" -o "$out/$(basename "$part").zst"
done

# (With the optional extensions built for the CI, and their manifests.)
(cd "$out" && shopt -s nullglob && sha256sum -- xeeonos_* ./*.conf | sed 's, \./, ,' > SHA256SUMS)
if [[ -n ${SIGNING_KEY:-} ]]; then
    gpg --batch --yes --local-user "$SIGNING_KEY" --detach-sign \
        --output "$out/SHA256SUMS.gpg" "$out/SHA256SUMS"
else
    echo "warning: SIGNING_KEY not set, update left unsigned" >&2
fi
ls -lh "$out"
