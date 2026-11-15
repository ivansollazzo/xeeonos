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
//! Dependencies (also in the manifest):
//!
//!   Requires=  extensions it needs: turning it on offers to turn them on
//!              too (downloading them); turning one of them off or
//!              removing it turns this one off too, after asking. At boot,
//!              an extension whose requirements are not all in place is
//!              left out.
//!   Conflicts= extensions that cannot be on together with it: turning it
//!              on offers to turn them off.
//!
//! Turning one on or off takes effect at once: the extensions are laid over
//! /usr again (systemd-sysext refresh), its services start or stop and its
//! hooks run, in the running system. Except an extension with `Live=no`
//! (one that brings kernel modules, which cannot be swapped in a running
//! system): that one, and every change made with `--no-apply`, waits for
//! the next restart.
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
#[derive(Debug, PartialEq)]
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
    /// Can be laid over /usr and taken away in the running system (Live=,
    /// default yes; no for kernel modules).
    pub live: bool,
    /// Installed from a file by the administrator (`xctl extensions install`).
    pub local: bool,
    /// Extensions it needs (Requires=) and that cannot be on with it
    /// (Conflicts=).
    pub requires: Vec<String>,
    pub conflicts: Vec<String>,
}

impl Default for Extension {
    fn default() -> Self {
        Extension {
            name: String::new(),
            description: String::new(),
            special: false,
            default: false,
            units: Vec::new(),
            masks: Vec::new(),
            setup: None,
            cleanup: None,
            boot: None,
            live: true,
            local: false,
            requires: Vec::new(),
            conflicts: Vec::new(),
        }
    }
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
            "Live" => ext.live = value != "no",
            "Requires" => ext.requires = words(value),
            "Conflicts" => ext.conflicts = words(value),
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
            format!("{} ({kind}){}", ext.description, relations(ext)),
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
            format!("{} ({kind}){}", ext.description, relations(ext)),
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

/// What an extension needs and cannot be on with, for the list.
fn relations(ext: &Extension) -> String {
    let mut text = String::new();
    if !ext.requires.is_empty() {
        text.push_str(&format!("; needs: {}", ext.requires.join(", ")));
    }
    if !ext.conflicts.is_empty() {
        text.push_str(&format!("; not with: {}", ext.conflicts.join(", ")));
    }
    text
}

/// The extensions `name` needs, directly or through others, those it needs
/// first (each once; a loop is cut where it closes). `requires` gives an
/// extension's Requires= (None: unknown here).
fn needed(name: &str, requires: &dyn Fn(&str) -> Option<Vec<String>>) -> Vec<String> {
    fn visit(
        name: &str,
        requires: &dyn Fn(&str) -> Option<Vec<String>>,
        seen: &mut Vec<String>,
        out: &mut Vec<String>,
    ) {
        if seen.iter().any(|s| s == name) {
            return;
        }
        seen.push(name.to_string());
        for dep in requires(name).unwrap_or_default() {
            visit(&dep, requires, seen, out);
            if !out.contains(&dep) && dep != seen[0] {
                out.push(dep);
            }
        }
    }
    let (mut seen, mut out) = (Vec::new(), Vec::new());
    visit(name, requires, &mut seen, &mut out);
    out
}

/// The extensions among `on` that need `name`, directly or through others,
/// the ones that need them first (to be turned off in this order).
fn needing(name: &str, all: &[Extension], on: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut queue = vec![name.to_string()];
    while let Some(current) = queue.pop() {
        for ext in all {
            if on.contains(&ext.name)
                && ext.name != name
                && ext.requires.contains(&current)
                && !out.contains(&ext.name)
            {
                out.push(ext.name.clone());
                queue.push(ext.name.clone());
            }
        }
    }
    out.reverse();
    out
}

/// The extensions among `on` that cannot be on together with `ext` (its
/// Conflicts=, or theirs naming it).
fn in_conflict(ext: &Extension, all: &[Extension], on: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for other in all {
        if other.name != ext.name
            && on.contains(&other.name)
            && (ext.conflicts.contains(&other.name) || other.conflicts.contains(&ext.name))
            && !out.contains(&other.name)
        {
            out.push(other.name.clone());
        }
    }
    out
}

/// The names of the extensions turned on.
fn names_on() -> Vec<String> {
    let choices = choices();
    known()
        .into_iter()
        .filter(|e| is_on(e, &choices))
        .map(|e| e.name)
        .collect()
}

/// Ask a yes/no question, or take `yes` (--yes) as the answer.
fn ask(yes: bool, title: &str, text: &str, what: &str) -> Result<bool> {
    if yes {
        println!("{text}");
        return Ok(true);
    }
    if !crate::ui::interactive() {
        return Err(err(format!(
            "{text}\nThere is no terminal to ask on: give --yes to {what}."
        )));
    }
    crate::ui::yes_no(title, text, false)
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

/// `xctl extensions enable <name> [--from <source>] [--keyring <file>] [--yes] [--no-apply]`
fn enable(args: &[String]) -> Result<()> {
    // The name first, then the options.
    let (name, options) = match args.split_first() {
        Some((name, rest)) if !name.starts_with('-') => (Some(name.clone()), rest),
        _ => (None, args),
    };
    let mut options = options.to_vec();
    let yes = options.iter().any(|o| o == "--yes");
    options.retain(|o| o != "--yes");
    let (from, keyrings, no_apply) = source_options(&options, &["--no-apply"])?;
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
    // Downloaded just now: its manifest is known from here on.
    let ext = find(&name)?;

    // What it needs: known here, or published for this version.
    let all = known();
    let on = names_on();
    let mut remote: Option<Vec<Extension>> = None;
    if ext
        .requires
        .iter()
        .any(|d| !all.iter().any(|a| &a.name == d))
    {
        remote = Some(published(&source, &version, &keyrings)?);
    }
    let lookup = |n: &str| -> Option<Vec<String>> {
        all.iter()
            .chain(remote.iter().flatten())
            .find(|e| e.name == n)
            .map(|e| e.requires.clone())
    };
    let deps = needed(&name, &lookup);
    if let Some(missing) = deps.iter().find(|d| lookup(d).is_none()) {
        return Err(err(format!(
            "{name} needs the extension {missing}, which is not published for XeeonOS {version}"
        )));
    }
    let deps_off: Vec<String> = deps.into_iter().filter(|d| !on.contains(d)).collect();
    if !deps_off.is_empty()
        && !ask(
            yes,
            "Extensions needed",
            &format!(
                "{name} needs: {}.\nThey will be turned on too.",
                deps_off.join(", ")
            ),
            "turn them on",
        )?
    {
        println!("Nothing changed.");
        return Ok(());
    }

    // What cannot be on with it (or with what it needs), and what needs that.
    let mut turning_on = deps_off.clone();
    turning_on.push(name.clone());
    let mut conflicting: Vec<String> = Vec::new();
    for n in &turning_on {
        let Some(e) = all
            .iter()
            .chain(remote.iter().flatten())
            .find(|e| &e.name == n)
        else {
            continue;
        };
        for c in in_conflict(e, &all, &on) {
            for x in needing(&c, &all, &on).into_iter().chain([c]) {
                if !conflicting.contains(&x) && !turning_on.contains(&x) {
                    conflicting.push(x);
                }
            }
        }
    }
    if !conflicting.is_empty()
        && !ask(
            yes,
            "Extensions in conflict",
            &format!(
                "{name} cannot be on together with: {}.\nThey will be turned off.",
                conflicting.join(", ")
            ),
            "turn them off",
        )?
    {
        println!("Nothing changed.");
        return Ok(());
    }
    turn_off_all(&conflicting, no_apply)?;
    for d in &deps_off {
        let dep = match find(d) {
            Ok(dep) => dep,
            Err(_) => Extension {
                name: d.clone(),
                ..Default::default()
            },
        };
        if dep.image(&version).is_none() {
            download(&dep, &version, &source, &keyrings)?;
        }
        set_choice(d, true)?;
        let dep = find(d)?;
        let state = turned_on(&dep, &version, no_apply)?;
        println!("Extension {d}: {state}.");
    }
    set_choice(&name, true)?;
    // What it needs comes only at the next restart (kernel modules): so
    // does it.
    let active = active_now();
    let wait = no_apply || needed(&name, &lookup).iter().any(|d| !active.contains(d));
    let state = turned_on(&ext, &version, wait)?;
    println!("Extension {name}: {state}.");
    Ok(())
}

/// Lay `ext`, just turned on, over the running system if it can be; what
/// that means, for the message.
fn turned_on(ext: &Extension, version: &str, no_apply: bool) -> Result<&'static str> {
    Ok(if active_now().contains(&ext.name) {
        "on"
    } else if no_apply {
        "on, from the next restart"
    } else if !ext.live {
        "on, from the next restart (it brings kernel modules: restart to use it)"
    } else {
        apply_on(ext, version)?;
        "on"
    })
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

/// `xctl extensions disable <name> [--yes] [--no-apply]`
fn disable(args: &[String]) -> Result<()> {
    let mut name = None;
    let mut yes = false;
    let mut no_apply = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            "--no-apply" => no_apply = true,
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
    let dependents = needing(&name, &known(), &names_on());
    if !dependents.is_empty()
        && !ask(
            yes,
            "Extensions that need it",
            &format!(
                "{} need {name}: they will be turned off too.",
                dependents.join(", ")
            ),
            "turn them off",
        )?
    {
        println!("Nothing changed.");
        return Ok(());
    }

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
    turn_off_all(&dependents, no_apply)?;
    set_choice(&name, false)?;
    turned_off(&ext, no_apply);
    println!("Extension {name}: {}.", off_state(&ext, no_apply));
    Ok(())
}

/// Turn off these extensions, in this order.
fn turn_off_all(names: &[String], no_apply: bool) -> Result<()> {
    for n in names {
        let other = find(n)?;
        set_choice(n, false)?;
        turned_off(&other, no_apply);
        println!("Extension {n}: {}.", off_state(&other, no_apply));
    }
    Ok(())
}

/// Take `ext` away from the running system, if it is laid over /usr and
/// can be (otherwise only its Cleanup runs, and it stays until the restart).
fn turned_off(ext: &Extension, no_apply: bool) {
    if active_now().contains(&ext.name) && ext.live && !no_apply {
        apply_off(ext);
    } else {
        clean_up(ext);
    }
}

fn off_state(ext: &Extension, no_apply: bool) -> &'static str {
    if !active_now().contains(&ext.name) {
        "off"
    } else if no_apply || ext.live {
        "off, from the next restart"
    } else {
        "off, from the next restart (it brings kernel modules)"
    }
}

/// `xctl extensions install <file> [--yes]`: an extension image from a file. Nobody
/// signed it and XeeonOS cannot check it: say so and ask first.
fn install(args: &[String]) -> Result<()> {
    let mut file = None;
    let mut yes = false;
    let mut no_apply = false;
    for arg in args {
        match arg.as_str() {
            "--yes" => yes = true,
            "--no-apply" => no_apply = true,
            other if file.is_none() && !other.starts_with('-') => file = Some(PathBuf::from(other)),
            other => {
                return Err(err(format!(
                    "unknown option for extensions install: {other}"
                )));
            }
        }
    }
    let file =
        file.ok_or_else(|| err("usage: xctl extensions install <file.xext> [--yes] [--no-apply]"))?;
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
    let name = name.clone();
    let mut ext = ext;
    ext.local = true;
    let state = turned_on(&ext, &running_version()?, no_apply)?;
    println!("Extension {name}: installed; {state}.");
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
    if !ext.live {
        text.push_str("Live=no\n");
    }
    if !ext.requires.is_empty() {
        text.push_str(&format!("Requires=\"{}\"\n", ext.requires.join(" ")));
    }
    if !ext.conflicts.is_empty() {
        text.push_str(&format!("Conflicts=\"{}\"\n", ext.conflicts.join(" ")));
    }
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
    let yes = args.iter().any(|a| a == "--yes");
    let rest: Vec<&String> = args.iter().filter(|a| *a != "--yes").collect();
    let [name] = rest[..] else {
        return Err(err(
            "usage: xctl extensions remove <name> [--yes] (see xctl extensions list)",
        ));
    };
    need_root("extensions remove")?;
    let ext = find(name)?;
    if ext.included_image().is_none() {
        let dependents = needing(name, &known(), &names_on());
        if !dependents.is_empty() {
            if !ask(
                yes,
                "Extensions that need it",
                &format!(
                    "{} need {name}: they will be turned off too.",
                    dependents.join(", ")
                ),
                "turn them off",
            )? {
                println!("Nothing changed.");
                return Ok(());
            }
            turn_off_all(&dependents, false)?;
        }
    }
    if ext.included_image().is_none() {
        turned_off(&ext, false);
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
    if active_now().contains(name) {
        println!("Extension {name}: removed (in use until the next restart).");
    } else {
        println!("Extension {name}: removed.");
    }
    Ok(())
}

/// `xctl extensions activate`: run once at every boot (xeeonos-extensions.service),
/// before the services start. Lays the extensions that are on over /usr and
/// brings in what they add: users, device rules, units.
fn activate() -> Result<()> {
    need_root("extensions activate")?;
    let version = running_version()?;
    let choices = choices();
    let mut active = Vec::new();
    let mut local = Vec::new();
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
        link(&ext.name, &image)?;
        if ext.local {
            local.push(ext.name.clone());
        }
        active.push(ext);
    }
    // Needs an extension that is not in place: left out (until it is).
    loop {
        let names: Vec<String> = active.iter().map(|e| e.name.clone()).collect();
        let Some(pos) = active
            .iter()
            .position(|e| e.requires.iter().any(|d| !names.contains(d)))
        else {
            break;
        };
        let ext = active.remove(pos);
        let missing: Vec<&String> = ext.requires.iter().filter(|d| !names.contains(d)).collect();
        println!(
            "Extension {}: on, but left out: it needs {}, not in place",
            ext.name,
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        unlink(&ext.name);
        local.retain(|n| n != &ext.name);
    }
    // Two that cannot be on together (turned on by hand): the second, by
    // name, is left out.
    let names: Vec<String> = active.iter().map(|e| e.name.clone()).collect();
    let mut out = Vec::new();
    for (i, ext) in active.iter().enumerate() {
        if in_conflict(ext, &active[..i], &names)
            .iter()
            .any(|c| !out.contains(c))
        {
            out.push(ext.name.clone());
        }
    }
    for name in &out {
        println!("Extension {name}: on, but left out: it cannot be on together with another one");
        unlink(name);
        local.retain(|n| n != name);
    }
    active.retain(|e| !out.contains(&e.name));
    if active.is_empty() {
        return Ok(());
    }
    if run_program("systemd-sysext", &["refresh"]).is_err() {
        // An extension from a file made for another version of XeeonOS is
        // refused: go on with XeeonOS's own.
        if local.is_empty() {
            return Err(err("systemd-sysext could not lay the extensions over /usr"));
        }
        for name in &local {
            println!("Extension {name} (from a file): left out, the extensions did not fit.");
            unlink(name);
        }
        active.retain(|e| !local.contains(&e.name));
        if active.is_empty() {
            return Ok(());
        }
        run_program("systemd-sysext", &["refresh"])?;
    }
    after_refresh()?;
    bring_in(&active)?;
    let names: Vec<&str> = active.iter().map(|e| e.name.as_str()).collect();
    println!("Extensions: {}", names.join(", "));
    Ok(())
}

/// Turn `ext` on in the running system: lay it over /usr with the others
/// already there, then do what a boot does for it.
fn apply_on(ext: &Extension, version: &str) -> Result<()> {
    let image = ext.image(version).ok_or_else(|| {
        err(format!(
            "the extension {} is not on this computer for XeeonOS {version}",
            ext.name
        ))
    })?;
    link(&ext.name, &image)?;
    if run_program("systemd-sysext", &["refresh"]).is_err() {
        // Back to how it was.
        unlink(&ext.name);
        let _ = run_program("systemd-sysext", &["refresh"]);
        return Err(err(format!(
            "the extension {} does not fit this system (made for another version of XeeonOS?)",
            ext.name
        )));
    }
    after_refresh()?;
    bring_in(std::slice::from_ref(ext))
}

/// Turn `ext` off in the running system: stop its services, give back the
/// units it masked, run its Cleanup, and take it away from /usr.
fn apply_off(ext: &Extension) {
    if !ext.units.is_empty() {
        let mut args = vec!["stop"];
        args.extend(ext.units.iter().map(String::as_str));
        let _ = run_program("systemctl", &args);
    }
    clean_up(ext);
    unlink(&ext.name);
    if run_program("systemd-sysext", &["refresh"]).is_err() {
        println!(
            "Extension {}: could not be taken away now; gone after the next restart",
            ext.name
        );
    }
    let _ = after_refresh();
    if !ext.masks.is_empty() {
        let mut args = vec!["unmask", "--runtime"];
        args.extend(ext.masks.iter().map(String::as_str));
        let _ = run_program("systemctl", &args);
        // What was masked and normally runs starts again.
        let again: Vec<&str> = ext
            .masks
            .iter()
            .map(String::as_str)
            .filter(|unit| {
                output("systemctl", &["is-enabled", unit])
                    .is_ok_and(|state| matches!(state.trim(), "enabled" | "static"))
            })
            .collect();
        if !again.is_empty() {
            let mut args = vec!["start", "--no-block"];
            args.extend(again);
            let _ = run_program("systemctl", &args);
        }
    }
}

/// /run/extensions/<name>.raw -> its image: what systemd-sysext lays over /usr.
fn link(name: &str, image: &Path) -> Result<()> {
    let run = Path::new(config::EXTENSIONS_RUN);
    fs::create_dir_all(run)?;
    let link = run.join(format!("{name}.raw"));
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(image, &link)?;
    Ok(())
}

fn unlink(name: &str) {
    let _ = fs::remove_file(Path::new(config::EXTENSIONS_RUN).join(format!("{name}.raw")));
}

/// After /usr changed: users and groups, device rules and units of the
/// extensions now there (or no longer).
fn after_refresh() -> Result<()> {
    run_program("systemd-sysusers", &[])?;
    let _ = run_program("udevadm", &["control", "--reload"]);
    compile_schemas();
    run_program("systemctl", &["daemon-reload"])
}

/// The settings (GSettings schemas) of the applications in the extensions.
/// The system's compiled list (/usr/share/glib-2.0/schemas/gschemas.compiled)
/// was made without them, and an application whose settings are missing
/// does not start (GNOME Software, Document Scanner). /usr cannot be
/// written: compile the system's and the extensions' together into /run,
/// which GSETTINGS_SCHEMA_DIR points to (usr/lib/environment.d).
fn compile_schemas() {
    let schemas = "/usr/share/glib-2.0/schemas";
    if !Path::new(schemas).is_dir() || !Path::new("/usr/bin/glib-compile-schemas").exists() {
        return;
    }
    let compiled = fs::create_dir_all(config::SCHEMAS_RUN).is_ok()
        && run_program(
            "glib-compile-schemas",
            &["--targetdir", config::SCHEMAS_RUN, schemas],
        )
        .is_ok();
    if !compiled {
        println!("Extensions: could not compile the settings of their applications");
    }
}

/// The hooks and services of extensions just laid over /usr: Disable=
/// (masked for this boot only, in /run), Setup= (once), Boot=, Enable=.
fn bring_in(exts: &[Extension]) -> Result<()> {
    let masks: Vec<&str> = exts
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
    for ext in exts {
        // Setup=: once (until the extension is turned off).
        if let Some(script) = &ext.setup {
            let marker = setup_marker(&ext.name);
            if !marker.exists() && run_hook(&ext.name, "Setup", script) {
                fs::create_dir_all(config::EXTENSIONS_SETUP)?;
                fs::write(&marker, "")?;
                println!("Extension {}: set up", ext.name);
            }
        }
        // Boot=: every boot (and when it is turned on).
        if let Some(script) = &ext.boot {
            run_hook(&ext.name, "Boot", script);
        }
    }
    let units: Vec<&str> = exts
        .iter()
        .flat_map(|e| e.units.iter().map(String::as_str))
        .collect();
    if !units.is_empty() {
        let mut args = vec!["start", "--no-block"];
        args.extend(units.iter());
        run_program("systemctl", &args)?;
    }
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
/// script, if its Setup ran (while the extension, and so the script, is
/// still laid over /usr).
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
        // Live by default; not one that brings kernel modules.
        assert!(ext.live);
        let ext = parse("touchpad", "Live=no\n");
        assert!(!ext.live);
        assert!(!parse("touchpad", &manifest_text(&ext)).live);
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
    fn dependencies() {
        let ext = |name: &str, requires: &[&str], conflicts: &[&str]| Extension {
            name: name.into(),
            requires: requires.iter().map(|s| s.to_string()).collect(),
            conflicts: conflicts.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let all = vec![
            ext("cuda", &["nvidia"], &[]),
            ext("nvidia", &[], &[]),
            ext("ml", &["cuda", "python"], &[]),
            ext("python", &[], &[]),
            ext("rocm", &[], &["rocm-sdk"]),
            ext("rocm-sdk", &[], &[]),
            ext("loop-a", &["loop-b"], &[]),
            ext("loop-b", &["loop-a"], &[]),
        ];
        let lookup = |n: &str| all.iter().find(|e| e.name == n).map(|e| e.requires.clone());
        // What it needs first, each once.
        assert_eq!(needed("ml", &lookup), vec!["nvidia", "cuda", "python"]);
        assert_eq!(needed("nvidia", &lookup), Vec::<String>::new());
        assert_eq!(needed("loop-a", &lookup), vec!["loop-b"]);
        // Who needs it, among those on, the furthest first.
        let on: Vec<String> = ["nvidia", "cuda", "ml", "python"]
            .map(String::from)
            .to_vec();
        assert_eq!(needing("nvidia", &all, &on), vec!["ml", "cuda"]);
        assert_eq!(needing("python", &all, &on), vec!["ml"]);
        assert!(needing("ml", &all, &on).is_empty());
        // Conflicts, named on either side.
        let on = vec!["rocm-sdk".to_string()];
        assert_eq!(in_conflict(&all[4], &all, &on), vec!["rocm-sdk"]);
        let on = vec!["rocm".to_string()];
        assert_eq!(in_conflict(&all[5], &all, &on), vec!["rocm"]);
        // Read from a manifest and written back the same.
        let parsed = parse("ml", "Requires=\"cuda python\"\nConflicts=rocm\n");
        assert_eq!(parsed.requires, vec!["cuda", "python"]);
        assert_eq!(parsed.conflicts, vec!["rocm"]);
        let again = parse("ml", &manifest_text(&parsed));
        assert_eq!(again.requires, parsed.requires);
        assert_eq!(again.conflicts, parsed.conflicts);
        assert_eq!(relations(&parsed), "; needs: cuda, python; not with: rocm");
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
