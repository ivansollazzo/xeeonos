#!/bin/bash
# The part of tools/mkext.sh that runs as root inside an Arch Linux
# container: install the packages of one extension into /work/tree/<name>
# and turn the result into what lives under /usr.
#
#   /src    the repository (read-only)
#   /work   pkgdb.tar (the system's package database), tree/ (the result)
#   /cache  downloaded packages
#
# Usage: mkext-install.sh <Arch snapshot, YYYY/MM/DD> <extension>
set -euo pipefail
snapshot=${1:?snapshot missing}
only=${2:?extension missing}

mkdir -p /tmp/nohooks
cat > /tmp/pacman.conf <<CONF
[options]
Architecture = x86_64
SigLevel = Required DatabaseOptional
# The packages' install hooks and scripts expect a whole system around them;
# what they do is done at boot instead (users, /etc, /var: systemd).
HookDir = /tmp/nohooks
ParallelDownloads = 8
DisableSandbox

[core]
Server = https://archive.archlinux.org/repos/$snapshot/\$repo/os/\$arch

[extra]
Server = https://archive.archlinux.org/repos/$snapshot/\$repo/os/\$arch
CONF

# The Arch keys that sign the packages.
pacman-key --init >/dev/null
pacman-key --populate archlinux >/dev/null

rm -rf /work/tree
for conf in /src/extensions/"$only"/extension.conf; do
    dir=$(dirname "$conf")
    name=$(basename "$dir")
    root=/work/tree/$name
    Packages="" Enable="" KeepDevelopment=no Reinstall=""
    # shellcheck disable=SC1090
    source "$conf"
    echo "::group::Extension $name: $Packages"

    # The system's packages count as installed: only what is missing comes.
    mkdir -p "$root/var/lib/pacman"
    tar -xf /work/pkgdb.tar -C "$root/var/lib/pacman"
    if [[ -n $Packages ]]; then
        # shellcheck disable=SC2086
        pacman --config /tmp/pacman.conf --root "$root" --dbpath "$root/var/lib/pacman" \
            --cachedir /cache --gpgdir /etc/pacman.d/gnupg \
            --noconfirm --noscriptlet --needed -Sy $Packages
    fi
    # Packages of the system, installed again whole (their headers).
    if [[ -n $Reinstall ]]; then
        # shellcheck disable=SC2086
        pacman --config /tmp/pacman.conf --root "$root" --dbpath "$root/var/lib/pacman" \
            --cachedir /cache --gpgdir /etc/pacman.d/gnupg \
            --noconfirm --noscriptlet -Sy $Reinstall
    fi
    rm -rf "$root/var/lib/pacman" "$root/var/cache/pacman" "$root/var/log/pacman.log"

    # An extension-specific step (e.g. compiling kernel modules).
    if [[ -x $dir/build.sh ]]; then
        "$dir/build.sh" "$root" "$snapshot"
    fi

    # Only /usr is laid over the system. The rest is recreated at boot by
    # systemd-tmpfiles from this file.
    tmpfiles=$root/usr/lib/tmpfiles.d/xeeonos-ext-$name.conf
    mkdir -p "$(dirname "$tmpfiles")" "$root/usr/share/factory"
    echo "# Made by tools/mkext-install.sh: what the extension $name needs outside /usr." > "$tmpfiles"
    # /etc: defaults kept in /usr/share/factory/etc, like the system's own
    # (the extension's files/ say which ones are linked or copied into /etc).
    if [[ -d $root/etc ]]; then
        cp --archive --no-target-directory "$root/etc" "$root/usr/share/factory/etc"
        rm -rf "$root/etc"
    fi
    # /opt/<x>: kept in /usr, reached through a link.
    if [[ -d $root/opt ]]; then
        mkdir -p "$root/usr/lib/xeeonos/opt"
        for item in "$root"/opt/*; do
            [[ -e $item ]] || continue
            mv "$item" "$root/usr/lib/xeeonos/opt/"
            echo "L+ /opt/$(basename "$item") - - - - /usr/lib/xeeonos/opt/$(basename "$item")" >> "$tmpfiles"
        done
        rm -rf "$root/opt"
    fi
    # /var: the directories the packages ship there (root's; anything else
    # is created by the package's own tmpfiles.d or by its service).
    if [[ -d $root/var ]]; then
        (cd "$root" && find var -mindepth 1 -type d -printf '%m /%p\n') | sort -k2 |
            while read -r mode path; do
                echo "d $path $mode root root -" >> "$tmpfiles"
            done
        rm -rf "$root/var"
    fi
    # Anything else at the top (empty mount points, links like /bin).
    find "$root" -mindepth 1 -maxdepth 1 ! -name usr -exec rm -rf {} +

    # Files only used to compile programs, as in the system image.
    if [[ $KeepDevelopment != yes ]]; then
        rm -rf "$root/usr/include" "$root/usr/lib/pkgconfig" "$root/usr/share/pkgconfig"
        find "$root/usr/lib" -name '*.a' -type f -delete
    fi
    rm -rf "$root/usr/share/libalpm"

    # The extension's own files.
    if [[ -d $dir/files ]]; then
        cp --archive --no-target-directory "$dir/files" "$root"
        chown -R root:root "$root/usr"
    fi

    # Its manifest, also inside the image (xctl ext install reads it there).
    mkdir -p "$root/usr/lib/xeeonos/extensions"
    cp "$conf" "$root/usr/lib/xeeonos/extensions/$name.conf"

    # Units started with the extension: the links `systemctl enable` would
    # make in /etc (WantedBy=, Alias=), made in /usr instead.
    units=$root/usr/lib/systemd/system
    for unit in $Enable; do
        # A unit of the system itself (e.g. avahi-daemon, which comes with
        # the desktop): nothing to link here, `xctl ext activate` starts it.
        if [[ ! -e $units/$unit ]]; then
            echo "extension $name: $unit is part of the system, started at boot"
            continue
        fi
        for target in $(sed -n 's/^WantedBy=//p' "$units/$unit"); do
            mkdir -p "$units/$target.wants"
            ln -sf "../$unit" "$units/$target.wants/$unit"
        done
        for alias in $(sed -n 's/^Alias=//p' "$units/$unit"); do
            ln -sf "$unit" "$units/$alias"
        done
    done

    du -sh "$root"
    echo "::endgroup::"
done
