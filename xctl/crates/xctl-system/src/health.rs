//! `xctl health`: is the disk in good shape?
//!
//! Two sources:
//! - Btrfs keeps error counters for the data partition (read, write and
//!   checksum errors, also those found by the monthly scrub);
//! - the disks' own self-assessment (SMART, via smartctl).
//!
//! Reading them needs root, so xeeonos-health.timer (daily, and after every
//! scrub) runs `xctl health` as root and notes the problems in
//! /var/lib/xeeonos/health; `xctl status` and the login message show them.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config;
use crate::util::{Result, is_root, output_opt};

pub fn run_health(args: &[String]) -> Result<()> {
    if let Some(arg) = args.first() {
        return Err(crate::util::err(format!(
            "unknown option for health: {arg}"
        )));
    }
    let list = if is_root() {
        let list = check();
        save(&list);
        list
    } else {
        problems()
    };
    if list.is_empty() {
        println!("Disk health: good");
    } else {
        for p in &list {
            println!("Disk health: PROBLEM: {p}");
        }
        println!("Copy your files somewhere else, then see `sudo xctl report`.");
    }
    Ok(())
}

/// The problems noted by the last check (empty: none, or never checked).
pub fn problems() -> Vec<String> {
    fs::read_to_string(config::HEALTH)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn save(list: &[String]) {
    let file = Path::new(config::HEALTH);
    if list.is_empty() {
        let _ = fs::remove_file(file);
        return;
    }
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(file, list.join("\n") + "\n");
}

/// Look now (root).
fn check() -> Vec<String> {
    let mut list = Vec::new();
    // --check: the exit status is not 0 when a counter is not zero.
    let stats = Command::new("btrfs")
        .args(["device", "stats", "--check", "/"])
        .output();
    if let Ok(out) = stats
        && !out.status.success()
        && String::from_utf8_lossy(&out.stdout).contains("errs")
    {
        list.push(
            "the data partition had read, write or checksum errors (btrfs device stats /)"
                .to_string(),
        );
    }
    for disk in disks() {
        if let Ok(out) = Command::new("smartctl")
            .args(["--health", "--quietmode=silent", &format!("/dev/{disk}")])
            .status()
            && let Some(code) = out.code()
            && smart_failing(code)
        {
            list.push(format!(
                "the disk {disk} reports that it is failing (SMART)"
            ));
        }
    }
    list
}

/// The physical disks (not zram, loop devices or optical drives).
fn disks() -> Vec<String> {
    output_opt(
        "lsblk",
        &["--nodeps", "--noheadings", "--output", "NAME,TYPE"],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|l| {
        let mut w = l.split_whitespace();
        let (name, kind) = (w.next()?, w.next()?);
        (kind == "disk" && !name.starts_with("zram")).then(|| name.to_string())
    })
    .collect()
}

/// smartctl's exit status is a set of bits: 8 = "DISK FAILING", 16 = a
/// pre-failure attribute is past its threshold. The others (no SMART on
/// this device, a command failed, old errors in the log) are not a verdict.
fn smart_failing(code: i32) -> bool {
    code & (8 | 16) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_bits() {
        assert!(!smart_failing(0));
        assert!(!smart_failing(4)); // no SMART (virtual disk)
        assert!(!smart_failing(64)); // errors in the log, disk still fine
        assert!(smart_failing(8));
        assert!(smart_failing(16 | 4));
    }
}
