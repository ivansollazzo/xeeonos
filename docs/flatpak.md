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

## Default applications

The desktop brings a list of applications to install from Flathub
(`/usr/share/flatpak/preinstall.d/`, format
[flatpak-preinstall(1)](https://man.archlinux.org/man/flatpak-preinstall.1)).
With GNOME (`mkosi.profiles/gnome`):

| Application | Flathub ID |
| --- | --- |
| Firefox (web) | `org.mozilla.firefox` |
| Convey (mail) | `net.donnybeelo.Convey` |
| Text Editor | `org.gnome.TextEditor` |
| Image Viewer (Loupe) | `org.gnome.Loupe` |
| Document Viewer (Papers) | `org.gnome.Papers` |
| Calculator | `org.gnome.Calculator` |
| Video Player (Showtime) | `org.gnome.Showtime` |
| Camera (Snapshot) | `org.gnome.Snapshot` |
| Calendar | `org.gnome.Calendar` |
| Contacts | `org.gnome.Contacts` |
| Disk Usage Analyzer | `org.gnome.baobab` |
| Characters | `org.gnome.Characters` |
| Fonts | `org.gnome.font-viewer` |
| Extension Manager | `com.mattjakeman.ExtensionManager` |
| Flatseal (permissions) | `com.github.tchx84.Flatseal` |

- The first-boot setup asks whether to install them (default: yes).
- They are downloaded **in the background, as soon as there is a network**:
  `xeeonos-flatpak-preinstall.timer` tries at boot and every 15 minutes
  (`/usr/lib/xeeonos/flatpak-preinstall`, `flatpak preinstall`). Without
  a network nothing happens and it tries again later; an interrupted
  download continues.
- Once installed, it stops. An update whose list adds applications installs
  those. An application the user removes is **never installed again**
  (Flatpak remembers it).
- `xctl status` shows how far they are (`Apps:`).
- An administrator can add a list of their own in
  `/etc/flatpak/preinstall.d/`.

## How Flathub is set up

`/usr/share/flatpak/remotes.d/flathub.flatpakrepo` (address and signing key
of Flathub, the file Flathub publishes). Flatpak adds it by itself as a
system remote.

## Tests

`tools/ci/flatpak.sh`: Flathub is there for the system and for the user
created by the setup, installed applications would appear in the menus, and
GNOME Software has its Flatpak support; the GNOME list has its 15
applications, the service is enabled, the setup's "no" is kept and shown by
`xctl status` (nothing is downloaded in CI).
