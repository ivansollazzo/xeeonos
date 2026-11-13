# Touchpad of the ThinkPad T14/P14s AMD and E495

Optional system extension `thinkpad-t14-amd-touchpad` (see
[extensions.md](extensions.md)). It packages
[thinkpad-t14-amd-touchpad](https://github.com/orospakr/thinkpad-t14-amd-touchpad)
(version 0.2.4, GPL-2.0): on these laptops the stock kernel drives the
Synaptics touchpad over PS/2 (80 Hz, two-finger gestures at about 40 Hz);
with three patched kernel modules (`i2c-piix4`, `rmi_smbus`, `psmouse`) it
runs over RMI4/SMBus (every 15 ms, gestures intact, TrackPoint still
working). Tested by the project on the T14 Gen 2a AMD, T14 Gen 1 AMD and E495.

```
sudo xctl ext enable thinkpad-t14-amd-touchpad
systemctl reboot
sudo thinkpad-t14-amd-touchpad-check   # are the patched modules in use, is the pad on RMI4?
grep Phys /proc/bus/input/devices      # the Synaptics device on an "rmi4-" path
```

The pad's name changes from "SynPS/2 Synaptics TouchPad" to "Synaptics
TM3471-0xx"; System Settings finds it by itself.

## How it is built

Instead of DKMS (which compiles on the computer, impossible with a sealed
`/usr`), CI compiles the modules for the exact kernel of each XeeonOS
version, from the project's tag with its checksum, applying its patches in
the order of its `dkms.conf` (`extensions/thinkpad-t14-amd-touchpad/build.sh`).
They go to `/usr/lib/modules/<kernel>/updates`, which the kernel prefers to
the stock modules. A new kernel in XeeonOS means a new build: if the patches
stop applying, CI fails instead of shipping the stock modules silently.

The modules are not signed. The Arch kernel loads them anyway (it marks
itself "tainted"), also with Secure Boot on.

To go back: `sudo xctl ext remove thinkpad-t14-amd-touchpad` and restart.

## Tests

`tools/ci/thinkpad-t14-amd-touchpad.sh`: in a virtual machine (no ThinkPad)
the extension is turned on, the patched `psmouse` replaces the stock one
(loaded for QEMU's PS/2 mouse) and knows the T14 Gen 2 AMD (`LEN2073`).
Whether the touchpad moves to SMBus can only be seen on the laptop.
