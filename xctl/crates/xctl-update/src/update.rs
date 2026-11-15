//! `xctl update`: download, verify and install a new XeeonOS version.
//!
//! An update is a directory of files (see tools/mkupdate.sh in the repo):
//!
//!   xeeonos_<v>.efi                              kernel (UKI)
//!   xeeonos_<v>.usr-x86-64.<uuid>.raw.zst         /usr partition
//!   xeeonos_<v>.usr-x86-64-verity.<uuid>.raw.zst  its dm-verity hash tree
//!   xeeonos_<v>.<name>.xext                       optional extensions (xctl extensions)
//!   SHA256SUMS, SHA256SUMS.gpg                    manifest + signature
//!
//! Steps: fetch the signed manifest (a few hundred bytes), check its
//! signature against the keys shipped in /usr, and read the version from the
//! kernel's name in it. Only when that version is newer than the running one
//! (and did not already fail here) fetch the files it lists, check their
//! hashes, then let systemd-sysupdate write them into the free /usr slot and
//! the ESP. The running system is never modified: the new version starts at
//! the next boot. `--check` stops after the version check, without root.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::boot;
use crate::channel;
use crate::config;
use crate::util::{Result, err, need_root, output, run as run_program};
// Getting the files (xctl-fetch), also for the installer and the extensions.
use crate::version::{compare, read_os_release};
pub use xctl_fetch::manifest::{
    Cleanup, Source, check_hashes, download_with, fetch_files, fetch_manifest, is_plain_file_name,
    manifest_files, newest_version, versions_in,
};

pub fn run(args: &[String]) -> Result<()> {
    // Parse the options.
    let mut from = None;
    let mut check = false;
    let mut keyrings = vec![PathBuf::from(config::KEYRING)];
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--from" => {
                from = Some(
                    it.next()
                        .ok_or_else(|| err("--from needs a source"))?
                        .clone(),
                )
            }
            "--check" => check = true,
            "--keyring" => keyrings.push(PathBuf::from(
                it.next().ok_or_else(|| err("--keyring needs a file"))?,
            )),
            other => return Err(err(format!("unknown option for update: {other}"))),
        }
    }
    if !check {
        need_root("update")?;
    }

    let os = read_os_release(config::OS_RELEASE)?;
    let current = os.get("IMAGE_VERSION").cloned().unwrap_or_default();

    // Without --from, use the channel's tag on the registry.
    let from_given = from.is_some();
    let source = match from {
        Some(f) => Source::parse(&f),
        None => {
            let Some(channel) = channel::current()? else {
                if check {
                    // The daily check on a development build: nothing to follow.
                    println!(
                        "This is a development build without an update channel: nothing to check."
                    );
                    return Ok(());
                }
                return Err(err(
                    "this is a development build without an update channel; use --from <source>",
                ));
            };
            let tag = channel::resolve(&channel)?;
            Source::Oci(format!("{}:{tag}", config::REGISTRY))
        }
    };

    // Start from an empty download directory, and clean it up at the end
    // whatever happens (the `Cleanup` value removes it when dropped).
    // (--check runs without root: it uses a private temporary directory.)
    let check_dir = std::env::temp_dir().join(format!("xctl-check-{}", std::process::id()));
    let cache = if check {
        check_dir.as_path()
    } else {
        Path::new(config::CACHE)
    };
    let _ = fs::remove_dir_all(cache);
    if !check {
        make_cache_subvolume(cache);
    }
    fs::create_dir_all(cache)?;
    let _cleanup = Cleanup(cache);

    // First only the signed manifest: it says which version the source has.
    fetch_manifest(&source, cache, &keyrings)?;
    let names = manifest_files(&fs::read_to_string(cache.join("SHA256SUMS"))?)?;
    let new = versions_in(&names)
        .into_iter()
        .max_by(|a, b| compare(a, b))
        .ok_or_else(|| err("the source contains no XeeonOS version"))?;
    // Whatever the outcome, the "update available" note is refreshed below.
    set_available(None);
    match compare(&new, &current) {
        Ordering::Equal => {
            println!("XeeonOS {current} is up to date.");
            return Ok(());
        }
        Ordering::Less => {
            println!(
                "The source has XeeonOS {new}, older than the running {current}: nothing to do."
            );
            return Ok(());
        }
        Ordering::Greater => {}
    }
    // A version turned off here (it failed to start 3 times, or the
    // administrator went back from it with `xctl rollback`): `sudo xctl
    // update` installs it again from scratch, but the daily check does not
    // offer it again, unless update.conf says OfferFailed=yes.
    let turned_off = boot::bad_versions().contains(&new);
    if check && turned_off && !offer_failed() {
        println!(
            "XeeonOS {new} is turned off on this computer (it did not start, or you went back \
             from it): not offered again. To install it again: sudo xctl update"
        );
        return Ok(());
    }
    if check {
        set_available(Some(&new));
        println!(
            "XeeonOS {new} is available (running: {current}). Install it with: sudo xctl update"
        );
        return Ok(());
    }

    // The new version's copy of the optional extensions turned on here (xctl
    // ext), published by the extensions' repository for every version (or,
    // with --from, in the same source). If one is not published yet, wait:
    // install nothing now, so the system does not lose it.
    let wanted = crate::ext::enabled_optional();
    let ext_dir = cache.join("extensions");
    let mut ext_files = Vec::new();
    if !wanted.is_empty() {
        let ext_source = if from_given {
            source.clone()
        } else {
            Source::parse(&crate::ext::default_source(&new))
        };
        fs::create_dir_all(&ext_dir)?;
        let published = fetch_manifest(&ext_source, &ext_dir, &keyrings)
            .and_then(|()| manifest_files(&fs::read_to_string(ext_dir.join("SHA256SUMS"))?))
            .unwrap_or_default();
        let mut missing = Vec::new();
        for name in &wanted {
            match crate::ext::files_of(name, &new, &published) {
                Some(files) => ext_files.extend(files),
                None => missing.push(name.as_str()),
            }
        }
        if !missing.is_empty() {
            println!(
                "XeeonOS {new} is out, but the extension(s) {} turned on here are not \
                 published for it yet (they are built right after every version): \
                 nothing installed. Try again later, or turn them off \
                 (sudo xctl extensions disable <name>) to update without them.",
                missing.join(", ")
            );
            return Ok(());
        }
        fetch_files(&ext_source, &ext_dir, &ext_files)?;
        check_hashes(&ext_dir)?;
    }

    // Only now the large files of the system (not the extensions published
    // with it, nor the package list for building them).
    let names: Vec<String> = names
        .into_iter()
        .filter(|file| {
            crate::ext::extension_of(file).is_none()
                && !file.ends_with(".pkgdb.tar.zst")
                && !file.ends_with(".build.txt")
        })
        .collect();
    fetch_files(&source, cache, &names)?;
    check_hashes(cache)?;
    // The copy of a turned-off version still on the disk may be damaged:
    // it is written again from the verified download, never reused.
    if turned_off {
        forget(&new)?;
    }
    check_boot_space(cache, &new, &current)?;
    // A copy of the data to go back to (xctl snapshots), if it can be taken.
    match crate::snapshots::create(&format!("before-{new}")) {
        Ok(name) => println!("Snapshot of the data: {name}"),
        Err(e) => println!("No snapshot of the data: {e}"),
    }
    println!("Installing XeeonOS {new} (running: {current})...");
    // The files were verified above against the signed manifest.
    run_program(config::SYSUPDATE, &["update", &new])?;
    for name in &wanted {
        crate::ext::store(&ext_dir, &new, name)?;
        println!("Extension {name}: installed for XeeonOS {new}.");
    }
    // Downloaded extensions of versions that are gone from the boot menu.
    if let Ok(entries) = boot::entries(&boot::esp()) {
        let keep: Vec<String> = entries.into_iter().map(|e| e.version).collect();
        crate::ext::prune(&keep);
    }
    println!("XeeonOS {new} is installed. Restart to start using it.");
    Ok(())
}

/// The download directory as a Btrfs subvolume of its own, so that the
/// snapshot of the data taken before installing (xctl snapshots) does not
/// keep a copy of the downloaded update (about 2 GB each). Where that is not
/// possible (not Btrfs), the caller creates a plain directory.
fn make_cache_subvolume(cache: &Path) {
    if let Some(parent) = cache.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = Command::new("btrfs")
        .args(["--quiet", "subvolume", "create"])
        .arg(cache)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Does update.conf ask the daily check to offer turned-off versions too?
/// (`OfferFailed=yes`; the default is no.)
fn offer_failed() -> bool {
    fs::read_to_string(config::UPDATE_CONF)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .any(|(k, v)| k.trim() == "OfferFailed" && matches!(v.trim(), "yes" | "true" | "1"))
}

/// Remove an installed, turned-off version, so that systemd-sysupdate
/// installs it again from scratch: its kernel leaves the boot menu, and its
/// /usr partitions are labeled "_empty", the label sysupdate gives (and
/// looks for in) a free slot. Never the running version (the caller only
/// passes a newer one).
fn forget(version: &str) -> Result<()> {
    let esp = boot::esp();
    for entry in boot::entries(&esp)? {
        if entry.version == version {
            fs::remove_file(esp.join("EFI/Linux").join(&entry.file))?;
        }
    }
    let label = format!("xeeonos_{version}");
    let list = output(
        "lsblk",
        &[
            "--raw",
            "--noheadings",
            "--output",
            "PARTLABEL,PKNAME,PARTN",
        ],
    )?;
    for line in list.lines() {
        let mut w = line.split_whitespace();
        if let (Some(l), Some(disk), Some(n)) = (w.next(), w.next(), w.next())
            && l == label
        {
            // The disk is in use: write the table only, sysupdate reads it
            // from the disk itself.
            run_program(
                "sfdisk",
                &[
                    "--quiet",
                    "--no-reread",
                    "--no-tell-kernel",
                    "--part-label",
                    &format!("/dev/{disk}"),
                    n,
                    "_empty",
                ],
            )?;
        }
    }
    println!("The copy of XeeonOS {version} on this computer was removed; installing it again.");
    Ok(())
}

/// Is there room on the boot partition for the new kernel? Checked before
/// anything is written, so that an update never stops half way.
/// systemd-sysupdate keeps 2 versions (InstancesMax=2): before writing the
/// new one it removes every version but the running one, so their space
/// counts as free.
fn check_boot_space(dir: &Path, new: &str, current: &str) -> Result<()> {
    let need = fs::metadata(dir.join(format!("xeeonos_{new}.efi")))?.len();
    let esp = boot::esp();
    let avail = output(
        "df",
        &["--output=avail", "--block-size=1", &esp.to_string_lossy()],
    )?;
    let mut free: u64 = avail
        .lines()
        .nth(1)
        .and_then(|l| l.trim().parse().ok())
        .ok_or_else(|| err(format!("cannot read the free space of {}", esp.display())))?;
    for entry in boot::entries(&esp)? {
        if entry.version != current
            && let Ok(meta) = fs::metadata(esp.join("EFI/Linux").join(&entry.file))
        {
            free += meta.len();
        }
    }
    if !fits(need, free) {
        return Err(err(format!(
            "not enough space on the boot partition ({}) for XeeonOS {new}: it needs {} MB, \
             {} MB can be made free. Nothing was changed. Please report this problem.",
            esp.display(),
            need / MB,
            free / MB
        )));
    }
    Ok(())
}

const MB: u64 = 1024 * 1024;

/// The kernel fits, with 16 MB to spare (boot loader files, the file system).
fn fits(need: u64, free: u64) -> bool {
    need + 16 * MB <= free
}

/// Remember (or forget) that a newer version is available, for the message
/// at login and `xctl status`. Only root can write it (the daily check runs as
/// root); for others this does nothing.
fn set_available(version: Option<&str>) {
    let file = Path::new(config::UPDATE_AVAILABLE);
    match version {
        Some(v) => {
            if let Some(dir) = file.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(file, format!("{v}\n"));
        }
        None => {
            let _ = fs::remove_file(file);
        }
    }
}

/// The version noted by the last check, if a newer one is available.
pub fn available() -> Option<String> {
    let v = fs::read_to_string(config::UPDATE_AVAILABLE).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_space() {
        assert!(fits(450 * MB, 500 * MB));
        assert!(!fits(450 * MB, 460 * MB));
    }
}
