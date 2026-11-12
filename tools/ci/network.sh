#!/usr/bin/env bash
# Network settings: a manual address, then back to DHCP.
# (NetworkManager is started by hand: systemd.run= replaces the boot target.)
source "$(dirname "$0")/common.sh"
fresh_disk
tools/boot-test.sh \
    --run "/bin/sh -c 'systemctl start NetworkManager.service && nm-online -s -q -t 60; xctl network ethernet --address 10.0.2.50/24 --gateway 10.0.2.2 --dns 10.0.2.3 && xctl network status | sed s,^,MANUAL-,; xctl network ethernet --dhcp && xctl network status | sed s,^,DHCP-,'" \
    --expect "MANUAL-Wired .*10\.0\.2\.50/24" \
    --expect "DHCP-Wired .*10\.0\.2\.15/24" \
    out/disk.raw out/serial-network.log
