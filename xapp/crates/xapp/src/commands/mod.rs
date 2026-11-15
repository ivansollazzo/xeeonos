//! The commands of xapp, one file per group:
//!
//!   run.rs      xapp run: the questions (Gatekeeper, permissions), then the
//!               sandbox
//!   manage.rs   install, remove, list, info, verify, permissions, logs
//!   tools.rs    build, and the commands used by the system: sync (the
//!               monitor), dialog (the dialog window), thumbnail (Files)

mod manage;
mod run;
mod tools;

use std::path::{Path, PathBuf};

use xapp_core::util::{Result, err, valid_app_id};
use xapp_desktop::registry;
use xapp_format::Container;

/// Pick the command. Returns the exit code.
pub fn dispatch(args: &[String]) -> Result<u8> {
    let mut args: Vec<String> = args.to_vec();
    // --yes: before the command, or right after it.
    for pos in [0usize, 1] {
        if args.get(pos).is_some_and(|a| a == "--yes" || a == "-y") {
            xapp_core::ui::assume_yes();
            args.remove(pos);
        }
    }
    let Some(cmd) = args.first().cloned() else {
        print!("{}", crate::HELP);
        return Ok(0);
    };
    let rest = &args[1..];
    match cmd.as_str() {
        "run" => run::run(rest),
        "install" => manage::install(rest),
        "remove" => manage::remove(rest),
        "list" => manage::list(),
        "info" => manage::info(rest),
        "verify" => manage::verify(rest),
        "permissions" => manage::permissions(rest),
        "logs" => manage::logs(rest),
        "build" => tools::build(rest),
        "sync" => tools::sync(rest),
        "dialog" => tools::dialog(rest),
        "thumbnail" => tools::thumbnail(rest),
        "help" | "--help" | "-h" => {
            print!("{}", crate::HELP);
            Ok(0)
        }
        "--version" | "version" => {
            println!("xapp {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        // A file given directly (`xapp Firefox.xapp`) means run it.
        other if other.ends_with(".xapp") => run::run(&args),
        other => Err(err(format!("unknown command \"{other}\" (xapp help)"))),
    }
}

/// A file, or the id of a registered application, opened.
pub fn open_target(target: &str) -> Result<Container> {
    let path = Path::new(target);
    if path.exists() {
        return Container::open(path);
    }
    if valid_app_id(target) {
        if let Some(e) = registry::get(target)
            && e.file.exists()
        {
            return Container::open(&e.file);
        }
        // Maybe the monitor has not seen it yet.
        let _ = xapp_desktop::sync();
        if let Some(e) = registry::get(target)
            && e.file.exists()
        {
            return Container::open(&e.file);
        }
    }
    Err(err(format!(
        "no application or file \"{target}\" (see xapp list)"
    )))
}

/// The application id of a target (an id, or a file's).
pub fn app_id_of(target: &str) -> Result<String> {
    if valid_app_id(target) && !Path::new(target).exists() {
        return Ok(target.to_string());
    }
    Ok(open_target(target)?.manifest.app_id)
}

/// Is `path` directly inside ~/Applications?
pub fn in_applications(path: &Path) -> bool {
    let apps = std::fs::canonicalize(xapp_core::paths::applications()).ok();
    let parent: Option<PathBuf> = std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    apps.is_some() && apps == parent
}
