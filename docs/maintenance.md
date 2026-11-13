# Maintenance

What XeeonOS does by itself to stay healthy, and how to report a problem.

| What | When | Unit |
|---|---|---|
| Check for a new version (downloads only the signed manifest) | 10 minutes after boot, then daily | `xeeonos-update-check.timer` |
| Snapshot of settings and users' files | before every update | `xctl update` ([snapshots.md](snapshots.md)) |
| TRIM of the SSD | weekly | `fstrim.timer` |
| Check of every file on the data partition (Btrfs scrub) | monthly | `xeeonos-scrub.timer` |
| Firmware metadata (fwupd, LVFS) | daily | `fwupd-refresh.timer` |
| Disk health: Btrfs error counters and the disks' SMART verdict | daily, and after every scrub | `xeeonos-health.timer` |
| Room on the boot partition for the new version | before every update | `xctl update` |

When an update is available, or the disk has a problem (`xctl health`),
`xctl status` and the login message say so;
installing it stays a choice: `sudo xctl update`. Firmware updates are listed
with `fwupdmgr get-updates` and installed with `sudo fwupdmgr update`.

## Memory

Swap is first compressed memory (zram, zstd, priority 100), up to half of
the RAM and at most 8 GB; the kernel settings that suit it are in
`/usr/lib/sysctl.d/90-xeeonos-zram.conf`. Behind it is a swap file as large
as the RAM, but at most a quarter of the data partition (`/swap/swapfile`,
priority 10), created at the first boot by
`xeeonos-swapfile.service` in its own Btrfs subvolume: encrypted with the
rest of the data, and left out of the snapshots.

When memory runs out anyway, systemd-oomd stops the group of programs
responsible instead of letting the computer freeze: a user's programs that
keep the memory under pressure more than half of the time for 20 seconds,
or the biggest user of swap when swap is 90% full.

## Hibernation

`systemctl hibernate` saves the memory into the swap file and powers off;
the next start unlocks the data partition and resumes the session (systemd
notes where the image is in the UEFI variable HibernateLocation; no `resume=`
setting is needed). A session can only resume with the kernel that saved it,
so if an update was installed before hibernating,
`/usr/lib/systemd/system-sleep/xeeonos-same-version` makes the next start
use the running version once; the new version starts at the restart after.
The image is in the encrypted data partition, protected like the rest of the
data.

## Crashes

When the kernel crashes, its last messages are kept by the firmware and
saved by systemd-pstore at the next start, in `/var/lib/systemd/pstore`;
`xctl report` includes them.

## Reporting a problem

    sudo xctl report

writes `/var/tmp/xeeonos-report-<date>.tar.zst`: versions, boot entries,
disks and their health (SMART), network, firewall, snapshots, failed
services, hardware, kernel crashes and the logs of this boot and of the
previous one. No passwords, keys or users' files.
Attach it to the [bug report](https://github.com/ivansollazzo/xeeonos/issues).

## Arch snapshot

`.github/workflows/snapshot.yml` runs every Monday (and by hand: Actions →
Arch snapshot → Run workflow). It opens (or updates) one pull
request that moves `Snapshot=` in `mkosi.conf` to the newest Arch Linux
Archive date. Its description lists the Arch news published since the
current snapshot and the packages of XeeonOS that change
(`tools/snapshot-report.py`); when there is news or a new major version of
an important package (kernel series, systemd, glibc, ...), it is opened as a
draft, to read first. Arch supports only whole upgrades, so a week that
looks risky is skipped by leaving the pull request open: the next run moves
it to a newer date. It needs the repository secret `XEEONOS_BOT_TOKEN` (a
fine-grained token for this repository with Contents and Pull requests
read/write), because pull requests opened with the workflow's own token do
not start the CI.
