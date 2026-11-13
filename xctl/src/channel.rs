//! `xctl channel`: which stream of updates this system follows.
//!
//! The channels are the tags on the registry, and the branches they are
//! built from: `stable` (fixed versions YY.MM.S, published by hand),
//! `testing` (what comes next) and `experimental` (free development).
//!
//! By default a system follows the channel its version was built for. The
//! administrator can choose another one; the choice is stored in
//! /etc/xeeonos/channel (admin configuration, kept across updates).

use std::fs;
use std::path::Path;

use crate::config;
use crate::oci;
use crate::util::{Result, err, need_root};
use crate::version::{CHANNELS, channel_of, is_valid_channel, read_os_release};

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

/// The tag to download for `channel`: the channel itself.
pub fn resolve(channel: &str) -> Result<String> {
    if !is_valid_channel(channel) {
        return Err(err(format!(
            "unknown channel '{channel}' (see xctl channel list; reset with sudo xctl channel reset)"
        )));
    }
    Ok(channel.to_string())
}

/// The channels with a version published on the registry.
fn published() -> Result<Vec<String>> {
    let tags = oci::tags(config::REGISTRY)?;
    Ok(CHANNELS
        .iter()
        .filter(|c| tags.iter().any(|t| t == *c))
        .map(|c| c.to_string())
        .collect())
}

fn describe(channel: &str) -> &'static str {
    match channel {
        "stable" => "fixed versions (YY.MM.S)",
        "testing" => "what comes next: new features and Arch snapshots",
        _ => "free development, may break",
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let shown = |c: Option<String>| c.unwrap_or_else(|| "none (development build)".into());
    match args.first().map(String::as_str) {
        None => {
            let channel = current()?;
            match channel.as_deref() {
                Some(c) if is_valid_channel(c) => println!("{c} ({})", describe(c)),
                _ => println!("{}", shown(channel)),
            }
        }
        Some("list") => list()?,
        Some("set") => {
            let name = args
                .get(1)
                .ok_or_else(|| err("usage: xctl channel set <channel> (see xctl channel list)"))?;
            set(name)?;
        }
        Some("reset") => {
            need_root("channel reset")?;
            if Path::new(config::CHANNEL_FILE).exists() {
                fs::remove_file(config::CHANNEL_FILE)?;
            }
            println!("Channel: {}", shown(current()?));
        }
        Some(other) => {
            return Err(err(format!(
                "unknown channel command '{other}' (expected list, set <channel> or reset)"
            )));
        }
    }
    Ok(())
}

/// `xctl channel list`: the channels, the current one marked.
fn list() -> Result<()> {
    let current = current()?;
    let published = published()?;
    for c in CHANNELS {
        let mark = if current.as_deref() == Some(c) {
            "*"
        } else {
            " "
        };
        let state = if published.iter().any(|p| p == c) {
            ""
        } else {
            " (nothing published yet)"
        };
        println!("{mark} {c:<14}{}{state}", describe(c));
    }
    println!("Switch with: sudo xctl channel set <channel>");
    Ok(())
}

/// `xctl channel set <name>`: only a channel that exists on the registry.
fn set(name: &str) -> Result<()> {
    need_root("channel set")?;
    if !is_valid_channel(name) {
        return Err(err(format!(
            "unknown channel '{name}' (see xctl channel list)"
        )));
    }
    if !published()?.iter().any(|c| c == name) {
        return Err(err(format!(
            "the channel '{name}' has no version published (see xctl channel list)"
        )));
    }
    fs::create_dir_all(Path::new(config::CHANNEL_FILE).parent().unwrap())?;
    fs::write(config::CHANNEL_FILE, format!("{name}\n"))?;
    println!("Channel: {name} (takes effect at the next xctl update)");
    Ok(())
}
