//! The default applications of the desktop, from Flathub: the lists in
//! preinstall.d (flatpak-preinstall(1)), installed in the background by the
//! extension flatpak (xeeonos-flatpak-preinstall) once there is a network.
//! The first-boot setup asks whether to install them; `xctl status` shows
//! how far they are.

use std::fs;
use std::path::Path;

/// Where the lists are: the desktop module's, and the administrator's.
const LISTS: &[&str] = &[
    "/usr/share/flatpak/preinstall.d",
    "/etc/flatpak/preinstall.d",
];
/// The setup was told not to install them.
const OFF: &str = "/var/lib/xeeonos/flatpak-preinstall-off";
/// The lists, as they are now, were installed (holds their checksum).
const DONE: &str = "/var/lib/xeeonos/flatpak-preinstall-done";
/// Applications of the system-wide Flatpak installation.
const INSTALLED: &str = "/var/lib/flatpak/app";

/// The applications to install, from the `[Flatpak Preinstall <name>]`
/// groups of a list (runtimes and `Install=false` groups left out).
fn parse(text: &str) -> Vec<String> {
    let mut apps = Vec::new();
    let mut current: Option<(String, bool)> = None;
    let mut flush = |c: &mut Option<(String, bool)>| {
        if let Some((name, keep)) = c.take()
            && keep
        {
            apps.push(name);
        }
    };
    for line in text.lines().map(str::trim) {
        if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(&mut current);
            current = group
                .strip_prefix("Flatpak Preinstall ")
                .map(|name| (name.trim().to_string(), true));
        } else if let Some((key, value)) = line.split_once('=')
            && let Some((_, keep)) = current.as_mut()
        {
            let value = value.trim().to_ascii_lowercase();
            match key.trim() {
                "IsRuntime" if value == "true" => *keep = false,
                "Install" if value == "false" => *keep = false,
                _ => {}
            }
        }
    }
    flush(&mut current);
    apps
}

/// All the default applications of this system (empty without a desktop
/// that has a list, or without the extension flatpak).
pub fn defaults() -> Vec<String> {
    let mut apps = Vec::new();
    for dir in LISTS {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "preinstall"))
            .collect();
        files.sort();
        for file in files {
            for app in parse(&fs::read_to_string(&file).unwrap_or_default()) {
                if !apps.contains(&app) {
                    apps.push(app);
                }
            }
        }
    }
    apps
}

/// Remember that the setup was told not to install them.
pub fn turn_off() -> std::io::Result<()> {
    if let Some(dir) = Path::new(OFF).parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(OFF, "")
}

/// One line for `xctl status`, if this system has default applications.
pub fn summary() -> Option<String> {
    let apps = defaults();
    if apps.is_empty() {
        return None;
    }
    if Path::new(OFF).exists() {
        return Some("not installed (chosen at the first start)".into());
    }
    let present = apps
        .iter()
        .filter(|a| Path::new(INSTALLED).join(a).exists())
        .count();
    let total = apps.len();
    Some(if Path::new(DONE).exists() {
        format!("installed ({present} of {total} present)")
    } else {
        format!("{present} of {total} installed, the rest as soon as there is a network")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists() {
        let text = "# comment\n[Flatpak Preinstall org.gnome.Loupe]\nBranch=stable\n\n\
                    [Flatpak Preinstall org.gnome.Platform]\nIsRuntime=true\n\
                    [Flatpak Preinstall org.example.Off]\nInstall=false\n\
                    [Something else]\n[Flatpak Preinstall org.mozilla.firefox]\n";
        assert_eq!(parse(text), ["org.gnome.Loupe", "org.mozilla.firefox"]);
    }
}
