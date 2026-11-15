//! `xctl encryption`: the disk password of the data partition.
//!
//! The data partition (users, settings, files: /etc, /var, /home) is always
//! encrypted (LUKS2, created by systemd-repart at the first boot, see
//! repart.d/30-root.conf). /usr is not encrypted: it holds no personal data
//! and is sealed (dm-verity).
//!
//! - **Off**: the partition opens with an empty password, so the computer
//!   starts without asking anything (the initrd tries the empty password
//!   first: systemd-cryptsetup@root.service.d in mkosi.initrd.conf).
//! - **On**: a disk password, asked at every start, and a recovery key
//!   shown once, for when the password is forgotten. The empty password is
//!   removed.
//!
//! Turning it on or off is instant: only the keys in the LUKS header change,
//! not the data. With `--reencrypt`, turning it on also re-encrypts the whole
//! partition with a new key (slow), so that a copy of the disk taken while
//! the password was empty cannot be opened either.
//!
//! - **TPM** (`xctl encryption tpm on|off`, only with the password on): the
//!   computer's TPM chip also holds a key, which it gives back only while
//!   Secure Boot and its keys are unchanged (PCR 7). The disk then opens by
//!   itself on this computer, and only when XeeonOS starts the way it was
//!   signed; taken out, or started otherwise, it needs the password. The
//!   password and the recovery key keep working. If the firmware's Secure
//!   Boot settings change (an update of the firmware or of its list of
//!   revoked signatures), the password is asked again: `xctl encryption tpm
//!   on` then binds the key to the new state.
//!
//! The commands of cryptsetup and systemd-cryptenroll get the passwords
//! through their standard input or environment, never on the command line
//! (visible to every user in the process list).

use std::io::{BufRead, Write};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::ui;
use crate::util::{Result, err, need_root, output};

/// The device-mapper name of the opened data partition (systemd's gpt-auto
/// generator calls it "root").
const MAPPER: &str = "root";

pub fn run_encryption(args: &[String]) -> Result<()> {
    let mut action = None;
    let mut tpm = false;
    let mut reencrypt = false;
    let mut from_stdin = false;
    for arg in args {
        match arg.as_str() {
            "--reencrypt" => reencrypt = true,
            "--password-stdin" => from_stdin = true,
            "tpm" if action.is_none() && !tpm => tpm = true,
            a @ ("status" | "on" | "off") if action.is_none() => action = Some(a),
            other => return Err(err(format!("unknown option for encryption: {other}"))),
        }
    }
    if tpm {
        return match action {
            Some("on") => {
                need_root("encryption tpm on")?;
                let password = match from_stdin {
                    true => read_stdin_line()?,
                    false => match ui::password(
                        "Disk password",
                        "The current disk password (or the recovery key):",
                    )? {
                        Some(p) => p,
                        None => return Ok(()),
                    },
                };
                tpm_on(&password)?;
                println!("{TPM_ON_TEXT}");
                Ok(())
            }
            Some("off") => {
                need_root("encryption tpm off")?;
                tpm_off()?;
                println!("TPM unlock is off: the disk password is asked at every start.");
                Ok(())
            }
            _ => Err(err("usage: xctl encryption tpm on|off")),
        };
    }
    match action {
        None | Some("status") => {
            let dev = data_partition()?;
            println!(
                "encryption: {}",
                if password_set(&dev)? {
                    "on (a password is asked at every start)"
                } else {
                    "off (the disk opens without a password)"
                }
            );
            if tpm_set(&dev)? {
                println!("tpm: on (the disk opens by itself on this computer, with Secure Boot)");
            } else {
                println!("tpm: off");
            }
            Ok(())
        }
        Some("on") => {
            need_root("encryption on")?;
            let password = match from_stdin {
                true => read_stdin_line()?,
                false => match ask_new_password("Disk password")? {
                    Some(p) => p,
                    None => return Ok(()),
                },
            };
            let key = turn_on(&password, reencrypt)?;
            show_recovery_key(&key)
        }
        Some(_) => {
            need_root("encryption off")?;
            let password = match from_stdin {
                true => read_stdin_line()?,
                false => match ui::password(
                    "Disk password",
                    "The current disk password (or the recovery key):",
                )? {
                    Some(p) => p,
                    None => return Ok(()),
                },
            };
            turn_off(&password)?;
            println!("Encryption is off: the computer starts without a disk password.");
            Ok(())
        }
    }
}

/// The window of Recovery and of `xctl encryption` without arguments.
pub fn screen() -> Result<()> {
    let dev = data_partition()?;
    let on = password_set(&dev)?;
    let tpm = on && tpm_set(&dev)?;
    let items: Vec<(String, String)> = if on {
        let mut items = vec![(
            "off".into(),
            "Turn the disk password off (the computer starts without it)".into(),
        )];
        if tpm {
            items.push((
                "tpm-off".into(),
                "Stop opening the disk with the TPM (ask the password)".into(),
            ));
        } else {
            items.push((
                "tpm-on".into(),
                "Open the disk with the TPM on this computer (needs Secure Boot)".into(),
            ));
        }
        items
    } else {
        vec![
            ("on".into(), "Turn on a disk password (instant)".into()),
            (
                "reencrypt".into(),
                "Turn on a disk password and re-encrypt everything (slow)".into(),
            ),
        ]
    };
    let state = match (on, tpm) {
        (true, true) => "on, opened by the TPM on this computer",
        (true, false) => "on: a password is asked at every start",
        _ => "off: the disk opens without a password",
    };
    let Some(choice) = ui::menu(
        "Disk encryption",
        &format!("The data on this disk is encrypted. Disk password: {state}."),
        &items,
        None,
    )?
    else {
        return Ok(());
    };
    if choice == "tpm-off" {
        tpm_off()?;
        return ui::message(
            "Disk encryption",
            "Done: the disk password is asked at every start.",
        );
    }
    if choice == "tpm-on" {
        let Some(password) = ui::password(
            "Disk password",
            "The current disk password (or the recovery key):",
        )?
        else {
            return Ok(());
        };
        tpm_on(&password)?;
        return ui::message("Disk encryption", TPM_ON_TEXT);
    }
    if choice == "off" {
        let Some(password) = ui::password(
            "Disk password",
            "The current disk password (or the recovery key):",
        )?
        else {
            return Ok(());
        };
        turn_off(&password)?;
        return ui::message("Disk encryption", "Done: no disk password is asked now.");
    }
    let Some(password) = ask_new_password("Disk password")? else {
        return Ok(());
    };
    if choice == "reencrypt" {
        ui::working(
            "Disk encryption",
            "Re-encrypting the whole data partition. This takes a while; the \
             computer can be used meanwhile, but do not turn it off.",
        );
    }
    let key = turn_on(&password, choice == "reencrypt")?;
    show_recovery_key(&key)
}

/// Ask for a new password twice, in windows. `None`: the user went back.
pub fn ask_new_password(title: &str) -> Result<Option<String>> {
    loop {
        let Some(first) = ui::password(
            title,
            "The new disk password. It is asked every time the computer starts, \
             before anything else (with the US keyboard layout):",
        )?
        else {
            return Ok(None);
        };
        if first.is_empty() {
            ui::message(title, "The password cannot be empty.")?;
            continue;
        }
        let Some(again) = ui::password(title, "The same password again:")? else {
            continue;
        };
        if again == first {
            return Ok(Some(first));
        }
        ui::message(title, "The passwords do not match, try again.")?;
    }
}

/// Show the recovery key, once.
pub fn show_recovery_key(key: &str) -> Result<()> {
    let text = format!(
        "Encryption is on. The disk password is asked at every start.\n\n\
         Recovery key (if the password is forgotten):\n\n    {key}\n\n\
         Write it down and keep it somewhere safe: it is shown only now."
    );
    if ui::interactive() {
        ui::message("Recovery key", &text)
    } else {
        println!("{text}");
        Ok(())
    }
}

/// Turn the disk password on: add the password and a recovery key, then
/// remove the empty password. Returns the recovery key.
///
/// (cryptsetup's command line cannot use an empty password, so the empty
/// one is handled by systemd-cryptenroll: `--unlock-key-file=/dev/null` is
/// an empty key, and `--wipe-slot=empty` finds the slots it opens.)
pub fn turn_on(password: &str, reencrypt: bool) -> Result<String> {
    if password.is_empty() {
        return Err(err("the disk password cannot be empty"));
    }
    let dev = data_partition()?;
    if password_set(&dev)? {
        return Err(err("the disk password is already on"));
    }
    let before = slots(&dev)?;
    // The password: systemd-cryptenroll reads it from $NEWPASSWORD.
    cryptenroll(
        &["--unlock-key-file=/dev/null", "--password"],
        &dev,
        &[("NEWPASSWORD", password)],
    )
    .map_err(|_| err("adding the disk password failed; nothing changed"))?;
    if reencrypt {
        // A new volume key. Only the new password's slot is kept: the empty
        // password goes away with the old key. Online: the partition stays
        // in use.
        let slot = slots(&dev)?
            .into_iter()
            .find(|s| !before.contains(s))
            .ok_or_else(|| err("cannot find the new password's key slot"))?;
        let mut child = Command::new("cryptsetup")
            .args([
                "reencrypt",
                "--batch-mode",
                "--key-file=-",
                &format!("--key-slot={slot}"),
                &dev,
            ])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| err(format!("cannot run cryptsetup: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(password.as_bytes())?;
        }
        if !child.wait()?.success() {
            return Err(err(
                "re-encrypting failed: the disk password is on, the empty password may \
                 still work; try `xctl encryption on --reencrypt` again",
            ));
        }
    }
    // The recovery key (printed on standard output), and away with the
    // empty password. Opened with the new password ($PASSWORD).
    let out = Command::new("systemd-cryptenroll")
        .args(["--recovery-key", "--wipe-slot=empty", &dev])
        .env("PASSWORD", password)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| err(format!("cannot run systemd-cryptenroll: {e}")))?;
    let key = recovery_key(&String::from_utf8_lossy(&out.stdout));
    match (out.status.success(), key) {
        (true, Some(key)) => Ok(key),
        _ => Err(err(
            "adding the recovery key failed: the disk password is on, but the \
             empty password may still work; try again",
        )),
    }
}

/// Turn the disk password off: add the empty password (opened with the
/// current password or the recovery key), then remove every other key.
pub fn turn_off(password: &str) -> Result<()> {
    let dev = data_partition()?;
    if !password_set(&dev)? {
        return Err(err("the disk password is already off"));
    }
    let others = slots(&dev)?;
    // Opened with $PASSWORD; the new, empty password from $NEWPASSWORD.
    cryptenroll(
        &["--password"],
        &dev,
        &[("PASSWORD", password), ("NEWPASSWORD", "")],
    )
    .map_err(|_| err("wrong password; nothing changed"))?;
    let wipe = others
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    cryptenroll(
        &[
            "--unlock-key-file=/dev/null",
            &format!("--wipe-slot={wipe}"),
        ],
        &dev,
        &[],
    )
    .map_err(|_| {
        err(
            "removing the old password failed: the empty password works, but \
             the old password does too; try again",
        )
    })
}

const TPM_ON_TEXT: &str = "TPM unlock is on: this computer opens the disk by itself while \
     Secure Boot is unchanged. The password and the recovery key still work.";

/// Let the TPM open the disk: a new key, sealed by the TPM to the current
/// Secure Boot state (PCR 7), replacing an older TPM key. Opened with the
/// password (or the recovery key).
pub fn tpm_on(password: &str) -> Result<()> {
    let dev = data_partition()?;
    if !password_set(&dev)? {
        return Err(err(
            "turn the disk password on first (xctl encryption on): without it the \
             disk opens by itself anyway",
        ));
    }
    if crate::secureboot::state() != crate::secureboot::State::On {
        return Err(err(
            "Secure Boot is off: the TPM would open the disk for any system started \
             on this computer. Turn Secure Boot on first (docs/secure-boot.md)",
        ));
    }
    if !tpm2_present() {
        return Err(err("this computer has no TPM 2.0 chip"));
    }
    // PCR 7 only, and no signed policy or pcrlock file (none is shipped).
    cryptenroll(
        &[
            "--tpm2-device=auto",
            "--tpm2-pcrs=7",
            "--tpm2-public-key=",
            "--wipe-slot=tpm2",
        ],
        &dev,
        &[("PASSWORD", password)],
    )
    .map_err(|_| err("wrong password, or the TPM refused; nothing changed"))
}

/// Remove the TPM key (no password needed: nothing is added).
pub fn tpm_off() -> Result<()> {
    let dev = data_partition()?;
    cryptenroll(&["--wipe-slot=tpm2"], &dev, &[])
}

/// Does the TPM hold a key for the partition?
pub fn tpm_set(dev: &str) -> Result<bool> {
    let json = output("cryptsetup", &["luksDump", "--dump-json-metadata", dev])?;
    Ok(has_token(&json, "systemd-tpm2"))
}

fn tpm2_present() -> bool {
    std::fs::read_to_string("/sys/class/tpm/tpm0/tpm_version_major").is_ok_and(|v| v.trim() == "2")
}

/// Run systemd-cryptenroll on `dev`, with secrets in the environment.
fn cryptenroll(args: &[&str], dev: &str, env: &[(&str, &str)]) -> Result<()> {
    let status = Command::new("systemd-cryptenroll")
        .args(args)
        .arg(dev)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .status()
        .map_err(|e| err(format!("cannot run systemd-cryptenroll: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(err("systemd-cryptenroll failed"))
    }
}

/// The encrypted data partition, e.g. /dev/nvme0n1p5: the device below the
/// opened /dev/mapper/root.
pub fn data_partition() -> Result<String> {
    let not = || {
        err(
            "the data partition of this computer is not encrypted (it was \
             installed before encryption existed); reinstall to encrypt it",
        )
    };
    let dm = std::fs::canonicalize(format!("/dev/mapper/{MAPPER}")).map_err(|_| not())?;
    let dm = dm
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(not)?;
    let uuid = std::fs::read_to_string(format!("/sys/block/{dm}/dm/uuid")).unwrap_or_default();
    if !uuid.starts_with("CRYPT-LUKS2") {
        return Err(not());
    }
    std::fs::read_dir(format!("/sys/block/{dm}/slaves"))?
        .flatten()
        .next()
        .map(|e| format!("/dev/{}", e.file_name().to_string_lossy()))
        .ok_or_else(not)
}

/// Is the disk password on? xctl turns it on together with a recovery key,
/// and removes both when it turns it off: the recovery key's token
/// ("systemd-recovery") in the LUKS header tells.
pub fn password_set(dev: &str) -> Result<bool> {
    let json = output("cryptsetup", &["luksDump", "--dump-json-metadata", dev])?;
    Ok(has_recovery_token(&json))
}

fn has_recovery_token(json: &str) -> bool {
    has_token(json, "systemd-recovery")
}

fn has_token(json: &str, kind: &str) -> bool {
    let meta: Value = serde_json::from_str(json).unwrap_or_default();
    meta["tokens"]
        .as_object()
        .is_some_and(|t| t.values().any(|v| v["type"] == kind))
}

/// All key slots of the partition.
fn slots(dev: &str) -> Result<Vec<u32>> {
    let json = output("cryptsetup", &["luksDump", "--dump-json-metadata", dev])?;
    Ok(parse_slots(&json))
}

fn parse_slots(json: &str) -> Vec<u32> {
    let meta: Value = serde_json::from_str(json).unwrap_or_default();
    let mut slots: Vec<u32> = meta["keyslots"]
        .as_object()
        .map(|o| o.keys().filter_map(|k| k.parse().ok()).collect())
        .unwrap_or_default();
    slots.sort();
    slots
}

/// The recovery key in systemd-cryptenroll's output: 8 groups of 8
/// characters, separated by '-'.
fn recovery_key(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|w| {
            let groups: Vec<&str> = w.split('-').collect();
            groups.len() == 8
                && groups
                    .iter()
                    .all(|g| g.len() == 8 && g.chars().all(|c| c.is_ascii_lowercase()))
        })
        .map(str::to_string)
}

fn read_stdin_line() -> Result<String> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key() {
        let out = "\n  cbuvhkcf-nbbtvrth-rtdfhdtr-fecrcrcn-cknjvtdd-knjfbghh-ljjtfcdb-ghufvehb\n";
        assert_eq!(
            recovery_key(out).as_deref(),
            Some("cbuvhkcf-nbbtvrth-rtdfhdtr-fecrcrcn-cknjvtdd-knjfbghh-ljjtfcdb-ghufvehb")
        );
        assert_eq!(recovery_key("no key here"), None);
    }

    #[test]
    fn keyslots() {
        let json = r#"{"keyslots":{"0":{"type":"luks2"},"2":{"type":"luks2"}},"tokens":{}}"#;
        assert_eq!(parse_slots(json), vec![0, 2]);
        assert!(!has_recovery_token(json));
        let on =
            r#"{"keyslots":{"1":{}},"tokens":{"0":{"type":"systemd-recovery","keyslots":["1"]}}}"#;
        assert!(has_recovery_token(on));
        assert!(!has_token(on, "systemd-tpm2"));
        let tpm = r#"{"keyslots":{"1":{},"2":{}},"tokens":{"0":{"type":"systemd-recovery"},"1":{"type":"systemd-tpm2"}}}"#;
        assert!(has_token(tpm, "systemd-tpm2"));
    }
}
