# At login: say so when a new XeeonOS version is available (noted by the daily
# check, xeeonos-update-check.timer), and when the disk has problems
# (xeeonos-health.timer).
if [ -r /var/lib/xeeonos/update-available ]; then
    printf 'XeeonOS %s is available: sudo xctl update\n' "$(cat /var/lib/xeeonos/update-available)"
fi
if [ -r /var/lib/xeeonos/health ]; then
    sed 's/^/Disk health: PROBLEM: /' /var/lib/xeeonos/health
    echo 'Copy your files somewhere else, then see: sudo xctl report'
fi
