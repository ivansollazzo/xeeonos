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
| `base` | core | Kernel, firmware, systemd, NetworkManager, XeeonOS identity and version |
| `system` | core | Sealed disk: ESP, A/B `/usr` slots (erofs + dm-verity), Btrfs data partition, hermetic `/etc` |
| `xctl` | core | The `xctl` system tool |
| `live` | optional | Live ISO: UKI running from RAM, root autologin; replaces `system` |

## Variants

A variant is a set of optional modules on top of the core ones:

| Variant | Command |
| --- | --- |
| Core (no desktop), installed system | `mkosi build` |
| Live ISO | `mkosi --profile=live build` |
| Desktop (planned) | `mkosi --profile=plasma,flatpak,xapp build` |

## Runtime modules

Optional extras that users switch on after installation (virtualization,
developer tools) are shipped separately as signed system extensions
(`systemd-sysext`) and managed with `xctl ext`. The desktop is not one of them:
it is part of the tested image of its variant.
