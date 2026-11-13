//! `xctl app`: .xapp applications (experimental, docs/xapp.md).
//!
//! An application is a directory `<Name>.xapp` in /var/lib/xapps (every
//! user sees it as ~/Applications): the application's own files and a
//! manifest, `xapp.conf`. Copying one there is installing it, deleting it is
//! removing it: a monitor (xeeonos-xapps.path) runs `xctl app sync`, which
//! writes the menu entries of the applications present and removes those of
//! the applications gone.
//!
//! xapp.conf (`Key=value` lines):
//!
//!   Name=Firefox                 shown in the menu
//!   Id=org.mozilla.firefox       unique, reverse domain name
//!   Exec=firefox/firefox %u      relative to the .xapp directory; %u/%f...
//!                                as in desktop entries
//!   Icon=firefox/browser/chrome/icons/default/default128.png
//!   Comment=Web browser
//!   Categories=Network;WebBrowser;
//!   MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;
//!   Runtime=host                 host: runs on the system's libraries;
//!                                distrobox: in a container (distrobox, from
//!                                the module tools) made at the first start,
//!                                with the Arch packages in Packages=
//!   Packages=gtk3 alsa-lib       (Runtime=distrobox only)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{Result, err, output, run as run_program};

/// Where the applications are.
pub const APPS: &str = "/var/lib/xapps";
/// The menu entries written by `sync` (in XDG_DATA_DIRS, see
/// /usr/lib/environment.d/60-xapps.conf).
pub const MENU: &str = "/var/lib/xeeonos/xapps/share/applications";
/// The distrobox container of Runtime=distrobox applications (per user).
const CONTAINER: &str = "xapps";

#[derive(Debug, Default, PartialEq)]
struct App {
    /// The directory name without ".xapp".
    dir_name: String,
    name: String,
    id: String,
    exec: String,
    icon: String,
    comment: String,
    categories: String,
    mime_type: String,
    runtime: String,
    packages: Vec<String>,
}

fn parse(dir_name: &str, text: &str) -> Result<App> {
    let mut app = App {
        dir_name: dir_name.to_string(),
        runtime: "host".to_string(),
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
            "Exec" => app.exec = value,
            "Icon" => app.icon = value,
            "Comment" => app.comment = value,
            "Categories" => app.categories = value,
            "MimeType" => app.mime_type = value,
            "Runtime" => app.runtime = value,
            "Packages" => app.packages = value.split_whitespace().map(String::from).collect(),
            _ => {}
        }
    }
    if app.name.is_empty() || app.exec.is_empty() {
        return Err(err(format!(
            "{dir_name}.xapp: xapp.conf needs Name= and Exec="
        )));
    }
    // The id names files: only safe characters.
    if app.id.is_empty() {
        app.id = dir_name.to_string();
    }
    if !app
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err(err(format!(
            "{dir_name}.xapp: Id= may only have letters, digits and ._-"
        )));
    }
    if !matches!(app.runtime.as_str(), "host" | "distrobox") {
        return Err(err(format!(
            "{dir_name}.xapp: Runtime= must be host or distrobox"
        )));
    }
    // Exec and Icon are inside the application.
    for path in [&app.exec, &app.icon] {
        if path.starts_with('/') || path.split('/').any(|p| p == "..") {
            return Err(err(format!(
                "{dir_name}.xapp: paths in xapp.conf must stay inside the application"
            )));
        }
    }
    Ok(app)
}

/// The applications in /var/lib/xapps that are complete (have a valid
/// xapp.conf), and the problems of the others.
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
        match fs::read_to_string(entry.path().join("xapp.conf")) {
            Ok(text) => match parse(dir_name, &text) {
                Ok(app) => apps.push(app),
                Err(e) => problems.push(e.0),
            },
            Err(_) => problems.push(format!("{file}: no xapp.conf (still being copied?)")),
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    (apps, problems)
}

fn find(name: &str) -> Result<App> {
    let (apps, _) = installed();
    apps.into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(name) || a.dir_name.eq_ignore_ascii_case(name))
        .ok_or_else(|| err(format!("no application {name} (see xctl app list)")))
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None | Some("list") => list(),
        Some("install") => install(&args[1..]),
        Some("remove") => remove(&args[1..]),
        Some("run") => launch(&args[1..]),
        Some("sync") => sync(args.get(1).is_some_and(|a| a == "--settle")),
        Some(other) => Err(err(format!(
            "unknown app command '{other}' (expected list, install <path>, remove <name> or run <name>)"
        ))),
    }
}

/// `xctl app list`
fn list() -> Result<()> {
    let (apps, problems) = installed();
    if apps.is_empty() && problems.is_empty() {
        println!(
            "No .xapp applications (copy one into ~/Applications, or: xctl app install <path>)."
        );
    }
    for app in &apps {
        println!("{}  ({}.xapp, {})", app.name, app.dir_name, app.runtime);
    }
    for p in problems {
        println!("Not usable: {p}");
    }
    Ok(())
}

/// `xctl app install <dir.xapp | archive>`: copy (or unpack) it into
/// /var/lib/xapps under a hidden name, then rename it: the monitor only ever
/// sees it whole. Administrators can write there without sudo.
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
                "--no-target-directory",
                &from.to_string_lossy(),
                &partial.join("app.xapp").to_string_lossy(),
            ],
        )?;
    } else if from.is_file() {
        // tar recognizes the compression (gz, xz, zst) by itself.
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
    let stem = from
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = stem.split(".xapp").next().unwrap_or("app");
    let app = parse(stem, &fs::read_to_string(dir.join("xapp.conf"))?)?;
    // Named after the application.
    let dir_name: String = app
        .name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || " ._-".contains(*c))
        .collect();
    let target = apps.join(format!("{}.xapp", dir_name.trim()));
    if target.exists() {
        return Err(err(format!(
            "{} is already installed (remove it first: xctl app remove {})",
            app.name, app.name
        )));
    }
    fs::rename(dir, &target)?;
    drop(cleanup);
    println!("{} installed in {}.", app.name, target.display());
    // The monitor does it too; do it now so the menu is ready on return.
    let _ = sync(false);
    Ok(())
}

/// `xctl app remove <name>`
fn remove(args: &[String]) -> Result<()> {
    let [name] = args else {
        return Err(err("usage: xctl app remove <name>"));
    };
    let app = find(name)?;
    let dir = Path::new(APPS).join(format!("{}.xapp", app.dir_name));
    fs::remove_dir_all(&dir)?;
    println!("{} removed.", app.name);
    let _ = sync(false);
    Ok(())
}

/// `xctl app run <name> [arguments]`: start an application with its runtime.
fn launch(args: &[String]) -> Result<()> {
    let Some((name, rest)) = args.split_first() else {
        return Err(err("usage: xctl app run <name> [arguments]"));
    };
    let app = find(name)?;
    let dir = Path::new(APPS).join(format!("{}.xapp", app.dir_name));
    let command = command_line(&app.exec, &dir, rest);
    let Some((program, program_args)) = command.split_first() else {
        return Err(err(format!("{}: empty Exec=", app.name)));
    };
    let status = if app.runtime == "distrobox" {
        ensure_container(&app)?;
        Command::new("distrobox")
            .args(["enter", "--name", CONTAINER, "--"])
            .arg(program)
            .args(program_args)
            .status()
    } else {
        Command::new(program).args(program_args).status()
    }
    .map_err(|e| err(format!("cannot start {}: {e}", app.name)))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Exec= as a command: the program inside the application's directory, the
/// desktop-entry field codes (%u, %f, ...) replaced by `args`.
fn command_line(exec: &str, dir: &Path, args: &[String]) -> Vec<String> {
    let mut words = exec.split_whitespace();
    let mut command = Vec::new();
    if let Some(program) = words.next() {
        command.push(dir.join(program).to_string_lossy().into_owned());
    }
    let mut placed = false;
    for word in words {
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

/// The container of Runtime=distrobox applications, made at the first start
/// (needs the network then), with the packages the application asks for.
fn ensure_container(app: &App) -> Result<()> {
    let exists = output("distrobox", &["list", "--no-color"])
        .unwrap_or_default()
        .lines()
        .any(|l| l.split('|').nth(1).is_some_and(|n| n.trim() == CONTAINER));
    if !exists {
        println!("Preparing the container for .xapp applications (first start only)...");
        run_program(
            "distrobox",
            &[
                "create",
                "--yes",
                "--name",
                CONTAINER,
                "--image",
                "docker.io/library/archlinux:latest",
            ],
        )?;
    }
    if !app.packages.is_empty() {
        let mut args = vec![
            "enter",
            "--name",
            CONTAINER,
            "--",
            "sudo",
            "pacman",
            "-Syu",
            "--needed",
            "--noconfirm",
        ];
        args.extend(app.packages.iter().map(String::as_str));
        run_program("distrobox", &args)?;
    }
    Ok(())
}

/// The menu entry of an application.
fn desktop_entry(app: &App, dir: &Path) -> String {
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

/// `xctl app sync`: make the menu match /var/lib/xapps. Run by the monitor
/// (xeeonos-xapps.path) whenever something is added or removed there, and at
/// boot. With --settle, wait while an application is still being copied
/// (its xapp.conf not there yet), up to two minutes.
fn sync(settle: bool) -> Result<()> {
    let menu = Path::new(MENU);
    fs::create_dir_all(menu)?;
    let mut waited = 0;
    loop {
        let (apps, problems) = installed();
        let mut wanted = Vec::new();
        for app in &apps {
            let dir = Path::new(APPS).join(format!("{}.xapp", app.dir_name));
            let file = format!("xapp-{}.desktop", app.id);
            let entry = desktop_entry(app, &dir);
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
        if !settle || problems.is_empty() || waited >= 120 {
            for p in problems {
                println!("Not usable: {p}");
            }
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
        waited += 3;
    }
    // The menus notice new entries by themselves; this refreshes the cache
    // of the MIME types (which application opens which files).
    let _ = Command::new("update-desktop-database")
        .arg(menu)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "Name=Firefox\nId=org.mozilla.firefox\nExec=firefox/firefox %u\n\
        Icon=firefox/browser/chrome/icons/default/default128.png\nCategories=Network;WebBrowser;\n";

    #[test]
    fn manifest() {
        let app = parse("Firefox", FIREFOX).unwrap();
        assert_eq!(app.id, "org.mozilla.firefox");
        assert_eq!(app.runtime, "host");
        assert!(parse("X", "Name=X\n").is_err());
        assert!(parse("X", "Name=X\nExec=/usr/bin/sh\n").is_err());
        assert!(parse("X", "Name=X\nExec=../x\n").is_err());
        assert!(parse("X", "Name=X\nExec=x\nId=a/b\n").is_err());
        assert!(parse("X", "Name=X\nExec=x\nRuntime=wine\n").is_err());
        let app = parse(
            "X",
            "Name=X\nExec=x\nRuntime=distrobox\nPackages=gtk3 nss\n",
        )
        .unwrap();
        assert_eq!(app.packages, vec!["gtk3", "nss"]);
    }

    #[test]
    fn command() {
        let dir = Path::new("/var/lib/xapps/Firefox.xapp");
        assert_eq!(
            command_line("firefox/firefox %u", dir, &["https://x".into()]),
            vec!["/var/lib/xapps/Firefox.xapp/firefox/firefox", "https://x"]
        );
        assert_eq!(
            command_line("bin/app --flag", dir, &["--version".into()]),
            vec!["/var/lib/xapps/Firefox.xapp/bin/app", "--flag", "--version"]
        );
    }

    #[test]
    fn entry() {
        let app = parse("Firefox", FIREFOX).unwrap();
        let e = desktop_entry(&app, Path::new("/var/lib/xapps/Firefox.xapp"));
        assert!(e.contains("Exec=/usr/bin/xctl app run \"Firefox\" \n"));
        assert!(e.contains("Icon=/var/lib/xapps/Firefox.xapp/firefox/browser/"));
        assert!(e.contains("Categories=Network;WebBrowser;\n"));
    }
}
