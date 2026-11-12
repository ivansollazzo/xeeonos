//! `xctl snapshots`: copies of the data (settings and users' files) taken
//! before every update, to go back to if something goes wrong.
//!
//! The data partition is Btrfs. A snapshot is a read-only copy of it, made in
//! an instant and taking no space until files change. They live in
//! /.snapshots/<name>; the newest few are kept (KEEP), older ones are deleted.
//!
//! Restoring brings back /etc (settings) and /home (users' files) as they were
//! in the snapshot. It runs only in Recovery, where nobody is logged in; the
//! current state is first saved as one more snapshot, so a restore can be
//! undone too. /var (logs, caches, the system's own state) is left as it is.

use std::fs;
use std::path::Path;

use crate::boot;
use crate::ui;
use crate::util::{Result, err, need_root, output, run};

/// Where the snapshots are kept, on the data partition.
pub const DIR: &str = "/.snapshots";
/// How many snapshots are kept.
const KEEP: usize = 3;
/// What a restore brings back.
const RESTORED: &[&str] = &["etc", "home"];

pub fn run_snapshots(args: &[String]) -> Result<()> {
    let mut words = Vec::new();
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            a => words.push(a),
        }
    }
    match words.as_slice() {
        [] | ["list"] => {
            let list = list()?;
            if list.is_empty() {
                println!("No snapshots yet: one is taken before every update.");
            }
            for name in list {
                println!("{name}");
            }
            Ok(())
        }
        ["create"] => {
            need_root("snapshots")?;
            let name = create("manual")?;
            println!("Snapshot {name} taken.");
            Ok(())
        }
        ["restore", name] => {
            need_root("snapshots")?;
            restore(name, yes)
        }
        _ => Err(err(
            "usage: xctl snapshots [list | create | restore <name> [--yes]]",
        )),
    }
}

/// The snapshots, oldest first (names start with the date and time).
pub fn list() -> Result<Vec<String>> {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(DIR) {
        for entry in entries {
            names.push(entry?.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Is / on Btrfs (the installed system; not the live ISO)?
fn supported() -> bool {
    output("stat", &["--file-system", "--format=%T", "/"]).is_ok_and(|t| t.trim() == "btrfs")
}

/// Take a snapshot of the data partition named "<date>-<reason>", then delete
/// the oldest ones beyond KEEP. Returns its name.
pub fn create(reason: &str) -> Result<String> {
    if !supported() {
        return Err(err("snapshots need the installed system (Btrfs)"));
    }
    fs::create_dir_all(DIR)?;
    let date = output("date", &["+%Y-%m-%d_%H%M%S"])?;
    let name = format!("{}-{reason}", date.trim());
    let path = format!("{DIR}/{name}");
    run(
        "btrfs",
        &["--quiet", "subvolume", "snapshot", "-r", "/", &path],
    )?;
    let all = list()?;
    if all.len() > KEEP {
        for old in &all[..all.len() - KEEP] {
            let _ = run(
                "btrfs",
                &["--quiet", "subvolume", "delete", &format!("{DIR}/{old}")],
            );
        }
    }
    Ok(name)
}

/// Bring back /etc and /home from a snapshot (Recovery only).
fn restore(name: &str, yes: bool) -> Result<()> {
    if !boot::in_recovery() {
        return Err(err(
            "restoring works only in Recovery; run `sudo xctl recovery` to restart into it",
        ));
    }
    if name.contains('/') || !list()?.iter().any(|n| n == name) {
        return Err(err(format!(
            "no snapshot named {name}; see `xctl snapshots`"
        )));
    }
    if !yes
        && !ui::yes_no(
            "Restore data",
            &format!(
                "Bring back settings and users' files as they were in {name}?\n\nThe \
                 current state is saved first as one more snapshot."
            ),
            true,
        )?
    {
        return Ok(());
    }
    let saved = create("before-restore")?;
    let from = Path::new(DIR).join(name);
    for dir in RESTORED {
        let target = Path::new("/").join(dir);
        // Empty the directory, then copy the snapshot's version back. On the
        // same Btrfs, --reflink shares the data: instant, and no extra space.
        if target.exists() {
            for entry in fs::read_dir(&target)? {
                let path = entry?.path();
                if path.is_dir() && !path.is_symlink() {
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
            }
        }
        let source = from.join(dir);
        if source.exists() {
            run(
                "cp",
                &[
                    "--archive",
                    "--reflink=always",
                    "--no-target-directory",
                    &source.to_string_lossy(),
                    &target.to_string_lossy(),
                ],
            )?;
        }
    }
    println!("Restored settings and files from {name} (the state before is in {saved}).");
    Ok(())
}

/// The Recovery menu item: pick a snapshot, then restore it.
pub fn screen() -> Result<()> {
    let names = list()?;
    if names.is_empty() {
        return ui::message("Restore data", "There are no snapshots yet.");
    }
    let items: Vec<(String, String)> = names
        .iter()
        .rev()
        .map(|n| (n.clone(), String::new()))
        .collect();
    let Some(name) = ui::menu(
        "Restore data",
        "Settings and users' files as they were at:",
        &items,
        None,
    )?
    else {
        return Ok(());
    };
    restore(&name, false)?;
    ui::message(
        "Restore data",
        &format!("Done: settings and files are back as in {name}."),
    )
}
