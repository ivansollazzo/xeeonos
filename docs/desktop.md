# Desktop: KDE Plasma

The published XeeonOS images have the KDE Plasma desktop (module
`mkosi.profiles/plasma`), kept minimal:

- the **Plasma session on Wayland** (X11 applications run through XWayland);
- the **login screen**: SDDM, also on Wayland (drawn by KWin, no X server).
  It starts after the first-boot setup (`xctl setup`), which creates the
  first user;
- settings for **network** (NetworkManager), **sound**, **Bluetooth**,
  **screens** and **power** (power profiles), all in System Settings. Sound
  (PipeWire) is the module `audio`, and Bluetooth is part of the core
  system, the same for every desktop;
- **printers** in System Settings (module `printing`, see
  [printing.md](printing.md));
- the applications that must be part of the system: **Konsole** (terminal),
  **Dolphin** (files), **Spectacle** (screenshots), **System Monitor**
  (processes and resources) and **Ark** (archives). Every other application
  comes from Flathub, with **Discover** as the app store (module `flatpak`,
  see [flatpak.md](flatpak.md)), and later as `.xapp` bundles, not inside
  the system image;
- graphics with Mesa for Intel, AMD and Nouveau (OpenGL, Vulkan, video
  decoding);
- Noto fonts for every language, emoji included.

The keyboard chosen at setup is also the layout of the login screen and of
the first user's desktop (`/etc/X11/xorg.conf.d/00-keyboard.conf` and
`~/.config/kxkbrc`).

## Settings

- The login screen: `/usr/lib/sddm/sddm.conf.d/xeeonos.conf`; a file in
  `/etc/sddm.conf.d/` overrides it (for example automatic login:
  `[Autologin]` `User=<name>` `Session=plasma`).
- Like everything in `/usr`, the desktop changes only with `xctl update`.
- The partitions of the system itself (system slots, data partition) are
  not listed as disks in Dolphin
  (`/usr/lib/udev/rules.d/85-xeeonos-system-disks.rules`).

## Without a desktop

The desktop is a module: `mkosi build` without `--profile=plasma` gives the
same system with a text console only, and another desktop would be another
module next to it. Recovery and the installer stay text-based.

## Tests

`tools/ci/desktop.sh` starts an image after an unattended setup and checks
the login screen, the keyboard layout, then an automatic login into Plasma
with sound. The virtual machine has no GPU: KWin draws with Mesa's software
renderer.
