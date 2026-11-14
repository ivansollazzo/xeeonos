# Modules

XeeonOS is assembled from small, independent modules. Each module does one job,
lives in its own directory and can be removed or replaced without touching the
others (for example, replacing the desktop with a different one).

| Kind | Location | Included |
| --- | --- | --- |
| Core module | `mkosi.conf.d/<module>/` | Always |
| Optional module | `mkosi.profiles/<module>/` | Only when selected with `--profile=` |

A module directory is a regular mkosi configuration directory: `mkosi.conf` for
settings and packages, `mkosi.extra/` for files copied into the image, and
optionally scripts such as `mkosi.postinst`. Shared settings (distribution,
snapshot, output, tools tree) stay in the top-level `mkosi.conf`.

## Current modules

| Module | Kind | Purpose |
| --- | --- | --- |
| `base` | core | Kernel, firmware, systemd, NetworkManager, Bluetooth, all languages, XeeonOS identity and version; removes pacman and build-only files |
| `system` | core | Sealed disk: ESP, A/B `/usr` slots (erofs + dm-verity), Btrfs data partition, hermetic `/etc`, sysupdate definitions, boot health check, first-boot setup, Recovery boot entry |
| `tools` | core | SSH (server off by default), FUSE 2 for AppImage; podman and distrobox are optional extensions, see [tools.md](tools.md); not in the live ISO |
| `xctl` | core | The `xctl` system tool (Rust, source in `xctl/`): status, update, rollback, channel, packages, setup, install-system |
| `live` | optional | Live ISO: UKI running from RAM, root autologin, installer tools; replaces `system` |
| `audio` | optional | Sound for every desktop: PipeWire (PulseAudio, ALSA and JACK interfaces) |
| `gnome` | optional | GNOME desktop, minimal: GNOME Shell on Wayland, GDM login screen, Settings (network, Bluetooth, screens, power, sound, printers), Console, Files, System Monitor (see [desktop.md](desktop.md)) |

The initrd built into every kernel image is configured in the top-level
`mkosi.initrd.conf/` (mkosi only reads it there): it restarts instead of
stopping at an emergency shell while a new version is on trial (see
[rollback.md](rollback.md)).

## Variants

A variant is a set of optional modules on top of the core ones:

| Variant | Command |
| --- | --- |
| Core (no desktop), installed system | `mkosi build` |
| Live ISO | `mkosi --profile=live build` |
| Desktop: what CI builds and publishes | `tools/mkext.sh` (extensions), then `mkosi --profile=gnome,audio --extra-tree=<extensions>:/usr/lib/xeeonos/extensions build` (see the workflow) |

## Extensions

Software on top of the sealed system, turned on and off after installation
with `xctl extensions`: `flatpak` and `printing` (included); the optional ones come from xeeonos-extensions. See
[extensions.md](extensions.md). The desktop is not one of them: it is part of
the tested image of its variant.
