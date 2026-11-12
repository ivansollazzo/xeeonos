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
    let mut reencrypt = false;
    let mut from_stdin = false;
    for arg in args {
        match arg.as_str() {
            "--reencrypt" => reencrypt = true,
            "--password-stdin" => from_stdin = true,
            a @ ("status" | "on" | "off") if action.is_none() => action = Some(a),
            other => return Err(err(format!("unknown option for encryption: {other}"))),
        }
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
    let items: Vec<(String, String)> = if on {
        vec![(
            "off".into(),
            "Turn the disk password off (the computer starts without it)".into(),
        )]
    } else {
        vec![
            ("on".into(), "Turn on a disk password (instant)".into()),
            (
                "reencrypt".into(),
                "Turn on a disk password and re-encrypt everything (slow)".into(),
            ),
        ]
    };
    let state = if on {
        "on: a password is asked at every start"
    } else {
        "off: the disk opens without a password"
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
pub fn turn_on(password: &str, reencrypt: bool) -> Result<String> {
    if password.is_empty() {
        return Err(err("the disk password cannot be empty"));
    }
    let dev = data_partition()?;
    let empty = empty_slots(&dev)?;
    if empty.is_empty() {
        return Err(err("the disk password is already on"));
    }
    if reencrypt {
        // A new volume key, while the empty password is still the only one
        // (every key slot must be opened to re-encrypt). Online: the
        // partition stays in use.
        crate::util::run(
            "cryptsetup",
            &["reencrypt", "--key-file=/dev/null", "--batch-mode", &dev],
        )?;
    }
    // The password: systemd-cryptenroll reads it from $NEWPASSWORD.
    let status = Command::new("systemd-cryptenroll")
        .args(["--unlock-key-file=/dev/null", "--password", &dev])
        .env("NEWPASSWORD", password)
        .stdin(Stdio::null())
        .status()
        .map_err(|e| err(format!("cannot run systemd-cryptenroll: {e}")))?;
    if !status.success() {
        return Err(err("adding the disk password failed; nothing changed"));
    }
    // The recovery key (printed on standard output), and away with the
    // empty password (by slot number, found before adding the others).
    let wipe = empty
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let out = Command::new("systemd-cryptenroll")
        .args([
            "--unlock-key-file=/dev/null",
            "--recovery-key",
            &format!("--wipe-slot={wipe}"),
            &dev,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| err(format!("cannot run systemd-cryptenroll: {e}")))?;
    let key = recovery_key(&String::from_utf8_lossy(&out.stdout));
    match (out.status.success(), key) {
        (true, Some(key)) => Ok(key),
        _ => Err(err(
            "adding the recovery key failed: the disk password is on, but the \
             empty password still works; try again",
        )),
    }
}

/// Turn the disk password off: add the empty password (checked with the
/// current one), then remove every other key.
pub fn turn_off(password: &str) -> Result<()> {
    let dev = data_partition()?;
    if !empty_slots(&dev)?.is_empty() {
        return Err(err("the disk password is already off"));
    }
    let others = slots(&dev)?;
    // `luksAddKey <dev> /dev/null`: the new key is empty; the existing key
    // comes from standard input ("--key-file=-"). PBKDF2 with few rounds:
    // an empty password needs no slow key derivation, and the start is
    // faster.
    let mut child = Command::new("cryptsetup")
        .args([
            "luksAddKey",
            "--key-file=-",
            "--pbkdf=pbkdf2",
            "--pbkdf-force-iterations=1000",
            "--batch-mode",
            &dev,
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| err(format!("cannot run cryptsetup: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        // Exactly the password, without a newline (--key-file=- reads it all).
        stdin.write_all(password.as_bytes())?;
    }
    if !child.wait()?.success() {
        return Err(err("wrong password; nothing changed"));
    }
    let wipe = others
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let status = Command::new("systemd-cryptenroll")
        .args([
            "--unlock-key-file=/dev/null",
            &format!("--wipe-slot={wipe}"),
            &dev,
        ])
        .stdin(Stdio::null())
        .status()
        .map_err(|e| err(format!("cannot run systemd-cryptenroll: {e}")))?;
    if !status.success() {
        return Err(err(
            "removing the old password failed: the empty password works, but \
             the old password does too; try again",
        ));
    }
    Ok(())
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

/// Is a disk password needed (no key slot opens with the empty password)?
pub fn password_set(dev: &str) -> Result<bool> {
    Ok(empty_slots(dev)?.is_empty())
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

/// The key slots that open with the empty password.
fn empty_slots(dev: &str) -> Result<Vec<u32>> {
    Ok(slots(dev)?
        .into_iter()
        .filter(|s| {
            Command::new("cryptsetup")
                .args([
                    "open",
                    "--test-passphrase",
                    "--key-file=/dev/null",
                    &format!("--key-slot={s}"),
                    dev,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|st| st.success())
        })
        .collect())
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
    }
}
