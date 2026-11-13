# Printing

Extension `printing` (`extensions/printing`, see [extensions.md](extensions.md)):
included in the system image, on from the start, special (turning it off
asks first).

- **CUPS** does the printing. It starts by itself at the first use of a
  printer. Modern printers (USB and network) work without drivers
  (IPP Everywhere, AirPrint).
- With the Plasma desktop, printers are added and managed in
  **System Settings, Printers**; the print queue is in the system tray.
  Adding a printer asks for the password of an administrator.
- From a terminal: `lpstat -p` (printers), `lp <file>` (print),
  and the CUPS page at <http://localhost:631>.

Printers and settings are in `/etc/cups` (kept across updates).

## Printers of the local network

Network printers announce themselves on the local network (mDNS). The
firewall is closed, so they are not found automatically until you let
those announcements in:

```
sudo xctl firewall allow 5353/udp
```

Without it, a network printer can still be added by its address
(for example `ipp://192.168.1.50/ipp/print`).

## Tests

`tools/ci/printing.sh`: CUPS starts on demand and a printer can be added,
Avahi runs and `.local` names are resolved through it, Plasma has its
Printers page.
