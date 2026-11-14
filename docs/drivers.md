# Drivers

Module `mkosi.conf.d/drivers`, part of every XeeonOS, the live ISO included
(so the ISO already works on the computer it installs, and reads every
disk). Limited to the supported hardware: Intel, AMD and Nouveau graphics,
no NVIDIA proprietary driver.

| Area | Packages |
| --- | --- |
| File systems | `dosfstools` (FAT), `exfatprogs` (exFAT), `ntfs-3g` (NTFS: check and repair Windows disks), `e2fsprogs` (ext4) |
| Processor | `intel-ucode`, `amd-ucode` (microcode, loaded early) |
| Graphics, Wi-Fi, Bluetooth | `linux-firmware-intel`, `-amdgpu`, `-radeon`, `-nvidia` (Nouveau), `-realtek`, `-atheros`, `-mediatek`, `-broadcom` |
| Laptop sound | `sof-firmware`, `alsa-ucm-conf` (recent Intel and AMD laptops), `linux-firmware-cirrus` (speaker amplifiers) |
| Wi-Fi rules | `wireless-regdb`: channels and power allowed in each country |
| USB modems | `usb_modeswitch`: 4G sticks and adapters that first show up as a CD-ROM |

The kernel reads FAT, exFAT, NTFS, ext4 and Btrfs by itself; these are the
tools to create, check and repair them (also used by Disks). Btrfs and
encryption tools are in the module `system`.

Optional, for some computers (extensions, see
[xeeonos-extensions](https://github.com/ivansollazzo/xeeonos-extensions)):
`thermald` (Intel processors), `tlp` (hand-tuned power saving), and drivers
for single models such as `thinkpad-t14-amd-touchpad`.

## Tests

`tools/ci/tools.sh` checks that the file system tools, the firmware for
sound, Broadcom and Cirrus, and the regulatory database are there.
