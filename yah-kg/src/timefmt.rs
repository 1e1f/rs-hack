//! @arch:layer(kg)
//! @arch:role(util)
//!
//! Tiny RFC 3339 (UTC, always `Z`) formatter and parser for `@yah:at`
//! timestamps. We intentionally don't pull in `chrono` / `time` — the
//! domain is fixed (always UTC, no fractional seconds, no offsets), so
//! a few dozen lines of arithmetic suffice.

/// Format a unix-seconds timestamp as RFC 3339 UTC, e.g. `2026-04-29T12:00:00Z`.
pub fn format_rfc3339(unix_secs: u64) -> String {
    let secs = unix_secs % 60;
    let mins = (unix_secs / 60) % 60;
    let hours = (unix_secs / 3600) % 24;
    let days = unix_secs / 86_400;
    let (y, m, d) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, mins, secs
    )
}

/// Parse an RFC 3339 UTC timestamp back to unix seconds. Accepts the
/// exact shape `format_rfc3339` produces (`YYYY-MM-DDTHH:MM:SSZ`); other
/// RFC 3339 variants (timezone offsets, fractional seconds) return
/// `None`. Strict-by-design — agents always read what we wrote.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    // Expected shape: 20 chars exactly.
    if s.len() != 20 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let y: u32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    let hh: u64 = s[11..13].parse().ok()?;
    let mm: u64 = s[14..16].parse().ok()?;
    let ss: u64 = s[17..19].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    let days = ymd_to_days(y, m, d)?;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// Howard Hinnant's days-from-civil. `z` is days since 1970-01-01.
fn days_to_ymd(z: u64) -> (u32, u32, u32) {
    let z = z as i64 + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

/// Inverse: civil date → days since 1970-01-01.
fn ymd_to_days(y: u32, m: u32, d: u32) -> Option<u64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = y as i64 - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as u64;
    let m = m as u64;
    let d = d as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    if days < 0 {
        None
    } else {
        Some(days as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_known_dates() {
        // 2026-04-29T12:00:00Z — the example from the design discussion.
        let s = "2026-04-29T12:00:00Z";
        let secs = parse_rfc3339(s).expect("parses");
        assert_eq!(format_rfc3339(secs), s);
    }

    #[test]
    fn epoch_round_trip() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse_rfc3339("2026-04-29 12:00:00Z").is_none()); // space, not T
        assert!(parse_rfc3339("2026-04-29T12:00:00").is_none()); // missing Z
        assert!(parse_rfc3339("2026-04-29T12:00:00+00:00").is_none()); // offset
        assert!(parse_rfc3339("2026-13-29T12:00:00Z").is_none()); // bad month
    }
}
