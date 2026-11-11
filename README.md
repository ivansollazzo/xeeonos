# XeeonOS

XeeonOS is an image-based Linux distribution built on Arch Linux around three principles:

- **Immutability**: the system is a sealed, read-only image; updates are atomic, with automatic rollback.
- **Simplicity**: one way to do each thing, and a system that updates and repairs itself.
- **Speed**: fast boot, a responsive desktop, fresh packages.

> Status: early development (Phase 0). Not ready for daily use.

## Branches

| Branch | Purpose |
| --- | --- |
| `experimental` | Development branch. Every commit produces an `experimental` build. |
| `main` | Stable releases only. |

Work happens on feature branches merged into `experimental` through pull requests.

## Modules

XeeonOS is assembled from independent modules: core modules in `mkosi.conf.d/`
are always included, optional ones in `mkosi.profiles/` are picked with
`mkosi --profile=...`. See [docs/modules.md](docs/modules.md).

## Building

XeeonOS is built with [mkosi](https://github.com/systemd/mkosi) (v27.1) from a dated
snapshot of the Arch Linux Archive, set in `mkosi.conf`. mkosi uses its own Arch tools
tree, so you can build from any Linux distribution.

```sh
mkosi --force build      # builds mkosi.output/xeeonos_<version>.efi
mkosi vm                 # boots it in a virtual machine
```

To produce the live ISO (needs `dosfstools`, `mtools`, `xorriso`):

```sh
tools/mkiso.sh mkosi.output/xeeonos_0.0.1.efi xeeonos.iso
tools/boot-test.sh xeeonos.iso   # optional: boot it in QEMU and wait for multi-user.target
```

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

0. **Foundations**: mkosi base on Arch, CI, live ISO. *(in progress)*
1. A/B `/usr` partitions with dm-verity, `systemd-sysupdate`, automatic rollback, installer.
2. KDE Plasma desktop and speed work.
3. `.xapp` applications in `/Applications`.
4. Public release 0.1.
