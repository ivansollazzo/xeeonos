# CI tests

One script per test job. Each one starts from a clean copy of what the build
job made, so the jobs are independent and run in parallel
(`.github/workflows/build.yml`, job `test`). A failed job can be re-run on its
own with "Re-run failed jobs".

**Targeted tests.** On a pull request, each test job first checks
(`affected.sh`) whether the pull request changed a file the test depends on
(`affects.txt`); if not, it finishes green at once, marked "not touched" in
the job summary (so the required checks are still satisfied). Changes to
the system's core, the build or the test machinery run every test, and so
does any file `affects.txt` does not know (documentation aside). Pushes to
channel branches (after a merge, before publishing) always run every test.

| Script | What it tests | Uses |
|---|---|---|
| `update.sh` | first boot, version check without download, A/B update, rollback, automatic fallback after a broken update | disk image, next version |
| `setup.sh` | first-boot setup (unattended), no Arch tools | disk image |
| `recovery.sh` | Recovery: restart into it, then erase everything | disk image |
| `network.sh` | network settings: manual address, then DHCP | disk image |
| `base.sh` | zram and swap file, systemd-oomd, firewall (closed, open/close a port), scheduled jobs, disk health, pstore, fwupd, `xctl report` | disk image |
| `snapshots.sh` | take a data snapshot, change files, restore it from Recovery | disk image |
| `desktop.sh` | GNOME: login screen (GDM on Wayland) after the setup, keyboard layout from the setup, GNOME session with that input source and sound, the module's services and helpers (Disks, fprintd, bolt, gvfs backends, OpenVPN, switcheroo-control), system partitions hidden from the file manager | disk image |
| `tools.sh` | every command line tool and Python (venv with pip, pipx, system protected from pip), module drivers (file system tools, firmware), SSH client, SSH server off, FUSE 2 (AppImage), user ID ranges for containers (extensions podman, distrobox) | disk image |
| `flatpak.sh` | Flatpak: Flathub set up for the whole system and for the user, GNOME Software with Flatpak support | disk image |
| `scanner.sh` | scanners: SANE with the driverless backend (airscan), ipp-usb, Avahi, Document Scanner, extension included and on | disk image |
| `printing.sh` | printing: CUPS on demand, add a printer, Avahi and `.local` names, Printers page of Settings (cups-pk-helper) | disk image |
| `extensions.sh` | system extensions: included ones on, special ones ask, the test extension ci-hello downloaded (signed, with its manifest), removed, installed from its file (with a warning) | disk image, system |
| `hibernate.sh` | hibernate into the swap file, start again: the session resumes | disk image |
| `secureboot.sh` | Secure Boot (firmware without keys): keys enrolled, signed start, TPM unlock without password, signed update, another TPM asks the password | disk image, next version |
| `encryption.sh` | disk password on and off (recovery key), then a normal start | disk image |
| `iso-install.sh` | live ISO boots, installs on a blank disk, installed system boots | live image, system |
| `iso-alongside.sh` | install next to another system | live image, system |

Environment: `VERSION` (and `NEXT` for update.sh) from `tools/version.sh`;
the build job's artifacts in `build/`; boot logs go to `out/serial-*.log`.
To add a feature's tests: add a script here, a line to the job's matrix and
its paths to `affects.txt`.
