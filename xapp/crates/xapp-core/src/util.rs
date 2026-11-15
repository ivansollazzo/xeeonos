//! Small helpers shared by every command: the error type, running external
//! programs, hex text, writing files safely.

use std::fmt;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

/// The error type of xapp: just a message for the user (as in xctl).
#[derive(Debug)]
pub struct Error(pub String);

/// Shorthand: `Result<T>` means `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

/// Build an error from anything printable: `return Err(err("..."))`.
pub fn err(msg: impl Into<String>) -> Error {
    Error(msg.into())
}

/// Run a program and return its standard output as text.
pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| err(format!("cannot run {program}: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(err(format!("{program} failed: {}", stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like `output`, but `None` instead of an error.
pub fn output_opt(program: &str, args: &[&str]) -> Option<String> {
    output(program, args).ok().map(|s| s.trim().to_string())
}

/// Run a program with its output going to the terminal; fail if it fails.
pub fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| err(format!("cannot run {program}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(err(format!("{program} failed ({status})")))
    }
}

/// Is `program` in one of the directories of $PATH?
pub fn have(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

/// A field of /proc/self/status ("Uid", "Gid"): the first number.
fn status_id(field: &str) -> u32 {
    // The standard library has no getuid(); /proc/self/status has the ids.
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(&format!("{field}:")))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|n| n.parse().ok()))
        })
        .unwrap_or(65534)
}

/// The user id of this process.
pub fn uid() -> u32 {
    status_id("Uid")
}

/// The group id of this process.
pub fn gid() -> u32 {
    status_id("Gid")
}

/// Bytes as lowercase hexadecimal text.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hexadecimal text back to bytes (`None` if it is not hex).
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Write a file all at once: to a temporary name first, then renamed, so
/// nobody ever reads it half written. The parent directory is created.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.tmp-{}", std::process::id()));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Write a file only if its content changes; true if it was written.
pub fn write_if_changed(path: &Path, data: &[u8]) -> Result<bool> {
    if fs::read(path).ok().as_deref() == Some(data) {
        return Ok(false);
    }
    write_atomic(path, data)?;
    Ok(true)
}

/// Read `key=value` lines (comments with #, blank lines ignored).
pub fn read_kv(path: &Path) -> Vec<(String, String)> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Is `id` a valid application id: like a reversed domain name
/// (org.mozilla.firefox), letters, digits, `.`, `_`, `-`, at least two parts?
pub fn valid_app_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    id.len() <= 255
        && parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('-')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
        assert_eq!(unhex("000fff"), Some(vec![0, 15, 255]));
        assert_eq!(unhex("0g"), None);
        assert_eq!(unhex("abc"), None);
    }

    #[test]
    fn app_ids() {
        assert!(valid_app_id("org.mozilla.firefox"));
        assert!(valid_app_id("org.xeeonos.Hello-World"));
        assert!(!valid_app_id("firefox"));
        assert!(!valid_app_id("org..firefox"));
        assert!(!valid_app_id("org.fire/fox"));
        assert!(!valid_app_id("../evil.app"));
    }
}
