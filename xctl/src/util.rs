//! Small helpers shared by every command: the error type and running
//! external programs.

use std::fmt;
use std::process::{Command, Stdio};

/// The error type of xctl: just a message for the user.
///
/// In Rust, functions that can fail return `Result<T, E>`: either `Ok(value)`
/// or `Err(error)`. We use a single, simple error type everywhere, so that the
/// `?` operator can pass errors up to `main`, which prints them.
#[derive(Debug)]
pub struct Error(pub String);

/// Shorthand: `Result<T>` means `Result<T, Error>` in this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lets `?` turn I/O errors (file not found, permission denied, ...) into our
/// error type automatically.
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
/// Fails if the program cannot be started or exits with an error.
pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| err(format!("cannot run {program}: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(err(format!("{program} failed: {}", stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like `output`, but returns `None` instead of an error: for information
/// that is nice to show but not essential.
pub fn output_opt(program: &str, args: &[&str]) -> Option<String> {
    output(program, args).ok().map(|s| s.trim().to_string())
}

/// Run a program with its output going straight to the terminal (so the user
/// sees progress), failing if it exits with an error.
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

/// True when running as root (user id 0).
pub fn is_root() -> bool {
    // The standard library has no getuid(); /proc/self/status has the ids.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1).map(|uid| uid == "0"))
        })
        .unwrap_or(false)
}

/// Fail with a clear message unless running as root.
pub fn need_root(command: &str) -> Result<()> {
    if is_root() {
        Ok(())
    } else {
        Err(err(format!(
            "this command needs root (try: sudo xctl {command})"
        )))
    }
}
