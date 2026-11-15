//! What `xapp sync` found in ~/Applications: one small file per application,
//! ~/.local/state/xapps/apps/<app id>, with lines `key=value` (file,
//! version, name, signer, image).

use std::fs;
use std::path::PathBuf;

use xapp_core::paths;
use xapp_core::util::{Result, read_kv, write_atomic};
use xapp_format::Container;

/// One registered application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub app_id: String,
    pub file: PathBuf,
    pub version: String,
    pub name: String,
    pub signer: String,
    pub image: String,
}

fn dir() -> PathBuf {
    paths::state().join("apps")
}

/// Record an application.
pub fn put(c: &Container) -> Result<()> {
    let m = &c.manifest;
    let text = format!(
        "file={}\nversion={}\nname={}\nsigner={}\nimage={}\n",
        c.path.display(),
        m.version,
        m.display_name().replace('\n', " "),
        m.signer,
        m.image.sha256
    );
    xapp_core::util::write_if_changed(&dir().join(&m.app_id), text.as_bytes())?;
    Ok(())
}

/// Forget an application (its file is gone).
pub fn remove(app_id: &str) {
    let _ = fs::remove_file(dir().join(app_id));
}

/// One application, if registered.
pub fn get(app_id: &str) -> Option<Entry> {
    let kv = read_kv(&dir().join(app_id));
    let val = |k: &str| kv.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    Some(Entry {
        app_id: app_id.to_string(),
        file: PathBuf::from(val("file")?),
        version: val("version").unwrap_or_default(),
        name: val("name").unwrap_or_default(),
        signer: val("signer").unwrap_or_default(),
        image: val("image").unwrap_or_default(),
    })
}

/// All registered applications, by id.
pub fn all() -> Vec<Entry> {
    let mut ids: Vec<String> = fs::read_dir(dir())
        .map(|e| {
            e.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    ids.retain(|i| !i.starts_with('.'));
    ids.sort();
    ids.iter().filter_map(|i| get(i)).collect()
}

/// Remember the highest version ever used of an application (to notice a
/// downgrade), and read it back.
pub fn last_version(app_id: &str) -> Option<String> {
    fs::read_to_string(paths::state().join("versions").join(app_id))
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn set_last_version(app_id: &str, version: &str) -> Result<()> {
    write_atomic(
        &paths::state().join("versions").join(app_id),
        format!("{version}\n").as_bytes(),
    )
}

/// Forget the remembered version (`xapp remove --purge`).
pub fn forget(app_id: &str) {
    let _ = fs::remove_file(paths::state().join("versions").join(app_id));
}
