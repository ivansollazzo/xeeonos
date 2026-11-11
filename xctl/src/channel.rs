//! `xctl channel`: which stream of updates this system follows.
//!
//! By default a system follows the channel its version was built for. The
//! administrator can choose another one; the choice is stored in
//! /etc/xeeonos/channel (admin configuration, kept across updates).

use std::fs;
use std::path::Path;

use crate::config;
use crate::util::{Result, err, need_root};
use crate::version::{channel_of, is_valid_channel, read_os_release};

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
        Some(name) if is_valid_channel(name) => {
            need_root("channel")?;
            fs::create_dir_all(Path::new(config::CHANNEL_FILE).parent().unwrap())?;
            fs::write(config::CHANNEL_FILE, format!("{name}\n"))?;
            println!("Channel: {name} (takes effect at the next xctl update)");
        }
        Some(name) => {
            return Err(err(format!(
                "unknown channel '{name}' (expected experimental or <N>.x-alpha, <N>.x-beta, <N>.x-stable)"
            )));
        }
    }
    Ok(())
}
