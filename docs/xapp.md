# .xapp applications (experimental)

Module `mkosi.profiles/xapp`. An `.xapp` is an application that is **a whole
container, ready to run**: everything it needs (an Arch Linux base, its
libraries, the application) is inside, so it works the same on every
XeeonOS version and needs no network to start. It is a directory:

```
Firefox.xapp/
  xapp.conf        the manifest
  image.tar.zst    the container image (docker/podman save archive, zstd)
  icon.png         the icon of the menu
```

Applications live in `/var/lib/xapps`, which every user sees as
**`~/Applications`**:

- **install**: copy `Name.xapp` into `~/Applications` (Dolphin, `cp`), or
  `xctl app install Name.xapp.tar` (a directory or an archive).
  Administrators (the first user) need no `sudo`.
- **remove**: delete it from `~/Applications`, or `xctl app remove <name>`.
- `xctl app list` (with "ready" when your container is prepared),
  `xctl app run <name> [arguments]`.

## What happens by itself

- The **system's monitor** (`xeeonos-xapps.path`, `xctl app sync`) sees every
  change in `/var/lib/xapps` and puts the application in the menu, or takes
  it out. An application still being copied is left alone until the copy is
  over.
- Each **user's monitor** (the user unit `xeeonos-xapps.path`, `xctl app
  sync --user`, also at every login) imports the image into the user's
  podman and makes the application's own distrobox container,
  `xapp-<id>`, in the background, with a notification when it is ready.
  A new version (a different image) gets a new container; when the
  application is gone, its container and image are deleted.
- `xctl app run` (what the menu entry starts) runs the application in its
  container; if it is not ready yet (the user was quicker than the monitor,
  or the .xapp was replaced in place), it is prepared first, with a
  notification.

One container per application: each has its own libraries and cannot break
the others. The user's files stay in the home directory, which the
container shares: a new container loses nothing. Applications built on the
same base share its layers in podman.

Applications are on the data partition: system updates do not touch them.

## xapp.conf

```
Name=Firefox                          # shown in the menu
Id=org.mozilla.firefox                # unique (reverse domain name)
Version=145.0-1                       # shown by xctl app list
Exec=/usr/bin/firefox %u              # the command inside the container
Icon=icon.png                         # inside the .xapp directory
Comment=Web browser
Categories=Network;WebBrowser;
MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;
```

## Making one

A recipe is a directory with a `Containerfile` and an `xapp.conf`; the
recipes live in the `xeeonos-applications` repository. The image must hold
what distrobox needs inside a container (see the test recipe), so that the
first start needs no network.

```
tools/mkxapp.sh <recipe dir> out      # out/Name.xapp and out/Name.xapp.tar
```

Two keys are for `tools/mkxapp.sh` only: `IconFrom=` (the icon's path in the
image) and `VersionFrom=` (the package whose version is the application's).

## Not yet

- signatures (an `.xapp` is trusted like any program you copy and run);
- a sandbox (applications run with the user's rights, through distrobox);
- a store or catalog; a graphical installer; a progress window (there are
  notifications).

## Tests

`tools/ci/xapp.sh`, with the recipe `tools/ci/xapp-firefox`: Firefox made
as a container, installed by an administrator without sudo, in the menu,
its container made by the user's monitor without network, Firefox starts
(`--version`); a copy into `~/Applications` gets its own menu entry and
container, its deletion removes both.
