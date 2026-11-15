//! The decisions: signature, revocation, pinning, trusted authors.
//!
//!   /usr/lib/xapp/trusted.d/*.pub  keys XeeonOS trusts (with the system)
//!   /etc/xapp/trusted.d/*.pub      keys the administrator trusts
//!   /usr/lib/xapp/revoked          revoked keys, applications, images
//!   /etc/xapp/revoked              the administrator's revocations
//!   ~/.local/state/xapps/keys/<id>      the key pinned for an application
//!   ~/.local/state/xapps/approved/<id>  "Open anyway" said for that key
//!   ~/.local/state/xapps/verified       images already checked (cache)
//!
//! A revocation file has lines `key <hex>`, `app <application id>`,
//! `image <sha256>`; `#` starts a comment.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use xapp_core::paths;
use xapp_core::util::{Result, err, write_atomic};
use xapp_format::Container;

use crate::keys;

/// How much we know about the author of a correctly signed application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    /// Signed by a key the system or the administrator trusts.
    Trusted,
    /// Unknown author, but the user already said "Open anyway".
    Approved,
    /// Unknown author, not yet approved: ask.
    Unknown,
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The trusted public keys (hex).
pub fn trusted_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for dir in [Path::new(paths::SYSTEM_DIR), Path::new(paths::ADMIN_DIR)] {
        let Ok(entries) = fs::read_dir(dir.join("trusted.d")) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for f in files
            .iter()
            .filter(|f| f.extension().is_some_and(|e| e == "pub"))
        {
            for l in lines(f) {
                if let Some(k) = l.split_whitespace().next() {
                    keys.push(k.to_lowercase());
                }
            }
        }
    }
    keys
}

/// Is this application revoked? Returns the reason.
pub fn revoked(c: &Container) -> Option<String> {
    let m = &c.manifest;
    for file in [
        Path::new(paths::SYSTEM_DIR).join("revoked"),
        Path::new(paths::ADMIN_DIR).join("revoked"),
    ] {
        for l in lines(&file) {
            let mut words = l.split_whitespace();
            let (kind, value) = (
                words.next().unwrap_or(""),
                words.next().unwrap_or("").to_lowercase(),
            );
            let hit = match kind {
                "key" => value == m.signer,
                "app" => value == m.app_id.to_lowercase(),
                "image" => value == m.image.sha256,
                _ => false,
            };
            if hit {
                return Some(format!("{kind} revoked by {}", file.display()));
            }
        }
    }
    None
}

fn pin_file(app_id: &str) -> PathBuf {
    paths::state().join("keys").join(app_id)
}

fn approval_file(app_id: &str) -> PathBuf {
    paths::state().join("approved").join(app_id)
}

/// The key pinned for an application, if any.
pub fn pinned(app_id: &str) -> Option<String> {
    fs::read_to_string(pin_file(app_id))
        .ok()
        .map(|s| s.trim().to_lowercase())
}

/// Remember the key of an application (first time it is accepted).
pub fn pin(app_id: &str, key: &str) -> Result<()> {
    if pinned(app_id).is_none() {
        write_atomic(&pin_file(app_id), format!("{key}\n").as_bytes())?;
    }
    Ok(())
}

/// Remember that the user said "Open anyway" for this author.
pub fn approve(app_id: &str, key: &str) -> Result<()> {
    write_atomic(&approval_file(app_id), format!("{key}\n").as_bytes())
}

/// Forget everything about an application (`xapp remove --purge`).
pub fn forget(app_id: &str) {
    let _ = fs::remove_file(pin_file(app_id));
    let _ = fs::remove_file(approval_file(app_id));
}

/// Check a .xapp: signature, revocation, pinning; then how well we know its
/// author. An error means: refuse, there is no way to force it. (The image
/// itself is checked by `check_image_cached`.)
pub fn check(c: &Container) -> Result<Trust> {
    let m = &c.manifest;
    keys::verify(&m.signer, &c.manifest_bytes, &c.signature)?;
    if let Some(why) = revoked(c) {
        return Err(err(format!("{} cannot be opened: {why}", m.app_id)));
    }
    if let Some(key) = pinned(&m.app_id)
        && key != m.signer
    {
        return Err(err(format!(
            "{} is signed by a different author ({}) than the copy used before ({}): it may be a fake",
            m.app_id,
            keys::fingerprint(&m.signer),
            keys::fingerprint(&key)
        )));
    }
    if trusted_keys().contains(&m.signer) {
        return Ok(Trust::Trusted);
    }
    let approved = fs::read_to_string(approval_file(&m.app_id))
        .map(|s| s.trim().to_lowercase() == m.signer)
        .unwrap_or(false);
    Ok(if approved {
        Trust::Approved
    } else {
        Trust::Unknown
    })
}

/// `check_image`, remembering files already checked (same path, size, time,
/// inode and image digest), so that starting an application does not read
/// its whole image every time.
pub fn check_image_cached(c: &Container) -> Result<()> {
    let meta = fs::metadata(&c.path)?;
    let path = fs::canonicalize(&c.path)?;
    let entry = format!(
        "{} {} {} {} {}",
        c.manifest.image.sha256,
        meta.len(),
        meta.mtime(),
        meta.ino(),
        path.display()
    );
    let cache = paths::state().join("verified");
    let known = fs::read_to_string(&cache).unwrap_or_default();
    if known.lines().any(|l| l == entry) {
        return Ok(());
    }
    c.check_image()?;
    // Keep the last 200 entries.
    let mut all: Vec<&str> = known
        .lines()
        .filter(|l| !l.ends_with(&format!(" {}", path.display())))
        .collect();
    all.push(&entry);
    let start = all.len().saturating_sub(200);
    let _ = write_atomic(&cache, (all[start..].join("\n") + "\n").as_bytes());
    Ok(())
}
