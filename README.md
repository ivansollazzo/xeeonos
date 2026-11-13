# XeeonOS

XeeonOS is an image-based Linux distribution based on the packages of
[Arch Linux](https://archlinux.org), built around three principles:

- **Immutability**: the system is a sealed, read-only image; updates are atomic, with automatic rollback.
- **Simplicity**: one way to do each thing, and a system that updates and repairs itself.
- **Speed**: fast boot, a responsive desktop, fresh packages.

> Status: early development (Phase 1). Not ready for daily use.

## Branches

The channel is the branch name:

| Branch | Channel | Version example |
| --- | --- | --- |
| `experimental` | Free experimentation, continuous builds | `0.1.0-experimental.42` |
| `<major>.<minor>-alpha`, `-beta`, `-stable` (later), e.g. `1.0-stable`, `1.1-beta` | Releases, one set of branches per minor version | `1.1.0-beta.57`, `1.0.0` |

On top of these, the channels `stable`, `beta` and `alpha` follow the newest
version (`xctl channel list`, see [docs/updates.md](docs/updates.md)).

Changes enter a branch only through a pull request with green CI; work branches
are deleted after merge. CI builds everything once, then runs one test job per
feature in parallel ([tools/ci/](tools/ci/README.md)). The base version lives in `mkosi.version`;
`tools/version.sh` adds the channel and the CI run number.

## Modules

XeeonOS is assembled from independent modules: core modules in `mkosi.conf.d/`
are always included, optional ones in `mkosi.profiles/` are picked with
`mkosi --profile=...`. See [docs/modules.md](docs/modules.md).

## Building

XeeonOS is built with [mkosi](https://github.com/systemd/mkosi) (v27.1) from a dated
snapshot of the Arch Linux Archive, set in `mkosi.conf`. mkosi uses its own Arch tools
tree, so you can build from any Linux distribution.

```sh
mkosi --force --profile=plasma,audio build   # sealed disk image with the KDE Plasma desktop:
                                             # mkosi.output/xeeonos_<version>.raw
                                             # (without --profile: no desktop, text console only)
tools/boot-test.sh mkosi.output/xeeonos_0.1.0.raw   # first boot in QEMU + `xctl status --check`
```

To produce the live ISO (needs `dosfstools`, `mtools`, `xorriso`):

```sh
mkosi --force --profile=live build   # mkosi.output/xeeonos_<version>.efi
tools/mkiso.sh mkosi.output/xeeonos_0.1.0.efi xeeonos.iso
tools/boot-test.sh xeeonos.iso       # optional: boot it in QEMU
```

## Disk layout

| Partition | Content | Created |
| --- | --- | --- |
| ESP (1 GB) | systemd-boot, one UKI per version | in the image |
| `/usr` slot A (8 GB) + verity | the OS, read-only erofs, dm-verity | in the image |
| `/usr` slot B (8 GB) + verity | empty, receives the next update | first boot |
| Data (rest of the disk) | Btrfs: `/etc`, `/var`, `/home` | first boot |

Each UKI carries the verity root hash of its `/usr`. `/etc` starts empty: vendor
defaults are linked from `/usr/share/factory/etc`, and what the administrator
writes in `/etc` wins.

## Updates

`xctl update` installs the newest version of the system's channel into the
free `/usr` slot; it starts at the next restart. Updates are published to
`ghcr.io/ivansollazzo/xeeonos` and signed. See [docs/updates.md](docs/updates.md).

A new version gets 3 attempts to start; if they all fail, the previous version
starts again by itself. `xctl rollback` goes back on request. See
[docs/rollback.md](docs/rollback.md).

Every push and pull request runs the same chain of jobs in GitHub Actions,
each starting only if the previous one passed: `xctl` (format, lint, unit
tests) → build (system, next version, live image: built once) → disk tests
(first boot, A/B update, rollback, setup, Recovery) → ISO (pack, boot,
install) → publish (channel branches only). The ISO is attached to the
workflow run as the `xeeonos-iso` artifact.

## Trying the ISO

- **Virtual machine**: open the ISO in GNOME Boxes, virt-manager or VirtualBox with
  **UEFI** firmware enabled.
- **USB stick**: write it with `dd`, Etcher or Ventoy, then pick the stick from your
  PC's boot menu. XeeonOS is signed with its own Secure Boot key: turn
  Secure Boot off to try it, or enroll the XeeonOS keys
  ([docs/secure-boot.md](docs/secure-boot.md)).

The live system runs entirely from RAM (allow at least 4 GB) and logs in as
`root` on the console. It does not touch your disks unless you install:
`xctl install-system` installs XeeonOS on a whole disk, or next to another
system in its free space, or manually. See
[docs/install.md](docs/install.md).

## First boot and Recovery

At the first boot `xctl setup` asks, in black and white dialog windows, for
the language, keyboard and time zone (applied at once), the network, the
computer name and the first user ([docs/setup.md](docs/setup.md)). The boot
menu has a **Recovery** entry (also `sudo xctl recovery`) with a menu to
reinstall, go back to the previous version, set up the network, open an
administrator shell or erase everything ([docs/recovery.md](docs/recovery.md)).
The data partition is always encrypted; a disk password can be turned on
and off at any time ([docs/encryption.md](docs/encryption.md)).

## Looking after itself

Closed to incoming connections by default (`xctl firewall`,
[docs/firewall.md](docs/firewall.md)). Before every update, settings and
users' files are saved in a snapshot that Recovery can bring back
([docs/snapshots.md](docs/snapshots.md)). A daily check says when an update
is available, and another one watches the disk's health; SSD trim, a
monthly data check, firmware updates (fwupd), compressed-memory swap with a
swap file behind it, hibernation and a guard against running out of memory
are set up out of the box, and `sudo xctl report` collects what a bug report
needs ([docs/maintenance.md](docs/maintenance.md)).

## Arch Linux

XeeonOS is built from Arch Linux packages (a dated snapshot of the Arch Linux
Archive), without Arch's package manager. It is not an official Arch Linux
product: please report problems with XeeonOS
[here](https://github.com/ivansollazzo/xeeonos/issues), not to Arch Linux.

## Roadmap

0. **Foundations**: mkosi base on Arch, CI, live ISO. *(done)*
1. *(in progress)* A/B `/usr` partitions with dm-verity, `systemd-sysupdate`, automatic rollback, installer.
2. KDE Plasma desktop and speed work.
3. `.xapp` applications in `/Applications`.
4. Public release 0.1.
