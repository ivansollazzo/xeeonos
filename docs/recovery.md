# Recovery (factory reset)

Every version in the boot menu has a second entry, **Recovery (erases
all data)**. Starting it erases the data partition (users, settings,
files: `/etc`, `/var`, `/home`) and creates it again, empty. The operating
system (`/usr`) is not touched, so the computer starts as just after
installation, with the first-boot setup ([setup.md](setup.md)).

The boot menu is hidden by default: hold **Space** while the computer starts
to show it.

## How it works

The kernel image (UKI) of each version carries a second profile,
`recovery` (`mkosi.conf.d/system/recovery.profile`), with the same
kernel and `/usr` and one more kernel argument, `systemd.factory_reset=1`.
`systemd-repart` sees it and erases every partition marked `FactoryReset=yes`
in `/usr/lib/repart.d`: only the data partition.

## Tests

CI boots with `systemd.factory_reset=1` after the first-boot setup test, and
checks that the user created by the setup is gone and that the boot menu
lists the Recovery entries.
