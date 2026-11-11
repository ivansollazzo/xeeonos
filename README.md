# XeeonOS

XeeonOS is an image-based Linux distribution built on Arch Linux around three principles:

- **Immutability**: the system is a sealed, read-only image; updates are atomic, with automatic rollback.
- **Simplicity**: one way to do each thing, and a system that updates and repairs itself.
- **Speed**: fast boot, a responsive desktop, fresh packages.

> Status: early development (Phase 1). Not ready for daily use.

## Branches

The channel is the branch name:

| Branch | Channel | Version example |
| --- | --- | --- |
| `experimental` | Free experimentation, continuous builds | `0.1.0-experimental.42` |
| `<N>.x/alpha`, `<N>.x/beta`, `<N>.x/stable` (later) | Releases of major version N | `1.0.0-beta.57`, `1.0.0` |

Changes enter a branch only through a pull request with green CI; work branches
are deleted after merge. The base version lives in `mkosi.version`;
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
mkosi --force build                  # sealed disk image: mkosi.output/xeeonos_<version>.raw
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

Every push and pull request runs the same steps in GitHub Actions; the ISO is
attached to the workflow run as the `xeeonos-iso` artifact.

## Trying the ISO

- **Virtual machine**: open the ISO in GNOME Boxes, virt-manager or VirtualBox with
  **UEFI** firmware enabled.
- **USB stick**: write it with `dd`, Etcher or Ventoy, then pick the stick from your
  PC's boot menu. Secure Boot must be disabled for now.

The Phase 0 live system runs entirely from RAM (allow at least 4 GB) and logs in
as `root` on the console. It does not touch your disks.

## Roadmap

0. **Foundations**: mkosi base on Arch, CI, live ISO. *(done)*
1. *(in progress)* A/B `/usr` partitions with dm-verity, `systemd-sysupdate`, automatic rollback, installer.
2. KDE Plasma desktop and speed work.
3. `.xapp` applications in `/Applications`.
4. Public release 0.1.
