# Firewall

XeeonOS is closed to incoming connections from the first boot. Only these
come in:

- answers to connections this computer opened (web pages, updates, ...);
- the computer talking to itself (loopback);
- ping (ICMP), which networks need to work well;
- the network's automatic configuration (DHCP and DHCPv6).

Going out is free.

## Commands

    xctl firewall                  # on or off, and the open ports
    sudo xctl firewall allow 22/tcp
    sudo xctl firewall deny 22/tcp

A port is written `<number>/tcp` or `<number>/udp`. The list of open ports is
`/etc/xeeonos/firewall-allow`, one per line; it is kept across updates and
brought back by a snapshot restore ([snapshots.md](snapshots.md)).

## How it works

`xeeonos-firewall.service` runs `xctl firewall load` early at boot, before
the network starts. It loads one nftables table, `inet xeeonos`, with an
input chain whose policy is drop. `allow` and `deny` rewrite the list and
reload the table at once. Stopping the service removes the table (everything
open).

## Tests

`tools/ci/base.sh` checks that the service is active, that the input policy
is drop, and that `allow`/`deny` open and close a port.
