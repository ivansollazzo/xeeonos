//! The text interface of the installer, the first-boot setup and Recovery:
//! dialog windows drawn by `whiptail` (the newt library, the same as `nmtui`),
//! in black and white.
//!
//! Each function shows one window and returns the answer; `None` means the
//! user chose "Back"/"Cancel" (or pressed Esc).

use std::io::IsTerminal;
use std::process::{Command, Stdio};

use crate::util::{Result, err};

/// newt colors: white on black, the selection in reverse (black on white),
/// like GRUB. `nmtui` started by xctl gets the same colors.
pub const COLORS: &str = "root=white,black;border=white,black;window=white,black;\
shadow=black,black;title=white,black;button=black,white;actbutton=black,white;\
compactbutton=white,black;checkbox=white,black;actcheckbox=black,white;\
entry=white,black;disentry=white,black;label=white,black;listbox=white,black;\
actlistbox=black,white;sellistbox=black,white;actsellistbox=black,white;\
textbox=white,black;acttextbox=black,white;emptyscale=white,black;\
fullscale=black,white;helpline=white,black;roottext=white,black";

const BACKTITLE: &str = "XeeonOS";
const HEIGHT: &str = "22";
const WIDTH: &str = "76";
const LIST_HEIGHT: &str = "12";

/// Is there a person at a terminal to answer? Without one, commands must be
/// given their answers as options (tests, scripts).
pub fn interactive() -> bool {
    std::io::stdin().is_terminal()
}

/// Run whiptail; whiptail draws on the terminal and writes the answer on its
/// standard error. Exit status: 0 OK, 1 Cancel/No, 255 Esc.
fn whiptail(args: &[&str]) -> Result<Option<String>> {
    if !interactive() {
        return Err(err("no terminal to ask on (give the answers as options)"));
    }
    let out = Command::new("whiptail")
        .env("NEWT_COLORS", COLORS)
        .args(["--backtitle", BACKTITLE])
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| err(format!("cannot run whiptail: {e}")))?;
    match out.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        )),
        _ => Ok(None),
    }
}

/// A list to choose from: (tag, description). Returns the chosen tag.
pub fn menu(
    title: &str,
    text: &str,
    items: &[(String, String)],
    default: Option<&str>,
) -> Result<Option<String>> {
    let mut args = vec!["--title", title, "--notags"];
    if let Some(d) = default {
        args.extend(["--default-item", d]);
    }
    args.extend(["--ok-button", "Continue", "--cancel-button", "Back"]);
    args.extend(["--menu", text, HEIGHT, WIDTH, LIST_HEIGHT]);
    for (tag, desc) in items {
        args.push(tag);
        args.push(desc);
    }
    whiptail(&args)
}

/// A line of text, pre-filled with `init`.
pub fn input(title: &str, text: &str, init: &str) -> Result<Option<String>> {
    whiptail(&[
        "--title",
        title,
        "--ok-button",
        "Continue",
        "--cancel-button",
        "Back",
        "--inputbox",
        text,
        "12",
        WIDTH,
        init,
    ])
}

/// A secret: what is typed is not shown.
pub fn password(title: &str, text: &str) -> Result<Option<String>> {
    whiptail(&[
        "--title",
        title,
        "--ok-button",
        "Continue",
        "--cancel-button",
        "Back",
        "--passwordbox",
        text,
        "12",
        WIDTH,
    ])
}

/// A yes/no question. With `default_no`, "No" is selected first (for
/// questions where "Yes" cannot be undone).
pub fn yes_no(title: &str, text: &str, default_no: bool) -> Result<bool> {
    let mut args = vec!["--title", title];
    if default_no {
        args.push("--defaultno");
    }
    args.extend(["--yesno", text, "14", WIDTH]);
    Ok(whiptail(&args)?.is_some())
}

/// A message with an OK button.
pub fn message(title: &str, text: &str) -> Result<()> {
    whiptail(&["--title", title, "--msgbox", text, "16", WIDTH])?;
    Ok(())
}

/// A message shown while something runs (no button; the next window or
/// output replaces it).
pub fn working(title: &str, text: &str) {
    if interactive() {
        let _ = Command::new("whiptail")
            .env("NEWT_COLORS", COLORS)
            .args(["--backtitle", BACKTITLE, "--title", title])
            .args(["--infobox", text, "8", WIDTH])
            .status();
    } else {
        println!("{text}");
    }
}
