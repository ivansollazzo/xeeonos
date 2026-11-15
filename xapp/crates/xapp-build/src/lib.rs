//! `xapp build`: making a .xapp from a recipe.
//!
//! A recipe is a directory with `recipe.json`:
//!
//! ```json
//! {
//!   "app_id": "org.mozilla.firefox",
//!   "name": "Firefox",
//!   "names": {"it": "Firefox"},
//!   "description": "Web browser",
//!   "snapshot": "2026/11/13",
//!   "packages": ["firefox"],
//!   "exec": ["/usr/lib/firefox/firefox"],
//!   "gpu": "host",
//!   "permissions": [{"id": "network", "reason": "To browse the web"}],
//!   "mime_types": ["text/html"],
//!   "categories": ["Network", "WebBrowser"],
//!   "wm_class": "firefox",
//!   "icon": "/usr/share/icons/hicolor/128x128/apps/firefox.png",
//!   "remove": ["usr/lib/firefox/updater"],
//!   "script": "build.sh"
//! }
//! ```
//!
//! `version` may be left out: it is then the version of the first package.
//! The packages are installed with pacman, from the Arch Linux Archive
//! snapshot of the recipe, in an Arch Linux container (podman or docker);
//! the result becomes the erofs image of the .xapp, signed with the
//! author's key: `--key <file>`, or the key text in $XAPP_SIGNING_KEY, or
//! ~/.config/xapp/signing.key (made the first time).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use xapp_core::paths;
use xapp_core::util::{Result, err, gid, have, read_kv, run as run_program, uid};
use xapp_format::container::{self, hash_bytes, hash_file_prefix};
use xapp_format::manifest::Digest;
use xapp_format::{FORMAT, Gpu, HOST_CONTRACT, Manifest, Permission};
use xapp_trust::keys::{self, Secret};

const ROOT_SCRIPT: &str = include_str!("build-root.sh");

/// Options of `xapp build`.
pub struct Options {
    pub recipe: PathBuf,
    pub key: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub work: Option<PathBuf>,
    /// A root directory already made (by other tools): packed as it is,
    /// without the container. Its packages are the recipe's business.
    pub root: Option<PathBuf>,
}

/// What the container build needs.
struct ContainerBuild<'a> {
    work: &'a Path,
    recipe_dir: &'a Path,
    icon: &'a str,
    script: &'a str,
    remove: &'a [String],
    snapshot: &'a str,
    packages: &'a [String],
}

/// Make the image in an Arch Linux container (build-root.sh): work/out/.
fn image_in_container(b: &ContainerBuild, app_id: &str) -> Result<()> {
    fs::write(b.work.join("build-root.sh"), ROOT_SCRIPT)?;
    let engine = engine()?;
    let image_name = std::env::var("XAPP_BUILD_IMAGE")
        .unwrap_or_else(|_| "docker.io/library/archlinux:base".into());
    let mut cmd = Command::new(&engine);
    cmd.args(["run", "--rm", "--privileged"])
        .arg("--volume")
        .arg(format!("{}:/work", b.work.display()))
        .arg("--volume")
        .arg(format!("{}:/recipe:ro", b.recipe_dir.display()))
        .args(["--env", &format!("XAPP_ICON={}", b.icon)])
        .args(["--env", &format!("XAPP_SCRIPT={}", b.script)])
        .args(["--env", &format!("XAPP_REMOVE={}", b.remove.join("\n"))]);
    if engine.ends_with("docker") {
        // Docker's root is the real root: give the results back to the user.
        cmd.args(["--env", &format!("XAPP_CHOWN={}:{}", uid(), gid())]);
    }
    cmd.arg(&image_name)
        .args(["/bin/bash", "/work/build-root.sh", b.snapshot])
        .args(b.packages);
    println!(
        "Building {app_id} from Arch snapshot {} ({engine})...",
        b.snapshot
    );
    let status = cmd
        .status()
        .map_err(|e| err(format!("cannot run {engine}: {e}")))?;
    if !status.success() {
        return Err(err(format!("the build in the container failed ({status})")));
    }
    Ok(())
}

/// Pack a ready root directory (`--root`): work/out/.
fn image_from_root(root: &Path, recipe_dir: &Path, icon: &str, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let image = out.join("image.erofs");
    let _ = fs::remove_file(&image);
    run_program(
        "mkfs.erofs",
        &[
            "--quiet",
            "-zlz4hc",
            "-T0",
            &image.to_string_lossy(),
            &root.to_string_lossy(),
        ],
    )?;
    let src = match icon.strip_prefix('/') {
        Some(inside) => root.join(inside),
        None => recipe_dir.join(icon),
    };
    let ext = icon.rsplit('.').next().unwrap_or("png");
    fs::copy(&src, out.join(format!("icon.{ext}")))
        .map_err(|e| err(format!("cannot read the icon {}: {e}", src.display())))?;
    let _ = fs::remove_file(out.join("info"));
    Ok(())
}

fn text(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn list(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The author's key, from the options, the environment or the user's own.
fn signing_key(opt: &Option<PathBuf>) -> Result<Secret> {
    if let Some(p) = opt {
        if !p.exists() {
            let k = keys::generate(p)?;
            println!(
                "New signing key: {} (public key {})",
                p.display(),
                k.public()
            );
            return Ok(k);
        }
        return keys::load(p);
    }
    if let Ok(text) = std::env::var("XAPP_SIGNING_KEY")
        && !text.trim().is_empty()
    {
        return keys::parse_secret(&text)
            .ok_or_else(|| err("XAPP_SIGNING_KEY is not an xapp secret key"));
    }
    let own = paths::config_home().join("xapp/signing.key");
    if own.exists() {
        return keys::load(&own);
    }
    let k = keys::generate(&own)?;
    println!(
        "New signing key: {} (keep it safe: updates of your applications need the same key)",
        own.display()
    );
    println!("Public key: {}", k.public());
    Ok(k)
}

/// The container program: $XAPP_CONTAINER, else podman, else docker.
fn engine() -> Result<String> {
    if let Ok(e) = std::env::var("XAPP_CONTAINER") {
        return Ok(e);
    }
    ["podman", "docker"]
        .into_iter()
        .find(|e| have(e))
        .map(String::from)
        .ok_or_else(|| {
            err("xapp build needs podman or docker (sudo xctl extensions enable podman)")
        })
}

/// "1:146.0.1-2" -> "146.0.1" (no epoch, no package release).
fn package_version(v: &str) -> String {
    let v = v.split_once(':').map_or(v, |(_, rest)| rest);
    v.rsplit_once('-').map_or(v, |(ver, _)| ver).to_string()
}

/// Build the .xapp; returns its path.
pub fn build(o: &Options) -> Result<PathBuf> {
    let recipe_path = o.recipe.join("recipe.json");
    let r: Value = serde_json::from_slice(
        &fs::read(&recipe_path)
            .map_err(|e| err(format!("cannot read {}: {e}", recipe_path.display())))?,
    )
    .map_err(|e| err(format!("{} is not valid JSON: {e}", recipe_path.display())))?;
    let snapshot = text(&r, "snapshot");
    if xapp_core::date::parse(&snapshot).is_none() {
        return Err(err("recipe: snapshot must be a date, YYYY/MM/DD"));
    }
    let app_id = text(&r, "app_id");
    if !xapp_core::util::valid_app_id(&app_id) {
        return Err(err(format!(
            "recipe: \"{app_id}\" is not a valid application id"
        )));
    }
    let packages = list(&r, "packages");
    let icon = text(&r, "icon");
    let icon_type = icon.rsplit('.').next().unwrap_or_default().to_lowercase();
    if !matches!(icon_type.as_str(), "png" | "svg") {
        return Err(err("recipe: icon must be a .png or .svg file"));
    }
    for p in r
        .get("permissions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = text(p, "id");
        if xapp_permissions::info(&id).is_none() {
            return Err(err(format!(
                "recipe: unknown permission \"{id}\" (known: {})",
                xapp_permissions::known()
            )));
        }
    }
    let key = signing_key(&o.key)?;

    // 1. The image, made in a container.
    let work = o
        .work
        .clone()
        .unwrap_or_else(|| paths::cache_home().join("xapp/build").join(&app_id));
    fs::create_dir_all(&work)?;
    let work = fs::canonicalize(&work)?;
    let recipe_dir = fs::canonicalize(&o.recipe)?;
    let out = work.join("out");
    match &o.root {
        Some(root) => image_from_root(root, &recipe_dir, &icon, &out)?,
        None => {
            let c = ContainerBuild {
                work: &work,
                recipe_dir: &recipe_dir,
                icon: &icon,
                script: &text(&r, "script"),
                remove: &list(&r, "remove"),
                snapshot: &snapshot,
                packages: &packages,
            };
            image_in_container(&c, &app_id)?;
        }
    }
    let image = out.join("image.erofs");
    let info = read_kv(&out.join("info"));
    let info = |k: &str| {
        info.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| text(&r, k))
    };

    // 2. The manifest.
    container::stamp_image(&image)?;
    let image_size = fs::metadata(&image)?.len();
    let icon_bytes = fs::read(out.join(format!("icon.{icon_type}")))?;
    let version = match text(&r, "version") {
        v if !v.is_empty() => v,
        _ => package_version(&info("main")),
    };
    let names = r
        .get("names")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let permissions = r
        .get("permissions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| Permission {
                    id: text(p, "id"),
                    reason: text(p, "reason"),
                })
                .collect()
        })
        .unwrap_or_default();
    let m = Manifest {
        format: FORMAT,
        app_id,
        name: text(&r, "name"),
        names,
        description: text(&r, "description"),
        version,
        exec: list(&r, "exec"),
        arch: "x86_64".into(),
        snapshot: snapshot.replace('/', "-"),
        host_contract: HOST_CONTRACT,
        gpu: if text(&r, "gpu") == "bundled" {
            Gpu::Bundled
        } else {
            Gpu::Host
        },
        glibc: info("glibc"),
        libstdcxx: info("libstdcxx"),
        permissions,
        mime_types: list(&r, "mime_types"),
        categories: list(&r, "categories"),
        wm_class: text(&r, "wm_class"),
        icon_type: icon_type.clone(),
        icon: Digest {
            size: icon_bytes.len() as u64,
            sha256: hash_bytes(&icon_bytes),
        },
        image: Digest {
            size: image_size,
            sha256: hash_file_prefix(&image, image_size)?,
        },
        signer: key.public(),
    };
    m.check()?;
    let manifest_bytes = m.to_json();
    let signature = key.sign(&manifest_bytes);

    // 3. The file.
    let output = o
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{}.xapp", file_name(&m.name))));
    container::write(&output, &image, &manifest_bytes, &signature, &icon_bytes)?;
    let _ = fs::remove_file(&image);
    println!(
        "Built {} {}: {} ({} MB), signed by {}",
        m.app_id,
        m.version,
        output.display(),
        fs::metadata(&output)?.len() / 1_000_000,
        keys::fingerprint(&m.signer)
    );
    Ok(output)
}

/// A file name from an application name ("Text Editor" stays, "/" goes).
pub fn file_name(name: &str) -> String {
    let n: String = name
        .chars()
        .map(|c| if c == '/' || c.is_control() { '-' } else { c })
        .collect();
    let n = n.trim().trim_start_matches('.').to_string();
    if n.is_empty() {
        "application".into()
    } else {
        n
    }
}

/// Where `xapp install` and `xapp build` put a file: ~/Applications/<name>.
pub fn install_path(source: &Path, name: &str) -> PathBuf {
    let base = source
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .filter(|f| f.to_lowercase().ends_with(".xapp"))
        .unwrap_or_else(|| format!("{}.xapp", file_name(name)));
    paths::applications().join(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_names() {
        assert_eq!(package_version("146.0.1-2"), "146.0.1");
        assert_eq!(package_version("1:2.0-1"), "2.0");
        assert_eq!(file_name("Text Editor"), "Text Editor");
        assert_eq!(file_name("a/b"), "a-b");
        assert_eq!(file_name(".."), "application");
    }
}
