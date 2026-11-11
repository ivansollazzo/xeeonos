# UKI profile "Recovery" (factory reset): a second entry in the boot menu
# for every version, with the same kernel and /usr. systemd.factory_reset=1
# makes systemd-repart erase the data partition (FactoryReset=yes in
# usr/lib/repart.d/30-root.conf) and create it again, empty: users, settings
# and files are gone, the system starts as after installation. See
# docs/recovery.md.
[UKIProfile]
Profile=
        ID=recovery
        TITLE=Recovery (erases all data)
Cmdline=systemd.factory_reset=1
