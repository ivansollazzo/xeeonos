# Scanners

Extension `scanner` (`extensions/scanner`, see [extensions.md](extensions.md)):
included in the system image, on from the start, special (turning it off
asks first).

- **SANE** does the scanning, with **sane-airscan**: driverless scanning
  (eSCL, WSD), the way most scanners and all-in-one printers of the last ten
  years work, over the network or USB.
- **ipp-usb** lets USB devices speak eSCL over USB. It starts by itself when
  one is plugged in.
- **Document Scanner** (simple-scan) is in the menu. It is part of the
  extension, not from Flathub, because it needs the system's SANE.
- From a terminal: `scanimage -L` (scanners found), `scanimage > page.pnm`.

Settings are in `/etc/sane.d`, a link to the defaults in
`/usr/share/factory/etc/sane.d`. To change them (for example to add a
network scanner by its address in `airscan.conf`), replace the link with a
copy.

## Scanners of the local network

Like network printers, network scanners announce themselves with mDNS
(Avahi), and the firewall is closed. To find them automatically:

```
sudo xctl firewall allow 5353/udp
```

Without it, a network scanner can still be added by its address in
`/etc/sane.d/airscan.conf`.

## Tests

`tools/ci/scanner.sh`: SANE with the airscan backend on, ipp-usb ready,
Avahi running, Document Scanner there, the extension included and on. The
virtual machine has no scanner: SANE starts, looks and finds none.
