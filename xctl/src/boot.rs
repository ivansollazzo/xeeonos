//! The boot menu: one kernel (UKI) per installed version, and automatic
//! rollback through boot counting.
//!
//! Each version has a file `xeeonos_<version>.efi` in `<ESP>/EFI/Linux`.
//! A freshly installed version gets a counter in its name:
//!
//! ```text
//! xeeonos_0.1.0-experimental.43+3-0.efi    3 attempts left, 0 done
//! ```
//!
//! At every boot of that version, systemd-boot moves one attempt from "left"
//! to "done" (`+2-1`, `+1-2`, `+0-3`) before starting it. When the system
//! reaches `boot-complete.target`, `systemd-bless-boot` removes the counter:
//! the version is good. If all attempts fail, the entry ends up with 0 left:
//! systemd-boot considers it bad, sorts it last and starts the previous
//! version instead.
//!
//! `xctl rollback` uses the same mechanism: it marks the running version as
//! bad (`+0`), so the next restart goes back to the previous one.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::util::{Result, err, need_root, output_opt};
use crate::version::{compare, read_os_release};

/// One version in the boot menu.
///
/// `#[derive(...)]` asks the compiler to write some standard code for us:
/// `Debug` (printing with `{:?}`, needed by the tests) and `PartialEq`
/// (comparing with `==`).
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub file: String,
    pub version: String,
    /// `None`: no counter, the version has started correctly before.
    pub tries_left: Option<u32>,
    pub tries_done: u32,
}

impl Entry {
    /// No attempts left: it failed to start (or was rolled back).
    pub fn is_bad(&self) -> bool {
        self.tries_left == Some(0)
    }

    /// A short description for `xctl status`.
    pub fn state(&self) -> String {
        match self.tries_left {
            None => "good".to_string(),
            Some(0) => "bad, will not be started".to_string(),
            Some(n) => format!("new, {n} attempt(s) left"),
        }
    }
}

/// Parse a file name like `xeeonos_<version>[+<left>[-<done>]].efi`.
/// Anything else (another OS's kernel, a stray file) gives `None`.
pub fn parse_entry(file: &str) -> Option<Entry> {
    // `?` on an Option: return None right away if there is no such prefix.
    let name = file.strip_prefix("xeeonos_")?.strip_suffix(".efi")?;
    let (version, counter) = match name.split_once('+') {
        Some((v, c)) => (v, Some(c)),
        None => (name, None),
    };
    if version.is_empty() {
        return None;
    }
    let (tries_left, tries_done) = match counter {
        None => (None, 0),
        Some(c) => {
            let (left, done) = c.split_once('-').unwrap_or((c, "0"));
            // `.ok()?` turns a parse error into None as well.
            (Some(left.parse().ok()?), done.parse().ok()?)
        }
    };
    Some(Entry {
        file: file.to_string(),
        version: version.to_string(),
        tries_left,
        tries_done,
    })
}

/// Where the kernels are: the boot partition (XBOOTLDR, /boot) if XeeonOS
/// was installed next to another system with a small EFI System Partition,
/// otherwise the EFI System Partition (normally /efi).
pub fn esp() -> PathBuf {
    output_opt("bootctl", &["--print-boot-path"])
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from("/efi"), PathBuf::from)
}

/// All XeeonOS versions in the boot menu, newest first.
pub fn entries(esp: &Path) -> Result<Vec<Entry>> {
    let dir = esp.join("EFI/Linux");
    let mut list = Vec::new();
    for item in
        fs::read_dir(&dir).map_err(|e| err(format!("cannot read {}: {e}", dir.display())))?
    {
        if let Some(entry) = parse_entry(&item?.file_name().to_string_lossy()) {
            list.push(entry);
        }
    }
    // `sort_by` takes a function that compares two elements; `b, a` (swapped)
    // sorts in descending order.
    list.sort_by(|a, b| compare(&b.version, &a.version));
    Ok(list)
}

/// Versions that failed to start or were rolled back. Used by `xctl update`
/// so that it does not install them again. Empty if the ESP cannot be read.
pub fn bad_versions() -> Vec<String> {
    entries(&esp())
        .unwrap_or_default()
        .into_iter()
        .filter(Entry::is_bad)
        .map(|e| e.version)
        .collect()
}

/// The version to go back to from `current`: the newest one that is older
/// and not bad.
pub fn previous<'a>(entries: &'a [Entry], current: &str) -> Option<&'a Entry> {
    entries
        .iter()
        .filter(|e| !e.is_bad() && compare(&e.version, current) == Ordering::Less)
        .max_by(|a, b| compare(&a.version, &b.version))
}

/// True when this boot is Recovery: the boot menu entry adds a word to the
/// kernel command line (see mkosi.conf.d/system/recovery.profile).
pub fn in_recovery() -> bool {
    has_recovery_word(&fs::read_to_string("/proc/cmdline").unwrap_or_default())
}

fn has_recovery_word(cmdline: &str) -> bool {
    cmdline
        .split_whitespace()
        .any(|w| w == config::RECOVERY_CMDLINE)
}

/// `xctl rollback`
pub fn rollback(args: &[String]) -> Result<()> {
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            other => return Err(err(format!("unknown option for rollback: {other}"))),
        }
    }
    need_root("rollback")?;

    let os = read_os_release(config::OS_RELEASE)?;
    let current = os.get("IMAGE_VERSION").cloned().unwrap_or_default();
    let esp = esp();
    let list = entries(&esp)?;

    let running = list
        .iter()
        .find(|e| e.version == current)
        .ok_or_else(|| err(format!("XeeonOS {current} is not in the boot menu")))?;
    let target = previous(&list, &current)
        .ok_or_else(|| err("there is no previous version to go back to"))?;

    // The previous version also needs its /usr: its slot is labeled with it.
    let usr = Path::new("/dev/disk/by-partlabel").join(format!("xeeonos_{}", target.version));
    if !usr.exists() {
        return Err(err(format!(
            "the /usr of XeeonOS {} is no longer on this disk",
            target.version
        )));
    }

    if !yes {
        if !crate::ui::interactive() {
            return Err(err(
                "no terminal to ask \"Go back to the previous version?\" on (give --yes)",
            ));
        }
        let question = format!(
            "Go back to XeeonOS {}?\n\nXeeonOS {current} will be turned off until the next \
             `sudo xctl update`, which installs it again. Your files and settings are not \
             touched.",
            target.version
        );
        if !crate::ui::yes_no("Rollback", &question, true)? {
            println!("Nothing changed.");
            return Ok(());
        }
    }

    // Mark the running version as bad: 0 attempts left.
    let dir = esp.join("EFI/Linux");
    let bad = format!("xeeonos_{}+0-{}.efi", running.version, running.tries_done);
    fs::rename(dir.join(&running.file), dir.join(&bad))?;
    // The ESP is FAT: make sure the rename reaches the disk.
    fs::File::open(&dir)?.sync_all()?;

    println!(
        "XeeonOS {current} is disabled. Restart to go back to XeeonOS {}.",
        target.version
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let e = parse_entry("xeeonos_0.1.0-experimental.42.efi").unwrap();
        assert_eq!(e.version, "0.1.0-experimental.42");
        assert_eq!(e.tries_left, None);
        assert!(!e.is_bad());

        let e = parse_entry("xeeonos_0.1.0-experimental.43+2-1.efi").unwrap();
        assert_eq!(e.version, "0.1.0-experimental.43");
        assert_eq!((e.tries_left, e.tries_done), (Some(2), 1));

        let e = parse_entry("xeeonos_1.0.0+0.efi").unwrap();
        assert!(e.is_bad());

        assert_eq!(parse_entry("arch-linux.efi"), None);
        assert_eq!(parse_entry("xeeonos_1.0.0+x.efi"), None);
        assert_eq!(parse_entry("xeeonos_.efi"), None);
    }

    #[test]
    fn recovery_cmdline() {
        assert!(has_recovery_word("root=dissect xeeonos.recovery quiet"));
        assert!(!has_recovery_word("root=dissect xeeonos.recovery=0"));
        assert!(!has_recovery_word(""));
    }

    #[test]
    fn previous_version() {
        let list: Vec<Entry> = [
            "xeeonos_1.0.3+0-3.efi", // bad
            "xeeonos_1.0.2.efi",     // running
            "xeeonos_1.0.1.efi",
            "xeeonos_1.0.0.efi",
        ]
        .iter()
        .filter_map(|f| parse_entry(f))
        .collect();
        assert_eq!(previous(&list, "1.0.2").unwrap().version, "1.0.1");
        assert_eq!(previous(&list, "1.0.0"), None);
    }
}
