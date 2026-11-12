# Maintenance

What XeeonOS does by itself to stay healthy, and how to report a problem.

| What | When | Unit |
|---|---|---|
| Check for a new version (downloads only the signed manifest) | 10 minutes after boot, then daily | `xeeonos-update-check.timer` |
| Snapshot of settings and users' files | before every update | `xctl update` ([snapshots.md](snapshots.md)) |
| TRIM of the SSD | weekly | `fstrim.timer` |
| Check of every file on the data partition (Btrfs scrub) | monthly | `xeeonos-scrub.timer` |
| Firmware metadata (fwupd, LVFS) | daily | `fwupd-refresh.timer` |

When an update is available, `xctl status` and the login message say so;
installing it stays a choice: `sudo xctl update`. Firmware updates are listed
with `fwupdmgr get-updates` and installed with `sudo fwupdmgr update`.

## Memory

Swap is compressed memory (zram, zstd), up to half of the RAM and at most
8 GB: no swap partition or file. The kernel settings that suit zram are in
`/usr/lib/sysctl.d/90-xeeonos-zram.conf`.

## Reporting a problem

    sudo xctl report

writes `/var/tmp/xeeonos-report-<date>.tar.zst`: versions, boot entries,
disks, network, firewall, snapshots, failed services, hardware and the logs
of this boot and of the previous one. No passwords, keys or users' files.
Attach it to the [bug report](https://github.com/ivansollazzo/xeeonos/issues).

## Arch snapshot

Every Monday `.github/workflows/snapshot.yml` opens (or updates) one pull
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
