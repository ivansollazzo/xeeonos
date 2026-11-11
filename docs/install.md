# Installing

The live ISO carries the system to install (`xeeonos/` on the ISO: the same
files as an update, see [updates.md](updates.md)), so installing needs no
network.

```sh
xctl install-system                    # asks for the disk, then for confirmation
xctl install-system --disk /dev/nvme0n1
```

For now the only mode is **whole disk**: everything on the chosen disk is
erased (you confirm by typing the disk's name). The disk needs at least 24 GiB.
Installing next to another system and manual partitioning come later.

## What it does

1. Checks the files on the ISO against their `SHA256SUMS`.
2. Creates a new partition table with `systemd-repart`: the EFI System
   Partition (1 GiB) and `/usr` slot A (verity hash tree + `/usr`), with the
   partition UUIDs from the file names (the kernel finds its `/usr` by them).
3. Decompresses `/usr` and its hash tree onto their partitions.
4. Installs systemd-boot (`bootctl install`, which also adds the firmware boot
   entry and the fallback loader `EFI/BOOT/BOOTX64.EFI`) and the kernel.

Slot B and the data partition are created by the installed system at its
first boot, from its own `/usr/lib/repart.d`, exactly as when it starts from
the disk image.

`--from <dir>` installs the version in another directory (made with
`tools/mkupdate.sh`) instead of the one on the ISO. `--yes` skips the
confirmation, for automated tests.

## Limits for now

- The first boot asks the standard systemd questions (language, keyboard,
  time zone, root password) on the console; the XeeonOS setup with user
  creation comes next.
- Started through Ventoy, the live system may not see the ISO's files: write
  the ISO to the stick directly (dd, Etcher), or use `--from`.

## Tests

CI (`iso` job) boots the ISO with a blank 24 GiB disk, runs
`xctl install-system --disk /dev/vda --yes`, then boots that disk and checks
that the system is sealed.
