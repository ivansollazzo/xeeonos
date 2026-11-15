//! The image of a .xapp, mounted read-only where the sandbox takes it from:
//! $XDG_RUNTIME_DIR/xapp/mnt/<image digest>. erofsfuse reads the erofs image
//! at the start of the file (the rest of the file is ignored by erofs), as
//! the user, without root. Two windows of the same application share the
//! mount; an updated file (new digest) gets a new one, so running instances
//! keep the old image until they end.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use xapp_core::paths;
use xapp_core::util::{Result, err};
use xapp_format::Container;

/// A mounted image. Dropping it unmounts, unless another instance still
/// uses it (then the unmount fails quietly and the last one does it).
pub struct Mount {
    pub dir: PathBuf,
}

/// Mount points of this process's view, from /proc/self/mountinfo (field 5;
/// spaces and the like are written as \040).
fn mounted(dir: &Path) -> bool {
    let want = dir.to_string_lossy();
    fs::read_to_string("/proc/self/mountinfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split(' ').nth(4))
        .any(|p| p.replace("\\040", " ") == want)
}

/// Mount the image of `c` (or use the mount already there).
pub fn mount(c: &Container) -> Result<Mount> {
    let dir = paths::runtime()
        .join("mnt")
        .join(&c.manifest.image.sha256[..24]);
    if !mounted(&dir) {
        fs::create_dir_all(&dir)?;
        let out = std::process::Command::new("erofsfuse")
            .arg(&c.path)
            .arg(&dir)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| err(format!("cannot run erofsfuse: {e}")))?;
        let start = Instant::now();
        while !mounted(&dir) {
            if start.elapsed() > Duration::from_secs(10) {
                return Err(err(format!(
                    "cannot mount the application's image: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    if !dir.join("usr").is_dir() {
        return Err(err("the application's image has no /usr"));
    }
    Ok(Mount { dir })
}

impl Drop for Mount {
    fn drop(&mut self) {
        let ok = std::process::Command::new("fusermount3")
            .args(["-u", "-q"])
            .arg(&self.dir)
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            let _ = fs::remove_dir(&self.dir);
        }
    }
}
