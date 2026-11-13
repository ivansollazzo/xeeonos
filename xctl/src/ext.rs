//! `xctl extensions`: system extensions.
//!
//! An extension adds software to /usr without changing the sealed system
//! image: it is a small read-only image of its own, laid over /usr at boot
//! (systemd-sysext). Its file, <name>.xext, is a systemd extension image
//! (erofs) with a XeeonOS manifest inside. Each one is built together with a XeeonOS version, from
//! the same Arch snapshot, and only fits that exact version.
//!
//! Two kinds (see docs/extensions.md):
//!
//!   included  its image is part of the system image itself
//!             (/usr/lib/xeeonos/extensions/<name>.xext): always there, sealed
//!             and updated with the system. It can only be turned off and on.
//!   optional  built for every XeeonOS version by the repository
//!             xeeonos-extensions and published as
//!             ghcr.io/ivansollazzo/xeeonos-extensions:<version>
//!             (xeeonos_<version>.<name>.xext and its manifest <name>.conf,
//!             with a manifest signed by the update key). `xctl extensions enable`
//!             downloads both into /var/lib/xeeonos/extensions/<version>/,
//!             and `xctl update` fetches the new version's copy of every
//!             enabled one (and waits while one is not published yet).
//!
//!   from a file  an extension image made by someone else, installed with
//!             `xctl extensions install <file>`: not signed, not checked by XeeonOS
//!             (a warning says so), kept in /var/lib/xeeonos/extensions-local.
//!
//! What every extension is (description, Special, Default, services, hooks)
//! is written in its manifest, <name>.conf: a copy of its extension.conf.
//!
//! Hooks: what an extension does to the system besides adding files.
//!
//!   Enable=   units started while it is on (and at every boot)
//!   Disable=  units of the system stopped and masked while it is on (the
//!             mask is only for this boot: turned off, the extension leaves
//!             nothing behind)
//!   Setup=    a script run once, at the first boot with the extension on
//!   Cleanup=  a script run when it is turned off or removed (only if its
//!             Setup ran), to undo what Setup did
//!   Boot=     a script run at every boot with the extension on
//!
//! The scripts are in the extension image, in
//! /usr/lib/xeeonos/extensions/<name>/, and run as root. The
//! system image holds the manifests of the included ones
//! (/usr/lib/xeeonos/extensions), a downloaded one keeps its own next to its
//! image, and every extension image holds its own.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::update::{self, Source};
use crate::util::{Result, err, need_root, output, run as run_program};
use crate::version::read_os_release;

/// One extension, as described by its `.conf` file.
#[derive(Debug, Default, PartialEq)]
pub struct Extension {
    pub name: String,
    pub description: String,
    /// Turning it off takes away something most of the system relies on:
    /// warn and ask first.
    pub special: bool,
    /// On unless the administrator turned it off.
    pub default: bool,
    /// Units to start once the extension is in place (Enable=).
    pub units: Vec<String>,
    /// Units of the system to stop and mask while it is on (Disable=).
    pub masks: Vec<String>,
    /// Scripts (in /usr/lib/xeeonos/extensions/<name>/): once, at turning
    /// off, at every boot.
    pub setup: Option<String>,
    pub cleanup: Option<String>,
    pub boot: Option<String>,
    /// Installed from a file by the administrator (`xctl extensions install`).
    pub local: bool,
}

impl Extension {
    /// Its image inside the system image, if it is an included one.
    fn included_image(&self) -> Option<PathBuf> {
        let image = Path::new(config::EXTENSIONS).join(format!("{}.xext", self.name));
        image.exists().then_some(image)
    }

    /// Where the downloaded image for `version` is kept.
    fn stored_image(&self, version: &str) -> PathBuf {
        Path::new(config::EXTENSIONS_STORE)
            .join(version)
            .join(format!("{}.xext", self.name))
    }

    /// The image to use on `version`, if there is one on this computer.
    fn image(&self, version: &str) -> Option<PathBuf> {
        if self.local {
            let image = Path::new(config::EXTENSIONS_LOCAL).join(format!("{}.xext", self.name));
            return image.exists().then_some(image);
        }
        self.included_image().or_else(|| {
            let stored = self.stored_image(version);
            stored.exists().then_some(stored)
        })
    }
}

/// Parse an extension.conf: `Key=value` lines, the value optionally in
/// double quotes (the build script reads the same file as shell variables).
fn parse(name: &str, text: &str) -> Extension {
    let mut ext = Extension {
        name: name.to_string(),
        ..Default::default()
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "Description" => ext.description = value.to_string(),
            "Special" => ext.special = value == "yes",
            "Default" => ext.default = value == "yes",
            "Enable" => ext.units = words(value),
            "Disable" => ext.masks = words(value),
            "Setup" => ext.setup = script(value),
            "Cleanup" => ext.cleanup = script(value),
            "Boot" => ext.boot = script(value),
            _ => {}
        }
    }
    ext
}

fn words(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_string).collect()
}

/// A hook script's name: a plain file name in the extension's directory
/// (anything else is ignored).
fn script(value: &str) -> Option<String> {
    update::is_plain_file_name(value).then(|| value.to_string())
}

/// The manifests (<name>.conf) in a directory.
fn manifests(dir: &Path) -> Vec<Extension> {
    let mut list = Vec::new();
    if let Ok(dir) = fs::read_dir(dir) {
        for entry in dir.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if let Some(name) = file.strip_suffix(".conf")
                && let Ok(text) = fs::read_to_string(entry.path())
            {
                list.push(parse(name, &text));
            }
        }
    }
    list
}

/// The extensions of this version of XeeonOS, and those installed from a
/// file, by name.
pub fn known() -> Vec<Extension> {
    let mut list = manifests(Path::new(config::EXTENSIONS_LOCAL));
    for ext in &mut list {
        ext.local = true;
    }
    let mut more = manifests(Path::new(config::EXTENSIONS));
    // The optional ones downloaded for this version.
    if let Ok(version) = running_version() {
        more.extend(manifests(
            &Path::new(config::EXTENSIONS_STORE).join(version),
        ));
    }
    for ext in more {
        // (An active extension from a file shows its own manifest there too.)
        if !list.iter().any(|l| l.name == ext.name) {
            list.push(ext);
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

fn find(name: &str) -> Result<Extension> {
    known().into_iter().find(|e| e.name == name).ok_or_else(|| {
        err(format!(
            "unknown extension '{name}' (see xctl extensions list)"
        ))
    })
}

/// The administrator's choices: `<name>=on|off` lines in
/// /etc/xeeonos/extensions. An extension not listed follows its Default.
fn choices() -> BTreeMap<String, bool> {
    parse_choices(&fs::read_to_string(config::EXTENSIONS_CHOICES).unwrap_or_default())
}

fn parse_choices(text: &str) -> BTreeMap<String, bool> {
    text.lines()
        .filter_map(|l| l.trim().split_once('='))
        .map(|(name, state)| (name.trim().to_string(), state.trim() == "on"))
        .collect()
}

fn set_choice(name: &str, on: bool) -> Result<()> {
    let mut all = choices();
    all.insert(name.to_string(), on);
    let text: String = all
        .iter()
        .map(|(n, on)| format!("{n}={}\n", if *on { "on" } else { "off" }))
        .collect();
    let file = Path::new(config::EXTENSIONS_CHOICES);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(file, text)?;
    Ok(())
}

fn is_on(ext: &Extension, choices: &BTreeMap<String, bool>) -> bool {
    choices.get(&ext.name).copied().unwrap_or(ext.default)
}

/// The names of the optional (downloaded) extensions that are turned on:
/// `xctl update` fetches their copy for the new version.
pub fn enabled_optional() -> Vec<String> {
    let choices = choices();
    known()
        .into_iter()
        .filter(|e| is_on(e, &choices) && !e.local && e.included_image().is_none())
        .map(|e| e.name)
        .collect()
}

/// The name of an extension's file in an update: xeeonos_<v>.<name>.xext
pub fn update_file(version: &str, name: &str) -> String {
    format!("xeeonos_{version}.{name}.xext")
}

/// The extension a file of an update belongs to, if it is one.
pub fn extension_of(file: &str) -> Option<&str> {
    let (_, name) = file.strip_suffix(".xext")?.rsplit_once('.')?;
    Some(name)
}

fn running_version() -> Result<String> {
    let os = read_os_release(config::OS_RELEASE)?;
    os.get("IMAGE_VERSION")
        .cloned()
        .ok_or_else(|| err("this system has no version (IMAGE_VERSION)"))
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None => list(&[]),
        Some("list") => list(&args[1..]),
        Some("enable") => enable(&args[1..]),
        Some("disable") => disable(&args[1..]),
        Some("install") => install(&args[1..]),
        Some("remove") => remove(&args[1..]),
        Some("activate") => activate(),
        Some(other) => Err(err(format!(
            "unknown extensions command '{other}' (expected list, enable <name>, disable <name>, \
             install <file> or remove <name>)"
        ))),
    }
}

/// `xctl extensions list [--from <source>] [--keyring <file>] [--offline]`:
/// the installed extensions (those of this version, the downloaded ones and
/// those from a file), then those that can be downloaded for this version:
/// their manifests, published with the signed manifest of the extensions'
/// registry.
fn list(args: &[String]) -> Result<()> {
    let (from, keyrings, offline) = source_options(args, &["--offline"])?;
    let version = running_version()?;
    let all = known();
    let choices = choices();
    let active = active_now();
    let mut rows = Vec::new();
    for ext in &all {
        let on = is_on(ext, &choices);
        let running = active.contains(&ext.name);
        let state = match (on, running) {
            (true, true) => "on",
            (false, false) => "off",
            (true, false) if ext.image(&version).is_none() => "on (not downloaded)",
            (true, false) => "on (from the next restart)",
            (false, true) => "off (from the next restart)",
        };
        let kind = match (ext.local, ext.included_image().is_some(), ext.special) {
            (true, _, _) => "installed from a file, not verified",
            (false, true, true) => "included, special",
            (false, true, false) => "included",
            (false, false, true) => "optional, special",
            (false, false, false) => "optional",
        };
        rows.push((
            ext.name.clone(),
            state.to_string(),
            format!("{} ({kind})", ext.description),
        ));
    }

    // What can be downloaded: not when offline, or asked not to look.
    let mut more = Vec::new();
    let mut note = None;
    if offline {
        note = Some("not checked (--offline)".to_string());
    } else {
        let source = Source::parse(&from.unwrap_or_else(|| default_source(&version)));
        if !matches!(source, Source::Dir(_)) && !network_up() {
            note = Some("not checked: no network".to_string());
        } else {
            match published(&source, &version, &keyrings) {
                Ok(list) => {
                    more = list
                        .into_iter()
                        .filter(|e| !all.iter().any(|a| a.name == e.name))
                        .collect()
                }
                Err(e) => note = Some(format!("could not be checked ({e})")),
            }
        }
    }
    for ext in &more {
        let kind = if ext.special {
            "optional, special"
        } else {
            "optional"
        };
        rows.push((
            ext.name.clone(),
            String::new(),
            format!("{} ({kind})", ext.description),
        ));
    }
    let width = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
    let row = |(name, state, what): &(String, String, String)| {
        println!("  {name:<width$}  {state:<27}  {what}");
    };

    println!("Installed:");
    if all.is_empty() {
        println!("  none");
    }
    rows.iter().take(all.len()).for_each(row);
    println!("Can be downloaded for XeeonOS {version}:");
    match (&note, more.is_empty()) {
        (Some(note), _) => println!("  {note}"),
        (None, true) => println!("  none"),
        (None, false) => rows.iter().skip(all.len()).for_each(row),
    }
    println!("Turn on or off with: sudo xctl extensions enable|disable <name>");
    Ok(())
}

/// Is there a network connection (asked once, without waiting)?
fn network_up() -> bool {
    std::process::Command::new("nm-online")
        .args(["--quiet", "--timeout=3"])
        .status()
        .is_ok_and(|s| s.success())
}

/// The options shared by list and enable: --from <source>,
/// --keyring <file>, and the flags in `flags` (true if one was given).
fn source_options(args: &[String], flags: &[&str]) -> Result<(Option<String>, Vec<PathBuf>, bool)> {
    let mut from = None;
    let mut keyrings = vec![PathBuf::from(config::KEYRING)];
    let mut flag = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--from" => {
                from = Some(
                    it.next()
                        .ok_or_else(|| err("--from needs a source"))?
                        .clone(),
                )
            }
            "--keyring" => keyrings.push(PathBuf::from(
                it.next().ok_or_else(|| err("--keyring needs a file"))?,
            )),
            other if flags.contains(&other) => flag = true,
            other => return Err(err(format!("unknown option: {other}"))),
        }
    }
    Ok((from, keyrings, flag))
}

/// The manifests of the optional extensions published for `version` at
/// `source`: every <name>.conf listed in its signed manifest next to the
/// image xeeonos_<version>.<name>.xext, checked against it.
fn published(source: &Source, version: &str, keyrings: &[PathBuf]) -> Result<Vec<Extension>> {
    let dir = std::env::temp_dir().join(format!("xctl-ext-list-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let _cleanup = update::Cleanup(&dir);
    update::fetch_manifest(source, &dir, keyrings)?;
    let names = update::manifest_files(&fs::read_to_string(dir.join("SHA256SUMS"))?)?;
    let confs: Vec<String> = names
        .iter()
        .filter_map(|n| n.strip_suffix(".conf"))
        .filter(|name| names.contains(&update_file(version, name)))
        .map(|name| format!("{name}.conf"))
        .collect();
    if confs.is_empty() {
        return Ok(Vec::new());
    }
    update::fetch_files(source, &dir, &confs)?;
    update::check_hashes(&dir)?;
    let mut list = manifests(&dir);
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// The extensions laid over /usr right now (the links made by `activate`).
fn active_now() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(dir) = fs::read_dir(config::EXTENSIONS_RUN) {
        for entry in dir.flatten() {
            if let Some(name) = entry.file_name().to_string_lossy().strip_suffix(".raw") {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// `xctl extensions enable <name> [--from <source>] [--keyring <file>]`
fn enable(args: &[String]) -> Result<()> {
    // The name first, then the options.
    let (name, options) = match args.split_first() {
        Some((name, rest)) if !name.starts_with('-') => (Some(name.clone()), rest),
        _ => (None, args),
    };
    let (from, keyrings, _) = source_options(options, &[])?;
    let name =
        name.ok_or_else(|| err("usage: xctl extensions enable <name> (see xctl extensions list)"))?;
    need_root("extensions enable")?;
    let version = running_version()?;
    // The copy built for the running version: by default from the
    // extensions' registry, where every version has its own tag.
    let source = Source::parse(&from.unwrap_or_else(|| default_source(&version)));
    let ext = match find(&name) {
        Ok(ext) => ext,
        // Not known here yet: an optional extension, known by its source.
        Err(_) => Extension {
            name: name.clone(),
            ..Default::default()
        },
    };
    if ext.image(&version).is_none() {
        download(&ext, &version, &source, &keyrings)?;
    }
    set_choice(&name, true)?;
    println!("Extension {name}: on, from the next restart.");
    Ok(())
}

/// Where the optional extensions for `version` are published.
pub fn default_source(version: &str) -> String {
    format!("oci://{}:{version}", config::EXT_REGISTRY)
}

/// The files to download for extension `name` on `version`, among the
/// `published` ones: its image, and its manifest if published too.
pub fn files_of(name: &str, version: &str, published: &[String]) -> Option<Vec<String>> {
    let image = update_file(version, name);
    if !published.contains(&image) {
        return None;
    }
    let mut files = vec![image];
    let conf = format!("{name}.conf");
    if published.contains(&conf) {
        files.push(conf);
    }
    Some(files)
}

/// Download the image of `ext` for `version` (and its manifest) from
/// `source`, checking them against the signed manifest like an update.
fn download(ext: &Extension, version: &str, source: &Source, keyrings: &[PathBuf]) -> Result<()> {
    let cache = Path::new(config::EXTENSIONS_CACHE);
    let _ = fs::remove_dir_all(cache);
    fs::create_dir_all(cache)?;
    let _cleanup = update::Cleanup(cache);

    update::fetch_manifest(source, cache, keyrings)?;
    let names = update::manifest_files(&fs::read_to_string(cache.join("SHA256SUMS"))?)?;
    let files = files_of(&ext.name, version, &names).ok_or_else(|| {
        err(format!(
            "the extension {} is not published for XeeonOS {version}",
            ext.name
        ))
    })?;
    update::fetch_files(source, cache, &files)?;
    update::check_hashes(cache)?;
    store(cache, version, &ext.name)
}

/// Put a downloaded, verified extension file (and its manifest, if it came
/// with it) into the store.
pub fn store(dir: &Path, version: &str, name: &str) -> Result<()> {
    let ext = Extension {
        name: name.to_string(),
        ..Default::default()
    };
    let image = ext.stored_image(version);
    if let Some(parent) = image.parent() {
        fs::create_dir_all(parent)?;
    }
    // Written under another name first: a half-written image is never used.
    let partial = image.with_extension("partial");
    copy_image(&dir.join(update_file(version, name)), &partial)?;
    fs::rename(&partial, &image)?;
    let conf = dir.join(format!("{name}.conf"));
    if conf.exists() {
        fs::copy(&conf, image.with_extension("conf"))?;
    }
    Ok(())
}

/// Copy an extension image. Some are several GB (rocm-sdk): on Btrfs the
/// copy shares the blocks of the original (reflink), so the download and
/// its stored copy do not take the space twice.
fn copy_image(from: &Path, to: &Path) -> Result<()> {
    let _ = fs::remove_file(to);
    let (from_s, to_s) = (from.to_string_lossy(), to.to_string_lossy());
    if run_program("cp", &["--reflink=auto", "--", &from_s, &to_s]).is_err() {
        fs::copy(from, to)?;
    }
    Ok(())
}

/// Remove the downloaded extensions of versions that are no longer
/// installed (`keep`: the versions in the boot menu).
pub fn prune(keep: &[String]) {
    let Ok(dir) = fs::read_dir(config::EXTENSIONS_STORE) else {
        return;
    };
    for entry in dir.flatten() {
        let version = entry.file_name().to_string_lossy().into_owned();
        if !keep.contains(&version) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// `xctl extensions disable <name> [--yes]`
fn disable(args: &[String]) -> Result<()> {
    let mut name = None;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            other if name.is_none() && !other.starts_with('-') => name = Some(other.to_string()),
            other => {
                return Err(err(format!(
                    "unknown option for extensions disable: {other}"
                )));
            }
        }
    }
    let name = name
        .ok_or_else(|| err("usage: xctl extensions disable <name> (see xctl extensions list)"))?;
    need_root("extensions disable")?;
    let ext = find(&name)?;

    if ext.special {
        let warning = format!(
            "{name} is a special extension: {}.\nWithout it, what depends on it stops \
             working (nothing is deleted: turning it on again brings everything back).",
            ext.description
        );
        if yes {
            println!("Warning: {warning}");
        } else {
            if !crate::ui::interactive() {
                return Err(err(format!(
                    "{name} is a special extension and there is no terminal to ask on (give --yes)"
                )));
            }
            if !crate::ui::yes_no(
                "Special extension",
                &format!("{warning}\n\nTurn {name} off?"),
                true,
            )? {
                println!("Nothing changed.");
                return Ok(());
            }
        }
    }
    set_choice(&name, false)?;
    clean_up(&ext);
    println!("Extension {name}: off, from the next restart.");
    Ok(())
}

/// `xctl extensions install <file> [--yes]`: an extension image from a file. Nobody
/// signed it and XeeonOS cannot check it: say so and ask first.
fn install(args: &[String]) -> Result<()> {
    let mut file = None;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            other if file.is_none() && !other.starts_with('-') => file = Some(PathBuf::from(other)),
            other => {
                return Err(err(format!(
                    "unknown option for extensions install: {other}"
                )));
            }
        }
    }
    let file = file.ok_or_else(|| err("usage: xctl extensions install <file.xext> [--yes]"))?;
    need_root("extensions install")?;
    if !file.is_file() {
        return Err(err(format!("{} is not a file", file.display())));
    }
    let ext = manifest_inside(&file)?;
    let name = &ext.name;
    if manifests(Path::new(config::EXTENSIONS))
        .iter()
        .any(|e| &e.name == name && e.image(&running_version().unwrap_or_default()).is_some())
        && !Path::new(config::EXTENSIONS_LOCAL)
            .join(format!("{name}.conf"))
            .exists()
    {
        return Err(err(format!(
            "an extension named {name} is already part of XeeonOS (see xctl extensions list)"
        )));
    }

    let warning = format!(
        "{name} ({}) comes from a file: it is not signed and XeeonOS has not checked it. \
         Once on, its programs are part of the system and can do anything on this \
         computer. Install it only if you trust where it comes from.",
        ext.description
    );
    if yes {
        println!("Warning: {warning}");
    } else {
        if !crate::ui::interactive() {
            return Err(err(
                "an extension from a file is not verified and there is no terminal to ask on \
                 (give --yes)",
            ));
        }
        if !crate::ui::yes_no(
            "Extension from a file",
            &format!("{warning}\n\nInstall {name}?"),
            true,
        )? {
            println!("Nothing changed.");
            return Ok(());
        }
    }

    let dir = Path::new(config::EXTENSIONS_LOCAL);
    fs::create_dir_all(dir)?;
    let image = dir.join(format!("{name}.xext"));
    let partial = image.with_extension("partial");
    fs::copy(&file, &partial)?;
    fs::rename(&partial, &image)?;
    fs::write(dir.join(format!("{name}.conf")), manifest_text(&ext))?;
    set_choice(name, true)?;
    println!("Extension {name}: installed; on, from the next restart.");
    Ok(())
}

/// Read the manifest inside an extension image
/// (usr/lib/xeeonos/extensions/<name>.conf): mount it read-only for a moment.
fn manifest_inside(image: &Path) -> Result<Extension> {
    let at = std::env::temp_dir().join(format!("xctl-ext-{}", std::process::id()));
    fs::create_dir_all(&at)?;
    let _cleanup = update::Cleanup(&at);
    output(
        "mount",
        &[
            "--options",
            "loop,ro,nodev,nosuid,noexec",
            &image.to_string_lossy(),
            &at.to_string_lossy(),
        ],
    )
    .map_err(|e| {
        err(format!(
            "{} is not an extension image ({e})",
            image.display()
        ))
    })?;
    let found = manifests(&at.join(config::EXTENSIONS.trim_start_matches('/')));
    let _ = output("umount", &[&at.to_string_lossy()]);
    let mut found = found.into_iter();
    match (found.next(), found.next()) {
        (Some(ext), None) if update::is_plain_file_name(&ext.name) => Ok(ext),
        _ => Err(err(format!(
            "{} has no manifest ({}/<name>.conf)",
            image.display(),
            config::EXTENSIONS
        ))),
    }
}

/// A manifest as text again (what `parse` reads).
fn manifest_text(ext: &Extension) -> String {
    let mut text = format!(
        "Description=\"{}\"\nSpecial={}\nDefault=no\nEnable=\"{}\"\nDisable=\"{}\"\n",
        ext.description.replace('"', "'"),
        if ext.special { "yes" } else { "no" },
        ext.units.join(" "),
        ext.masks.join(" ")
    );
    for (key, value) in [
        ("Setup", &ext.setup),
        ("Cleanup", &ext.cleanup),
        ("Boot", &ext.boot),
    ] {
        if let Some(value) = value {
            text.push_str(&format!("{key}={value}\n"));
        }
    }
    text
}

/// `xctl extensions remove <name>`: turn an optional extension off and delete its
/// images (downloaded, or installed from a file). An included one is part of the system image: it can
/// only be turned off.
fn remove(args: &[String]) -> Result<()> {
    let [name] = args else {
        return Err(err(
            "usage: xctl extensions remove <name> (see xctl extensions list)",
        ));
    };
    need_root("extensions remove")?;
    let ext = find(name)?;
    if ext.included_image().is_none() {
        clean_up(&ext);
    }
    if ext.local {
        let dir = Path::new(config::EXTENSIONS_LOCAL);
        let _ = fs::remove_file(dir.join(format!("{name}.xext")));
        let _ = fs::remove_file(dir.join(format!("{name}.conf")));
    } else if ext.included_image().is_some() {
        return Err(err(format!(
            "{name} is included in the system and cannot be removed; \
             turn it off with: sudo xctl extensions disable {name}"
        )));
    }
    if let Ok(dir) = fs::read_dir(config::EXTENSIONS_STORE) {
        for version in dir.flatten() {
            let _ = fs::remove_file(version.path().join(format!("{name}.xext")));
            // The version's directory too, once empty.
            let _ = fs::remove_dir(version.path());
        }
    }
    set_choice(name, false)?;
    println!("Extension {name}: removed (in use until the next restart).");
    Ok(())
}

/// `xctl extensions activate`: run once at every boot (xeeonos-extensions.service),
/// before the services start. Lays the extensions that are on over /usr and
/// brings in what they add: users, device rules, units.
fn activate() -> Result<()> {
    need_root("extensions activate")?;
    let version = running_version()?;
    let choices = choices();
    let run = Path::new(config::EXTENSIONS_RUN);
    let mut units = Vec::new();
    let mut active = Vec::new();
    let mut local = Vec::new();
    let mut hooked = Vec::new();
    for ext in known() {
        if !is_on(&ext, &choices) {
            continue;
        }
        let Some(image) = ext.image(&version) else {
            println!(
                "Extension {}: on, but not downloaded for XeeonOS {version} \
                 (sudo xctl extensions enable {})",
                ext.name, ext.name
            );
            continue;
        };
        fs::create_dir_all(run)?;
        let link = run.join(format!("{}.raw", ext.name));
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&image, &link)?;
        if ext.local {
            local.push((ext.name.clone(), link, ext.units.clone()));
        }
        units.extend(ext.units.iter().cloned());
        active.push(ext.name.clone());
        hooked.push(ext);
    }
    if active.is_empty() {
        return Ok(());
    }
    if run_program("systemd-sysext", &["refresh"]).is_err() {
        // An extension from a file made for another version of XeeonOS is
        // refused: go on with XeeonOS's own.
        if local.is_empty() {
            return Err(err("systemd-sysext could not lay the extensions over /usr"));
        }
        for (name, link, its_units) in &local {
            println!("Extension {name} (from a file): left out, the extensions did not fit.");
            let _ = fs::remove_file(link);
            active.retain(|a| a != name);
            units.retain(|u| !its_units.contains(u));
        }
        if active.is_empty() {
            return Ok(());
        }
        hooked.retain(|e| active.contains(&e.name));
        run_program("systemd-sysext", &["refresh"])?;
    }
    // Users and groups of the extensions (the boot's own run came before).
    run_program("systemd-sysusers", &[])?;
    let _ = run_program("udevadm", &["control", "--reload"]);
    // systemd read its units before the extensions were there.
    run_program("systemctl", &["daemon-reload"])?;
    // Disable=: units of the system that must not run next to it, masked
    // for this boot only (/run).
    let masks: Vec<&str> = hooked
        .iter()
        .flat_map(|e| e.masks.iter().map(String::as_str))
        .collect();
    if !masks.is_empty() {
        let mut args = vec!["mask", "--runtime", "--now"];
        args.extend(masks.iter());
        if run_program("systemctl", &args).is_err() {
            println!("Extensions: could not mask {}", masks.join(" "));
        }
    }
    for ext in &hooked {
        // Setup=: once (until the extension is turned off).
        if let Some(script) = &ext.setup {
            let marker = setup_marker(&ext.name);
            if !marker.exists() && run_hook(&ext.name, "Setup", script) {
                fs::create_dir_all(config::EXTENSIONS_SETUP)?;
                fs::write(&marker, "")?;
                println!("Extension {}: set up", ext.name);
            }
        }
        // Boot=: every boot.
        if let Some(script) = &ext.boot {
            run_hook(&ext.name, "Boot", script);
        }
    }
    if !units.is_empty() {
        let mut args = vec!["start", "--no-block"];
        args.extend(units.iter().map(String::as_str));
        run_program("systemctl", &args)?;
    }
    println!("Extensions: {}", active.join(", "));
    Ok(())
}

/// Where an extension's hook scripts are (inside its image, so only while
/// it is laid over /usr).
fn hook_path(name: &str, script: &str) -> PathBuf {
    Path::new(config::EXTENSIONS).join(name).join(script)
}

fn setup_marker(name: &str) -> PathBuf {
    Path::new(config::EXTENSIONS_SETUP).join(name)
}

/// Run one hook script of `name`; a failure is reported, not fatal (the
/// rest of the system must still start).
fn run_hook(name: &str, kind: &str, script: &str) -> bool {
    let path = hook_path(name, script);
    if !path.is_file() {
        println!(
            "Extension {name}: its {kind} script {} is missing",
            path.display()
        );
        return false;
    }
    let ok = std::process::Command::new(&path)
        .env("XEEONOS_EXTENSION", name)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        println!("Extension {name}: its {kind} script failed");
    }
    ok
}

/// What an extension does when it is turned off or removed: run its Cleanup
/// script, if its Setup ran. The script is still there: the extension stays
/// laid over /usr until the next restart.
fn clean_up(ext: &Extension) {
    let marker = setup_marker(&ext.name);
    if !marker.exists() {
        return;
    }
    if let Some(script) = &ext.cleanup {
        if active_now().contains(&ext.name) {
            if run_hook(&ext.name, "Cleanup", script) {
                println!("Extension {}: cleaned up", ext.name);
            }
        } else {
            println!(
                "Extension {}: not in use, its Cleanup script cannot run",
                ext.name
            );
        }
    }
    // Turned on again later, its Setup runs again.
    let _ = fs::remove_file(marker);
}

/// One line for `xctl status`: the extensions in use.
pub fn summary() -> Option<String> {
    let mut active = active_now();
    active.sort();
    (!active.is_empty()).then(|| active.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf() {
        let ext = parse(
            "docker",
            "# comment\nDescription=\"Docker containers\"\nSpecial=no\nDefault=no\n\
             Packages=\"docker docker-compose\"\nEnable=\"docker.socket containerd.service\"\n",
        );
        assert_eq!(ext.description, "Docker containers");
        assert!(!ext.special && !ext.default);
        assert_eq!(ext.units, vec!["docker.socket", "containerd.service"]);
        let ext = parse("flatpak", "Special=yes\nDefault=yes\n");
        assert!(ext.special && ext.default && ext.units.is_empty());
    }

    #[test]
    fn hooks() {
        let ext = parse(
            "ci-hello",
            "Disable=\"a.timer b.service\"\nSetup=setup\nCleanup=\"cleanup\"\nBoot=../boot\n",
        );
        assert_eq!(ext.masks, vec!["a.timer", "b.service"]);
        assert_eq!(ext.setup.as_deref(), Some("setup"));
        assert_eq!(ext.cleanup.as_deref(), Some("cleanup"));
        // Not a plain file name: ignored.
        assert_eq!(ext.boot, None);
        // Written again for an extension from a file, read back the same.
        let again = parse("ci-hello", &manifest_text(&ext));
        assert_eq!(again.masks, ext.masks);
        assert_eq!(again.setup, ext.setup);
        assert_eq!(again.cleanup, ext.cleanup);
    }

    #[test]
    fn on_and_off() {
        let choices = parse_choices("docker=on\nflatpak=off\n\n");
        let ext = |name: &str, default| Extension {
            name: name.into(),
            default,
            ..Default::default()
        };
        assert!(is_on(&ext("docker", false), &choices));
        assert!(!is_on(&ext("flatpak", true), &choices));
        assert!(is_on(&ext("printing", true), &choices));
        assert!(!is_on(&ext("rocm", false), &choices));
    }

    #[test]
    fn file_names() {
        let file = update_file("26.12-testing.7", "docker");
        assert_eq!(file, "xeeonos_26.12-testing.7.docker.xext");
        assert_eq!(extension_of(&file), Some("docker"));
        assert_eq!(extension_of("xeeonos_0.1.0.usr-x86-64.abc.raw.zst"), None);
        assert_eq!(extension_of("xeeonos_0.1.0.efi"), None);
    }
}
