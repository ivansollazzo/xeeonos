//! `xapp run`: what happens when an application starts.
//!
//!   1. its file is checked: signature, revocation, pinning, image (a file
//!      that fails is refused, without a way to force it);
//!   2. questions, like macOS asks them: a file outside ~/Applications
//!      (move it there?), an author XeeonOS does not know ("Open anyway"),
//!      an older version than the one used before;
//!   3. the permissions it declared and the user has not decided yet, each
//!      with the application's reason;
//!   4. the sandbox.
//!
//! The questions are terminal dialogs (xapp_core::ui): in the terminal, or
//! in a terminal window opened for them when started from the menu.

use std::fs;
use std::path::PathBuf;

use xapp_core::paths;
use xapp_core::ui::{self, Question};
use xapp_core::util::{Result, err};
use xapp_core::version;
use xapp_desktop::registry;
use xapp_format::Container;
use xapp_permissions as perms;
use xapp_trust::{self as trust, Trust, keys};

use super::{in_applications, open_target};

/// Tell the user something went wrong (also when started from the menu,
/// where nobody sees the error on a terminal), then fail.
fn refuse(title: &str, message: String) -> Result<u8> {
    if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        let _ = ui::ask(&[Question::message(title, &message)]);
    }
    Err(err(message))
}

/// Move a file into ~/Applications (renamed if the name is taken).
fn move_to_applications(c: &Container) -> Result<PathBuf> {
    let dest = xapp_build::install_path(&c.path, &c.manifest.name);
    fs::create_dir_all(paths::applications())?;
    if fs::rename(&c.path, &dest).is_err() {
        // Another disk: copy, then delete the original.
        let tmp = dest.with_file_name(format!(".{}.part", c.manifest.app_id));
        fs::copy(&c.path, &tmp)?;
        fs::rename(&tmp, &dest)?;
        fs::remove_file(&c.path)?;
    }
    Ok(dest)
}

pub fn run(args: &[String]) -> Result<u8> {
    let Some(target) = args.first() else {
        return Err(err("usage: xapp run [--yes] <file|app> [args...]"));
    };
    let mut c = open_target(target)?;
    let m = c.manifest.clone();
    let name = m.display_name().to_string();

    // 1. The file.
    let trust_state = match trust::check(&c) {
        Ok(t) => t,
        Err(e) => return refuse(&format!("{name} cannot be opened"), e.to_string()),
    };
    if let Err(e) = trust::check_image_cached(&c) {
        return refuse(&format!("{name} cannot be opened"), e.to_string());
    }

    // 2. The questions before opening.
    let outside = !in_applications(&c.path);
    let older =
        registry::last_version(&m.app_id).filter(|last| version::compare(&m.version, last).is_lt());
    let mut gate: Vec<(&str, Question)> = Vec::new();
    if outside {
        gate.push((
            "place",
            Question {
                title: format!("Open {name}"),
                text: format!(
                    "{name} is not in your Applications folder:\n{}\n\nMove it there to find it in the menu.",
                    c.path.display()
                ),
                choices: vec![
                    ("open".into(), "Open only this time".into()),
                    ("move".into(), "Move to Applications and open".into()),
                    ("cancel".into(), "Cancel".into()),
                ],
                default: 1,
            },
        ));
    }
    if trust_state == Trust::Unknown {
        gate.push((
            "author",
            Question::yes_no(
                &format!("{name}: unknown author"),
                &format!(
                    "{name} {} comes from an author XeeonOS does not know (key {}).\n\n\
                     The file is signed and was not changed since, but nobody vouches for its author.\n\
                     Open it only if you trust where you got it. It runs in its sandbox and asks \
                     before using your folders, microphone or camera.",
                    m.version,
                    keys::fingerprint(&m.signer)
                ),
                "Open anyway",
                "Cancel",
                false,
            ),
        ));
    }
    if let Some(last) = &older {
        gate.push((
            "older",
            Question::yes_no(
                &format!("{name}: older version"),
                &format!(
                    "This is {name} {}, older than the {last} used before.\nIts data may not work with an older version.",
                    m.version
                ),
                "Open the older version",
                "Cancel",
                false,
            ),
        ));
    }
    if !gate.is_empty() {
        let questions: Vec<Question> = gate.iter().map(|(_, q)| q.clone()).collect();
        let Some(answers) = ui::ask(&questions) else {
            return Err(err(format!(
                "{name} needs an answer before it opens, and there is no terminal to ask in (use --yes)"
            )));
        };
        for ((what, _), answer) in gate.iter().zip(&answers) {
            match (*what, answer.as_str()) {
                ("place", "cancel") | (_, "no") => return Ok(1),
                ("place", "move") => {
                    let dest = move_to_applications(&c)?;
                    c = Container::open(&dest)?;
                    let _ = xapp_desktop::sync();
                    println!("{name} moved to {}", dest.display());
                }
                ("author", "yes") => trust::policy::approve(&m.app_id, &m.signer)?,
                _ => {}
            }
        }
    }
    trust::policy::pin(&m.app_id, &m.signer)?;
    let newest = match registry::last_version(&m.app_id) {
        Some(last) if version::compare(&last, &m.version).is_gt() && older.is_none() => last,
        _ => m.version.clone(),
    };
    registry::set_last_version(&m.app_id, &newest)?;

    // 3. Permissions not decided yet.
    let pending: Vec<(String, Question)> = perms::pending(&m, true)
        .into_iter()
        .map(|p| {
            let label = perms::info(&p.id).map_or(p.id.as_str(), |i| i.label);
            let text = format!("{name} wants: {label}.\n\n\"{}\"\n\nYou can change this later with:\nxapp permissions {} {} yes|no", p.reason, m.app_id, p.id);
            (p.id.clone(), Question::yes_no(&format!("{name}: permission"), &text, "Allow", "Don't allow", false))
        })
        .collect();
    if !pending.is_empty() {
        let questions: Vec<Question> = pending.iter().map(|(_, q)| q.clone()).collect();
        match ui::ask(&questions) {
            Some(answers) => {
                for ((id, _), a) in pending.iter().zip(answers) {
                    perms::set(&m.app_id, id, Some(a == "yes"))?;
                }
                // The menu entry follows permissions such as autostart.
                if in_applications(&c.path) {
                    let _ = xapp_desktop::sync();
                }
            }
            None => eprintln!(
                "xapp: no terminal to ask about {}'s permissions: not given this time",
                name
            ),
        }
    }

    // 4. The sandbox.
    let code = xapp_sandbox::run(&xapp_sandbox::Launch {
        container: &c,
        known_author: true,
        args: args[1..].to_vec(),
    })?;
    Ok(code.clamp(0, 255) as u8)
}
