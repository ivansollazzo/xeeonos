//! `xctl recovery`: the Recovery menu.
//!
//! Every version in the boot menu has a second entry, "Recovery" (a UKI
//! profile, see mkosi.conf.d/system/recovery.profile). It starts the same
//! kernel and /usr, but instead of the normal system it opens this menu
//! (xeeonos-recovery.service, on the first console).
//!
//! On the normal, running system `xctl recovery` only offers to restart into
//! Recovery: repairing a system while it runs is not safe.
//!
//! The menu can also be driven without a keyboard, one action at a time:
//! `xctl recovery <action> [--yes]` (used by the tests).

use std::process::Command;

use serde_json::Value;

use crate::boot;
use crate::ui;
use crate::update;
use crate::util::{Result, err, need_root, output, run};

/// The menu: (action name, description).
const ACTIONS: &[(&str, &str)] = &[
    (
        "reinstall",
        "Reinstall XeeonOS (keeps users, files and settings)",
    ),
    ("rollback", "Go back to the previous version"),
    (
        "restore",
        "Restore data (settings and files) from a snapshot",
    ),
    ("network", "Network settings (Wi-Fi, wired, ...)"),
    ("encryption", "Disk password (turn on or off)"),
    ("shell", "Shell (needs an administrator's password)"),
    ("erase", "Erase everything and start again from the setup"),
    ("reboot", "Restart"),
];

/// The word to type to confirm "erase".
const ERASE_WORD: &str = "ERASE";

pub fn run_recovery(args: &[String]) -> Result<()> {
    let mut action = None;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            a if action.is_none() && ACTIONS.iter().any(|(name, _)| *name == a) => {
                action = Some(a.to_string())
            }
            other => return Err(err(format!("unknown option for recovery: {other}"))),
        }
    }
    need_root("recovery")?;

    if !boot::in_recovery() {
        if action.is_some() {
            return Err(err(
                "this works only in Recovery; run `sudo xctl recovery` to restart into it",
            ));
        }
        return restart_into_recovery(yes);
    }

    match action {
        Some(a) => perform(&a, yes),
        None => menu(),
    }
}

/// On the running system: offer to start Recovery at the next boot only.
fn restart_into_recovery(yes: bool) -> Result<()> {
    if !yes && !ui::interactive() {
        return Err(err(
            "no terminal to ask \"Restart into Recovery now?\" on (give --yes)",
        ));
    }
    if !yes
        && !ui::yes_no(
            "Recovery",
            "Restart into Recovery now?\n\nRecovery can reinstall XeeonOS, go back to the \
             previous version, change the network settings, open a shell or erase \
             everything.",
            true,
        )?
    {
        println!("Nothing changed.");
        return Ok(());
    }
    let list = output("bootctl", &["list", "--json=short"])?;
    let id = recovery_entry(&list, &loader_entries())?;
    // One-shot: only the next boot starts Recovery; the one after is normal.
    run("bootctl", &["set-oneshot", &id])?;
    println!("Restarting into Recovery ({id})...");
    reboot()
}

/// The boot menu entry of Recovery for the running version. The running
/// entry is "selected" in `bootctl list --json=short`; its Recovery entry is
/// the second profile of the same kernel image, which systemd-boot names
/// after the profile's ID: `<id>@recovery` (`ID=recovery` in
/// recovery.profile; `<id>@1` for a profile without an ID). systemd-boot's own list of entries (`loader`, from the
/// LoaderEntries EFI variable) is checked first: `bootctl set-oneshot` takes
/// systemd-boot's names.
fn recovery_entry(json: &str, loader: &[String]) -> Result<String> {
    let entries: Value =
        serde_json::from_str(json).map_err(|e| err(format!("cannot read the boot menu: {e}")))?;
    let entries = entries.as_array().cloned().unwrap_or_default();
    let selected = entries
        .iter()
        .find(|e| e["isSelected"].as_bool() == Some(true))
        .and_then(|e| e["id"].as_str())
        .ok_or_else(|| err("cannot find the running version in the boot menu"))?;
    let base = selected.split('@').next().unwrap_or(selected);
    let wanted = [format!("{base}@recovery"), format!("{base}@1")];
    let ids: Vec<String> = loader
        .iter()
        .cloned()
        .chain(
            entries
                .iter()
                .filter_map(|e| e["id"].as_str().map(str::to_string)),
        )
        .collect();
    ids.iter()
        .find(|id| wanted.contains(id))
        .cloned()
        .ok_or_else(|| {
            err(format!(
                "the boot menu has no Recovery entry for {base} (entries: {})",
                ids.join(", ")
            ))
        })
}

/// systemd-boot's names of the boot menu entries, from the LoaderEntries
/// EFI variable: 4 bytes of attributes, then UTF-16 names ending in NUL.
fn loader_entries() -> Vec<String> {
    let path = "/sys/firmware/efi/efivars/LoaderEntries-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";
    let Ok(data) = std::fs::read(path) else {
        return Vec::new();
    };
    parse_loader_entries(&data)
}

fn parse_loader_entries(data: &[u8]) -> Vec<String> {
    let units: Vec<u16> = data
        .get(4..)
        .unwrap_or_default()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    units
        .split(|u| *u == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// The interactive menu, shown on the console in Recovery.
fn menu() -> Result<()> {
    let items: Vec<(String, String)> = ACTIONS
        .iter()
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .collect();
    let mut last: Option<String> = None;
    loop {
        let chosen = ui::menu(
            "Recovery",
            "What do you want to do?",
            &items,
            last.as_deref(),
        )?;
        // "Back" has nowhere to go: the menu is the whole of Recovery.
        let Some(name) = chosen else { continue };
        if let Err(e) = perform(&name, false) {
            ui::message("Recovery", &format!("It did not work:\n\n{e}"))?;
        }
        last = Some(name);
    }
}

fn perform(action: &str, yes: bool) -> Result<()> {
    match action {
        "reinstall" => reinstall(yes),
        "rollback" => {
            // The menu already asked which action; --yes skips the question.
            let args = if yes {
                vec!["--yes".to_string()]
            } else {
                Vec::new()
            };
            boot::rollback(&args)?;
            offer_reboot(yes)
        }
        "restore" => crate::snapshots::screen(),
        "network" => crate::network::screen().map(|_| ()),
        "shell" => shell(),
        "encryption" => crate::encryption::screen(),
        "erase" => erase(yes),
        "reboot" => reboot(),
        _ => Err(err(format!("unknown action: {action}"))),
    }
}

/// Download and install the newest version of the channel. /usr is sealed
/// (dm-verity), so a version that starts is known to be intact: reinstalling
/// means installing the newest one, next to the running one.
fn reinstall(yes: bool) -> Result<()> {
    if !yes
        && !ui::yes_no(
            "Reinstall XeeonOS",
            "The system files of this version are sealed and intact.\n\nDownload the \
             newest version of XeeonOS and install it? Users, files and settings stay. \
             This needs the network.",
            false,
        )?
    {
        return Ok(());
    }
    update::run(&[])?;
    offer_reboot(yes)
}

/// A root shell, for an administrator: the password of a user in the
/// `wheel` group is asked by `su` itself, run without root rights so that it
/// cannot skip the password.
fn shell() -> Result<()> {
    let Some(user) = ui::input(
        "Shell",
        "The name of an administrator (you will be asked for the password):",
        "",
    )?
    else {
        return Ok(());
    };
    let groups = output("id", &["-nG", &user]).map_err(|_| err(format!("no user '{user}'")))?;
    if !groups.split_whitespace().any(|g| g == "wheel") {
        return Err(err(format!("{user} is not an administrator")));
    }
    let _ = Command::new("clear").status();
    println!("Type `sudo -i` for root rights, `exit` to go back to the menu.");
    run(
        "setpriv",
        &[
            "--reuid=nobody",
            "--regid=nobody",
            "--clear-groups",
            "su",
            "--login",
            &user,
        ],
    )
}

/// Erase the data partition (users, settings, files) at the next boot. The
/// request is an EFI variable read by systemd at boot; /usr is not touched.
fn erase(yes: bool) -> Result<()> {
    if !yes {
        let answer = ui::input(
            "Erase everything",
            &format!(
                "This erases all users, settings and files on this computer. The \
                 computer then starts again with the first-boot setup.\n\nType \
                 {ERASE_WORD} to confirm:"
            ),
            "",
        )?;
        if answer.as_deref() != Some(ERASE_WORD) {
            ui::message("Erase everything", "Nothing changed.")?;
            return Ok(());
        }
    }
    run("/usr/lib/systemd/systemd-factory-reset", &["request"])?;
    ui::working(
        "Erase everything",
        "All data will be erased at the restart.",
    );
    reboot()
}

fn offer_reboot(yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if ui::yes_no(
        "Restart",
        "Done. Restart now to start the new version?",
        false,
    )? {
        reboot()
    } else {
        Ok(())
    }
}

fn reboot() -> Result<()> {
    // `Command::status` waits; systemd stops this program while restarting.
    let _ = Command::new("systemctl").arg("reboot").status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_recovery_entry() {
        let json = r#"[
            {"id":"xeeonos_1.0.1.efi","isSelected":false},
            {"id":"xeeonos_1.0.1.efi@recovery","isSelected":false},
            {"id":"xeeonos_1.0.2.efi","isSelected":true},
            {"id":"xeeonos_1.0.2.efi@recovery","isSelected":false}
        ]"#;
        assert_eq!(
            recovery_entry(json, &[]).unwrap(),
            "xeeonos_1.0.2.efi@recovery"
        );
        assert!(recovery_entry("[]", &[]).is_err());
        // Only systemd-boot knows the Recovery entry.
        let json = r#"[{"id":"xeeonos_1.0.2.efi","isSelected":true}]"#;
        let loader = vec![
            "xeeonos_1.0.2.efi".to_string(),
            "xeeonos_1.0.2.efi@recovery".to_string(),
        ];
        assert_eq!(
            recovery_entry(json, &loader).unwrap(),
            "xeeonos_1.0.2.efi@recovery"
        );
    }

    #[test]
    fn loader_variable() {
        let mut data = vec![7, 0, 0, 0];
        for name in ["a.efi", "a.efi@recovery"] {
            for u in name.encode_utf16().chain([0]) {
                data.extend(u.to_le_bytes());
            }
        }
        assert_eq!(parse_loader_entries(&data), vec!["a.efi", "a.efi@recovery"]);
    }
}
