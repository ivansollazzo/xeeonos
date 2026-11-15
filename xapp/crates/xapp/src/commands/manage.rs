//! install, remove, list, info, verify, permissions, logs.

use std::fs;

use xapp_core::paths;
use xapp_core::util::{Result, err, run as run_program};
use xapp_desktop::{registry, sync::files};
use xapp_format::Container;
use xapp_permissions::{self as perms, State};
use xapp_trust::{self as trust, Trust, keys};

use super::{app_id_of, open_target};

/// Check a file before anything is done with it.
fn checked(path: &str) -> Result<(Container, Trust)> {
    let c = Container::open(std::path::Path::new(path))?;
    let t = trust::check(&c)?;
    trust::check_image_cached(&c)?;
    Ok((c, t))
}

/// `xapp install <file>`: copy into ~/Applications (the monitor does the
/// rest; here it is done at once).
pub fn install(args: &[String]) -> Result<u8> {
    let Some(file) = args.first() else {
        return Err(err("usage: xapp install <file.xapp>"));
    };
    let (c, t) = checked(file)?;
    let dest = xapp_build::install_path(&c.path, &c.manifest.name);
    if fs::canonicalize(&c.path).ok() != fs::canonicalize(&dest).ok() {
        fs::create_dir_all(paths::applications())?;
        // Copied under a hidden name, then renamed: the monitor never sees
        // half a file.
        let tmp = dest.with_file_name(format!(".{}.part", c.manifest.app_id));
        fs::copy(&c.path, &tmp)?;
        fs::rename(&tmp, &dest)?;
    }
    xapp_desktop::sync()?;
    println!(
        "Installed {} {}: {}",
        c.manifest.display_name(),
        c.manifest.version,
        dest.display()
    );
    if t == Trust::Unknown {
        println!(
            "Unknown author ({}): you will be asked when it first opens.",
            keys::fingerprint(&c.manifest.signer)
        );
    }
    Ok(0)
}

/// `xapp remove <app> [--purge]`.
pub fn remove(args: &[String]) -> Result<u8> {
    let Some(target) = args.first() else {
        return Err(err("usage: xapp remove <app> [--purge]"));
    };
    let purge = args.iter().any(|a| a == "--purge");
    let id = app_id_of(target)?;
    let mut removed = 0;
    for f in files() {
        if Container::open(&f).is_ok_and(|c| c.manifest.app_id == id) {
            fs::remove_file(&f)?;
            println!("Removed {}", f.display());
            removed += 1;
        }
    }
    xapp_desktop::sync()?;
    let data = paths::app_data(&id);
    if purge {
        if data.exists() {
            fs::remove_dir_all(&data)?;
        }
        perms::forget(&id);
        trust::policy::forget(&id);
        registry::forget(&id);
        println!("Data, permissions and key of {id} deleted.");
    } else if data.exists() {
        println!(
            "Its data stays in {} (xapp remove {id} --purge deletes it).",
            data.display()
        );
    }
    if removed == 0 && !purge {
        return Err(err(format!(
            "{id} is not in {}",
            paths::applications().display()
        )));
    }
    Ok(0)
}

/// `xapp list`.
pub fn list() -> Result<u8> {
    let report = xapp_desktop::sync()?;
    let entries = registry::all();
    if entries.is_empty() {
        println!("No applications in {}.", paths::applications().display());
    }
    let trusted = trust::policy::trusted_keys();
    for e in &entries {
        let author = if trusted.contains(&e.signer) {
            "trusted author"
        } else {
            "author not verified"
        };
        println!("{:<24} {:<32} {:<14} {author}", e.name, e.app_id, e.version);
    }
    for (f, why) in &report.skipped {
        println!("Left out: {} ({why})", f.display());
    }
    Ok(0)
}

/// `xapp info <app|file>`.
pub fn info(args: &[String]) -> Result<u8> {
    let Some(target) = args.first() else {
        return Err(err("usage: xapp info <app|file>"));
    };
    let c = open_target(target)?;
    let m = &c.manifest;
    let state = trust::check(&c);
    let known = matches!(state, Ok(Trust::Trusted | Trust::Approved));
    println!("{} {}", m.display_name(), m.version);
    println!("Id:          {}", m.app_id);
    if !m.description.is_empty() {
        println!("Description: {}", m.description);
    }
    println!(
        "File:        {} ({} MB)",
        c.path.display(),
        fs::metadata(&c.path)?.len() / 1_000_000
    );
    let author = match &state {
        Ok(Trust::Trusted) => "trusted".to_string(),
        Ok(Trust::Approved) => "not verified; opened anyway before".to_string(),
        Ok(Trust::Unknown) => "not verified; asked when it opens".to_string(),
        Err(e) => format!("REFUSED: {e}"),
    };
    println!("Author:      {} ({author})", keys::fingerprint(&m.signer));
    let age = xapp_core::date::parse(&m.snapshot).map(|d| xapp_core::date::today() - d);
    let old = match age {
        Some(days) if days > 365 => format!(
            " (more than {} months old: its libraries may lack security fixes)",
            days / 30
        ),
        _ => String::new(),
    };
    println!("Based on:    Arch Linux snapshot {}{old}", m.snapshot);
    println!(
        "Graphics:    {}",
        if m.gpu == xapp_format::Gpu::Host {
            "drivers of the system"
        } else {
            "its own drivers"
        }
    );
    println!("Data:        {}", paths::app_data(&m.app_id).display());
    if m.permissions.is_empty() {
        println!("Permissions: none");
    } else {
        println!("Permissions:");
        for (p, s) in perms::all(m, known) {
            let s = match s {
                State::Yes => "allowed",
                State::No => "not allowed",
                State::Ask => "asked when it opens",
            };
            let label = perms::info(&p.id).map_or("unknown to this xapp", |i| i.label);
            println!("  {:<14} {:<20} {label}: \"{}\"", p.id, s, p.reason);
        }
    }
    Ok(0)
}

/// `xapp verify <file>`: the whole check, the image read again.
pub fn verify(args: &[String]) -> Result<u8> {
    let Some(file) = args.first() else {
        return Err(err("usage: xapp verify <file.xapp>"));
    };
    let c = Container::open(std::path::Path::new(file))?;
    let t = trust::check(&c)?;
    c.check_image()?;
    let author = match t {
        Trust::Trusted => "trusted author",
        Trust::Approved => "author not verified (opened anyway before)",
        Trust::Unknown => "author not verified",
    };
    println!(
        "{}: {} {} is intact, signed by {} ({author}).",
        file,
        c.manifest.app_id,
        c.manifest.version,
        keys::fingerprint(&c.manifest.signer)
    );
    Ok(0)
}

/// `xapp permissions <app> [<permission> yes|no|ask]`.
pub fn permissions(args: &[String]) -> Result<u8> {
    let Some(target) = args.first() else {
        return Err(err(
            "usage: xapp permissions <app> [<permission> yes|no|ask]",
        ));
    };
    if args.len() >= 3 {
        let id = app_id_of(target)?;
        let value = match args[2].as_str() {
            "yes" | "allow" | "on" => Some(true),
            "no" | "deny" | "off" => Some(false),
            "ask" | "reset" => None,
            other => return Err(err(format!("\"{other}\": say yes, no or ask"))),
        };
        perms::set(&id, &args[1], value)?;
        println!(
            "{id}: {} {}",
            args[1],
            match value {
                Some(true) => "allowed",
                Some(false) => "not allowed",
                None => "asked at the next start",
            }
        );
        let _ = xapp_desktop::sync();
        return Ok(0);
    }
    let c = open_target(target)?;
    let known = matches!(trust::check(&c), Ok(Trust::Trusted | Trust::Approved));
    if c.manifest.permissions.is_empty() {
        println!("{} asks for no permissions.", c.manifest.app_id);
    }
    for (p, s) in perms::all(&c.manifest, known) {
        let s = match s {
            State::Yes => "yes",
            State::No => "no",
            State::Ask => "ask",
        };
        println!("{:<14} {s:<4} {}", p.id, p.reason);
    }
    Ok(0)
}

/// `xapp logs <app> [-f]`.
pub fn logs(args: &[String]) -> Result<u8> {
    let Some(target) = args.first() else {
        return Err(err("usage: xapp logs <app> [-f]"));
    };
    let id = app_id_of(target)?;
    let ident = format!("xapp-{id}");
    let mut cmd = vec!["--user", "--no-pager", "-t", ident.as_str()];
    if args.iter().any(|a| a == "-f" || a == "--follow") {
        cmd.push("-f");
    }
    run_program("journalctl", &cmd)?;
    Ok(0)
}
