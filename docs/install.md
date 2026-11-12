# Installing

The live ISO carries the system to install (`xeeonos/` on the ISO: the same
files as an update, see [updates.md](updates.md)), so installing needs no
network.

```sh
xctl install-system                    # dialog windows: how, which disk, confirm
xctl install-system --disk /dev/nvme0n1                 # whole disk
xctl install-system --disk /dev/nvme0n1 --free-space    # next to another system
```

The installer is a set of black and white dialog windows (whiptail, like
`nmtui`). The first window asks how to install:

1. **Erase a disk and install XeeonOS**: everything on the chosen disk is
   erased (you confirm by typing the disk's name). The disk needs at least
   24 GiB.
2. **Install next to another system**: XeeonOS goes into the free space of
   a disk that already has another system (Windows, another Linux). The
   other partitions are not changed, moved or resized: make the free space
   first (e.g. with Windows' Disk Management, "Shrink volume"). It needs at
   least 24 GiB of free space in one piece, and a GPT disk.
   - If the disk has an EFI System Partition with at least 300 MiB free, the
     boot loader and the kernels go there.
   - If it is smaller (Windows makes one of 100 MiB), XeeonOS creates its own
     boot partition (XBOOTLDR, 1 GiB) in the free space for the kernels; the
     EFI System Partition gets only systemd-boot.
   - If there is none, a new EFI System Partition (1 GiB) is created.
   The boot menu (systemd-boot) also offers the other system.
3. **Manual**: choose the disk, then the whole disk or its free space, then
   where the kernels go (the existing EFI System Partition, or a separate
   boot partition).

Then it asks whether a **disk password** should be asked at every start
(the data is always encrypted; see [encryption.md](encryption.md)). If yes,
the first-boot setup asks for it. `--encrypt` gives the same answer without
the windows.

XeeonOS is not installed next to a system that already uses partitions of
the types XeeonOS uses for itself (root, `/usr`, XBOOTLDR, as on systemd's
Discoverable Partitions): XeeonOS would take them for its own at its first
boot.

`xctl install-system` works only in the live ISO or in Recovery
([recovery.md](recovery.md)). On a running installed system it refuses:
reinstalling there would erase the disk the system is running from.

## What it does

1. Checks the files on the ISO against their `SHA256SUMS`.
2. Creates the partitions with `systemd-repart`: a new partition table with
   the EFI System Partition (1 GiB) on a whole disk, or only the new
   partitions in the free space; and `/usr` slot A (verity hash tree +
   `/usr`), with the partition UUIDs from the file names (the kernel finds
   its `/usr` by them).
3. Decompresses `/usr` and its hash tree onto their partitions.
4. Installs systemd-boot (`bootctl install`, which also adds the firmware boot
   entry and the fallback loader `EFI/BOOT/BOOTX64.EFI`) and the kernel.

Slot B and the data partition are created by the installed system at its
first boot, from its own `/usr/lib/repart.d`, exactly as when it starts from
the disk image.

`--from <dir>` installs the version in another directory (made with
`tools/mkupdate.sh`) instead of the one on the ISO. `--free-space` uses the
free space instead of the whole disk; `--separate-boot` also creates the
boot partition even if the EFI System Partition has room. `--yes` skips the
confirmation, for automated tests.

## Limits for now

- Started through Ventoy, the live system may not see the ISO's files: write
  the ISO to the stick directly (dd, Etcher), or use `--from`.

## Tests

CI (`iso` job) boots the ISO with a blank 24 GiB disk, runs
`xctl install-system --disk /dev/vda --yes`, then boots that disk and checks
that the system is sealed. Then it does the same on a disk with "another
system" (a 100 MiB EFI System Partition and a data partition with a marker)
using `--free-space`: the installed system must start from its own boot
partition, and the other partition must be unchanged.
