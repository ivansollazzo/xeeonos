# .xapp applications (experimental)

Module `mkosi.profiles/xapp`. An `.xapp` is an application in one
directory: its own files and a manifest, `xapp.conf`. Applications live in
`/var/lib/xapps`, which every user sees as **`~/Applications`**:

- **install**: copy `Name.xapp` into `~/Applications` (Dolphin, `cp`), or
  `xctl app install Name.xapp` (a directory or an archive: `.tar`, `.tar.gz`,
  `.tar.xz`, `.tar.zst`). Administrators (the first user) need no `sudo`.
- **remove**: delete it from `~/Applications`, or `xctl app remove <name>`.
- A monitor (`xeeonos-xapps.path`) sees every change and puts the
  application in the menu, or takes it out (`xctl app sync`).
- `xctl app list`, `xctl app run <name> [arguments]`.

Applications are on the data partition: system updates do not touch them.

## xapp.conf

```
Name=Firefox                          # shown in the menu
Id=org.mozilla.firefox                # unique (reverse domain name)
Exec=firefox/firefox %u               # inside the .xapp directory
Icon=firefox/browser/chrome/icons/default/default128.png
Comment=Web browser
Categories=Network;WebBrowser;
MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;
Runtime=host
```

`Runtime=`:

- `host` (default): the application runs on the system's libraries, like
  Mozilla's Firefox build (GTK 3 is part of the desktop).
- `distrobox`: it runs in a container (distrobox, from the core module
  `tools`), one per user, named `xapps`, made at the first start (needs the
  network then) from Arch Linux, with the packages listed in `Packages=`.
  For applications that need libraries the system does not have.

## Example: Firefox

```
tools/mkxapp-firefox.sh out [it]     # newest Firefox from Mozilla
cp -r out/Firefox.xapp ~/Applications/
```

Firefox updates itself inside its directory, for administrators.

## Not yet

- signatures (an `.xapp` is trusted like any program you copy and run);
- a sandbox (applications run with the user's rights, like native ones);
- a store or catalog; a graphical installer.

## Tests

`tools/ci/xapp.sh`: Firefox made from Mozilla's build, installed by an
administrator without sudo, in the menu, starts (`--version`); a copy into
`~/Applications` and its deletion are picked up by the monitor.
