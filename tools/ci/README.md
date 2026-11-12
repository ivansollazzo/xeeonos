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
| `base.sh` | zram swap, firewall (closed, open/close a port), scheduled jobs, fwupd, `xctl report` | disk image |
| `snapshots.sh` | take a data snapshot, change files, restore it from Recovery | disk image |
| `encryption.sh` | disk password on and off (recovery key), then a normal start | disk image |
| `iso-install.sh` | live ISO boots, installs on a blank disk, installed system boots | live image, system |
| `iso-alongside.sh` | install next to another system | live image, system |

Environment: `VERSION` (and `NEXT` for update.sh) from `tools/version.sh`;
the build job's artifacts in `build/`; boot logs go to `out/serial-*.log`.
To add a feature's tests: add a script here and a line to the job's matrix.
