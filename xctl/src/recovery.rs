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

use std::io::{self, BufRead, Write};
use std::process::Command;

use serde_json::Value;

use crate::boot;
use crate::update;
use crate::util::{Result, err, need_root, output, run};

/// The menu: (action name, description).
const ACTIONS: &[(&str, &str)] = &[
    (
        "reinstall",
        "Reinstall XeeonOS (keeps users, files and settings)",
    ),
    ("rollback", "Go back to the previous version"),
    ("network", "Network settings (Wi-Fi, wired, ...)"),
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
    if !yes {
        let answer = ask("Restart into Recovery now? [y/N] ")?;
        if !matches!(answer.as_str(), "y" | "Y" | "yes") {
            println!("Nothing changed.");
            return Ok(());
        }
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
/// the second profile of the same kernel image, which systemd-boot calls
/// `<id>@1`. systemd-boot's own list of entries (`loader`, from the
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
    let wanted = format!("{base}@1");
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
        .find(|id| **id == wanted)
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
    loop {
        println!();
        println!("XeeonOS Recovery");
        println!();
        for (i, (_, text)) in ACTIONS.iter().enumerate() {
            println!("  {}. {text}", i + 1);
        }
        println!();
        let answer = ask("Choose (1-6): ")?;
        let chosen = answer
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| ACTIONS.get(n));
        match chosen {
            Some((name, _)) => {
                if let Err(e) = perform(name, false) {
                    println!("xctl: {e}");
                }
            }
            None => println!("Please type a number from 1 to {}.", ACTIONS.len()),
        }
    }
}

fn perform(action: &str, yes: bool) -> Result<()> {
    match action {
        "reinstall" => reinstall(yes),
        "rollback" => {
            boot::rollback(&[])?;
            offer_reboot(yes)
        }
        "network" => run("nmtui", &[]),
        "shell" => shell(),
        "erase" => erase(yes),
        "reboot" => reboot(),
        _ => Err(err(format!("unknown action: {action}"))),
    }
}

/// Download and install the newest version of the channel. /usr is sealed
/// (dm-verity), so a version that starts is known to be intact: reinstalling
/// means installing the newest one, next to the running one.
fn reinstall(yes: bool) -> Result<()> {
    println!("The system files of this version are sealed and intact.");
    println!("Looking for the newest version of XeeonOS (needs the network)...");
    update::run(&[])?;
    offer_reboot(yes)
}

/// A root shell, for an administrator: the password of a user in the
/// `wheel` group is asked by `su` itself, run without root rights so that it
/// cannot skip the password.
fn shell() -> Result<()> {
    let user = ask("Administrator name: ")?;
    let groups = output("id", &["-nG", &user]).map_err(|_| err(format!("no user '{user}'")))?;
    if !groups.split_whitespace().any(|g| g == "wheel") {
        return Err(err(format!("{user} is not an administrator")));
    }
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
        println!("This erases all users, settings and files on this computer.");
        let answer = ask(&format!("Type {ERASE_WORD} to confirm: "))?;
        if answer != ERASE_WORD {
            println!("Nothing changed.");
            return Ok(());
        }
    }
    run("/usr/lib/systemd/systemd-factory-reset", &["request"])?;
    println!("All data will be erased at the restart.");
    reboot()
}

fn offer_reboot(yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    let answer = ask("Restart now? [Y/n] ")?;
    if matches!(answer.as_str(), "" | "y" | "Y" | "yes") {
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

fn ask(question: &str) -> Result<String> {
    print!("{question}");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line)? == 0 {
        return Err(err("no answer (end of input)"));
    }
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_recovery_entry() {
        let json = r#"[
            {"id":"xeeonos_1.0.1.efi","isSelected":false},
            {"id":"xeeonos_1.0.1.efi@1","isSelected":false},
            {"id":"xeeonos_1.0.2.efi","isSelected":true},
            {"id":"xeeonos_1.0.2.efi@1","isSelected":false}
        ]"#;
        assert_eq!(recovery_entry(json, &[]).unwrap(), "xeeonos_1.0.2.efi@1");
        assert!(recovery_entry("[]", &[]).is_err());
        // Only systemd-boot knows the Recovery entry.
        let json = r#"[{"id":"xeeonos_1.0.2.efi","isSelected":true}]"#;
        let loader = vec![
            "xeeonos_1.0.2.efi".to_string(),
            "xeeonos_1.0.2.efi@1".to_string(),
        ];
        assert_eq!(
            recovery_entry(json, &loader).unwrap(),
            "xeeonos_1.0.2.efi@1"
        );
    }

    #[test]
    fn loader_variable() {
        let mut data = vec![7, 0, 0, 0];
        for name in ["a.efi", "a.efi@1"] {
            for u in name.encode_utf16().chain([0]) {
                data.extend(u.to_le_bytes());
            }
        }
        assert_eq!(parse_loader_entries(&data), vec!["a.efi", "a.efi@1"]);
    }
}
