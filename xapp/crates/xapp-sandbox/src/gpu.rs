//! Graphics drivers lent by the system (manifest `gpu: host`, the default).
//!
//! An application's own minimal Arch may be old, and an old Mesa does not
//! know new graphics cards. So the system lends its own, always the newest:
//! Mesa (OpenGL, EGL, GBM, Vulkan drivers), with every library it needs,
//! glibc and libstdc++ included (Steam's runtime does the same). This works
//! because newer C libraries run programs built for older ones; the rule is
//! "the newest wins", which holds when the system's glibc and libstdc++ are
//! at least as new as the application's: checked first. Otherwise the
//! application keeps its own drivers (or draws without acceleration), with
//! a message.
//!
//! Each lent library either replaces the application's file of the same
//! name (bound over it) or, when the application has none, goes to
//! /run/host/gpu/lib (on LD_LIBRARY_PATH). Drivers and their description
//! files go under /run/host/gpu.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use xapp_core::util::{Result, err, output_opt};
use xapp_core::version;

/// What the sandbox needs for the lent drivers.
#[derive(Default, Debug)]
pub struct Setup {
    /// (path on the system, path in the sandbox), read-only.
    pub binds: Vec<(PathBuf, String)>,
    pub env: Vec<(String, String)>,
}

const LIB: &str = "/usr/lib";
const HOST_GPU: &str = "/run/host/gpu";

/// The C library's own pieces: always taken together, all from the same
/// side, when the system's glibc is lent.
const GLIBC: &[&str] = &[
    "ld-linux-x86-64.so.2",
    "libc.so.6",
    "libm.so.6",
    "libmvec.so.1",
    "libpthread.so.0",
    "libdl.so.2",
    "librt.so.1",
    "libresolv.so.2",
    "libutil.so.1",
    "libanl.so.1",
    "libBrokenLocale.so.1",
    "libc_malloc_debug.so.0",
    "libthread_db.so.1",
    "libnss_files.so.2",
    "libnss_dns.so.2",
    "libnss_compat.so.2",
    "libnss_hesiod.so.2",
    "libnss_db.so.2",
];

/// The libraries the drivers start from (their dependencies follow).
const SEEDS: &[&str] = &[
    "libEGL.so.",
    "libEGL_mesa.so.",
    "libGL.so.",
    "libGLX.so.",
    "libGLX_mesa.so.",
    "libGLdispatch.so.",
    "libOpenGL.so.",
    "libGLESv2.so.",
    "libgbm.so.",
    "libvulkan.so.",
    "libvulkan_",
    "libVkLayer_MESA",
    "libgallium",
];

/// "2.42" from the system's libc ("... stable release version 2.42.").
fn host_glibc() -> Option<String> {
    let text = output_opt("/usr/lib/libc.so.6", &[])?;
    let line = text.lines().next()?;
    let v = line.split("version ").nth(1)?;
    Some(v.trim_end_matches('.').trim().to_string())
}

/// The first two numbers of a version ("2.42+r9+g...-1" -> "2.42").
fn major_minor(v: &str) -> String {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}

/// The last number of the real file behind libstdc++.so.6
/// (libstdc++.so.6.0.34 -> 34), in a tree rooted at `root`.
fn libstdcxx_level(root: &Path) -> Option<u64> {
    let link = root.join("usr/lib/libstdc++.so.6");
    let mut name = link.file_name()?.to_string_lossy().into_owned();
    // Follow relative links inside the same directory (absolute ones would
    // point out of the image).
    for _ in 0..8 {
        match fs::read_link(root.join("usr/lib").join(&name)) {
            Ok(target) if target.is_relative() => {
                name = target.file_name()?.to_string_lossy().into_owned();
            }
            _ => break,
        }
    }
    name.rsplit('.').next()?.parse().ok()
}

/// The regular file that `usr/lib/<name>` is in the image, following
/// relative links inside usr/lib; `None` when there is none.
fn real_file_in(image: &Path, name: &str) -> Option<String> {
    let dir = image.join("usr/lib");
    let mut name = name.to_string();
    for _ in 0..8 {
        let meta = fs::symlink_metadata(dir.join(&name)).ok()?;
        if meta.is_file() {
            return Some(name);
        }
        if !meta.file_type().is_symlink() {
            return None;
        }
        let target = fs::read_link(dir.join(&name)).ok()?;
        if !target.is_relative() || target.components().count() != 1 {
            return None;
        }
        name = target.to_string_lossy().into_owned();
    }
    None
}

/// Can the system lend its drivers to this application? `Err` says why not.
pub fn compatible(image: &Path, app_glibc: &str) -> Result<()> {
    let host = host_glibc().ok_or_else(|| err("cannot read the system's glibc version"))?;
    if !app_glibc.is_empty()
        && version::compare(&major_minor(app_glibc), &major_minor(&host))
            == std::cmp::Ordering::Greater
    {
        return Err(err(format!(
            "the application's glibc ({app_glibc}) is newer than the system's ({host})"
        )));
    }
    if let (Some(app), Some(sys)) = (libstdcxx_level(image), libstdcxx_level(Path::new("/")))
        && app > sys
    {
        return Err(err(format!(
            "the application's libstdc++ (level {app}) is newer than the system's ({sys})"
        )));
    }
    Ok(())
}

/// The driver libraries of the system, and everything they load (ldd).
fn host_libraries() -> Vec<PathBuf> {
    let mut seeds: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(LIB) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // "libEGL.so." needs a version after .so (libEGL.so alone is a
            // development link); the others are drivers named *.so.
            let seed = SEEDS.iter().any(|s| {
                name.starts_with(s)
                    && (s.ends_with(".so.") || name.ends_with(".so") || name.contains(".so."))
            });
            if seed {
                seeds.push(e.path());
            }
        }
    }
    for dir in ["/usr/lib/dri", "/usr/lib/gbm"] {
        if let Ok(entries) = fs::read_dir(dir) {
            seeds.extend(
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "so")),
            );
        }
    }
    let mut all: BTreeMap<String, PathBuf> = BTreeMap::new();
    for s in &seeds {
        if s.parent() == Some(Path::new(LIB)) {
            all.insert(
                s.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                s.clone(),
            );
        }
    }
    let args: Vec<String> = seeds.iter().map(|p| p.display().to_string()).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if let Some(text) = output_opt("ldd", &args) {
        for line in text.lines() {
            // "\tlibdrm.so.2 => /usr/lib/libdrm.so.2 (0x...)" or
            // "\t/usr/lib64/ld-linux-x86-64.so.2 (0x...)". With several
            // files ldd also prints a header per file, not indented
            // ("/usr/lib/dri/apple_dri.so:"): not a library to lend.
            if !line.starts_with(char::is_whitespace) {
                continue;
            }
            let path = match line.split_once("=>") {
                Some((_, rest)) => rest.split_whitespace().next(),
                None => line
                    .split_whitespace()
                    .next()
                    .filter(|p| p.starts_with('/')),
            };
            let Some(path) = path.filter(|p| p.starts_with('/')) else {
                continue;
            };
            let path = PathBuf::from(path);
            if !path.exists() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            all.entry(name).or_insert(path);
        }
    }
    // The whole C library comes along.
    for g in GLIBC {
        let p = Path::new(LIB).join(g);
        if p.exists() {
            all.entry(g.to_string()).or_insert(p);
        }
    }
    all.into_values().collect()
}

/// A Vulkan driver description with its library named by file name only
/// (found on LD_LIBRARY_PATH or in /usr/lib of the sandbox).
fn rewrite_icd(text: &str) -> String {
    text.lines()
        .map(|l| {
            if let Some(start) = l.find("\"library_path\"") {
                let rest = &l[start..];
                if let (Some(a), Some(b)) = (rest.find(": \"").map(|i| i + 3), rest.rfind('"'))
                    && b > a
                {
                    let path = &rest[a..b];
                    let name = path.rsplit('/').next().unwrap_or(path);
                    return format!("{}{}{}", &l[..start + a], name, &rest[b..]);
                }
            }
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prepare the lent drivers for an application whose image is mounted at
/// `image`; generated files go to `instance`.
pub fn setup(image: &Path, instance: &Path) -> Result<Setup> {
    let mut s = Setup::default();
    let mut extra_lib = false;
    for lib in host_libraries() {
        let name = lib
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let real = fs::canonicalize(&lib).unwrap_or(lib.clone());
        // bwrap cannot mount on a link (libEGL.so.1 -> libEGL.so.1.1.0):
        // bind over the file the application's link leads to.
        if let Some(file) = real_file_in(image, &name) {
            s.binds.push((real, format!("{LIB}/{file}")));
        } else {
            s.binds.push((real, format!("{HOST_GPU}/lib/{name}")));
            extra_lib = true;
        }
    }
    if extra_lib {
        s.env
            .push(("LD_LIBRARY_PATH".into(), format!("{HOST_GPU}/lib")));
    }
    // Driver directories: over the application's own, or beside.
    for (dir, var) in [("dri", "LIBGL_DRIVERS_PATH"), ("gbm", "GBM_BACKENDS_PATH")] {
        let host = Path::new(LIB).join(dir);
        if host.is_dir() {
            let dest = if image.join("usr/lib").join(dir).is_dir() {
                format!("{LIB}/{dir}")
            } else {
                format!("{HOST_GPU}/{dir}")
            };
            s.binds.push((host, dest.clone()));
            s.env.push((var.into(), dest));
        }
    }
    let glvnd = Path::new("/usr/share/glvnd/egl_vendor.d");
    if glvnd.is_dir() {
        s.binds
            .push((glvnd.to_path_buf(), format!("{HOST_GPU}/egl_vendor.d")));
        s.env.push((
            "__EGL_VENDOR_LIBRARY_DIRS".into(),
            format!("{HOST_GPU}/egl_vendor.d"),
        ));
    }
    let drirc = Path::new("/usr/share/drirc.d");
    if drirc.is_dir() && image.join("usr/share/drirc.d").is_dir() {
        s.binds
            .push((drirc.to_path_buf(), "/usr/share/drirc.d".into()));
    }
    // Vulkan: the system's drivers, described with library names only.
    let icd_dir = instance.join("icd.d");
    let mut icds = Vec::new();
    if let Ok(entries) = fs::read_dir("/usr/share/vulkan/icd.d") {
        fs::create_dir_all(&icd_dir)?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Ok(text) = fs::read_to_string(e.path()) {
                fs::write(icd_dir.join(&name), rewrite_icd(&text))?;
                icds.push(format!("{HOST_GPU}/icd.d/{name}"));
            }
        }
    }
    if !icds.is_empty() {
        s.binds.push((icd_dir, format!("{HOST_GPU}/icd.d")));
        icds.sort();
        s.env.push(("VK_DRIVER_FILES".into(), icds.join(":")));
        s.env.push(("VK_ICD_FILENAMES".into(), icds.join(":")));
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_files() {
        let root = std::env::temp_dir().join(format!("xapp-gpu-test-{}", std::process::id()));
        let lib = root.join("usr/lib");
        fs::create_dir_all(&lib).unwrap();
        fs::write(lib.join("libEGL.so.1.1.0"), b"x").unwrap();
        std::os::unix::fs::symlink("libEGL.so.1.1.0", lib.join("libEGL.so.1")).unwrap();
        std::os::unix::fs::symlink("/usr/lib/x.so", lib.join("abs.so.1")).unwrap();
        assert_eq!(
            real_file_in(&root, "libEGL.so.1").as_deref(),
            Some("libEGL.so.1.1.0")
        );
        assert_eq!(
            real_file_in(&root, "libEGL.so.1.1.0").as_deref(),
            Some("libEGL.so.1.1.0")
        );
        assert_eq!(real_file_in(&root, "abs.so.1"), None);
        assert_eq!(real_file_in(&root, "missing.so.1"), None);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn versions() {
        assert_eq!(major_minor("2.42+r9+gabc-1"), "2.42");
        assert_eq!(major_minor("2.42"), "2.42");
    }

    #[test]
    fn icd() {
        let text = "{\n    \"ICD\": {\n        \"library_path\": \"/usr/lib/libvulkan_lvp.so\",\n        \"api_version\": \"1.4\"\n    }\n}";
        let out = rewrite_icd(text);
        assert!(out.contains("\"library_path\": \"libvulkan_lvp.so\","));
        assert!(out.contains("\"api_version\": \"1.4\""));
    }
}
