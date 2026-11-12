# First boot

At the first boot, before the login prompt, `xctl setup` asks, in black and
white dialog windows ("Back" goes to the previous step):

- the **language** (a short list, or any other locale name such as
  `nl_NL.UTF-8`: every language is in the image);
- the **keyboard layout** (suggested from the language);
- the **time zone** (suggested from the language; region, then city);
- the **network**: a wired or Wi-Fi connection, automatic (DHCP) or with a
  manual address, gateway and DNS (`xctl network`, see below); "Done" skips
  it;
- the **computer name**;
- the **first user**: full name, user name, password, and whether it is an
  administrator (member of `wheel`, can use `sudo`).

Language, keyboard and time zone take effect as soon as they are chosen:
after choosing the Italian keyboard, the next answer is already typed with
it. The account is created at the end.

The root account stays locked: administrators use `sudo`.

Everything is written to `/etc` and `/home` on the data partition; the sealed
`/usr` is not touched. The setup runs again after a factory reset
([recovery.md](recovery.md)), until it completes once
(`/var/lib/xeeonos/setup-done`).

## Changing things later

- Language: `sudo localectl set-locale LANG=de_DE.UTF-8` (from the next
  login). Every UTF-8 language is installed; nothing to generate.
- Keyboard: `sudo localectl set-keymap de`.
- Time zone: `sudo timedatectl set-timezone Europe/Berlin`.
- Computer name: `sudo hostnamectl hostname my-pc`.
- Network: `sudo xctl network` (the same window as in the setup), or
  `nmtui`. Without a terminal:
  `sudo xctl network ethernet --address 192.168.1.20/24 --gateway 192.168.1.1 --dns 1.1.1.1`,
  `sudo xctl network ethernet --dhcp`,
  `sudo xctl network wifi "My network" --password ...`,
  `xctl network status`.
- Users: `useradd`, `passwd`, `usermod -aG wheel <user>` as usual.

With Plasma (Phase 2) the same settings are in System Settings, and the
setup becomes a graphical assistant.

## Unattended setup

Pass the answers as the system credential `xeeonos.setup`, a file of
`key=value` lines:

```
language=it_IT.UTF-8
keyboard=it
timezone=Europe/Rome
hostname=my-pc
user=ivan
fullname=Ivan
password=...
admin=yes
```

`keyboard`, `timezone`, `hostname`, `fullname` and `admin` are optional. CI
uses this (`tools/boot-test.sh --setup <file>`).

## What is not in the image

XeeonOS changes only as a whole (`xctl update`), so Arch's tools for
installing and configuring packages are removed at the end of the build:
pacman and its configuration, the mirror list and keyring, the package
database, pacman hooks, `locale-gen`. Files only used to compile programs
(`/usr/include`, static libraries, pkg-config files) are removed too:
development happens in containers (distrobox).

`xctl packages [name]` lists the packages of the running version, with
their versions; the same list is published with every update
(`xeeonos_<version>.manifest`).
