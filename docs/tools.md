# Tools: command line, Python, SSH, AppImage

Module `mkosi.conf.d/tools`, part of every installed XeeonOS (not of the
live ISO).

## Command line

The tools that guides and scripts take for granted:

| Area | Commands |
| --- | --- |
| Files and archives | `which`, `tree`, `rsync`, `zip`, `unzip`, `7z`, `ncdu`, `lsof`, `bc`, `jq`, `vim` |
| Help | `man` (pages offline), Tab completion of options (`bash-completion`) |
| Sessions | `tmux`, `screen`: they survive a dropped SSH connection |
| Hardware | `lsusb`, `dmidecode`, `nvme`, `sensors` (temperatures, fans), `powertop` |
| Processes | `htop`, `strace` |
| Network | `wget`, `dig`, `nslookup`, `traceroute`, `mtr`, `iw`, `ethtool`, `hostname`, `nc`, `whois` |
| Code | `git` |

Their defaults (`/etc/vimrc`, `/etc/screenrc`, `/etc/sensors3.conf`, ...)
are links to `/usr/share/factory/etc`: to change one, replace the link with
a copy, or use the file in your home (`~/.vimrc`, ...).

## Python

`python`, with `pip` and `pipx`. `/usr` is read-only, so `sudo pip install`
cannot change the system (pip refuses, and says so). Instead:

```
python -m venv .venv               # libraries for a project, in a venv
.venv/bin/pip install requests
pipx install yt-dlp                # a command line program, in ~/.local/bin
pipx upgrade-all
```

To build programs (compilers, headers) there is the optional extension
`dev-tools`; whole development environments belong in a container (below).

## SSH

The client (`ssh`, `scp`, `ssh-keygen`) works as usual.

The server is installed but off, and the firewall is closed. To reach this
computer from another one:

```
sudo xctl ssh-server on      # starts sshd (also at every boot) and opens 22/tcp
xctl ssh-server              # on or off?
sudo xctl ssh-server off     # stops it and closes the port again
```

The service and the firewall port always go together.

Its settings and keys are in `/etc/ssh` (kept across updates).

## AppImage

Applications distributed as a single `.AppImage` file run as they are:

```
chmod +x Application.AppImage
./Application.AppImage
```

They need FUSE 2 (`libfuse.so.2`, `fusermount`), which is part of this
module. For everything else, prefer Flatpak (GNOME Software).

## Containers

The system image cannot be changed, so software that is not an application
(a project's toolchain and libraries, another distribution's packages) lives
in containers, in your home directory. podman and distrobox are optional
extensions ([xeeonos-extensions](https://github.com/ivansollazzo/xeeonos-extensions)):

```
sudo xctl extensions enable distrobox     # brings podman with it
distrobox create --name dev --image archlinux:latest
distrobox enter dev
```

- **podman** (`sudo xctl extensions enable podman` alone) runs the containers,
  without root: each user has their own, in `~/.local/share/containers`.
- **distrobox** makes a container feel like the system: same home
  directory, same user, graphical applications included.

This module gives every user the ranges of user IDs that containers
without root need (`/etc/subuid`, `/etc/subgid`), so turning an extension
on needs nothing else.

## Tests

`tools/ci/tools.sh`: every command line tool is there; Python makes a
venv with pip, and pip cannot install into the system; the module drivers'
tools and firmware are there; the user created by the setup has ID ranges for
containers; the SSH client is there; the SSH server is off, and
`xctl ssh-server on|off` turns it and its port on and off; FUSE 2 is there
for AppImage files.
