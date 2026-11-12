//! `xctl channel`: which stream of updates this system follows.
//!
//! The channels are the tags on the registry: `experimental`, and one
//! channel per minor version and stage (`1.0-stable`, `1.1-beta`,
//! `2.0-alpha`). On top of them, `stable`, `beta` and `alpha` follow the
//! newest version (see `version::FOLLOWING`): xctl works out at each update
//! which channel that is now.
//!
//! By default a system follows the channel its version was built for. The
//! administrator can choose another one; the choice is stored in
//! /etc/xeeonos/channel (admin configuration, kept across updates).

use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use crate::config;
use crate::oci;
use crate::util::{Result, err, need_root};
use crate::version::{
    FOLLOWING, candidates, channel_of, compare, is_valid_channel, read_os_release,
};

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

/// The tag to download for `channel`: the channel itself, or for a
/// following channel (`stable`, `beta`, `alpha`) the channel it is on now.
pub fn resolve(channel: &str) -> Result<String> {
    if !FOLLOWING.contains(&channel) {
        return Ok(channel.to_string());
    }
    resolve_following(channel, &published()?)
}

fn resolve_following(following: &str, published: &[String]) -> Result<String> {
    let mut best: Option<(String, String)> = None; // (channel, version)
    for c in candidates(following, published) {
        // Inside one minor version, the channel with the newest version
        // (e.g. 2.0-stable once 2.0.0 is out, if 2.0-beta is still 2.0.0-beta.9).
        let artifact = oci::Artifact::open(&format!("{}:{c}", config::REGISTRY))?;
        let Some(version) = artifact
            .file_names()
            .iter()
            .filter_map(|n| n.strip_prefix("xeeonos_")?.strip_suffix(".efi"))
            .map(str::to_string)
            .max_by(|a, b| compare(a, b))
        else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, v)| compare(&version, v) == Ordering::Greater)
        {
            best = Some((c, version));
        }
    }
    best.map(|(c, _)| c).ok_or_else(|| {
        err(format!(
            "no version is published yet for the {following} channel"
        ))
    })
}

/// The channels published on the registry (its tags that are channels, not
/// exact versions).
fn published() -> Result<Vec<String>> {
    let mut channels: Vec<String> = oci::tags(config::REGISTRY)?
        .into_iter()
        .filter(|t| is_valid_channel(t))
        .collect();
    channels.sort_by_key(|c| std::cmp::Reverse(order(c)));
    Ok(channels)
}

/// Sort key: newest minor version first, and inside one the most stable
/// first; experimental last.
fn order(channel: &str) -> (u64, u64, u8) {
    let Some((number, stage)) = channel.split_once('-') else {
        return (0, 0, 0);
    };
    let (major, minor) = number.split_once('.').unwrap_or_default();
    let rank = match stage {
        "stable" => 3,
        "beta" => 2,
        _ => 1,
    };
    (
        major.parse().unwrap_or_default(),
        minor.parse().unwrap_or_default(),
        rank,
    )
}

fn describe_following(name: &str) -> &'static str {
    match name {
        "stable" => "the newest stable version",
        "beta" => "the newest beta, or a newer stable version",
        _ => "the newest alpha, or a newer beta or stable version",
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let shown = |c: Option<String>| c.unwrap_or_else(|| "none (development build)".into());
    match args.first().map(String::as_str) {
        None => {
            let channel = current()?;
            match channel.as_deref() {
                Some(f) if FOLLOWING.contains(&f) => {
                    println!("{f} (follows {})", describe_following(f))
                }
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

/// `xctl channel list`: the channels on the registry, the current one marked.
fn list() -> Result<()> {
    let current = current()?;
    let published = published()?;
    let mark = |c: &str| {
        if current.as_deref() == Some(c) {
            "*"
        } else {
            " "
        }
    };
    println!("Following the newest version:");
    for f in FOLLOWING {
        let now = match resolve_following(f, &published) {
            Ok(c) => format!("now {c}"),
            Err(_) => "nothing published yet".into(),
        };
        println!("{} {f:<14}{} ({now})", mark(f), describe_following(f));
    }
    println!("Channels:");
    for c in &published {
        println!("{} {c}", mark(c));
    }
    println!("Switch with: sudo xctl channel set <channel>");
    Ok(())
}

/// `xctl channel set <name>`: only a channel that exists on the registry.
fn set(name: &str) -> Result<()> {
    need_root("channel set")?;
    if !FOLLOWING.contains(&name) && !is_valid_channel(name) {
        return Err(err(format!(
            "unknown channel '{name}' (see xctl channel list)"
        )));
    }
    let published = published()?;
    let exists = if FOLLOWING.contains(&name) {
        !candidates(name, &published).is_empty()
    } else {
        published.iter().any(|c| c == name)
    };
    if !exists {
        return Err(err(format!(
            "the channel '{name}' has no version published (see xctl channel list)"
        )));
    }
    fs::create_dir_all(Path::new(config::CHANNEL_FILE).parent().unwrap())?;
    fs::write(config::CHANNEL_FILE, format!("{name}\n"))?;
    println!("Channel: {name} (takes effect at the next xctl update)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_order() {
        let mut c = vec![
            "experimental",
            "1.0-stable",
            "2.0-alpha",
            "1.1-beta",
            "1.1-stable",
        ];
        c.sort_by_key(|c| std::cmp::Reverse(order(c)));
        assert_eq!(
            c,
            vec![
                "2.0-alpha",
                "1.1-stable",
                "1.1-beta",
                "1.0-stable",
                "experimental"
            ]
        );
    }
}
