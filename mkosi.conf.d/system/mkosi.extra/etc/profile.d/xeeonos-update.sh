# At login: say so when a new XeeonOS version is available (noted by the daily
# check, xeeonos-update-check.timer).
if [ -r /var/lib/xeeonos/update-available ]; then
    printf 'XeeonOS %s is available: sudo xctl update\n' "$(cat /var/lib/xeeonos/update-available)"
fi
