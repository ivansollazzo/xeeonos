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
sudo xctl rollback      # the running version is marked bad
                        # (xeeonos_<version>+0-<n>.efi); restart to go back
```

It goes to the newest older version that is not bad, and only if its `/usr`
slot is still on the disk.

A bad version is not installed again by `xctl update`: it waits for a newer
one.

`xctl status` shows the boot menu with the state of each version (good, new
with attempts left, bad) and which one is running.

## Tests

CI (`tools/ci/update.sh`), after the update test, on the same disk:

1. `xctl rollback` on N+1, restart: N runs, N+1 is shown as bad.
2. Update to a broken N+2 (the start of its `/usr` overwritten, so dm-verity
   refuses it): it installs, fails to start 3 times, and N starts again with
   N+2 shown as bad.
