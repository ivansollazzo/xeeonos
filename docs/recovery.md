# Recovery

Every version in the boot menu has a second entry, **Recovery**. It starts
the same version, but instead of the normal system it shows the Recovery
menu on the screen, in black and white dialog windows:

1. **Reinstall XeeonOS**: downloads the newest version of your channel and
   installs it next to the running one. Users, files and settings stay.
   The system files of a version that starts are sealed and verified
   (dm-verity), so they cannot be damaged: reinstalling means moving to the
   newest version.
2. **Go back to the previous version**: the same as `xctl rollback`.
3. **Network settings**: the same window as in the first-boot setup
   (`xctl network`): choose a Wi-Fi network, or set a wired or Wi-Fi
   connection to automatic (DHCP) or to a manual address; "Advanced" opens
   `nmtui`. Saved networks already work in Recovery.
4. **Shell**: asks for the name and password of an administrator (a user of
   the `wheel` group), then opens a shell as that user; `sudo -i` gives root
   rights.
5. **Erase everything**: type `ERASE` to confirm. At the restart the data
   partition (users, settings, files: `/etc`, `/var`, `/home`) is erased and
   created again, empty; the computer starts as just after installation, with
   the first-boot setup ([setup.md](setup.md)). The system (`/usr`) is not
   touched.
6. **Restart**.

`xctl install-system` also works in Recovery (see [install.md](install.md)).

## Starting Recovery

- From the running system: `sudo xctl recovery` asks whether to restart into
  Recovery. Only the next boot starts Recovery; the one after is normal again.
- From the boot menu: hold **Space** while the computer starts, then choose
  **Recovery** under the version you want.

The menu never runs on the normal system: repairing a system while it runs is
not safe. If no version starts at all, use the installation medium
([install.md](install.md)).

## How it works

- The kernel image (UKI) of each version carries a second profile,
  `recovery` (`mkosi.conf.d/system/recovery.profile`), with the same kernel
  and `/usr` and two more kernel arguments: `xeeonos.recovery` (so that xctl
  knows) and `systemd.unit=xeeonos-recovery.target`.
- `xeeonos-recovery.target` starts the basic system, NetworkManager and
  `xeeonos-recovery.service`, which runs `xctl recovery` on the first console.
- `sudo xctl recovery` on the running system sets the Recovery entry of the
  running version as the boot menu's one-shot choice (`bootctl set-oneshot`)
  and restarts.
- "Erase everything" runs `systemd-factory-reset request`: an EFI variable
  that asks for a factory reset at the next boot. `systemd-repart` then erases
  the partitions marked `FactoryReset=yes` in `/usr/lib/repart.d`: only the
  data partition.
- Each action can also be run without the menu, in Recovery only:
  `xctl recovery <reinstall|rollback|network|shell|erase|reboot> [--yes]`.

## Tests

CI starts from the system set up by the first-boot test: `xctl recovery
--yes` restarts into Recovery, `xctl recovery erase --yes` asks for the
reset, and after the next restart the user is gone and the system is still
sealed.
