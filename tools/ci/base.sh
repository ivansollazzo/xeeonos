#!/usr/bin/env bash
# The base services of the installed system, on a fresh disk: swap in
# compressed memory and the swap file, the out-of-memory guard, the firewall,
# the scheduled jobs, disk health, crash logs, firmware updates and the bug
# report.
# (No double quotes in the command: it goes inside systemd.run="...".)
# (systemd.run= replaces the boot target: sysinit.target is reached, so zram
# and the firewall are up; timers are only checked as enabled.)
source "$(dirname "$0")/common.sh"
fresh_disk
tools/boot-test.sh \
    --run "/bin/sh -c '\
swapon --show=NAME,TYPE,PRIO --noheadings | sed s,^,SWAP-,; \
systemctl start systemd-oomd.service; for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do oomctl dump | grep -A1 Swap.Monitored | grep -q Path:./\$ && { echo OOMD-WATCHES-SWAP; break; }; sleep 2; done; \
echo PSTORE-\$(systemctl is-enabled systemd-pstore.service); \
xctl health; \
systemctl is-active xeeonos-firewall.service | sed s,^,FIREWALL-,; \
nft list chain inet xeeonos input | grep -q policy.drop && echo INPUT-CLOSED; \
xctl firewall allow 2222/tcp; nft list chain inet xeeonos input | grep -q tcp.dport.2222.accept && echo PORT-OPEN; \
xctl firewall deny 2222/tcp; nft list chain inet xeeonos input | grep -q 2222 || echo PORT-CLOSED; \
xctl firewall status; \
for t in xeeonos-update-check.timer xeeonos-scrub.timer xeeonos-health.timer fstrim.timer fwupd-refresh.timer; do echo TIMER-\$t-\$(systemctl is-enabled \$t); done; \
xctl update --check; \
systemctl cat fwupd.service >/dev/null && command -v fwupdmgr >/dev/null && echo HAS-FWUPD; \
xctl report && tar --zstd -tf /var/tmp/xeeonos-report-*.tar.zst | grep -c .txt | sed s,^,REPORT-FILES-,'" \
    --expect "SWAP-/dev/zram0 +partition +100" \
    --expect "SWAP-/swap/swapfile +file +10" \
    --expect "OOMD-WATCHES-SWAP" \
    --expect "PSTORE-enabled" \
    --expect "Disk health: good" \
    --expect "FIREWALL-active" \
    --expect "INPUT-CLOSED" \
    --expect "PORT-OPEN" \
    --expect "PORT-CLOSED" \
    --expect "Firewall: +on" \
    --expect "TIMER-xeeonos-update-check.timer-enabled" \
    --expect "TIMER-xeeonos-scrub.timer-enabled" \
    --expect "TIMER-xeeonos-health.timer-enabled" \
    --expect "TIMER-fstrim.timer-enabled" \
    --expect "TIMER-fwupd-refresh.timer-enabled" \
    --expect "(development build without an update channel: nothing to check|Checking ghcr\.io/.*:$(tools/version.sh --channel)\.\.\.)" \
    --expect "HAS-FWUPD" \
    --expect "REPORT-FILES-18" \
    out/disk.raw out/serial-base.log
