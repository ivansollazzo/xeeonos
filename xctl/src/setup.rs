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
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};

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

/// Ask every question on the console. A wrong answer is asked again.
fn ask_all() -> Result<Answers> {
    println!("\nWelcome to XeeonOS! A few questions before you start.\n");

    println!("Language:");
    for (i, (_, name, _, _)) in LANGUAGES.iter().enumerate() {
        println!("  {}) {name}", i + 1);
    }
    let language = ask_valid(
        "Number, or another language (e.g. nl_NL.UTF-8) [1]: ",
        "1",
        |a| {
            let locale = match a.parse::<usize>() {
                Ok(n) if (1..=LANGUAGES.len()).contains(&n) => LANGUAGES[n - 1].0.to_string(),
                _ => a.to_string(),
            };
            check_language(&locale).map(|_| locale)
        },
    )?;
    let (keyboard, timezone) = defaults_for(&language);

    let keyboard = ask_valid(&format!("Keyboard layout [{keyboard}]: "), &keyboard, |a| {
        check_keyboard(a).map(|_| a.to_string())
    })?;
    let timezone = ask_valid(&format!("Time zone [{timezone}]: "), &timezone, |a| {
        check_timezone(a).map(|_| a.to_string())
    })?;
    let hostname = ask_valid("Computer name [xeeonos]: ", "xeeonos", |a| {
        check_hostname(a).map(|_| a.to_string())
    })?;

    println!("\nYour user account:");
    let fullname = ask("Full name: ")?;
    let suggested = fullname
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let user = ask_valid(&format!("User name [{suggested}]: "), &suggested, |a| {
        check_user(a).map(|_| a.to_string())
    })?;
    let password = loop {
        let first = ask_password("Password: ")?;
        if first.is_empty() {
            println!("  The password cannot be empty.");
            continue;
        }
        if ask_password("Password again: ")? == first {
            break first;
        }
        println!("  The passwords do not match, try again.");
    };
    let admin = ask_valid("Administrator (can use sudo)? [Y/n]: ", "y", |a| {
        match a.to_lowercase().as_str() {
            "y" | "yes" | "s" | "si" | "sì" => Ok(true),
            "n" | "no" => Ok(false),
            _ => Err(err("answer y or n")),
        }
    })?;

    Ok(Answers {
        language,
        keyboard,
        timezone,
        hostname,
        user,
        fullname,
        password,
        admin,
    })
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

fn ask(question: &str) -> Result<String> {
    print!("{question}");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line)? == 0 {
        return Err(err("no answer (end of input)"));
    }
    Ok(line.trim().to_string())
}

/// Ask until `check` accepts the answer; an empty answer means `default`.
/// `impl Fn(&str) -> Result<T>`: any function (or closure) of that shape.
fn ask_valid<T>(question: &str, default: &str, check: impl Fn(&str) -> Result<T>) -> Result<T> {
    loop {
        let answer = ask(question)?;
        let answer = if answer.is_empty() { default } else { &answer };
        match check(answer) {
            Ok(value) => return Ok(value),
            Err(e) => println!("  {e}"),
        }
    }
}

/// Ask without showing what is typed.
fn ask_password(question: &str) -> Result<String> {
    let _ = Command::new("stty").arg("-echo").status();
    let answer = ask(question);
    let _ = Command::new("stty").arg("echo").status();
    println!();
    answer
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
