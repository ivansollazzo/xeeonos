//! XeeonOS versions and channels.
//!
//! A version looks like `26.11-experimental.42`, `26.12-testing.7` or
//! `26.12.2` (YY.MM.S, a stable release). The part after the first `-` names
//! the channel it was built for.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;

use crate::util::Result;

/// Read `/usr/lib/os-release` (or another file in the same format) into a
/// map of KEY -> value, removing the quotes around values.
pub fn read_os_release(path: &str) -> Result<HashMap<String, String>> {
    let text = fs::read_to_string(path)?;
    Ok(parse_os_release(&text))
}

pub fn parse_os_release(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| {
            (
                k.to_string(),
                v.trim_matches(|c| c == '"' || c == '\'').to_string(),
            )
        })
        .collect()
}

/// The channels, which are also the branches and the tags on the registry:
/// `experimental` (free development), `testing` (what comes next: new
/// features, new Arch snapshots) and `stable` (fixed versions, published by
/// hand).
pub const CHANNELS: [&str; 3] = ["stable", "testing", "experimental"];

/// The channel a version belongs to (its tag on the registry):
///
/// | version                  | channel        |
/// |--------------------------|----------------|
/// | `26.11-experimental.42`  | `experimental` |
/// | `26.12-testing.7`        | `testing`      |
/// | `26.12.2`                | `stable`       |
/// | `26.11-dev.8`, `-rc.3`   | none           |
///
/// `Option<String>` is Rust's way of saying "maybe a value": `Some(..)` or
/// `None`. Builds that are not published have no channel.
pub fn channel_of(version: &str) -> Option<String> {
    let (number, rest) = match version.split_once('-') {
        Some((n, r)) => (n, Some(r)),
        None => (version, None),
    };
    let parts: Vec<&str> = number.split('.').collect();
    if parts.iter().any(|p| !is_number(p)) {
        return None;
    }
    match rest.map(|r| r.split('.').next().unwrap_or_default()) {
        // YY.MM.S
        None if parts.len() == 3 => Some("stable".to_string()),
        Some(stage @ ("experimental" | "testing")) if parts.len() == 2 => Some(stage.to_string()),
        _ => None,
    }
}

/// Is `name` a channel users can choose?
pub fn is_valid_channel(name: &str) -> bool {
    CHANNELS.contains(&name)
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Compare two versions the way people expect: numbers by value, so that
/// `0.1.0-experimental.12` is newer than `0.1.0-experimental.9`.
///
/// First the release (`1.2.0`), then the part after `-`: a release is
/// newer than its own pre-releases (`1.2.0` > `1.2.0-beta.5`).
///
/// Each part is cut into runs of digits and runs of other characters; runs
/// of digits compare as numbers, the rest as text. A part that is a prefix
/// of the other is older (`1.0` < `1.0.1`).
pub fn compare(a: &str, b: &str) -> Ordering {
    let (release_a, pre_a) = split_pre(a);
    let (release_b, pre_b) = split_pre(b);
    compare_runs(release_a, release_b).then_with(|| match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(pa), Some(pb)) => compare_runs(pa, pb),
    })
}

/// `1.2.0-beta.5` -> (`1.2.0`, Some(`beta.5`)).
fn split_pre(v: &str) -> (&str, Option<&str>) {
    match v.split_once('-') {
        Some((release, pre)) => (release, Some(pre)),
        None => (v, None),
    }
}

fn compare_runs(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        match (a.is_empty(), b.is_empty()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let (ra, rest_a) = split_run(a);
        let (rb, rest_b) = split_run(b);
        let both_digits = ra.as_bytes()[0].is_ascii_digit() && rb.as_bytes()[0].is_ascii_digit();
        let ord = if both_digits {
            // Compare numbers without parsing (no overflow on long numbers):
            // without leading zeros, the longer number is bigger.
            let (na, nb) = (ra.trim_start_matches('0'), rb.trim_start_matches('0'));
            na.len().cmp(&nb.len()).then(na.cmp(nb))
        } else {
            ra.cmp(rb)
        };
        if ord != Ordering::Equal {
            return ord;
        }
        (a, b) = (rest_a, rest_b);
    }
}

/// Split off the first run of digits, or of non-digits, from a non-empty string.
fn split_run(s: &str) -> (&str, &str) {
    let digits = s.as_bytes()[0].is_ascii_digit();
    let end = s
        .find(|c: char| c.is_ascii_digit() != digits)
        .unwrap_or(s.len());
    s.split_at(end)
}

// Tests: run them with `cargo test`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels() {
        assert_eq!(
            channel_of("26.11-experimental.42").as_deref(),
            Some("experimental")
        );
        assert_eq!(channel_of("26.12-testing.7").as_deref(), Some("testing"));
        assert_eq!(channel_of("26.12.2").as_deref(), Some("stable"));
        assert_eq!(channel_of("27.1.1").as_deref(), Some("stable"));
        assert_eq!(channel_of("26.11-dev.8"), None);
        assert_eq!(channel_of("26.11-rc.3"), None);
        assert_eq!(channel_of("26.12"), None);
        assert_eq!(channel_of("26.12.1-testing.3"), None);
        assert_eq!(channel_of("unknown"), None);
    }

    #[test]
    fn valid_channels() {
        for ok in ["experimental", "testing", "stable"] {
            assert!(is_valid_channel(ok), "{ok}");
        }
        for bad in ["", "beta", "1.0-stable", "stable/../", "Stable"] {
            assert!(!is_valid_channel(bad), "{bad}");
        }
    }

    #[test]
    fn version_order() {
        use Ordering::*;
        assert_eq!(
            compare("0.1.0-experimental.12", "0.1.0-experimental.9"),
            Greater
        );
        assert_eq!(
            compare("0.1.0-experimental.12", "0.1.0-experimental.12"),
            Equal
        );
        assert_eq!(compare("0.1.0-dev.8", "0.1.0-dev.8.1"), Less);
        assert_eq!(compare("1.10.0", "1.9.9"), Greater);
        assert_eq!(compare("2.0.0", "10.0.0"), Less);
        assert_eq!(compare("26.12.10", "26.12.9"), Greater);
        assert_eq!(compare("27.1.1", "26.12.3"), Greater);
        assert_eq!(compare("26.12-testing.3", "26.11-testing.40"), Greater);
        // A release is newer than its pre-releases, older than the next ones.
        assert_eq!(compare("1.1.0", "1.1.0-beta.5"), Greater);
        assert_eq!(compare("1.1.0-alpha.9", "1.1.0-beta.1"), Less);
        assert_eq!(compare("1.1.0", "1.1.1-beta.1"), Less);
        assert_eq!(compare("2.0.0-beta.3", "1.1.1-beta.1"), Greater);
    }

    #[test]
    fn os_release() {
        let m = parse_os_release(
            "NAME=\"XeeonOS\"\n# comment\nIMAGE_VERSION='0.1.0-dev.1'\nID=xeeonos\n",
        );
        assert_eq!(m["NAME"], "XeeonOS");
        assert_eq!(m["IMAGE_VERSION"], "0.1.0-dev.1");
        assert_eq!(m["ID"], "xeeonos");
    }
}
