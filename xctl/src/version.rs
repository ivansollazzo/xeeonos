//! XeeonOS versions and channels.
//!
//! A version looks like `0.1.0-experimental.42`, `1.2.0-beta.7` or `1.2.0`.
//! The part after the first `-` names the channel it was built for.

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

/// The channel a version belongs to, which is also its tag on the registry:
///
/// | version                 | channel        |
/// |-------------------------|----------------|
/// | `0.1.0-experimental.42` | `experimental` |
/// | `1.2.0-beta.7`          | `1.2-beta`     |
/// | `1.2.3`                 | `1.2-stable`   |
/// | `0.1.0-dev.8`           | none           |
///
/// `Option<String>` is Rust's way of saying "maybe a value": `Some(..)` or
/// `None`. Development builds have no channel.
pub fn channel_of(version: &str) -> Option<String> {
    let (number, rest) = match version.split_once('-') {
        Some((n, r)) => (n, Some(r)),
        None => (version, None),
    };
    let mut parts = number.split('.');
    let (major, minor) = (parts.next()?, parts.next()?);
    if !is_number(major) || !is_number(minor) {
        return None;
    }
    match rest.map(|r| r.split('.').next().unwrap_or_default()) {
        None => Some(format!("{major}.{minor}-stable")),
        Some("experimental") => Some("experimental".to_string()),
        Some(pre @ ("alpha" | "beta")) => Some(format!("{major}.{minor}-{pre}")),
        _ => None,
    }
}

/// Is `name` a channel users can choose? `experimental`, or a release
/// channel `<major>.<minor>-alpha|beta|stable` (e.g. `1.0-stable`): the
/// same name as its branch and its tag on the registry.
pub fn is_valid_channel(name: &str) -> bool {
    if name == "experimental" {
        return true;
    }
    let Some((number, stage)) = name.split_once('-') else {
        return false;
    };
    let Some((major, minor)) = number.split_once('.') else {
        return false;
    };
    is_number(major) && is_number(minor) && matches!(stage, "alpha" | "beta" | "stable")
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// The code names of the minor versions (file `codenames` in the
/// repository, /usr/share/xeeonos/codenames on the system): `1.0 maverick`.
/// The stable channel of the version a code name stands for, if known.
pub fn channel_of_codename(list: &str, name: &str) -> Option<String> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(char::is_whitespace))
        .find(|(_, n)| n.trim() == name)
        .map(|(minor, _)| format!("{minor}-stable"))
        .filter(|c| is_valid_channel(c))
}

/// Compare two versions the way people expect: numbers by value, so that
/// `0.1.0-experimental.12` is newer than `0.1.0-experimental.9`.
///
/// Both strings are cut into runs of digits and runs of other characters;
/// runs of digits compare as numbers, the rest as text. A version that is a
/// prefix of the other is older (`1.0` < `1.0.1`).
pub fn compare(a: &str, b: &str) -> Ordering {
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
            channel_of("0.1.0-experimental.42").as_deref(),
            Some("experimental")
        );
        assert_eq!(channel_of("1.2.0-beta.7").as_deref(), Some("1.2-beta"));
        assert_eq!(channel_of("1.2.0-alpha.3").as_deref(), Some("1.2-alpha"));
        assert_eq!(channel_of("1.2.3").as_deref(), Some("1.2-stable"));
        assert_eq!(channel_of("1.0.0").as_deref(), Some("1.0-stable"));
        assert_eq!(channel_of("0.1.0-dev.8"), None);
        assert_eq!(channel_of("unknown"), None);
    }

    #[test]
    fn valid_channels() {
        for ok in ["experimental", "1.0-beta", "12.3-stable", "0.1-alpha"] {
            assert!(is_valid_channel(ok), "{ok}");
        }
        for bad in [
            "",
            "beta",
            "x-beta",
            "1.x-beta",
            "1-beta",
            "1.0-gamma",
            "1.0-beta/../",
            "a.0-beta",
            "1.0.0-stable",
        ] {
            assert!(!is_valid_channel(bad), "{bad}");
        }
    }

    #[test]
    fn codenames() {
        let list = "# comment\n# 0.9 old\n1.0 maverick\n1.1  dodo\n";
        assert_eq!(
            channel_of_codename(list, "maverick").as_deref(),
            Some("1.0-stable")
        );
        assert_eq!(
            channel_of_codename(list, "dodo").as_deref(),
            Some("1.1-stable")
        );
        assert_eq!(channel_of_codename(list, "old"), None);
        assert_eq!(channel_of_codename(list, "nope"), None);
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
