# Tools: SSH, AppImage

Module `mkosi.conf.d/tools`, part of every installed XeeonOS (not of the
live ISO).

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
(compilers, command line tools, another distribution's packages) lives in
containers, in your home directory. podman and distrobox are optional
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

`tools/ci/tools.sh`: the user created by the setup has ID ranges for
containers; the SSH client is there; the SSH server is off, and
`xctl ssh-server on|off` turns it and its port on and off; FUSE 2 is there
for AppImage files.
