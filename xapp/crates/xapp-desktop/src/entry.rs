//! Menu entries: one .desktop file per application in
//! ~/.local/share/applications/<app id>.desktop (named after the id, so the
//! desktop matches the application's windows to it), its icon in
//! ~/.local/state/xapps/icons, and, with the permission `autostart`, a copy
//! in ~/.config/autostart. Every file made by xapp carries `X-Xapp-Id=`, so
//! xapp only ever removes its own.

use std::fs;
use std::path::PathBuf;

use xapp_core::paths;
use xapp_core::util::{Result, write_if_changed};
use xapp_format::Container;

pub const MARK: &str = "X-Xapp-Id=";

pub fn applications_dir() -> PathBuf {
    paths::data_home().join("applications")
}

fn autostart_dir() -> PathBuf {
    paths::config_home().join("autostart")
}

fn icon_path(app_id: &str, kind: &str) -> PathBuf {
    paths::state()
        .join("icons")
        .join(format!("{app_id}.{kind}"))
}

/// One line of a .desktop file: no line breaks in values.
fn clean(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// The text of the menu entry.
pub fn desktop_file(c: &Container, icon: &str) -> String {
    let m = &c.manifest;
    let mut t = String::from("[Desktop Entry]\nType=Application\n");
    t += &format!("Name={}\n", clean(&m.name));
    for (lang, name) in &m.names {
        t += &format!("Name[{}]={}\n", clean(lang), clean(name));
    }
    if !m.description.is_empty() {
        t += &format!("Comment={}\n", clean(&m.description));
    }
    t += &format!(
        "Exec=xapp run {} %U\nIcon={icon}\nTerminal=false\n",
        m.app_id
    );
    if !m.categories.is_empty() {
        t += &format!("Categories={};\n", clean(&m.categories.join(";")));
    }
    if !m.mime_types.is_empty() {
        t += &format!("MimeType={};\n", clean(&m.mime_types.join(";")));
    }
    if !m.wm_class.is_empty() {
        t += &format!("StartupWMClass={}\n", clean(&m.wm_class));
    }
    t += &format!(
        "{MARK}{}\nX-Xapp-File={}\n",
        m.app_id,
        clean(&c.path.display().to_string())
    );
    t
}

/// Write (or update) the menu entry and icon of an application; true if
/// something changed.
pub fn register(c: &Container, autostart: bool) -> Result<bool> {
    let m = &c.manifest;
    let icon = icon_path(&m.app_id, &m.icon_type);
    let mut changed = write_if_changed(&icon, &c.icon)?;
    let text = desktop_file(c, &icon.display().to_string());
    changed |= write_if_changed(
        &applications_dir().join(format!("{}.desktop", m.app_id)),
        text.as_bytes(),
    )?;
    let auto = autostart_dir().join(format!("{}.desktop", m.app_id));
    if autostart {
        changed |= write_if_changed(&auto, text.as_bytes())?;
    } else if is_ours(&auto) {
        let _ = fs::remove_file(&auto);
        changed = true;
    }
    Ok(changed)
}

fn is_ours(path: &std::path::Path) -> bool {
    fs::read_to_string(path).is_ok_and(|t| t.lines().any(|l| l.starts_with(MARK)))
}

/// The application ids that have a menu entry made by xapp.
pub fn registered_ids() -> Vec<String> {
    let mut ids = Vec::new();
    if let Ok(entries) = fs::read_dir(applications_dir()) {
        for e in entries.flatten() {
            if let Ok(text) = fs::read_to_string(e.path())
                && let Some(id) = text.lines().find_map(|l| l.strip_prefix(MARK))
            {
                ids.push(id.trim().to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Remove the menu entry, icon and autostart copy of an application.
pub fn unregister(app_id: &str) {
    for f in [
        applications_dir().join(format!("{app_id}.desktop")),
        autostart_dir().join(format!("{app_id}.desktop")),
    ] {
        if is_ours(&f) {
            let _ = fs::remove_file(f);
        }
    }
    for kind in ["png", "svg"] {
        let _ = fs::remove_file(icon_path(app_id, kind));
    }
}
