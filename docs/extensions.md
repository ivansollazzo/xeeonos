# System extensions

The system image is sealed: nothing can be installed into `/usr`. A system
extension adds software to it anyway, without breaking the seal: it is a
small read-only image of its own (a `.xext` file: a
[systemd extension image](https://uapi-group.org/specifications/specs/extension_image/)
with a XeeonOS manifest inside), laid over `/usr` at every boot
(`systemd-sysext`, started by `xeeonos-extensions.service`).

```
xctl extensions list                      # installed ones and their state, then those
                                          # that can be downloaded for this version
sudo xctl extensions enable docker        # download (signed) and turn on
sudo xctl extensions disable docker       # turn off
sudo xctl extensions remove docker        # turn off and delete
sudo xctl extensions install my.xext      # from a file: not verified, asks first
```

Changes take effect **at once**, without restarting: the extensions are
laid over `/usr` again (`systemd-sysext refresh`), and the extension's
services start or stop and its hooks run in the running system. Two
exceptions wait for the next restart: an extension with `Live=no` (one that
brings kernel modules, which cannot be swapped in a running system) and any
change made with `--no-apply` (enable, disable, install). Programs already
running from an extension that is turned off keep running until they exit.

`xctl extensions list` shows two sections: **Installed** (included,
downloaded, from a file) and **Can be downloaded** for the running version.
The second one is read from the extensions' registry: the manifests
(`<name>.conf`) published next to the images and listed in the signed
`SHA256SUMS`, so an extension added to xeeonos-extensions later shows up
without a system update. Without a network, or with `--offline`, only the
first section is shown.

## Kinds

| Kind | Where it comes from | Can be |
| --- | --- | --- |
| Included | inside the system image (`/usr/lib/xeeonos/extensions/<name>.xext`): sealed and updated with the system, on from the start, also when installed from the ISO without a network | turned off and on |
| Optional | built for every version by the repository [xeeonos-extensions](https://github.com/ivansollazzo/xeeonos-extensions) and published as `ghcr.io/ivansollazzo/xeeonos-extensions:<version>` (`xeeonos_<version>.<name>.xext` and its manifest `<name>.conf`, signed with the update key) | downloaded, turned on and off, removed |
| From a file | `sudo xctl extensions install <file.xext>`: anything, not signed and not checked by XeeonOS (a warning says so) | turned on and off, removed |

**Special** extensions take away something the rest of the system relies on
when they are turned off: XeeonOS shows a warning and asks first.

Every extension is built for one exact version of XeeonOS, from the same Arch
snapshot. `sudo xctl update` also downloads the new version's copy of the
optional extensions that are on, and keeps the old one for going back
(`xctl rollback`). Every XeeonOS build tells xeeonos-extensions, which builds
and publishes its extensions by itself soon after; until they are out,
`sudo xctl update` waits (installs nothing and says why), so an update never
takes an extension away. An extension from a file made for another version is left
out at boot (`xctl extensions list` still shows it).

Recovery always starts without extensions.

## The extensions

| Name | Kind | What |
| --- | --- | --- |
| `flatpak` | included, special | Applications from Flathub, GNOME Software (see [flatpak.md](flatpak.md)) |
| `printing` | included, special | Printers: CUPS, network printers, Printers page (see [printing.md](printing.md)) |
| `scanner` | included, special | Scanners: SANE, driverless scanning (network and USB), Document Scanner (see [scanner.md](scanner.md)) |
| `podman`, `distrobox`, `docker`, `rocm`, `rocm-sdk`, `thermald`, `tlp`, `dev-tools`, `thinkpad-t14-amd-touchpad` | optional | see [xeeonos-extensions](https://github.com/ivansollazzo/xeeonos-extensions) |
| `ci-hello` | optional, CI only | the test extension of `tools/ci/extensions.sh` |

## Making one

The included ones are here, the optional ones in xeeonos-extensions, made
the same way. One directory per extension in `extensions/<name>/`:

- `extension.conf`, the manifest (shell syntax):
  - `Description=`, shown by `xctl extensions list`;
  - `Special=yes|no`, `Default=yes|no` (yes: included in the system image);
  - `Packages=`, Arch packages: only what the system does not already have
    goes into the extension;
  - hooks, what it does to the system besides adding files:
    - `Enable=`, units to start with it (`docker.socket`);
    - `Disable=`, units of the system stopped and masked while it is on
      (masked for the current boot only, in `/run`: turned off, it leaves
      nothing behind);
    - `Setup=`, a script run once, when it is first turned on (at once, or
      at the next boot);
    - `Cleanup=`, a script run when it is turned off or removed (only if
      its `Setup=` ran), to undo what `Setup=` did; turned on again,
      `Setup=` runs again;
    - `Boot=`, a script run at every boot with it on (and when it is
      turned on);
  - `Live=no` for an extension that brings kernel modules: turning it on
    or off waits for the next restart.

    The scripts are file names in `files/usr/lib/xeeonos/extensions/<name>/`
    (executable), run as root with `XEEONOS_EXTENSION=<name>`; a failing
    script is reported and the boot goes on.
- `files/`, files of its own (only under `/usr`; defaults for `/etc` go in
  `/usr/share/factory/etc`, with a `tmpfiles.d` line that links or copies
  them).
- `build.sh` (optional), an extra build step.

`tools/mkext.sh` builds them all (CI does it before the system). Only `/usr`
is laid over the system: what packages put in `/etc` moves to
`/usr/share/factory/etc`, `/opt/<x>` to `/usr/lib/xeeonos/opt/<x>` (with a link
back), directories in `/var` become `tmpfiles.d` lines. The packages' own
install hooks do not run: what they would do at install time is done when
the extensions are laid over `/usr` (`xctl extensions activate`, and when one
is turned on or off): users (`systemd-sysusers`), device rules, units, and
the settings of their applications (GSettings schemas, compiled with the
system's into `/run/xeeonos/gsettings`, where `GSETTINGS_SCHEMA_DIR` points:
without them GNOME Software or Document Scanner would not start).

An extension from outside: any `systemd-sysext` image whose
`usr/lib/extension-release.d/extension-release.<name>` says `ID=xeeonos` and
`SYSEXT_LEVEL=<version>` (or `ID=_any`), with its manifest in
`usr/lib/xeeonos/extensions/<name>.conf`.

## Tests

`tools/ci/extensions.sh`: the included ones are on; turning off a special one
asks; ci-hello (unknown to the system before) is listed among those that can
be downloaded, is downloaded with a valid signature and its manifest, works,
runs its hooks (Setup once, Boot, Disable masks a unit, Cleanup when
removed), and is installed again from its file with the warning; an
included one cannot be removed.
`flatpak.sh` and `printing.sh` test what the included ones bring.
