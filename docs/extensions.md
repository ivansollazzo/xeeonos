# System extensions

The system image is sealed: nothing can be installed into `/usr`. A system
extension adds software to it anyway, without breaking the seal: it is a
small read-only image of its own (a `.xext` file: a
[systemd extension image](https://uapi-group.org/specifications/specs/extension_image/)
with a XeeonOS manifest inside), laid over `/usr` at every boot
(`systemd-sysext`, started by `xeeonos-extensions.service`).

```
xctl ext list                      # every extension of this version and its state
sudo xctl ext enable docker        # download (signed) and turn on
sudo xctl ext disable docker       # turn off
sudo xctl ext remove docker        # turn off and delete
sudo xctl ext install my.xext      # from a file: not verified, asks first
```

Changes take effect at the next restart.

## Kinds

| Kind | Where it comes from | Can be |
| --- | --- | --- |
| Included | inside the system image (`/usr/lib/xeeonos/extensions/<name>.xext`): sealed and updated with the system, on from the start, also when installed from the ISO without a network | turned off and on |
| Optional | published with every version on the registry (`xeeonos_<version>.<name>.xext`), covered by the same signed manifest as the update | downloaded, turned on and off, removed |
| From a file | `sudo xctl ext install <file.xext>`: anything, not signed and not checked by XeeonOS (a warning says so) | turned on and off, removed |

**Special** extensions take away something the rest of the system relies on
when they are turned off: XeeonOS shows a warning and asks first.

Every extension is built for one exact version of XeeonOS, from the same Arch
snapshot. `sudo xctl update` also downloads the new version's copy of the
optional extensions that are on, and keeps the old one for going back
(`xctl rollback`). An extension from a file made for another version is left
out at boot (`xctl ext list` still shows it).

Recovery always starts without extensions.

## The extensions

| Name | Kind | What |
| --- | --- | --- |
| `flatpak` | included, special | Applications from Flathub, Discover (see [flatpak.md](flatpak.md)) |
| `printing` | included, special | Printers: CUPS, network printers, Printers page (see [printing.md](printing.md)) |
| `docker` | optional | The Docker engine, Compose, Buildx; users of the group `docker` use it without sudo |

## Making one

One directory per extension in `extensions/<name>/`:

- `extension.conf`, the manifest (shell syntax):
  - `Description=`, shown by `xctl ext list`;
  - `Special=yes|no`, `Default=yes|no` (yes: included in the system image);
  - `Packages=`, Arch packages: only what the system does not already have
    goes into the extension;
  - `Enable=`, units to start with it (`docker.socket`).
- `files/`, files of its own (only under `/usr`; defaults for `/etc` go in
  `/usr/share/factory/etc`, with a `tmpfiles.d` line that links or copies
  them).
- `build.sh` (optional), an extra build step.

`tools/mkext.sh` builds them all (CI does it before the system). Only `/usr`
is laid over the system: what packages put in `/etc` moves to
`/usr/share/factory/etc`, `/opt/<x>` to `/usr/lib/xeeonos/opt/<x>` (with a link
back), directories in `/var` become `tmpfiles.d` lines.

An extension from outside: any `systemd-sysext` image whose
`usr/lib/extension-release.d/extension-release.<name>` says `ID=xeeonos` and
`SYSEXT_LEVEL=<version>` (or `ID=_any`), with its manifest in
`usr/lib/xeeonos/extensions/<name>.conf`.

## Tests

`tools/ci/extensions.sh`: the included ones are on; turning off a special one
asks; docker is downloaded with a valid signature, works, and is installed
again from its file with the warning; an included one cannot be removed.
`flatpak.sh` and `printing.sh` test what the included ones bring.
