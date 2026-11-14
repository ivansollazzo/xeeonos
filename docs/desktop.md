# Desktop: GNOME

The published XeeonOS images have the GNOME desktop (module
`mkosi.profiles/gnome`), kept minimal:

- **GNOME Shell on Wayland** (X11 applications run through XWayland);
- the **login screen**: GDM, also on Wayland. It starts after the
  first-boot setup (`xctl setup`), which creates the first user;
- **Settings** for network (NetworkManager), sound, Bluetooth, screens,
  power (power profiles) and printers, all in one place. Sound (PipeWire) is
  the module `audio`, Bluetooth is part of the core system, printers come
  with the extension `printing` (see [printing.md](printing.md)), scanners
  with the extension `scanner` (see [scanner.md](scanner.md));
- the applications that must be part of the system: **Console** (terminal),
  **Files** (Nautilus, which also opens archives) and **System Monitor**
  (processes and resources), and **Disks** (format USB sticks, disk
  health, disk images). Screenshots are part of GNOME Shell. Every
  other application comes from Flathub, with **GNOME Software** as the app
  store (extension `flatpak`, see [flatpak.md](flatpak.md));
- the services GNOME uses by itself and shows in Settings: the
  **fingerprint reader** (fprintd: login and unlock), **Thunderbolt and
  USB4** devices (bolt), automatic **rotation and brightness**
  (iio-sensor-proxy), "Launch using Discrete Graphics Card" on laptops with
  **two GPUs** (switcheroo-control), **OpenVPN** connections, files over
  **Bluetooth**;
- in Files: **phones** (MTP) and cameras, **iPhones**, **shared folders**
  of Windows, NAS and NFS;
- graphics with Mesa for Intel, AMD and Nouveau (OpenGL, Vulkan, video
  decoding); `glxinfo` and `vulkaninfo` tell which GPU draws;
- the interface font is Adwaita Sans (part of GNOME); Noto fonts cover every
  other language, emoji included.

The keyboard chosen at setup is also the layout of the login screen
(`/etc/X11/xorg.conf.d/00-keyboard.conf`, read by systemd-localed), and
GNOME makes it the first user's input source at their first login.

## Settings

- The login screen: `/etc/gdm` is a link to the defaults in
  `/usr/share/factory/etc/gdm`. To change them (for example automatic
  login: `[daemon]` `AutomaticLoginEnable=True` `AutomaticLogin=<name>` in
  `custom.conf`), replace the link with a copy.
- Like everything in `/usr`, the desktop changes only with `xctl update`.
- The partitions of the system itself (system slots, data partition) are
  not listed as disks in Files
  (`/usr/lib/udev/rules.d/85-xeeonos-system-disks.rules`).

## Without a desktop

The desktop is a module: `mkosi build` without `--profile=gnome` gives the
same system with a text console only, and another desktop would be another
module next to it. Recovery and the installer stay text-based.

## Tests

`tools/ci/desktop.sh` starts an image after an unattended setup and checks
the login screen and the keyboard layout, then an automatic login into
GNOME with that keyboard as input source and with sound. The virtual
machine has no GPU: GNOME Shell draws with Mesa's software renderer.
