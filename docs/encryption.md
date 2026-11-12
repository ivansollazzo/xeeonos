# Disk encryption

The data partition (users, settings, files: `/etc`, `/var`, `/home`) is
**always encrypted** with LUKS2. The system (`/usr`) is not: it holds no
personal data, is the same on every computer, and is sealed (dm-verity), so
it cannot be changed without that being noticed.

What changes is only whether a **disk password** is asked:

- **Off** (the default): the encryption key is protected by an empty
  password. The computer starts without asking anything, as if the disk were
  not encrypted. A stolen disk can be read.
- **On**: a password is asked every time the computer starts, before
  anything else. A **recovery key** is created as well and shown once: it
  opens the disk if the password is forgotten. Without one of the two, the
  data cannot be read.

The installer asks which one you want. If you choose "on", the first-boot
setup asks for the disk password at the end (it offers your account
password) and shows the recovery key.

## Turning it on or off

```sh
sudo xctl encryption              # a window: on, on + re-encrypt, or off
xctl encryption status
sudo xctl encryption on           # asks for the new password; shows the recovery key
sudo xctl encryption on --reencrypt
sudo xctl encryption off          # asks for the password (or the recovery key)
```

The same is in Recovery ("Disk password").

Turning it on or off is **instant**: only the keys stored in the LUKS header
change, not the data. With **on + re-encrypt** the whole partition is also
re-encrypted with a new key while the computer is in use (it takes a while).
This matters only if someone may have copied the disk while the password was
off: with the old key, such a copy could still be opened.

The password is typed at the start with the **US keyboard layout** (letters
and digits are in the same place as on most keyboards; some symbols are not).

## How it works

- `systemd-repart` creates the data partition at the first boot (and again
  after "Erase everything" in Recovery) with `Encrypt=key-file` and no key
  file: an empty key, in a key slot with a fast key derivation (there is
  nothing to protect).
- In the initrd, systemd opens it as `/dev/mapper/root`; a drop-in for
  `systemd-cryptsetup@root.service` adds `try-empty-password=yes`, so an
  empty password is tried first. If it fails, the password is asked on the
  console.
- **On**: `systemd-cryptenroll` adds the password and a recovery key, then
  removes the empty key slot.
- **Off**: `cryptsetup luksAddKey` adds an empty key slot (checked with the
  current password), then `systemd-cryptenroll` removes the other slots.
- No TPM for now: it will come with Secure Boot.

Systems installed before encryption existed have an unencrypted data
partition; `xctl encryption` says so. Reinstalling makes it encrypted.

## Tests

CI (test 11) checks that the data partition is encrypted with the password
off, turns the password on, reads the recovery key, turns the password off
with the recovery key, and starts the computer again without a password.
