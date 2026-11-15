//! `xapp sync`: bring the menu in line with ~/Applications.
//!
//! For every .xapp file of the folder: read it, check its signature,
//! revocation, pinning and image (a file still being copied is not
//! complete yet: it is tried again a little later); when several files have
//! the same application id, the highest version wins. Each application left
//! gets its menu entry; entries of applications whose file is gone are
//! removed (their data in ~/.var/xapps stays, always).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use xapp_core::paths;
use xapp_core::util::{Result, have};
use xapp_core::version;
use xapp_format::Container;
use xapp_permissions as perms;
use xapp_trust::{self as trust, Trust};

use crate::{entry, mime, registry};

/// What a sync did.
#[derive(Default, Debug)]
pub struct Report {
    pub registered: Vec<String>,
    pub removed: Vec<String>,
    /// (file, why it was left out)
    pub skipped: Vec<(PathBuf, String)>,
}

/// The .xapp files of ~/Applications.
pub fn files() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(paths::applications())
        .map(|e| {
            e.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("xapp"))
                        && !p
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .starts_with('.')
                        && p.is_file()
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Changed in the last minute: maybe still being copied.
fn recent(p: &PathBuf) -> bool {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(60))
}

/// One pass. Returns the report and whether some file looked incomplete.
fn pass() -> Result<(Report, bool)> {
    let mut report = Report::default();
    let mut incomplete = false;
    let mut best: BTreeMap<String, Container> = BTreeMap::new();
    for f in files() {
        let c = match Container::open(&f) {
            Ok(c) => c,
            Err(e) => {
                if recent(&f) {
                    incomplete = true;
                } else {
                    report.skipped.push((f, e.to_string()));
                }
                continue;
            }
        };
        let checked = trust::check(&c).and_then(|t| trust::check_image_cached(&c).map(|_| t));
        match checked {
            Ok(_) => {}
            Err(e) => {
                report.skipped.push((f, e.to_string()));
                continue;
            }
        }
        let id = c.manifest.app_id.clone();
        let newer = best
            .get(&id)
            .is_none_or(|old| version::compare(&c.manifest.version, &old.manifest.version).is_gt());
        if newer {
            if let Some(old) = best.remove(&id) {
                report.skipped.push((
                    old.path.clone(),
                    format!("a newer version of {id} is in {}", f.display()),
                ));
            }
            best.insert(id, c);
        } else {
            report
                .skipped
                .push((f, format!("a newer version of {id} is there")));
        }
    }
    let mut changed = false;
    for (id, c) in &best {
        // The first key seen for an application is remembered.
        trust::policy::pin(id, &c.manifest.signer)?;
        let known = !matches!(trust::check(c), Ok(Trust::Unknown));
        let autostart = perms::granted(&c.manifest, "autostart", known);
        changed |= entry::register(c, autostart)?;
        registry::put(c)?;
        report.registered.push(id.clone());
    }
    let mut stale: Vec<String> = entry::registered_ids();
    stale.extend(registry::all().into_iter().map(|e| e.app_id));
    stale.sort();
    stale.dedup();
    for id in stale.into_iter().filter(|id| !best.contains_key(id)) {
        entry::unregister(&id);
        registry::remove(&id);
        report.removed.push(id);
        changed = true;
    }
    if changed && have("update-desktop-database") {
        let _ = std::process::Command::new("update-desktop-database")
            .args(["-q", &entry::applications_dir().to_string_lossy()])
            .status();
    }
    Ok((report, incomplete))
}

/// `xapp sync`. Waits (up to about a minute) for files still being copied.
pub fn sync() -> Result<Report> {
    fs::create_dir_all(paths::applications())?;
    mime::setup()?;
    let mut tries = 0;
    loop {
        let (report, incomplete) = pass()?;
        tries += 1;
        if !incomplete || tries >= 30 {
            return Ok(report);
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
