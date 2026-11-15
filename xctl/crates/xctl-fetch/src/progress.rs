//! A progress line for large downloads, when someone is watching:
//!
//!   xeeonos_26.11.3.usr-x86-64….raw.zst   812.4 / 1674.0 MB  48%  9.8 MB/s  1:28 left
//!
//! curl downloads into the file as usual; meanwhile xctl looks at how big
//! the file has grown, a few times per second. Without a terminal (services,
//! logs) nothing changes: no progress line, only the usual messages.

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use xctl_core::util::{Result, err};

const MB: f64 = 1024.0 * 1024.0;

/// Is someone watching (standard error is a terminal)?
pub fn watching() -> bool {
    std::io::stderr().is_terminal()
}

/// Download `url` into `to` with curl (`args`: its options, without the
/// output and the URL), drawing a progress line for `label`; `total` is the
/// size, if known.
pub fn curl_with_progress(
    args: &[&str],
    url: &str,
    to: &Path,
    label: &str,
    total: Option<u64>,
) -> Result<()> {
    let to_s = to.to_str().ok_or_else(|| err("non-UTF-8 path"))?;
    let mut child = Command::new("curl")
        .args(args)
        .args(["--output", to_s, url])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| err(format!("cannot run curl: {e}")))?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| err(format!("curl: {e}")))? {
            break status;
        }
        let done = fs::metadata(to).map(|m| m.len()).unwrap_or(0);
        draw(label, done, total, start.elapsed());
        std::thread::sleep(Duration::from_millis(250));
    };
    let done = fs::metadata(to).map(|m| m.len()).unwrap_or(0);
    if status.success() {
        finish(label, done, start.elapsed());
        Ok(())
    } else {
        clear();
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        Err(err(format!("curl failed: {}", stderr.trim())))
    }
}

/// One progress line, drawn over the previous one.
fn draw(label: &str, done: u64, total: Option<u64>, elapsed: Duration) {
    eprint!("\r\x1b[2K{}", line(label, done, total, elapsed));
    let _ = std::io::stderr().flush();
}

/// The last line: how much, in how long.
fn finish(label: &str, done: u64, elapsed: Duration) {
    eprintln!(
        "\r\x1b[2K  {:<44}  {:>7.1} MB  done in {}",
        short(label),
        done as f64 / MB,
        clock(elapsed.as_secs())
    );
}

fn clear() {
    eprint!("\r\x1b[2K");
}

/// The text of a progress line (without the terminal codes).
pub fn line(label: &str, done: u64, total: Option<u64>, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64().max(0.001);
    let speed = done as f64 / secs; // bytes per second
    let mut text = format!("  {:<44}  {:>7.1}", short(label), done as f64 / MB);
    match total {
        Some(total) if total > 0 => {
            let pct = (done.min(total) * 100 / total) as u32;
            text.push_str(&format!(" / {:.1} MB  {pct:>3}%", total as f64 / MB));
            text.push_str(&format!("  {:>5.1} MB/s", speed / MB));
            if speed > 0.0 && done > 0 {
                let left = (total.saturating_sub(done)) as f64 / speed;
                text.push_str(&format!("  {} left", clock(left as u64)));
            }
        }
        _ => text.push_str(&format!(" MB  {:>5.1} MB/s", speed / MB)),
    }
    text
}

/// A file name that fits the line: long ones keep their start and end.
fn short(label: &str) -> String {
    let chars: Vec<char> = label.chars().collect();
    if chars.len() <= 44 {
        return label.to_string();
    }
    let head: String = chars[..30].iter().collect();
    let tail: String = chars[chars.len() - 13..].iter().collect();
    format!("{head}…{tail}")
}

/// Seconds as m:ss (or h:mm:ss).
fn clock(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        let mb = 1024 * 1024;
        let l = line(
            "xeeonos_26.11.3.efi",
            50 * mb,
            Some(100 * mb),
            Duration::from_secs(10),
        );
        assert!(l.contains("50.0 / 100.0 MB"), "{l}");
        assert!(l.contains(" 50%"), "{l}");
        assert!(l.contains("5.0 MB/s"), "{l}");
        assert!(l.contains("0:10 left"), "{l}");
        // Size not known: no percentage.
        let l = line("SHA256SUMS", 2 * mb, None, Duration::from_secs(1));
        assert!(l.contains("2.0 MB") && !l.contains('%'), "{l}");
        assert_eq!(clock(3725), "1:02:05");
        assert_eq!(clock(65), "1:05");
        let long = "xeeonos_26.11-testing.200.usr-x86-64.0123456789abcdef0123456789abcdef.raw.zst";
        let s = short(long);
        assert_eq!(s.chars().count(), 44);
        assert!(s.starts_with("xeeonos_26.11-testing.200.usr") && s.ends_with("bcdef.raw.zst"));
    }
}
