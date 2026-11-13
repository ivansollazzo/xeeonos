# Tools: SSH, AppImage

Module `mkosi.conf.d/tools`, part of every installed XeeonOS (not of the
live ISO).

## SSH

The client (`ssh`, `scp`, `ssh-keygen`) works as usual.

The server is installed but off, and the firewall is closed. To reach this
computer from another one:

```
sudo systemctl enable --now sshd
sudo xctl firewall allow 22/tcp
```

and to turn it off again:

```
sudo systemctl disable --now sshd
sudo xctl firewall deny 22/tcp
```

Its settings and keys are in `/etc/ssh` (kept across updates).

## AppImage

Applications distributed as a single `.AppImage` file run as they are:

```
chmod +x Application.AppImage
./Application.AppImage
```

They need FUSE 2 (`libfuse.so.2`, `fusermount`), which is part of this
module. For everything else, prefer Flatpak (Discover).

## Containers

The system image cannot be changed, so software that is not an application
(compilers, command line tools, another distribution's packages) lives in
containers, in your home directory. podman and distrobox are optional
extensions ([xeeonos-extensions](https://github.com/ivansollazzo/xeeonos-extensions)):

```
sudo xctl ext enable distrobox     # brings podman with it
distrobox create --name dev --image archlinux:latest
distrobox enter dev
```

- **podman** (`sudo xctl ext enable podman` alone) runs the containers,
  without root: each user has their own, in `~/.local/share/containers`.
- **distrobox** makes a container feel like the system: same home
  directory, same user, graphical applications included.

This module gives every user the ranges of user IDs that containers
without root need (`/etc/subuid`, `/etc/subgid`), so turning an extension
on needs nothing else.

## Tests

`tools/ci/tools.sh`: the user created by the setup has ID ranges for
containers; the SSH client is there; the SSH server is off; FUSE 2 is there
for AppImage files.
