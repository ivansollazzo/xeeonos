//! Comparing application versions ("146.0.1", "2.0-beta3"): numbers compare
//! as numbers, words as text, more parts is newer (1.0.1 > 1.0).

use std::cmp::Ordering;

/// One piece of a version: a number or a word.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Part {
    // Declared first, so a word sorts before a number: 1.0beta < 1.0.1.
    Word(String),
    Number(u64),
}

fn parts(v: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, out: &mut Vec<Part>| {
        if !current.is_empty() {
            out.push(match current.parse() {
                Ok(n) => Part::Number(n),
                Err(_) => Part::Word(current.to_lowercase()),
            });
            current.clear();
        }
    };
    for c in v.chars() {
        if !c.is_ascii_alphanumeric() {
            flush(&mut current, &mut out);
            continue;
        }
        // A change between digits and letters also starts a new piece.
        if let Some(last) = current.chars().last()
            && last.is_ascii_digit() != c.is_ascii_digit()
        {
            flush(&mut current, &mut out);
        }
        current.push(c);
    }
    flush(&mut current, &mut out);
    out
}

/// Compare two versions.
pub fn compare(a: &str, b: &str) -> Ordering {
    parts(a).cmp(&parts(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order() {
        assert_eq!(compare("146.0", "145.9.9"), Ordering::Greater);
        assert_eq!(compare("1.0.1", "1.0"), Ordering::Greater);
        assert_eq!(compare("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare("2.0", "2.0"), Ordering::Equal);
        assert_eq!(compare("1.0beta", "1.0.1"), Ordering::Less);
        assert_eq!(compare("2.42", "2.41+r9"), Ordering::Greater);
    }
}
