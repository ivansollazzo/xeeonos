//! UEFI Secure Boot (docs/secure-boot.md): is it on, and can the XeeonOS
//! keys be enrolled?
//!
//! Every version of XeeonOS is signed (the UKIs and systemd-boot). The keys
//! to enroll in the firmware are on the boot partition (loader/keys/auto);
//! when the firmware is in Setup Mode, systemd-boot enrolls them (by itself
//! in a virtual machine, from its menu on a real computer).

use std::fs;
use std::path::Path;

/// The global EFI variables (UEFI specification).
const SECURE_BOOT: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
const SETUP_MODE: &str = "/sys/firmware/efi/efivars/SetupMode-8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// The keys this version is signed with, to enroll (absent: unsigned build).
pub const KEYS: &str = "/usr/share/xeeonos/secureboot";

#[derive(Debug, PartialEq)]
pub enum State {
    /// Enforced: only signed boot loaders and kernels start.
    On,
    /// Off, with the firmware in Setup Mode: keys can be enrolled.
    Setup,
    /// Off (or not a UEFI computer).
    Off,
}

pub fn state() -> State {
    if efi_flag(SECURE_BOOT) == Some(true) {
        State::On
    } else if efi_flag(SETUP_MODE) == Some(true) {
        State::Setup
    } else {
        State::Off
    }
}

/// A one-byte EFI variable: efivarfs puts 4 bytes of attributes first.
fn efi_flag(path: &str) -> Option<bool> {
    parse_flag(&fs::read(path).ok()?)
}

fn parse_flag(data: &[u8]) -> Option<bool> {
    data.get(4).map(|b| *b == 1)
}

/// The line of `xctl status`.
pub fn describe() -> String {
    let signed = Path::new(KEYS).join("db.auth").exists();
    match (state(), signed) {
        (State::On, _) => "on".into(),
        (State::Setup, true) => "off; the firmware is ready for the XeeonOS keys: \
                                 restart and choose \"Enroll Secure Boot keys: auto\" in the boot menu"
            .into(),
        (_, true) => "off (see docs/secure-boot.md to turn it on)".into(),
        (_, false) => "off (this build is not signed)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags() {
        assert_eq!(parse_flag(&[6, 0, 0, 0, 1]), Some(true));
        assert_eq!(parse_flag(&[6, 0, 0, 0, 0]), Some(false));
        assert_eq!(parse_flag(&[6, 0, 0]), None);
    }
}
