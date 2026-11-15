//! The filter between an application and the session bus (D-Bus):
//! xdg-dbus-proxy, with a policy that denies everything except what is
//! listed here. The application gets the proxy's socket as its session bus.
//!
//! Always allowed: owning its own name (org.example.App, needed by GTK and
//! Qt), the parts of the desktop portal that ask the user or only open
//! something (opening links and files, settings like dark style, inhibiting
//! sleep, screen sharing, printing, e-mail). With permissions: notifications,
//! the camera portal. Everything else (other applications, GNOME Shell,
//! secrets, the file manager...) does not exist for the application.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use xapp_core::paths;
use xapp_core::util::have;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

/// The proxy's rules for an application.
pub fn rules(app_id: &str, notifications: bool, camera: bool) -> Vec<String> {
    let mut r = vec![format!("--own={app_id}"), format!("--own={app_id}.*")];
    let mut portal = vec![
        "org.freedesktop.DBus.Properties.*",
        "org.freedesktop.portal.OpenURI.*",
        "org.freedesktop.portal.Settings.*",
        "org.freedesktop.portal.Inhibit.*",
        "org.freedesktop.portal.ScreenCast.*",
        "org.freedesktop.portal.Print.*",
        "org.freedesktop.portal.Email.*",
        "org.freedesktop.portal.Session.*",
        "org.freedesktop.portal.Request.*",
    ];
    if notifications {
        portal.push("org.freedesktop.portal.Notification.*");
        r.push("--talk=org.freedesktop.Notifications".into());
    }
    if camera {
        portal.push("org.freedesktop.portal.Camera.*");
    }
    for iface in portal {
        r.push(format!("--call={PORTAL}={iface}@{PORTAL_PATH}"));
        r.push(format!("--call={PORTAL}={iface}@{PORTAL_PATH}/*"));
    }
    r.push(format!(
        "--broadcast={PORTAL}=org.freedesktop.portal.*@{PORTAL_PATH}"
    ));
    r.push(format!(
        "--broadcast={PORTAL}=org.freedesktop.portal.*@{PORTAL_PATH}/*"
    ));
    r
}

/// The session bus address of the user, if there is one.
fn session_bus() -> Option<String> {
    if let Ok(a) = std::env::var("DBUS_SESSION_BUS_ADDRESS")
        && !a.is_empty()
    {
        return Some(a);
    }
    let socket = paths::runtime_dir().join("bus");
    socket
        .exists()
        .then(|| format!("unix:path={}", socket.display()))
}

/// A running proxy; dropping it stops it.
pub struct Proxy {
    child: Child,
    pub socket: PathBuf,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Start the proxy for an application, its socket in `instance`. `None`:
/// no session bus or no xdg-dbus-proxy (the application then has no bus).
pub fn start(instance: &Path, rules: &[String]) -> Option<Proxy> {
    let address = session_bus()?;
    if !have("xdg-dbus-proxy") {
        return None;
    }
    let socket = instance.join("bus");
    let child = Command::new("xdg-dbus-proxy")
        .arg(address)
        .arg(&socket)
        .arg("--filter")
        .args(rules)
        .stdin(Stdio::null())
        .spawn()
        .ok()?;
    let proxy = Proxy { child, socket };
    let start = Instant::now();
    while !proxy.socket.exists() {
        if start.elapsed() > Duration::from_secs(5) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Some(proxy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy() {
        let r = rules("org.mozilla.firefox", false, false);
        assert!(r.contains(&"--own=org.mozilla.firefox".to_string()));
        assert!(!r.iter().any(|x| x.contains("Notification")));
        assert!(!r.iter().any(|x| x.contains("Camera")));
        assert!(!r.iter().any(|x| x.starts_with("--talk=org.gnome")));
        let r = rules("org.mozilla.firefox", true, true);
        assert!(r.contains(&"--talk=org.freedesktop.Notifications".to_string()));
        assert!(r.iter().any(|x| x.contains("portal.Camera")));
    }
}
