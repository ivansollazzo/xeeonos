# Secure Boot and TPM unlock

With UEFI Secure Boot on, the firmware starts only boot loaders and kernels
signed with a key it trusts. XeeonOS signs every version with its own key,
so a computer that trusts it starts XeeonOS, and nothing changed by someone
else. On top of that, the TPM chip can open the encrypted disk by itself,
but only when XeeonOS started this way.

## What is signed

- the Unified Kernel Images (kernel, initrd, command line, also the
  Recovery profile): `/usr` is sealed by its verity hash, which is inside the
  UKI, so the signature covers the whole system;
- systemd-boot. mkosi signs the copy in `/usr`
  (`systemd-bootx64.efi.signed`). The installer puts it on the boot partition,
  and `systemd-boot-update.service` installs newer versions from there.

The live ISO's UKI is signed too.

## Enrolling the keys

A new computer trusts only Microsoft's keys, so the XeeonOS keys have to be
enrolled once:

1. In the firmware settings (often F2 or Del at power on), find Secure Boot
   and choose **Reset to Setup Mode**, or **Clear all Secure Boot keys**.
   Leave Secure Boot itself on if the firmware allows it.
2. Start XeeonOS and open the boot menu by holding Space while it starts.
   Choose **Enroll Secure Boot keys: auto**. systemd-boot warns, waits 15
   seconds (any key stops it), enrolls and restarts.
3. Secure Boot is now on. `xctl status` shows `Secure Boot: on`.

In a virtual machine, systemd-boot does this by itself at the first start.

The keys are in `loader/keys/auto/` on the boot partition (PK, KEK, db), and
in `/usr/share/xeeonos/secureboot/`:

- **PK** (owner of the platform): the XeeonOS key;
- **KEK**: the XeeonOS key and Microsoft's KEK (2011 and 2023), so that
  firmware and Windows updates of the revocation list keep working;
- **db** (allowed signers): the XeeonOS key and Microsoft's certificates
  (Windows 2011/2023, UEFI CA 2011/2023, Option ROM CA 2023).

Microsoft's certificates matter. Without them Windows, next to XeeonOS,
would stop starting, and so would the firmware of graphics and network
cards (option ROMs, signed by Microsoft): you could be left with no picture
at boot. They are in `secureboot/microsoft/` in the repository, taken from
Ubuntu's OVMF firmware package.

`xctl status` says what to do:

| `Secure Boot:` | Meaning |
|---|---|
| `on` | On, with the XeeonOS keys |
| `off; the firmware is ready for the XeeonOS keys ...` | Setup Mode: step 2 |
| `off (see docs/secure-boot.md ...)` | Off: step 1 |
| `off (this build is not signed)` | A development build without a key |

## TPM unlock

The data partition is always encrypted (see [encryption.md](encryption.md)).
With the disk password on, the TPM can also hold a key:

```
sudo xctl encryption tpm on     # asks the disk password (or the recovery key)
sudo xctl encryption tpm off
```

Both are also in Recovery → Disk encryption.

- The TPM gives the key back only when PCR 7 has the same value as at
  enrollment. PCR 7 records the Secure Boot state, the enrolled keys and
  the key that signed what started, so the disk opens by itself only when
  XeeonOS, signed by its key, started with Secure Boot on, on this
  computer. Updates are signed with the same key, so they do not change
  it.
- The password and the recovery key keep working. If the disk is moved to
  another computer, Secure Boot is turned off, or the keys change, the
  password is asked.
- The firmware's revocation list (dbx) is part of PCR 7. When an update
  changes it (firmware updates, `fwupdmgr`), the password is asked once.
  `sudo xctl encryption tpm on` then binds the key to the new state.
- It needs Secure Boot on: without it the TPM would open the disk for any
  system started on the computer. It also needs the password on, since
  otherwise the disk opens by itself anyway.
- How it works: `systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7`. In
  the initrd, systemd-cryptsetup tries the TPM first (`tpm2-device=auto`,
  with `tpm2-tss` and the TPM drivers in the initrd), then the empty
  password, then asks.

## The signing key

- **Pull requests** are signed with a throwaway key made for each build
  (`tools/secureboot.sh --test`). Their images are never published.
- **Channel builds** are signed with the XeeonOS key. Its private part is
  only in the repository secret `XEEONOS_SECUREBOOT_KEY` (and in the
  owner's backup). The certificate, which is public, is
  `secureboot/xeeonos.crt`.
- Until `secureboot/xeeonos.crt` exists, channel builds are not signed.
  Once it exists, a channel build without the secret stops: computers with
  the keys enrolled could not start an unsigned version.

Creating the key (once, on your own computer):

```
openssl req -new -x509 -newkey rsa:2048 -nodes -sha256 -days 7300 \
    -subj "/CN=XeeonOS Secure Boot/" \
    -keyout xeeonos-secureboot.key -out xeeonos.crt
```

RSA 2048 is what all firmware supports, and firmware does not check the
expiry date. Then:

1. On GitHub, in the repository, go to Settings → Secrets and variables →
   Actions → New repository secret. Name it `XEEONOS_SECUREBOOT_KEY`, and
   paste the whole content of `xeeonos-secureboot.key` as the value.
2. Commit `xeeonos.crt` as `secureboot/xeeonos.crt`.
3. Keep a backup of `xeeonos-secureboot.key` somewhere safe and offline.
   Every future version must be signed with it.

## Tests

`tools/ci/secureboot.sh` uses an OVMF firmware with Secure Boot that starts
without keys, plus a TPM emulator (swtpm). It checks four things:
- systemd-boot enrolls the keys and XeeonOS starts with Secure Boot on;
- with the password and TPM unlock on, the next start asks no password;
- an update still starts with Secure Boot, and the TPM still opens the
  disk;
- with another TPM, the password is asked.

The other tests run without Secure Boot, so they also check that a signed
build starts on a computer without it.
