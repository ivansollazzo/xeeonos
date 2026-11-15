//! The file type of .xapp files (application/x-xapp, recognized by the name
//! and by the magic at the start of the file) and who opens them
//! (xapp-run.desktop: `xapp run <file>`).
//!
//! The extension ships the description in /usr/share/xapp; it is put into
//! the user's own file type database (~/.local/share/mime), because the
//! system's database inside the sealed /usr cannot be rebuilt.

use std::fs;
use std::path::Path;

use xapp_core::paths;
use xapp_core::util::{Result, have, run, write_if_changed};

pub const TYPE: &str = "application/x-xapp";
pub const HANDLER: &str = "xapp-run.desktop";

/// Make sure the user's database knows .xapp files, and that a double
/// click runs them. True if something changed.
pub fn setup() -> Result<bool> {
    let mut changed = false;
    if let Ok(xml) = fs::read(paths::MIME_XML) {
        let mime = paths::data_home().join("mime");
        if write_if_changed(&mime.join("packages/xeeonos-xapp.xml"), &xml)?
            || !mime.join("mime.cache").exists()
        {
            changed = true;
            if have("update-mime-database") {
                let _ = run("update-mime-database", &[&mime.to_string_lossy()]);
            }
        }
    }
    changed |= set_default(&paths::config_home().join("mimeapps.list"))?;
    Ok(changed)
}

/// Add `application/x-xapp=xapp-run.desktop` to [Default Applications],
/// unless the user already chose something for it.
fn set_default(path: &Path) -> Result<bool> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let (out, changed) = with_default(&text);
    if changed {
        write_if_changed(path, out.as_bytes())?;
    }
    Ok(changed)
}

fn with_default(text: &str) -> (String, bool) {
    let line = format!("{TYPE}={HANDLER};");
    let mut out = Vec::new();
    let mut in_defaults = false;
    let mut found_group = false;
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if in_defaults {
                out.push(line.clone());
                in_defaults = false;
            }
            if t == "[Default Applications]" {
                in_defaults = true;
                found_group = true;
            }
        } else if in_defaults && t.starts_with(&format!("{TYPE}=")) {
            return (text.to_string(), false);
        }
        out.push(l.to_string());
    }
    if in_defaults {
        out.push(line.clone());
    }
    if !found_group {
        if !out.is_empty() {
            out.push(String::new());
        }
        out.push("[Default Applications]".into());
        out.push(line);
    }
    (out.join("\n") + "\n", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let (t, c) = with_default("");
        assert!(c);
        assert_eq!(
            t,
            "[Default Applications]\napplication/x-xapp=xapp-run.desktop;\n"
        );
        let (t, c) = with_default(
            "[Default Applications]\ntext/plain=gedit.desktop\n[Added Associations]\nx=y\n",
        );
        assert!(c);
        assert!(
            t.contains("text/plain=gedit.desktop\napplication/x-xapp=xapp-run.desktop;\n[Added")
        );
        let (_, c) = with_default("[Default Applications]\napplication/x-xapp=other.desktop\n");
        assert!(!c);
    }
}
