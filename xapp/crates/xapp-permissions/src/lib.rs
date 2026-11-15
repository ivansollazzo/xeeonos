//! Permissions: what an application may do beyond running in its sandbox,
//! like the TCC of macOS.
//!
//! The application declares in its manifest the permissions it may ask for,
//! each with a reason ("Firefox uses the microphone for video calls"). A
//! permission it did not declare can never be given. The user decides the
//! others: some are given by default (network, graphics, notifications, for
//! an application from a trusted or approved author), the rest are asked
//! when the application starts, and every choice can be changed with
//! `xapp permissions`. What is not given does not exist in the sandbox: a
//! folder not given is not there at all.
//!
//! Choices: ~/.local/state/xapps/permissions/<app id>, lines `<id>=yes|no`.

use std::fs;
use std::path::PathBuf;

use xapp_core::paths;
use xapp_core::util::{Result, err, read_kv, write_atomic};
use xapp_format::{Manifest, Permission};

/// A known permission.
pub struct Info {
    pub id: &'static str,
    /// What it gives, for people.
    pub label: &'static str,
    /// Given without asking (to applications whose author is trusted or
    /// approved), unless the user said no.
    pub default: bool,
    /// For folders: the XDG name of the folder (DOCUMENTS, DOWNLOAD, ...)
    /// and the English default.
    pub folder: Option<(&'static str, &'static str)>,
}

pub const CATALOG: &[Info] = &[
    Info {
        id: "network",
        label: "Network and internet",
        default: true,
        folder: None,
    },
    Info {
        id: "gpu",
        label: "Graphics acceleration (GPU)",
        default: true,
        folder: None,
    },
    Info {
        id: "notifications",
        label: "Notifications",
        default: true,
        folder: None,
    },
    Info {
        id: "documents",
        label: "Your Documents folder",
        default: false,
        folder: Some(("DOCUMENTS", "Documents")),
    },
    Info {
        id: "downloads",
        label: "Your Downloads folder",
        default: false,
        folder: Some(("DOWNLOAD", "Downloads")),
    },
    Info {
        id: "pictures",
        label: "Your Pictures folder",
        default: false,
        folder: Some(("PICTURES", "Pictures")),
    },
    Info {
        id: "music",
        label: "Your Music folder",
        default: false,
        folder: Some(("MUSIC", "Music")),
    },
    Info {
        id: "videos",
        label: "Your Videos folder",
        default: false,
        folder: Some(("VIDEOS", "Videos")),
    },
    Info {
        id: "desktop",
        label: "Your Desktop folder",
        default: false,
        folder: Some(("DESKTOP", "Desktop")),
    },
    Info {
        id: "audio",
        label: "Sound and microphone",
        default: false,
        folder: None,
    },
    Info {
        id: "camera",
        label: "Camera",
        default: false,
        folder: None,
    },
    Info {
        id: "x11",
        label: "X11 windows (older programs; weaker isolation)",
        default: false,
        folder: None,
    },
    Info {
        id: "autostart",
        label: "Start when you log in",
        default: false,
        folder: None,
    },
];

/// A permission's description, if it is a known one.
pub fn info(id: &str) -> Option<&'static Info> {
    CATALOG.iter().find(|i| i.id == id)
}

/// The state of one permission for one application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Yes,
    No,
    /// Not decided yet: to ask at the next start.
    Ask,
}

fn file(app_id: &str) -> PathBuf {
    paths::state().join("permissions").join(app_id)
}

/// What the user decided (`None`: nothing yet).
pub fn decided(app_id: &str, id: &str) -> Option<bool> {
    read_kv(&file(app_id))
        .into_iter()
        .rev()
        .find(|(k, _)| k == id)
        .map(|(_, v)| v == "yes")
}

/// Record a choice (`None`: forget it, ask again).
pub fn set(app_id: &str, id: &str, value: Option<bool>) -> Result<()> {
    if info(id).is_none() {
        return Err(err(format!(
            "unknown permission \"{id}\" (known: {})",
            known()
        )));
    }
    let mut entries: Vec<(String, String)> = read_kv(&file(app_id))
        .into_iter()
        .filter(|(k, _)| k != id)
        .collect();
    if let Some(v) = value {
        entries.push((id.to_string(), if v { "yes" } else { "no" }.to_string()));
    }
    let text: String = entries.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    write_atomic(&file(app_id), text.as_bytes())
}

/// Forget all the choices for an application.
pub fn forget(app_id: &str) {
    let _ = fs::remove_file(file(app_id));
}

/// The names of all known permissions, for messages.
pub fn known() -> String {
    CATALOG.iter().map(|i| i.id).collect::<Vec<_>>().join(", ")
}

/// The state of a permission. `known_author`: trusted or approved author
/// (defaults apply only then).
pub fn state(m: &Manifest, id: &str, known_author: bool) -> State {
    let Some(i) = info(id) else { return State::No };
    if m.declares(id).is_none() {
        return State::No;
    }
    match decided(&m.app_id, id) {
        Some(true) => State::Yes,
        Some(false) => State::No,
        None if i.default && known_author => State::Yes,
        None => State::Ask,
    }
}

/// Is the permission given?
pub fn granted(m: &Manifest, id: &str, known_author: bool) -> bool {
    state(m, id, known_author) == State::Yes
}

/// The declared permissions still to ask about, in catalog order.
pub fn pending(m: &Manifest, known_author: bool) -> Vec<&Permission> {
    CATALOG
        .iter()
        .filter_map(|i| m.declares(i.id))
        .filter(|p| state(m, &p.id, known_author) == State::Ask)
        .collect()
}

/// Every declared permission with its state (unknown ones: never given).
pub fn all(m: &Manifest, known_author: bool) -> Vec<(&Permission, State)> {
    m.permissions
        .iter()
        .map(|p| (p, state(m, &p.id, known_author)))
        .collect()
}

/// The real path of a user folder (from ~/.config/user-dirs.dirs, which
/// holds the translated names: ~/Documenti in Italian).
pub fn folder_path(id: &str) -> Option<PathBuf> {
    let (xdg, default) = info(id)?.folder?;
    let home = paths::home();
    let dirs = fs::read_to_string(paths::config_home().join("user-dirs.dirs")).unwrap_or_default();
    let key = format!("XDG_{xdg}_DIR=");
    let from_file = dirs.lines().find_map(|l| {
        let value = l.trim().strip_prefix(&key)?.trim_matches('"');
        let path = if let Some(rest) = value.strip_prefix("$HOME") {
            PathBuf::from(format!("{}{rest}", home.display()))
        } else {
            PathBuf::from(value)
        };
        // A folder set to the home itself means "none" for xdg-user-dirs.
        (path.is_absolute() && path != home).then_some(path)
    });
    Some(from_file.unwrap_or_else(|| home.join(default)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        for (n, i) in CATALOG.iter().enumerate() {
            assert!(
                CATALOG[n + 1..].iter().all(|j| j.id != i.id),
                "duplicate {}",
                i.id
            );
            assert!(
                !(i.default && i.folder.is_some()),
                "a folder is never given by default"
            );
        }
        assert!(info("camera").is_some());
        assert!(info("everything").is_none());
    }
}
