# UKI profile "Recovery": a second entry in the boot menu for every version,
# with the same kernel and /usr. Instead of the normal system it starts
# xeeonos-recovery.target, which shows the Recovery menu (`xctl recovery`):
# reinstall, previous version, network, shell, erase everything, restart.
# The word xeeonos.recovery tells xctl that this boot is Recovery.
# See docs/recovery.md.
[UKIProfile]
Profile=
        ID=recovery
        TITLE=Recovery
Cmdline=xeeonos.recovery systemd.unit=xeeonos-recovery.target
