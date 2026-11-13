# CI tests

One script per test job. Each one starts from a clean copy of what the build
job made, so the jobs are independent and run in parallel
(`.github/workflows/build.yml`, job `test`). A failed job can be re-run on its
own with "Re-run failed jobs".

| Script | What it tests | Uses |
|---|---|---|
| `update.sh` | first boot, version check without download, A/B update, rollback, automatic fallback after a broken update | disk image, next version |
| `setup.sh` | first-boot setup (unattended), no Arch tools | disk image |
| `recovery.sh` | Recovery: restart into it, then erase everything | disk image |
| `network.sh` | network settings: manual address, then DHCP | disk image |
| `base.sh` | zram and swap file, systemd-oomd, firewall (closed, open/close a port), scheduled jobs, disk health, pstore, fwupd, `xctl report` | disk image |
| `snapshots.sh` | take a data snapshot, change files, restore it from Recovery | disk image |
| `desktop.sh` | KDE Plasma: login screen (SDDM on Wayland) after the setup, keyboard layout from the setup, Plasma session with sound, system partitions hidden from the file manager | disk image |
| `tools.sh` | containers without root for the user of the setup (podman, distrobox), SSH client, SSH server off | disk image |
| `flatpak.sh` | Flatpak: Flathub set up for the whole system and for the user, Discover with Flatpak support | disk image |
| `printing.sh` | printing: CUPS on demand, add a printer, Avahi and `.local` names, Plasma's Printers page | disk image |
| `hibernate.sh` | hibernate into the swap file, start again: the session resumes | disk image |
| `secureboot.sh` | Secure Boot (firmware without keys): keys enrolled, signed start, TPM unlock without password, signed update, another TPM asks the password | disk image, next version |
| `encryption.sh` | disk password on and off (recovery key), then a normal start | disk image |
| `iso-install.sh` | live ISO boots, installs on a blank disk, installed system boots | live image, system |
| `iso-alongside.sh` | install next to another system | live image, system |

Environment: `VERSION` (and `NEXT` for update.sh) from `tools/version.sh`;
the build job's artifacts in `build/`; boot logs go to `out/serial-*.log`.
To add a feature's tests: add a script here and a line to the job's matrix.
