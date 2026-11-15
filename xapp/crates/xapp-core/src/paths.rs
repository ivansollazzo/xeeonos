//! Where things are. Everything of an application lives in the user's home
//! (no root needed), except the system's lists of trusted and revoked keys.
//!
//!   ~/Applications/                 the .xapp files (one per application)
//!   ~/.var/xapps/<app id>/          the application's data: its home inside
//!                                   the sandbox (never deleted by itself)
//!   ~/.local/state/xapps/           what xapp remembers: registry, keys,
//!                                   permissions, icons
//!   ~/.local/share/applications/    the menu entries made by xapp sync
//!   $XDG_RUNTIME_DIR/xapp/          mounted images, running instances

use std::env;
use std::path::PathBuf;

/// Lists of the system (read-only, come with XeeonOS updates).
pub const SYSTEM_DIR: &str = "/usr/lib/xapp";
/// The administrator's additions (trusted keys, revocations).
pub const ADMIN_DIR: &str = "/etc/xapp";
/// The file-type description of .xapp files (copied to the user's database).
pub const MIME_XML: &str = "/usr/share/xapp/xeeonos-xapp.xml";

/// The user's home directory.
pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// An XDG base directory: the variable if set (and absolute), else the default
/// under the home directory.
fn xdg(var: &str, default: &str) -> PathBuf {
    env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(default))
}

/// ~/Applications: where the user puts .xapp files.
pub fn applications() -> PathBuf {
    home().join("Applications")
}

/// ~/.var/xapps/<app id>: the data of one application.
pub fn app_data(app_id: &str) -> PathBuf {
    home().join(".var/xapps").join(app_id)
}

/// ~/.local/state/xapps: what xapp remembers.
pub fn state() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("xapps")
}

/// ~/.local/share (menu entries, file types).
pub fn data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// ~/.config.
pub fn config_home() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// ~/.cache.
pub fn cache_home() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}

/// $XDG_RUNTIME_DIR (/run/user/<uid>), where the session's sockets are.
pub fn runtime_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", crate::util::uid())))
}

/// $XDG_RUNTIME_DIR/xapp: mounted images, running instances, dialogs.
pub fn runtime() -> PathBuf {
    runtime_dir().join("xapp")
}
