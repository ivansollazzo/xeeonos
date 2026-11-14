# Updates

XeeonOS updates the whole system at once, A/B style: the new version is
written into the free `/usr` slot while the current one keeps running, and it
starts at the next restart. Nothing is installed automatically by default.

## What an update is

One directory of files per version, built by CI with `tools/mkupdate.sh`:

| File | Content |
| --- | --- |
| `xeeonos_<v>.efi` | Unified Kernel Image (kernel + initrd + command line with the verity hash of its `/usr`) |
| `xeeonos_<v>.usr-x86-64.<uuid>.raw.zst` | the `/usr` partition (erofs) |
| `xeeonos_<v>.usr-x86-64-verity.<uuid>.raw.zst` | its dm-verity hash tree |
| `SHA256SUMS`, `SHA256SUMS.gpg` | manifest and its GnuPG signature |

## Where updates come from

Every push to `testing`, and every stable release, publishes its update to the GitHub container
registry as an OCI artifact, readable without login:

    ghcr.io/ivansollazzo/xeeonos:<version>   e.g. 26.12-testing.7, 26.12.2
    ghcr.io/ivansollazzo/xeeonos:<channel>   testing, stable

Pull requests never publish.

## On the system

```sh
xctl status                      # version, channel, /usr slots
xctl update --check              # is there a newer version? (no root, no download)
sudo xctl update                 # install the newest version of the channel
xctl update --from /run/media/usb/update        # from a directory
xctl update --from http://server/xeeonos/       # from a web directory
xctl channel                     # show the channel
xctl channel list                # the channels published on the registry
sudo xctl channel set testing    # switch channel (stored in /etc/xeeonos/channel)
sudo xctl channel reset          # back to the channel of the running version
```

`xctl update` downloads only the signed manifest first (a few hundred bytes)
and checks its signature with the keys in `/usr/lib/xeeonos/update-keyring.gpg`.
The manifest names the version (`xeeonos_<version>.efi`). If that version is
not newer than the running one, or already failed to start on this computer,
it stops there: nothing else is downloaded. Otherwise it downloads the files
the manifest lists, checks their hashes, and only then runs
`systemd-sysupdate`, which writes them into the free slot
(`/usr/lib/sysupdate.d/`). Updates only move forward: a channel with an older
version installs nothing. `--check` stops after the version check.

Before writing the new version, `xctl update` takes a snapshot of the
settings and users' files ([snapshots.md](snapshots.md)). A timer runs
`xctl update --check` every day and `xctl status` and the login message say
when a new version is available ([maintenance.md](maintenance.md)).

## Channels and versions

The channel is the branch, with the same name everywhere (branch, `xctl
channel`, registry tag). Versions follow the month (YY.MM: 26.11, then 27.1
in January):

| Branch / channel | What | Versions |
|---|---|---|
| `testing` | What comes next: new features, new Arch snapshots; every merge is published | `26.12-testing.7` |
| `stable` | Fixed versions; fixes come as backports from testing | `26.12.1`, `26.12.2`, `27.1.1` |

**Stable versions are published by hand.** A merge into `stable` is built
and tested (`26.12-rc.<n>`), never published. Actions → **Pubblica
versione** → Run workflow (on `stable`), `.github/workflows/release.yml`:

1. works out the number YY.MM.S: the month of today, S one more than the
   last version of this month (tags `v26.12.*`); S grows at every release
   (a new snapshot, something important, a backport);
2. builds and runs every test (`build.yml`);
3. only if all are green: publishes the signed update as `26.12.2` and on
   the channel `stable`, and the GitHub release `v26.12.2` with the ISO
   (which tags the commit `v26.12.2`). If anything is red, nothing is
   published.

**Two ISOs.** The **full ISO** carries the system and installs offline; at
2.6 GB it is over GitHub's 2 GiB limit for release files, so it stays in
the run's artifact `xeeonos-iso` (14 days). The **network ISO** carries only
the live system (about 750 MB): `xctl install-system` downloads the system
from the channel the ISO was built for, the same signed files as an update
(it asks to set up the network if there is none). Both are tested
(`Test iso-install`).

**The network ISO is on GitHub Releases**, the one the tests installed from,
with `SHA256SUMS` signed by the update signing key (`SHA256SUMS.gpg`):

| Channel | Release | Kept |
|---|---|---|
| `testing` | pre-release `v26.12-testing.7`, never "Latest" | only the newest (older testing pre-releases and their tags are deleted) |
| `stable` | release `v26.12.2`, marked "Latest" | all |

GitHub takes files up to 2 GiB: a bigger network ISO would not be published
(the run shows a warning); it stays in the run's artifact `xeeonos-iso-net`.

**Arch snapshots are chosen by hand.** Actions → **Nuovo snapshot** → Run
workflow (`.github/workflows/snapshot.yml`): the branch (testing)
and a date (empty: the newest). It opens a pull request with
the Arch news and the packages that change; merge it when it is green.
Nothing moves the snapshot by itself.

**Extensions** fit exactly one version (`SYSEXT_LEVEL=<version>`, see
[extensions.md](extensions.md)): every published version has its own.

Updates only go forward: a channel whose newest version is older than the
running one offers nothing until it passes it.

## Signing key

The current key is **XeeonOS updates**, fingerprint
`58AE 3043 5152 8B13 481B  1317 A7AC BBA9 8DC0 A7D8` (ed25519), committed in
`mkosi.conf.d/xctl/mkosi.extra/usr/lib/xeeonos/update-keyring.gpg`.

### One-time setup (done)

Updates are only published once the signing key exists. On your own computer:

```sh
gpg --quick-gen-key "XeeonOS updates" ed25519 sign never   # no passphrase
gpg --armor --export-secret-keys "XeeonOS updates"          # -> repository secret
gpg --export "XeeonOS updates" > update-keyring.gpg        # -> public key
```

1. Save the armored secret key as the repository secret `XEEONOS_SIGNING_KEY`
   (Settings → Secrets and variables → Actions).
2. Commit the public key as
   `mkosi.conf.d/xctl/mkosi.extra/usr/lib/xeeonos/update-keyring.gpg`.
3. After the first publish, make the `xeeonos` package public (GitHub →
   Packages → xeeonos → Package settings → Change visibility).

Images built before step 2 carry no key and can only be updated with
`xctl update --keyring <file>`.

## Tests

CI builds every change twice (version N and N+1), boots N, updates it to N+1
over HTTP with a throwaway key, and boots again to check that N+1 runs from
the other slot (`tools/ci/update.sh`). Against a source holding only the
signed manifest (no images), `--check` must report the new version before the
update, and once updated both `--check` and `xctl update` must say "up to
date": had they tried to download anything, they would have failed. Rollback tests follow (see [rollback.md](rollback.md)).
