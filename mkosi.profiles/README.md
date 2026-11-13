# Optional modules

Each directory here is an optional XeeonOS module (an mkosi profile), enabled with
`mkosi --profile=<module>[,<module>...] build`. A module holds its own `mkosi.conf`
and, when needed, `mkosi.extra/` (files), `mkosi.postinst` (setup script) and so on.

Modules: `live` (the live ISO), `plasma` (KDE Plasma desktop), `audio`
(sound: PipeWire).
Planned: `flatpak` (Flatpak + Flathub), `xapp` (`.xapp` applications), `dev`
(distrobox and podman).
