//! `xctl channel`: which stream of updates this system follows.
//!
//! By default a system follows the channel its version was built for. The
//! administrator can choose another one; the choice is stored in
//! /etc/xeeonos/channel (admin configuration, kept across updates).

use std::fs;
use std::path::Path;

use crate::config;
use crate::util::{Result, err, need_root};
use crate::version::{channel_of, channel_of_codename, is_valid_channel, read_os_release};

/// The code names of the minor versions (see the file `codenames`).
const CODENAMES: &str = "/usr/share/xeeonos/codenames";

/// The channel this system follows, if any (development builds have none).
pub fn current() -> Result<Option<String>> {
    if let Ok(chosen) = fs::read_to_string(config::CHANNEL_FILE) {
        let chosen = chosen.trim();
        if !chosen.is_empty() {
            return Ok(Some(chosen.to_string()));
        }
    }
    let os = read_os_release(config::OS_RELEASE)?;
    Ok(os.get("IMAGE_VERSION").and_then(|v| channel_of(v)))
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None => {
            println!(
                "{}",
                current()?.as_deref().unwrap_or("none (development build)")
            );
        }
        Some("--reset") => {
            need_root("channel --reset")?;
            if Path::new(config::CHANNEL_FILE).exists() {
                fs::remove_file(config::CHANNEL_FILE)?;
            }
            println!(
                "Channel: {}",
                current()?.as_deref().unwrap_or("none (development build)")
            );
        }
        Some(name) => {
            // A code name stands for the stable channel of its version.
            let channel = if is_valid_channel(name) {
                name.to_string()
            } else {
                let list = fs::read_to_string(CODENAMES).unwrap_or_default();
                channel_of_codename(&list, name).ok_or_else(|| {
                    err(format!(
                        "unknown channel '{name}' (expected experimental, a channel like \
                         1.0-stable, 1.0-beta, 1.1-alpha, or the code name of a version)"
                    ))
                })?
            };
            need_root("channel")?;
            fs::create_dir_all(Path::new(config::CHANNEL_FILE).parent().unwrap())?;
            fs::write(config::CHANNEL_FILE, format!("{channel}\n"))?;
            println!("Channel: {channel} (takes effect at the next xctl update)");
        }
    }
    Ok(())
}
