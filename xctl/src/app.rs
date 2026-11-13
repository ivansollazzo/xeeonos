//! `xctl app`: .xapp applications (experimental, docs/xapp.md).
//!
//! An application is a directory `<Name>.xapp` in /var/lib/xapps (every
//! user sees it as ~/Applications). It holds a whole container, ready to
//! run: its image (`image.tar.zst`, everything the application needs, made
//! from a recipe by tools/mkxapp.sh), a manifest (`xapp.conf`) and an icon.
//! Copying one there is installing it, deleting it is removing it:
//!
//! - the system's monitor (xeeonos-xapps.path, `xctl app sync`, as root)
//!   puts the application in the menu, or takes it out;
//! - each user's monitor (the user unit xeeonos-xapps.path, `xctl app sync
//!   --user`) imports the image into the user's podman and makes the
//!   application's own distrobox container, `xapp-<id>`, in the background
//!   (no network needed); when the .xapp changes (a new version) it makes
//!   the container again, when the .xapp is gone it deletes both.
//!
//! `xctl app run` (the menu entry) starts the application in its container;
//! if the container is not ready yet it is prepared first, with a
//! notification. The user's files stay in the home directory, which the
//! container shares: making the container again loses nothing.
//!
//! xapp.conf (`Key=value` lines):
//!
//!   Name=Firefox                 shown in the menu
//!   Id=org.mozilla.firefox       unique, reverse domain name
//!   Version=145.0-1              shown by `xctl app list`
//!   Exec=/usr/bin/firefox %u     the command inside the container; %u/%f...
//!                                as in desktop entries
//!   Icon=icon.png                inside the .xapp directory
//!   Comment=Web browser
//!   Categories=Network;WebBrowser;
//!   MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::util::{Result, err, is_root, output};

/// Where the applications are.
pub const APPS: &str = "/var/lib/xapps";
/// The menu entries written by `sync` (in XDG_DATA_DIRS, see
/// /usr/lib/environment.d/60-xapps.conf).
pub const MENU: &str = "/var/lib/xeeonos/xapps/share/applications";
/// The container image inside every .xapp (`podman save`/`docker save`
/// archive, compressed with zstd).
const IMAGE: &str = "image.tar.zst";
/// An .xapp still being copied (its image changed less than this long ago)
/// is left alone until the copy is over.
const SETTLE: Duration = Duration::from_secs(10);

#[derive(Debug, Default, PartialEq)]
struct App {
    /// The directory name without ".xapp".
    dir_name: String,
    name: String,
    id: String,
    version: String,
    exec: String,
    icon: String,
    comment: String,
    categories: String,
    mime_type: String,
}

impl App {
    fn dir(&self) -> PathBuf {
        Path::new(APPS).join(format!("{}.xapp", self.dir_name))
    }
    /// The application's distrobox container (one per user).
    fn container(&self) -> String {
        format!("xapp-{}", self.id.to_lowercase())
    }
    /// The name of its image in the user's podman.
    fn image_ref(&self) -> String {
        format!("localhost/xapp/{}:current", self.id.to_lowercase())
    }
}

fn parse(dir_name: &str, text: &str) -> Result<App> {
    let mut app = App {
        dir_name: dir_name.to_string(),
        ..Default::default()
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "Name" => app.name = value,
            "Id" => app.id = value,
            "Version" => app.version = value,
            "Exec" => app.exec = value,
            "Icon" => app.icon = value,
            "Comment" => app.comment = value,
            "Categories" => app.categories = value,
            "MimeType" => app.mime_type = value,
            _ => {}
        }
    }
    if app.name.is_empty() || app.exec.is_empty() || app.id.is_empty() {
        return Err(err(format!(
            "{dir_name}.xapp: xapp.conf needs Name=, Id= and Exec="
        )));
    }
    // The id names containers, images and files: only safe characters.
    if !app
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        || !app.id.starts_with(|c: char| c.is_ascii_alphanumeric())
    {
        return Err(err(format!(
            "{dir_name}.xapp: Id= may only have letters, digits and ._- (starting with a letter or digit)"
        )));
    }
    // The icon is inside the application.
    if app.icon.starts_with('/') || app.icon.split('/').any(|p| p == "..") {
        return Err(err(format!(
            "{dir_name}.xapp: Icon= must be a file inside the application"
        )));
    }
    Ok(app)
}

/// When the image of an application last changed, and its size: what tells
/// a new version (or a copy still going on) apart.
fn image_stamp(dir: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(dir.join(IMAGE)).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The applications in /var/lib/xapps that are complete (a valid xapp.conf,
/// an image that is not being copied any more), and the problems of the
/// others.
fn installed() -> (Vec<App>, Vec<String>) {
    let mut apps = Vec::new();
    let mut problems = Vec::new();
    let Ok(dir) = fs::read_dir(APPS) else {
        return (apps, problems);
    };
    for entry in dir.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(dir_name) = file.strip_suffix(".xapp") else {
            continue;
        };
        if dir_name.starts_with('.') {
            continue;
        }
        let app = match fs::read_to_string(entry.path().join("xapp.conf")) {
            Ok(text) => match parse(dir_name, &text) {
                Ok(app) => app,
                Err(e) => {
                    problems.push(e.0);
                    continue;
                }
            },
            Err(_) => {
                problems.push(format!("{file}: no xapp.conf (still being copied?)"));
                continue;
            }
        };
        match image_stamp(&entry.path()) {
            None => problems.push(format!("{file}: no {IMAGE} (still being copied?)")),
            Some((changed, _)) if changed.elapsed().unwrap_or_default() < SETTLE => {
                problems.push(format!("{file}: still being copied"))
            }
            Some(_) => apps.push(app),
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    (apps, problems)
}

fn find(name: &str) -> Result<App> {
    let (apps, _) = installed();
    apps.into_iter()
        .find(|a| {
            a.name.eq_ignore_ascii_case(name)
                || a.dir_name.eq_ignore_ascii_case(name)
                || a.id.eq_ignore_ascii_case(name)
        })
        .ok_or_else(|| err(format!("no application {name} (see xctl app list)")))
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None | Some("list") => list(),
        Some("install") => install(&args[1..]),
        Some("remove") => remove(&args[1..]),
        Some("run") => launch(&args[1..]),
        Some("sync") => {
            let rest = &args[1..];
            let settle = rest.iter().any(|a| a == "--settle");
            if rest.iter().any(|a| a == "--user") {
                sync_user(settle)
            } else {
                sync(settle)
            }
        }
        Some(other) => Err(err(format!(
            "unknown app command '{other}' (expected list, install <path>, remove <name> or run <name>)"
        ))),
    }
}

/// `xctl app list`: the applications, and whether this user's container of
/// each one is ready.
fn list() -> Result<()> {
    let (apps, problems) = installed();
    if apps.is_empty() && problems.is_empty() {
        println!(
            "No .xapp applications (copy one into ~/Applications, or: xctl app install <path>)."
        );
    }
    let user = !is_root();
    for app in &apps {
        let version = if app.version.is_empty() {
            String::new()
        } else {
            format!(" {}", app.version)
        };
        let state = if !user {
            ""
        } else if is_ready(app) {
            ", ready"
        } else {
            ", not prepared yet"
        };
        println!("{}{version}  ({}.xapp{state})", app.name, app.dir_name);
    }
    for p in problems {
        println!("Not usable: {p}");
    }
    Ok(())
}

/// `xctl app install <dir.xapp | archive>`: copy (or unpack) it into
/// /var/lib/xapps under a hidden name, then rename it: the monitors only
/// ever see it whole. Administrators can write there without sudo.
fn install(args: &[String]) -> Result<()> {
    let [from] = args else {
        return Err(err(
            "usage: xctl app install <Name.xapp directory or archive>",
        ));
    };
    let from = PathBuf::from(from);
    let apps = Path::new(APPS);
    let partial = apps.join(format!(".install-{}", std::process::id()));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial)?;
    let cleanup = crate::update::Cleanup(&partial);

    if from.is_dir() {
        output(
            "cp",
            &[
                "--recursive",
                "--reflink=auto",
                "--no-target-directory",
                &from.to_string_lossy(),
                &partial.join("app.xapp").to_string_lossy(),
            ],
        )?;
    } else if from.is_file() {
        // tar recognizes a compression (gz, xz, zst) by itself.
        output(
            "tar",
            &[
                "--extract",
                "--no-same-owner",
                "--file",
                &from.to_string_lossy(),
                "--directory",
                &partial.to_string_lossy(),
            ],
        )?;
    } else {
        return Err(err(format!("{} not found", from.display())));
    }
    // The application: the one .xapp directory there.
    let found: Vec<PathBuf> = fs::read_dir(&partial)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("xapp.conf").is_file())
        .collect();
    let [dir] = found.as_slice() else {
        return Err(err(format!(
            "{} does not hold one application (a directory with xapp.conf)",
            from.display()
        )));
    };
    let app = parse("app", &fs::read_to_string(dir.join("xapp.conf"))?)?;
    if !dir.join(IMAGE).is_file() {
        return Err(err(format!(
            "{} has no container image ({IMAGE}): not an .xapp of this kind",
            from.display()
        )));
    }
    if let Ok(other) = find(&app.id) {
        return Err(err(format!(
            "{} is already installed as {}.xapp (remove it first: xctl app remove {})",
            app.name, other.dir_name, other.name
        )));
    }
    // Named after the application.
    let dir_name: String = app
        .name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || " ._-".contains(*c))
        .collect();
    let target = apps.join(format!("{}.xapp", dir_name.trim()));
    if target.exists() {
        return Err(err(format!(
            "{} already exists (remove it first: xctl app remove {})",
            target.display(),
            app.name
        )));
    }
    fs::rename(dir, &target)?;
    drop(cleanup);
    println!("{} installed in {}.", app.name, target.display());
    // The monitors do the rest (menu, container) by themselves.
    Ok(())
}

/// `xctl app remove <name>`
fn remove(args: &[String]) -> Result<()> {
    let [name] = args else {
        return Err(err("usage: xctl app remove <name>"));
    };
    let app = find(name)?;
    fs::remove_dir_all(app.dir())?;
    println!("{} removed.", app.name);
    Ok(())
}

/// `xctl app run <name> [arguments]`: start an application in its
/// container, preparing the container first if needed.
fn launch(args: &[String]) -> Result<()> {
    let Some((name, rest)) = args.split_first() else {
        return Err(err("usage: xctl app run <name> [arguments]"));
    };
    let app = find(name)?;
    prepare(&app, true)?;
    let command = command_line(&app.exec, rest);
    if command.is_empty() {
        return Err(err(format!("{}: empty Exec=", app.name)));
    }
    let status = Command::new("distrobox")
        .args(["enter", "--name", &app.container(), "--"])
        .args(&command)
        .status()
        .map_err(|e| err(format!("cannot start {}: {e}", app.name)))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Exec= as a command, the desktop-entry field codes (%u, %f, ...) replaced
/// by `args`.
fn command_line(exec: &str, args: &[String]) -> Vec<String> {
    let mut command = Vec::new();
    let mut placed = false;
    for word in exec.split_whitespace() {
        if matches!(word, "%u" | "%U" | "%f" | "%F") {
            command.extend(args.iter().cloned());
            placed = true;
        } else if !word.starts_with('%') {
            command.push(word.to_string());
        }
    }
    if !placed {
        command.extend(args.iter().cloned());
    }
    command
}

/// Where this user's state of the applications is kept: which image each
/// container was made from.
fn state_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        });
    base.join("xeeonos/xapps")
}

/// What this user's container of `app` was made from: "<seconds> <size>
/// <image id>".
fn stamp_text(app: &App) -> Option<String> {
    let (changed, size) = image_stamp(&app.dir())?;
    let secs = changed
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{secs} {size}"))
}

fn container_exists(name: &str) -> bool {
    Command::new("podman")
        .args(["container", "exists", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// True when this user's container of `app` is made from the image the
/// .xapp has now.
fn is_ready(app: &App) -> bool {
    let recorded = fs::read_to_string(state_dir().join(&app.id)).unwrap_or_default();
    let now = stamp_text(app);
    now.is_some_and(|now| recorded.starts_with(&format!("{now} ")))
        && container_exists(&app.container())
}

/// Tell the user something on the desktop (if there is one): a notification
/// through D-Bus, with gdbus (part of GLib, always there).
fn notify(app: &App, text: &str) {
    let icon = if app.icon.is_empty() {
        String::new()
    } else {
        app.dir().join(&app.icon).to_string_lossy().into_owned()
    };
    let _ = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest=org.freedesktop.Notifications",
            "--object-path=/org/freedesktop/Notifications",
            "--method=org.freedesktop.Notifications.Notify",
            "--timeout=5",
            "XeeonOS",
            "0",
            &icon,
            &app.name,
            text,
            "[]",
            "{}",
            "5000",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Make this user's container of `app` from the image in the .xapp, unless
/// it is already made from that image: import the image into podman (no
/// network needed), make the distrobox container again, run its first start
/// (distrobox sets itself up inside), forget the image it replaces.
/// One at a time per application (the monitor and a start can meet).
fn prepare(app: &App, interactive: bool) -> Result<()> {
    let state = state_dir();
    fs::create_dir_all(&state)?;
    let lock = fs::File::create(state.join(format!(".{}.lock", app.id)))?;
    lock.lock()?;
    if is_ready(app) {
        return Ok(());
    }
    let Some(stamp) = stamp_text(app) else {
        return Err(err(format!("{}: no {IMAGE}", app.name)));
    };
    let message = format!("Preparing {} (only the first time)...", app.name);
    println!("{message}");
    notify(app, &message);
    let result = (|| -> Result<String> {
        let image = app.dir().join(IMAGE);
        let loaded = output(
            "podman",
            &["load", "--quiet", "--input", &image.to_string_lossy()],
        )?;
        // "Loaded image: <name or id>" (the first one, if several).
        let loaded = loaded
            .lines()
            .find_map(|l| {
                l.strip_prefix("Loaded image")
                    .map(|r| r.trim_start_matches(['(', 's', ')', ':', ' ']))
            })
            .and_then(|r| r.split([',', ' ']).next())
            .map(str::to_string)
            .ok_or_else(|| err(format!("{}: podman loaded no image", app.name)))?;
        let image_ref = app.image_ref();
        output("podman", &["tag", &loaded, &image_ref])?;
        let id = output(
            "podman",
            &["image", "inspect", "--format", "{{.Id}}", &image_ref],
        )?
        .trim()
        .to_string();
        let container = app.container();
        if container_exists(&container) {
            output("podman", &["rm", "--force", &container])?;
        }
        output(
            "distrobox",
            &[
                "create",
                "--yes",
                "--no-entry",
                "--name",
                &container,
                "--image",
                &image_ref,
            ],
        )?;
        // The first start: distrobox sets the container up for this user.
        let first = Command::new("distrobox")
            .args(["enter", "--name", &container, "--", "true"])
            .stdin(Stdio::null())
            .stdout(if interactive {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .status()
            .map_err(|e| err(format!("cannot run distrobox: {e}")))?;
        if !first.success() {
            return Err(err(format!("{}: the container did not start", app.name)));
        }
        Ok(id)
    })();
    let id = match result {
        Ok(id) => id,
        Err(e) => {
            notify(app, &format!("Could not prepare it: {}", e.0));
            return Err(e);
        }
    };
    // The image the container was made from before (an older version).
    let previous = fs::read_to_string(state.join(&app.id)).unwrap_or_default();
    if let Some(old) = previous.split_whitespace().nth(2)
        && old != id
    {
        let _ = output("podman", &["rmi", old]);
    }
    fs::write(state.join(&app.id), format!("{stamp} {id}\n"))?;
    println!("{} is ready.", app.name);
    notify(app, "Ready.");
    Ok(())
}

/// Delete this user's container and image of an application that is gone.
fn forget(id: &str) {
    let lower = id.to_lowercase();
    let _ = output("podman", &["rm", "--force", &format!("xapp-{lower}")]);
    let _ = output(
        "podman",
        &["rmi", &format!("localhost/xapp/{lower}:current")],
    );
    let state = state_dir();
    if let Ok(text) = fs::read_to_string(state.join(id))
        && let Some(old) = text.split_whitespace().nth(2)
    {
        let _ = output("podman", &["rmi", old]);
    }
    let _ = fs::remove_file(state.join(id));
    let _ = fs::remove_file(state.join(format!(".{id}.lock")));
    println!("{id}: container and image deleted");
}

/// Wait (up to `limit`) while an application is still being copied.
fn settled(limit: Duration) -> (Vec<App>, Vec<String>) {
    let start = SystemTime::now();
    loop {
        let (apps, problems) = installed();
        if problems.is_empty() || start.elapsed().unwrap_or_default() >= limit {
            return (apps, problems);
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// `xctl app sync --user`: make this user's containers match /var/lib/xapps.
/// Run by the user's monitor at login and whenever something is added,
/// replaced or removed there.
fn sync_user(settle: bool) -> Result<()> {
    if is_root() {
        return Err(err("xctl app sync --user: run it as the user"));
    }
    let (apps, problems) = if settle {
        settled(Duration::from_secs(1800))
    } else {
        installed()
    };
    for p in &problems {
        println!("Not usable: {p}");
    }
    let mut failed = false;
    for app in &apps {
        if let Err(e) = prepare(app, false) {
            eprintln!("{}: {}", app.name, e.0);
            failed = true;
        }
    }
    // Applications this user had and that are gone: their state files.
    if let Ok(dir) = fs::read_dir(state_dir()) {
        for entry in dir.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !id.starts_with('.') && !apps.iter().any(|a| a.id == id) {
                forget(&id);
            }
        }
    }
    if failed {
        Err(err("some applications could not be prepared"))
    } else {
        Ok(())
    }
}

/// The menu entry of an application.
fn desktop_entry(app: &App) -> String {
    let dir = app.dir();
    let mut entry = format!(
        "[Desktop Entry]\n\
         # Written by xctl app sync from {dir}/xapp.conf: do not edit.\n\
         Type=Application\n\
         Name={}\n\
         Exec=/usr/bin/xctl app run \"{}\" {}\n\
         X-XeeonOS-Xapp={}\n",
        app.name,
        app.dir_name,
        if app.mime_type.is_empty() { "" } else { "%U" },
        dir.display(),
        dir = dir.display(),
    );
    if !app.icon.is_empty() {
        entry.push_str(&format!("Icon={}\n", dir.join(&app.icon).display()));
    }
    for (key, value) in [
        ("Comment", &app.comment),
        ("Categories", &app.categories),
        ("MimeType", &app.mime_type),
    ] {
        if !value.is_empty() {
            entry.push_str(&format!("{key}={value}\n"));
        }
    }
    entry
}

/// `xctl app sync`: make the menu match /var/lib/xapps. Run (as root) by the
/// monitor (xeeonos-xapps.path) whenever something is added or removed
/// there, and at boot. With --settle, wait while an application is still
/// being copied, up to half an hour (an image can be large).
fn sync(settle: bool) -> Result<()> {
    let menu = Path::new(MENU);
    fs::create_dir_all(menu)?;
    let (apps, problems) = if settle {
        settled(Duration::from_secs(1800))
    } else {
        installed()
    };
    let mut wanted = Vec::new();
    for app in &apps {
        let file = format!("xapp-{}.desktop", app.id);
        let entry = desktop_entry(app);
        if fs::read_to_string(menu.join(&file)).ok().as_deref() != Some(entry.as_str()) {
            fs::write(menu.join(&file), entry)?;
            println!("Menu: {} added", app.name);
        }
        wanted.push(file);
    }
    for entry in fs::read_dir(menu)?.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.starts_with("xapp-") && !wanted.contains(&file) {
            fs::remove_file(entry.path())?;
            println!("Menu: {file} removed");
        }
    }
    for p in problems {
        println!("Not usable: {p}");
    }
    // The menus notice new entries by themselves; this refreshes the cache
    // of the MIME types (which application opens which files).
    let _ = Command::new("update-desktop-database")
        .arg(menu)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "Name=Firefox\nId=org.mozilla.firefox\nVersion=145.0-1\n\
        Exec=/usr/bin/firefox %u\nIcon=icon.png\nCategories=Network;WebBrowser;\n";

    #[test]
    fn manifest() {
        let app = parse("Firefox", FIREFOX).unwrap();
        assert_eq!(app.id, "org.mozilla.firefox");
        assert_eq!(app.version, "145.0-1");
        assert_eq!(app.container(), "xapp-org.mozilla.firefox");
        assert_eq!(
            app.image_ref(),
            "localhost/xapp/org.mozilla.firefox:current"
        );
        assert!(parse("X", "Name=X\nExec=x\n").is_err()); // no Id
        assert!(parse("X", "Name=X\nId=x\n").is_err()); // no Exec
        assert!(parse("X", "Name=X\nExec=x\nId=a/b\n").is_err());
        assert!(parse("X", "Name=X\nExec=x\nId=.x\n").is_err());
        assert!(parse("X", "Name=X\nExec=x\nId=x\nIcon=../i.png\n").is_err());
        assert!(parse("X", "Name=X\nExec=x\nId=x\nIcon=/i.png\n").is_err());
        let app = parse("X", "Name=X\nExec=x\nId=Org.X\n").unwrap();
        assert_eq!(app.container(), "xapp-org.x");
    }

    #[test]
    fn command() {
        assert_eq!(
            command_line("/usr/bin/firefox %u", &["https://x".into()]),
            vec!["/usr/bin/firefox", "https://x"]
        );
        assert_eq!(
            command_line("app --flag", &["--version".into()]),
            vec!["app", "--flag", "--version"]
        );
        assert_eq!(command_line("app %U", &[]), vec!["app"]);
    }

    #[test]
    fn entry() {
        let app = parse("Firefox", FIREFOX).unwrap();
        let e = desktop_entry(&app);
        assert!(e.contains("Exec=/usr/bin/xctl app run \"Firefox\" \n"));
        assert!(e.contains("Icon=/var/lib/xapps/Firefox.xapp/icon.png\n"));
        assert!(e.contains("Categories=Network;WebBrowser;\n"));
    }
}
