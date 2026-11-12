//! `xctl setup`: the first-boot setup. Runs once, before the login prompt
//! (xeeonos-setup.service), and again after a factory reset.
//!
//! It asks for the language, keyboard, time zone and computer name, creates
//! the first user, and keeps the root account locked (as on macOS: an
//! administrator uses sudo). Everything it writes lives in /etc and /home on
//! the data partition: the sealed /usr is never touched.
//!
//! Unattended setup: the answers can be given as a file of `key=value` lines
//! (keys: language, keyboard, timezone, hostname, user, fullname, password,
//! admin), passed as the system credential `xeeonos.setup`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::ui;
use crate::util::{Result, err, need_root, run};

/// Marks the setup as done; the service does not start again while it exists.
const DONE: &str = "/var/lib/xeeonos/setup-done";

/// Languages offered in the menu: (locale, name, keyboard, time zone).
/// Any other language can be typed as a locale name (e.g. nl_NL.UTF-8):
/// all of them are in the image.
const LANGUAGES: &[(&str, &str, &str, &str)] = &[
    ("it_IT.UTF-8", "Italiano", "it", "Europe/Rome"),
    ("en_US.UTF-8", "English (US)", "us", "America/New_York"),
    ("en_GB.UTF-8", "English (UK)", "uk", "Europe/London"),
    ("de_DE.UTF-8", "Deutsch", "de", "Europe/Berlin"),
    ("fr_FR.UTF-8", "Français", "fr", "Europe/Paris"),
    ("es_ES.UTF-8", "Español", "es", "Europe/Madrid"),
    (
        "pt_BR.UTF-8",
        "Português (Brasil)",
        "br-abnt2",
        "America/Sao_Paulo",
    ),
];

/// The answers to the setup questions.
#[derive(Debug)]
struct Answers {
    language: String,
    keyboard: String,
    timezone: String,
    hostname: String,
    user: String,
    fullname: String,
    password: String,
    admin: bool,
}

pub fn run_setup(args: &[String]) -> Result<()> {
    if let Some(arg) = args.first() {
        return Err(err(format!("unknown option for setup: {arg}")));
    }
    need_root("setup")?;

    // systemd puts imported credentials in $CREDENTIALS_DIRECTORY.
    let answers = match std::env::var("CREDENTIALS_DIRECTORY")
        .ok()
        .map(|dir| Path::new(&dir).join("xeeonos.setup"))
        .filter(|f| f.exists())
    {
        Some(file) => {
            println!("XeeonOS setup: using the answers in {}", file.display());
            from_credential(&fs::read_to_string(file)?)?
        }
        None => ask_all()?,
    };
    apply(&answers)?;

    fs::create_dir_all(Path::new(DONE).parent().unwrap_or(Path::new("/")))?;
    fs::write(DONE, "")?;
    println!("XeeonOS setup: done. Welcome, {}!", answers.user);
    Ok(())
}

/// Parse `key=value` lines into answers, checking every value.
fn from_credential(text: &str) -> Result<Answers> {
    let map: HashMap<&str, &str> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();
    let get = |key: &str| -> Result<String> {
        map.get(key)
            .map(|v| v.to_string())
            .ok_or_else(|| err(format!("xeeonos.setup: missing {key}=")))
    };
    let language = get("language")?;
    let defaults = defaults_for(&language);
    let answers = Answers {
        keyboard: map.get("keyboard").map_or(defaults.0, |v| v.to_string()),
        timezone: map.get("timezone").map_or(defaults.1, |v| v.to_string()),
        hostname: map
            .get("hostname")
            .map_or("xeeonos".into(), |v| v.to_string()),
        user: get("user")?,
        fullname: map.get("fullname").map_or(String::new(), |v| v.to_string()),
        password: get("password")?,
        admin: map.get("admin").is_none_or(|v| *v == "yes"),
        language,
    };
    check_language(&answers.language)?;
    check_keyboard(&answers.keyboard)?;
    check_timezone(&answers.timezone)?;
    check_hostname(&answers.hostname)?;
    check_user(&answers.user)?;
    Ok(answers)
}

/// Keyboard layouts offered in the menu (console keymap names); any other
/// can be typed.
const KEYBOARDS: &[(&str, &str)] = &[
    ("it", "Italiano"),
    ("us", "English (US)"),
    ("uk", "English (UK)"),
    ("de", "Deutsch"),
    ("de_CH-latin1", "Deutsch (Schweiz)"),
    ("fr", "Français"),
    ("fr_CH", "Français (Suisse)"),
    ("es", "Español"),
    ("br-abnt2", "Português (Brasil)"),
    ("pt-latin1", "Português"),
];

/// The setup as dialog windows, one step at a time; "Back" goes to the
/// previous step. Language, keyboard and time zone take effect as soon as
/// they are chosen (the keyboard can be used right away with the new
/// layout); the account is created only at the end.
fn ask_all() -> Result<Answers> {
    let mut a = Answers {
        language: "en_US.UTF-8".into(),
        keyboard: "us".into(),
        timezone: "UTC".into(),
        hostname: "xeeonos".into(),
        user: String::new(),
        fullname: String::new(),
        password: String::new(),
        admin: true,
    };
    let mut step: usize = 0;
    loop {
        let forward = match step {
            0 => ask_language(&mut a)?,
            1 => ask_keyboard(&mut a)?,
            2 => ask_timezone(&mut a)?,
            3 => crate::network::screen()?,
            4 => ask_hostname(&mut a)?,
            5 => ask_account(&mut a)?,
            _ => return Ok(a),
        };
        if forward {
            step += 1;
        } else {
            step = step.saturating_sub(1);
        }
    }
}

fn ask_language(a: &mut Answers) -> Result<bool> {
    let mut items: Vec<(String, String)> = LANGUAGES
        .iter()
        .map(|l| (l.0.to_string(), l.1.to_string()))
        .collect();
    items.push(("other".into(), "Other...".into()));
    let Some(mut choice) = ui::menu(
        "Welcome to XeeonOS",
        "Choose your language:",
        &items,
        Some(&a.language),
    )?
    else {
        // The first step: there is nothing to go back to.
        return Ok(false);
    };
    if choice == "other" {
        let Some(typed) = ui::input("Language", "A language, e.g. nl_NL.UTF-8:", "")? else {
            return Ok(false);
        };
        choice = typed;
    }
    if let Err(e) = check_language(&choice) {
        ui::message("Language", &e.to_string())?;
        return Ok(false);
    }
    if choice != a.language {
        let (keyboard, timezone) = defaults_for(&choice);
        a.keyboard = keyboard;
        a.timezone = timezone;
    }
    a.language = choice;
    apply_language(&a.language)?;
    Ok(true)
}

fn ask_keyboard(a: &mut Answers) -> Result<bool> {
    let mut items: Vec<(String, String)> = KEYBOARDS
        .iter()
        .map(|k| (k.0.to_string(), format!("{} ({})", k.1, k.0)))
        .collect();
    if !KEYBOARDS.iter().any(|k| k.0 == a.keyboard) {
        items.insert(0, (a.keyboard.clone(), a.keyboard.clone()));
    }
    items.push(("other".into(), "Other...".into()));
    let Some(mut choice) = ui::menu(
        "Keyboard",
        "Choose the keyboard layout. It works as soon as you choose it.",
        &items,
        Some(&a.keyboard),
    )?
    else {
        return Ok(false);
    };
    if choice == "other" {
        let Some(typed) = ui::input("Keyboard", "A keyboard layout, e.g. sv-latin1:", "")? else {
            return ask_keyboard(a);
        };
        choice = typed;
    }
    if let Err(e) = check_keyboard(&choice) {
        ui::message("Keyboard", &e.to_string())?;
        // Ask again: stay on this step.
        return ask_keyboard(a);
    }
    a.keyboard = choice;
    apply_keyboard(&a.keyboard)?;
    Ok(true)
}

fn ask_timezone(a: &mut Answers) -> Result<bool> {
    let zones = timezones();
    loop {
        let current_region = a.timezone.split('/').next().unwrap_or("").to_string();
        let mut regions: Vec<String> = zones
            .iter()
            .map(|z| z.split('/').next().unwrap_or(z).to_string())
            .collect();
        regions.dedup();
        regions.push("UTC".into());
        let items: Vec<(String, String)> = regions.iter().map(|r| (r.clone(), r.clone())).collect();
        let Some(region) = ui::menu(
            "Time zone",
            &format!("Now: {}. Choose the region:", a.timezone),
            &items,
            Some(&current_region),
        )?
        else {
            return Ok(false);
        };
        let chosen = if region == "UTC" {
            "UTC".to_string()
        } else {
            let items: Vec<(String, String)> = zones
                .iter()
                .filter(|z| z.starts_with(&format!("{region}/")))
                .map(|z| (z.clone(), z[region.len() + 1..].replace('_', " ")))
                .collect();
            match ui::menu("Time zone", "Choose the city:", &items, Some(&a.timezone))? {
                Some(z) => z,
                // Back to the list of regions.
                None => continue,
            }
        };
        check_timezone(&chosen)?;
        a.timezone = chosen;
        apply_timezone(&a.timezone)?;
        return Ok(true);
    }
}

fn ask_hostname(a: &mut Answers) -> Result<bool> {
    loop {
        let Some(name) = ui::input(
            "Computer name",
            "The name of this computer on the network:",
            &a.hostname,
        )?
        else {
            return Ok(false);
        };
        match check_hostname(&name) {
            Ok(()) => {
                a.hostname = name;
                return Ok(true);
            }
            Err(e) => ui::message("Computer name", &e.to_string())?,
        }
    }
}

fn ask_account(a: &mut Answers) -> Result<bool> {
    let Some(fullname) = ui::input("Your account", "Your full name:", &a.fullname)? else {
        return Ok(false);
    };
    a.fullname = fullname;
    let suggested = if a.user.is_empty() {
        a.fullname
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    } else {
        a.user.clone()
    };
    loop {
        let Some(user) = ui::input(
            "Your account",
            "Your user name (lowercase letters and digits):",
            &suggested,
        )?
        else {
            return ask_account(a);
        };
        match check_user(&user) {
            Ok(()) => {
                a.user = user;
                break;
            }
            Err(e) => ui::message("Your account", &e.to_string())?,
        }
    }
    loop {
        let Some(first) = ui::password("Your account", &format!("Password for {}:", a.user))?
        else {
            return ask_account(a);
        };
        if first.is_empty() {
            ui::message("Your account", "The password cannot be empty.")?;
            continue;
        }
        let Some(again) = ui::password("Your account", "The same password again:")? else {
            continue;
        };
        if again == first {
            a.password = first;
            break;
        }
        ui::message("Your account", "The passwords do not match, try again.")?;
    }
    a.admin = ui::yes_no(
        "Your account",
        &format!(
            "Make {} an administrator?\n\nAn administrator can change the system \
             settings and use sudo. The first account is usually an administrator.",
            a.user
        ),
        false,
    )?;
    Ok(true)
}

/// The time zones of the world, from tzdata's zone1970.tab (one per region
/// of the world, the same list `timedatectl list-timezones` uses), sorted.
fn timezones() -> Vec<String> {
    let text = fs::read_to_string("/usr/share/zoneinfo/zone1970.tab").unwrap_or_default();
    let mut zones: Vec<String> = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split('\t').nth(2))
        .map(str::to_string)
        .collect();
    zones.sort();
    zones.dedup();
    zones
}

/// The language: written to /etc/locale.conf, and used from now on by the
/// windows of the setup.
fn apply_language(locale: &str) -> Result<()> {
    run(
        "systemd-firstboot",
        &["--force", &format!("--locale={locale}")],
    )?;
    // SAFETY: xctl has one thread; nothing reads the environment meanwhile.
    unsafe { std::env::set_var("LANG", locale) };
    Ok(())
}

/// The keyboard: written to /etc/vconsole.conf and loaded on the console at
/// once, so the next answer is typed with the new layout.
fn apply_keyboard(keymap: &str) -> Result<()> {
    run(
        "systemd-firstboot",
        &["--force", &format!("--keymap={keymap}")],
    )?;
    let _ = Command::new("loadkeys")
        .arg("-q")
        .arg(keymap)
        .stdout(Stdio::null())
        .status();
    Ok(())
}

/// The time zone: /etc/localtime, at once.
fn apply_timezone(zone: &str) -> Result<()> {
    run(
        "systemd-firstboot",
        &["--force", &format!("--timezone={zone}")],
    )?;
    // SAFETY: as above.
    unsafe { std::env::set_var("TZ", format!(":{zone}")) };
    Ok(())
}

/// Write the configuration and create the user.
fn apply(a: &Answers) -> Result<()> {
    // systemd-firstboot writes /etc/locale.conf, vconsole.conf, localtime and
    // hostname, without needing any running service.
    run(
        "systemd-firstboot",
        &[
            "--force",
            &format!("--locale={}", a.language),
            &format!("--keymap={}", a.keyboard),
            &format!("--timezone={}", a.timezone),
            &format!("--hostname={}", a.hostname),
        ],
    )?;

    // Lock root: administrators use sudo.
    run("usermod", &["--password", "!*", "root"])?;

    let mut args = vec!["--create-home", "--shell", "/bin/bash"];
    if !a.fullname.is_empty() {
        args.extend(["--comment", &a.fullname]);
    }
    if a.admin {
        args.extend(["--groups", "wheel"]);
    }
    args.push(&a.user);
    run("useradd", &args)?;

    // chpasswd reads "user:password" on its standard input, so the password
    // never appears in the list of processes.
    let mut child = Command::new("chpasswd")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| err(format!("cannot run chpasswd: {e}")))?;
    if let Some(stdin) = child.stdin.as_mut() {
        writeln!(stdin, "{}:{}", a.user, a.password)?;
    }
    if !child.wait()?.success() {
        return Err(err("setting the password failed"));
    }
    println!(
        "XeeonOS setup: created user {}{}",
        a.user,
        if a.admin { " (administrator)" } else { "" }
    );
    Ok(())
}

/// Default keyboard and time zone for a language.
fn defaults_for(language: &str) -> (String, String) {
    LANGUAGES
        .iter()
        .find(|l| l.0 == language)
        .map_or(("us".into(), "UTC".into()), |l| (l.2.into(), l.3.into()))
}

fn check_language(locale: &str) -> Result<()> {
    // glibc lists every locale it knows in SUPPORTED, e.g. "it_IT.UTF-8 UTF-8".
    let supported = fs::read_to_string("/usr/share/i18n/SUPPORTED").unwrap_or_default();
    if supported
        .lines()
        .any(|l| l.split_whitespace().next() == Some(locale))
        && locale.ends_with(".UTF-8")
    {
        Ok(())
    } else {
        Err(err(format!("unknown language: {locale}")))
    }
}

fn check_keyboard(name: &str) -> Result<()> {
    if is_simple_name(name)
        && find_file(
            Path::new("/usr/share/kbd/keymaps"),
            &format!("{name}.map.gz"),
        )
    {
        Ok(())
    } else {
        Err(err(format!("unknown keyboard layout: {name}")))
    }
}

fn check_timezone(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && !name.contains("..")
        && !name.starts_with('/')
        && Path::new("/usr/share/zoneinfo").join(name).is_file();
    if ok {
        Ok(())
    } else {
        Err(err(format!("unknown time zone: {name} (e.g. Europe/Rome)")))
    }
}

fn check_hostname(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(err("a computer name has letters, digits and '-' only"))
    }
}

fn check_user(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let ok = name.len() <= 32
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && !matches!(name, "root" | "bin" | "daemon" | "nobody");
    if ok {
        Ok(())
    } else {
        Err(err(
            "a user name starts with a lowercase letter and has lowercase letters, digits, '-' and '_' only",
        ))
    }
}

fn is_simple_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Is there a file called `name` somewhere below `dir`?
fn find_file(dir: &Path, name: &str) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let path = e.path();
        if path.is_dir() {
            find_file(&path, name)
        } else {
            e.file_name() == name
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(check_user("ivan").is_ok());
        assert!(check_user("Ivan").is_err());
        assert!(check_user("root").is_err());
        assert!(check_user("").is_err());
        assert!(check_hostname("my-pc").is_ok());
        assert!(check_hostname("my pc").is_err());
        assert!(check_timezone("../etc/passwd").is_err());
    }

    #[test]
    fn defaults() {
        assert_eq!(
            defaults_for("it_IT.UTF-8"),
            ("it".to_string(), "Europe/Rome".to_string())
        );
        assert_eq!(
            defaults_for("nl_NL.UTF-8"),
            ("us".to_string(), "UTC".to_string())
        );
    }
}
