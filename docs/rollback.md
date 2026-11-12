# Rollback

A new version never replaces the one that works: it goes into the other `/usr`
slot, and the previous version stays installed until the next update. If the
new version cannot start, or you do not like it, XeeonOS goes back.

## Automatic: boot counting

A newly installed version gets 3 attempts to start. Its kernel is installed as

    /efi/EFI/Linux/xeeonos_<version>+3-0.efi      3 attempts left, 0 done

and at every start systemd-boot moves one attempt from "left" to "done"
(`+2-1`, `+1-2`, `+0-3`). The start counts as successful when the system
reaches `boot-complete.target`, which requires `xeeonos-boot-check.service`
(`xctl status --check`: `/usr` read-only and verity-protected, Btrfs data,
two `/usr` slots). Then `systemd-bless-boot` removes the counter
(`xeeonos_<version>.efi`): the version is good from then on.

If the attempts run out, the entry has 0 attempts left: systemd-boot considers
it bad, puts it last in the menu and starts the previous version.

So that a failed start does not stop at an emergency shell waiting for
someone, a failure in the initrd (e.g. `/usr` cannot be mounted) restarts the
computer while a version is on trial (`mkosi.initrd.conf/`), and a kernel panic
restarts it after 10 seconds (`panic=10`). A version already known to be good,
and the live ISO, still open the emergency shell.

## Manual: `xctl rollback`

```sh
sudo xctl rollback      # asks first, then marks the running version bad
                        # (xeeonos_<version>+0-<n>.efi); restart to go back
sudo xctl rollback --yes   # without asking (scripts)
```

It also works from the Recovery menu.

It goes to the newest older version that is not bad, and only if its `/usr`
slot is still on the disk.

A bad (turned off) version, whether it failed to start or you went back from
it, can always be installed again with `sudo xctl update`: the copy on the
disk is removed (its kernel leaves the boot menu, its `/usr` slot is labeled
`_empty`) and the version is downloaded and written again, since that copy
may be damaged. The daily check does not offer a turned-off version again,
so it does not come back every day; `xctl status` says it is turned off. To
have the daily check offer it anyway, write in `/etc/xeeonos/update.conf`:

```ini
OfferFailed=yes
```

`xctl status` shows the boot menu with the state of each version (good, new
with attempts left, bad) and which one is running.

## Tests

CI (`tools/ci/update.sh`), after the update test, on the same disk:

1. `xctl rollback --yes` on N+1, restart: N runs, N+1 is shown as bad.
2. The check does not offer N+1; `xctl update` writes it again; it starts.
3. Update to a broken N+2 (the start of its `/usr` overwritten, so dm-verity
   refuses it): it installs, fails to start 3 times, and N+1 starts again
   with N+2 shown as bad.
