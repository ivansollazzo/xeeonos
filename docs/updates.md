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

Every push to a channel branch publishes its update to the GitHub container
registry as an OCI artifact, readable without login:

    ghcr.io/ivansollazzo/xeeonos:<version>   e.g. 0.1.0-experimental.42
    ghcr.io/ivansollazzo/xeeonos:<channel>   e.g. experimental, 1.x-beta, 1.x-stable

Pull requests never publish.

## On the system

```sh
xctl status                      # version, channel, /usr slots
xctl update                      # newest version of the channel
xctl update --from /run/media/usb/update        # from a directory
xctl update --from http://server/xeeonos/       # from a web directory
xctl channel                     # show the channel
sudo xctl channel 1.x-beta       # switch channel (stored in /etc/xeeonos/channel)
```

`xctl update` downloads the signed manifest first, checks its signature with
the keys in `/usr/lib/xeeonos/update-keyring.gpg`, downloads the files it
lists, checks their hashes, and only then runs `systemd-sysupdate`, which
writes them into the free slot (`/usr/lib/sysupdate.d/`). Updates only move
forward: a channel with an older version installs nothing.

## Channels and versions

The channel is the branch: `experimental`, then `<N>.x/alpha`, `<N>.x/beta`,
`<N>.x/stable`. `mkosi.version` holds the base version, and on a `<N>.x/*`
branch it must start with `N.` (the channel tag is derived from it).

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
the other slot. Rollback tests follow (see [rollback.md](rollback.md)).
