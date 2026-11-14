# Applications from Flathub (Flatpak)

Extension `flatpak` (`extensions/flatpak`, see [extensions.md](extensions.md)):
included in the system image, on from the start, special (turning it off
asks first).

The system image cannot be changed, so applications are not installed into
it: they come from [Flathub](https://flathub.org/) as Flatpak applications.

- Flathub is already set up, **for the whole system**: an application is
  installed once, in `/var/lib/flatpak`, for every user. It is on the data
  partition, so applications stay across system updates and are updated on
  their own.
- **GNOME Software** is the app store: search, install, update and remove
  applications there. Installing asks for the password of an administrator.
  The permissions of each application are in Settings, Apps.
- From a terminal:

  ```
  flatpak search <name>
  flatpak install flathub <application>
  flatpak update
  flatpak uninstall <application>
  ```

No application is preinstalled for now.

## How Flathub is set up

`/usr/share/flatpak/remotes.d/flathub.flatpakrepo` (address and signing key
of Flathub, the file Flathub publishes). Flatpak adds it by itself as a
system remote.

## Tests

`tools/ci/flatpak.sh`: Flathub is there for the system and for the user
created by the setup, installed applications would appear in the menus, and
GNOME Software has its Flatpak support.
