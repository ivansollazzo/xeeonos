#!/usr/bin/env bash
# Network settings: a manual address, then back to DHCP.
# (NetworkManager is started by hand: systemd.run= replaces the boot target.)
source "$(dirname "$0")/common.sh"
fresh_disk
cat > out/network-test.sh <<'SCRIPT'
# NetworkManager reports a connection as activated a moment before its
# address shows (with KVM the test is fast enough to see the old one):
# settle waits up to 20 s for the address in the status.
settle() {
    for i in $(seq 20); do
        xctl network status | grep -q "$1" && return 0
        sleep 1
    done
}
systemctl start NetworkManager.service && nm-online -s -q -t 60
xctl network ethernet --address 10.0.2.50/24 --gateway 10.0.2.2 --dns 10.0.2.3 &&
    settle 10.0.2.50/ && xctl network status | sed s,^,MANUAL-,
xctl network ethernet --dhcp &&
    settle 10.0.2.15/ && xctl network status | sed s,^,DHCP-,
SCRIPT
tools/boot-test.sh \
    --credential network-test=out/network-test.sh \
    --run "/bin/sh /run/credentials/@system/network-test" \
    --expect "MANUAL-Wired .*10\.0\.2\.50/24" \
    --expect "DHCP-Wired .*10\.0\.2\.15/24" \
    out/disk.raw out/serial-network.log
