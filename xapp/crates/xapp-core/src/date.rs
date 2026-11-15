//! Dates without a date library: today's date and the age of an Arch
//! snapshot, counted in days since 1970-01-01.

use std::time::{SystemTime, UNIX_EPOCH};

/// Days since 1970-01-01 of a calendar date (proleptic Gregorian calendar,
/// Howard Hinnant's "days_from_civil").
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Today, in days since 1970-01-01.
pub fn today() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as i64)
        .unwrap_or(0)
}

/// A date written YYYY-MM-DD or YYYY/MM/DD, in days since 1970-01-01.
pub fn parse(date: &str) -> Option<i64> {
    let mut it = date.trim().split(['-', '/']).map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parse("2000-03-01"), Some(11017));
        assert_eq!(parse("2026/11/13"), parse("2026-11-13"));
        assert_eq!(parse("2026-13-01"), None);
        assert_eq!(parse("nope"), None);
    }
}
