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
| `base` | core | Kernel, firmware, systemd, NetworkManager, all languages, XeeonOS identity and version; removes pacman and build-only files |
| `system` | core | Sealed disk: ESP, A/B `/usr` slots (erofs + dm-verity), Btrfs data partition, hermetic `/etc`, sysupdate definitions, boot health check, first-boot setup, Recovery boot entry |
| `xctl` | core | The `xctl` system tool (Rust, source in `xctl/`): status, update, rollback, channel, packages, setup, install-system |
| `live` | optional | Live ISO: UKI running from RAM, root autologin, installer tools; replaces `system` |
| `plasma` | optional | KDE Plasma desktop, minimal: Wayland session, SDDM login screen, network/sound/Bluetooth/power settings, Konsole, Dolphin, Spectacle (see [desktop.md](desktop.md)) |

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
| Desktop: what CI builds and publishes | `mkosi --profile=plasma build` |
| Desktop, later | `mkosi --profile=plasma,flatpak,xapp build` |

## Runtime modules

Optional extras that users switch on after installation (virtualization,
developer tools) are shipped separately as signed system extensions
(`systemd-sysext`) and managed with `xctl ext`. The desktop is not one of them:
it is part of the tested image of its variant.
