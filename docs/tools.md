# Tools: containers and SSH

Module `mkosi.conf.d/tools`, part of every installed XeeonOS (not of the
live ISO).

## Containers

The system image cannot be changed, so software that is not an application
(compilers, command line tools, another distribution's packages) lives in
containers, in your home directory:

```
distrobox create --name dev --image archlinux:latest
distrobox enter dev
```

- **podman** runs the containers, without root: each user has their own,
  in `~/.local/share/containers`.
- **distrobox** makes a container feel like the system: same home
  directory, same user, graphical applications included. `.xapp`
  applications are built on it.

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

## Tests

`tools/ci/tools.sh`: the user created by the setup has ID ranges for
containers and podman works without root; distrobox and the SSH client are
there; the SSH server is off.
