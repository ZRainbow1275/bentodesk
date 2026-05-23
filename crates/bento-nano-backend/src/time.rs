//! T-Q1 — hand-rolled UTC time helpers (replaces `chrono` per spec §8.1).
//!
//! 1.x backend modules use `chrono::Utc::now().to_rfc3339()` for
//! `last_modified` / `captured_at` / `added_at` timestamps and
//! `chrono::DateTime::parse_from_rfc3339(s).signed_duration_since(now)` for
//! "is this older than N days?" predicates. Spec §8.1 forbids `chrono`
//! (proc-macro deps + locale tables push binary size beyond the 6 MB ceiling).
//!
//! This module covers exactly the operations the backend needs:
//!
//! - [`now_rfc3339`] — emit `YYYY-MM-DDTHH:MM:SS.mmmZ` (millisecond precision,
//!   always UTC, never offset)
//! - [`now_compact_rfc3339`] — emit `YYYYMMDDTHHMMSSmmmZ` for filesystem-safe
//!   sortable filenames (used by `timeline::checkpoint`)
//! - [`parse_rfc3339_to_unix_secs`] — strict parser for the `Z`/`+00:00`
//!   forms 1.x writes, returns Unix epoch seconds (no offset bookkeeping —
//!   nano stores everything as UTC)
//! - [`age_seconds_since`] — compute `now - parsed` in whole seconds; safe
//!   wrapper for the "older than N days" predicate
//!
//! Out of scope: timezone conversion, locale formatting, leap-second
//! handling, dates before 1970, dates after 9999. These are not used by any
//! 1.x consumer in Wave 5d.

use core::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the RFC3339 parser.
#[derive(Debug)]
pub enum TimeError {
    /// String was shorter than the smallest valid RFC3339 form
    /// (`YYYY-MM-DDTHH:MM:SSZ` = 20 chars).
    TooShort { got: usize },
    /// String contained a non-ASCII byte where ASCII was required.
    NotAscii,
    /// A character that should have been a fixed delimiter (`-`, `T`, `:`, `Z`)
    /// did not match.
    BadDelimiter { at: usize, expected: u8, got: u8 },
    /// A digit field contained a non-digit character.
    BadDigit { at: usize, got: u8 },
    /// One of the calendar fields (month/day/hour/minute/second) was outside
    /// its valid range.
    OutOfRange { field: &'static str, value: u32 },
    /// Trailing characters beyond the parsed timestamp form (allowed only for
    /// the optional `.fff` fractional-second tail and `Z` / `+00:00`).
    TrailingGarbage { at: usize },
    /// System clock returned a `SystemTime` before UNIX_EPOCH. Should never
    /// happen in production; surfaced as an error so we never silently
    /// underflow.
    ClockBeforeEpoch,
}

impl core::fmt::Display for TimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort { got } => write!(f, "rfc3339 too short: {got} bytes"),
            Self::NotAscii => f.write_str("rfc3339 contains non-ascii"),
            Self::BadDelimiter { at, expected, got } => write!(
                f,
                "rfc3339 expected delimiter {:?} at {at}, got {:?}",
                *expected as char, *got as char
            ),
            Self::BadDigit { at, got } => {
                write!(f, "rfc3339 expected digit at {at}, got {:?}", *got as char)
            }
            Self::OutOfRange { field, value } => write!(f, "rfc3339 {field} out of range: {value}"),
            Self::TrailingGarbage { at } => write!(f, "rfc3339 trailing garbage at byte {at}"),
            Self::ClockBeforeEpoch => f.write_str("system clock returned a time before UNIX_EPOCH"),
        }
    }
}

impl core::error::Error for TimeError {}

// ─── "now" emitters ──────────────────────────────────────────────────

/// Current UTC instant formatted as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
///
/// Always UTC, always millisecond precision, never an offset other than `Z`.
/// Matches `chrono::Utc::now().to_rfc3339()` to within the millisecond
/// precision the 1.x callers expected (chrono defaults to fractional seconds
/// up to nanos; we deliberately truncate to ms because every `last_modified`
/// / `added_at` field in the layout schema is sub-second-irrelevant).
///
/// Falls back to the UNIX epoch if the system clock is before 1970 — that
/// case is impossible on a healthy Windows install but we never panic.
pub fn now_rfc3339() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => format_rfc3339_millis(d),
        Err(_) => format_rfc3339_millis(Duration::ZERO),
    }
}

/// Current UTC instant formatted as `YYYYMMDDTHHMMSSmmmZ` — filesystem-safe,
/// lexicographically sortable, no separators. Used by
/// `timeline::checkpoint::new_checkpoint_id`.
pub fn now_compact_rfc3339() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let parts = unix_to_components(d.as_secs() as i64, d.subsec_millis());
    let mut s = String::with_capacity(20);
    let _ = write!(
        s,
        "{:04}{:02}{:02}T{:02}{:02}{:02}{:03}Z",
        parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second, parts.millis
    );
    s
}

/// Format a `SystemTime` (e.g. `metadata.modified()`) as
/// `YYYY-MM-DDTHH:MM:SS.mmmZ`. Returns the empty string when the system
/// time is before UNIX_EPOCH (matches 1.x `metadata.modified().ok()`
/// fallback shape).
pub fn system_time_to_rfc3339(t: SystemTime) -> String {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => format_rfc3339_millis(d),
        Err(_) => String::new(),
    }
}

/// Format a duration-since-epoch as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn format_rfc3339_millis(d: Duration) -> String {
    let parts = unix_to_components(d.as_secs() as i64, d.subsec_millis());
    let mut s = String::with_capacity(24);
    let _ = write!(
        s,
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second, parts.millis
    );
    s
}

// ─── Parser ──────────────────────────────────────────────────────────

/// Parse an RFC3339 string into Unix epoch seconds.
///
/// Accepted forms (matching what 1.x writes):
///
/// - `YYYY-MM-DDTHH:MM:SS` + `Z`
/// - `YYYY-MM-DDTHH:MM:SS.fff` + `Z` (fraction up to 9 digits, truncated)
/// - `YYYY-MM-DDTHH:MM:SS+00:00` (only `+00:00` / `-00:00` accepted —
///   nano backend stores everything as UTC, so any non-zero offset is a
///   bug in the producer)
///
/// Rejected: lower-case `t`/`z`, missing seconds, week dates, durations.
pub fn parse_rfc3339_to_unix_secs(s: &str) -> Result<i64, TimeError> {
    if !s.is_ascii() {
        return Err(TimeError::NotAscii);
    }
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return Err(TimeError::TooShort { got: bytes.len() });
    }

    // Date: YYYY-MM-DD
    let year = parse_uint(bytes, 0, 4)? as i64;
    expect(bytes, 4, b'-')?;
    let month = parse_uint(bytes, 5, 2)?;
    expect(bytes, 7, b'-')?;
    let day = parse_uint(bytes, 8, 2)?;
    expect(bytes, 10, b'T')?;
    let hour = parse_uint(bytes, 11, 2)?;
    expect(bytes, 13, b':')?;
    let minute = parse_uint(bytes, 14, 2)?;
    expect(bytes, 16, b':')?;
    let second = parse_uint(bytes, 17, 2)?;

    if !(1..=12).contains(&month) {
        return Err(TimeError::OutOfRange {
            field: "month",
            value: month,
        });
    }
    if !(1..=days_in_month(year, month)).contains(&day) {
        return Err(TimeError::OutOfRange {
            field: "day",
            value: day,
        });
    }
    if hour > 23 {
        return Err(TimeError::OutOfRange {
            field: "hour",
            value: hour,
        });
    }
    if minute > 59 {
        return Err(TimeError::OutOfRange {
            field: "minute",
            value: minute,
        });
    }
    if second > 59 {
        return Err(TimeError::OutOfRange {
            field: "second",
            value: second,
        });
    }

    // Optional `.fff` fractional + mandatory `Z` or `+00:00` / `-00:00`.
    let mut idx = 19;
    if idx < bytes.len() && bytes[idx] == b'.' {
        idx += 1;
        let start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == start {
            return Err(TimeError::BadDigit {
                at: idx,
                got: bytes.get(idx).copied().unwrap_or(0),
            });
        }
    }

    if idx >= bytes.len() {
        return Err(TimeError::TooShort { got: bytes.len() });
    }
    match bytes[idx] {
        b'Z' => {
            if idx + 1 != bytes.len() {
                return Err(TimeError::TrailingGarbage { at: idx + 1 });
            }
        }
        b'+' | b'-' => {
            // Only +00:00 / -00:00 accepted.
            if idx + 6 != bytes.len() {
                return Err(TimeError::TrailingGarbage { at: idx + 6 });
            }
            let off_h = parse_uint(bytes, idx + 1, 2)?;
            expect(bytes, idx + 3, b':')?;
            let off_m = parse_uint(bytes, idx + 4, 2)?;
            if off_h != 0 || off_m != 0 {
                return Err(TimeError::OutOfRange {
                    field: "offset",
                    value: off_h * 100 + off_m,
                });
            }
        }
        other => {
            return Err(TimeError::BadDelimiter {
                at: idx,
                expected: b'Z',
                got: other,
            });
        }
    }

    Ok(components_to_unix_secs(
        year,
        month as i32,
        day as i32,
        hour as i32,
        minute as i32,
        second as i32,
    ))
}

/// Whole-second age between `iso` and "now". Returns `0` for empty input or
/// for parse failures (the 1.x consumers swallow parse errors the same way,
/// and the resulting "0 seconds old" never matches any "older than N days"
/// predicate so the rule is silently skipped — same behaviour as 1.x).
pub fn age_seconds_since(iso: &str) -> i64 {
    if iso.is_empty() {
        return 0;
    }
    let parsed = match parse_rfc3339_to_unix_secs(iso) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (now - parsed).max(0)
}

/// Whole-day age between `iso` and "now". Convenience wrapper over
/// [`age_seconds_since`] for the rules-engine `CreatedBefore { days_ago }`
/// predicates.
pub fn age_days_since(iso: &str) -> i64 {
    age_seconds_since(iso) / 86_400
}

// ─── Internals: civil-date arithmetic (Howard Hinnant algorithm) ─────

#[derive(Debug, Clone, Copy)]
struct DateComponents {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
}

/// Parse `n` ASCII digits starting at `bytes[at]` into a `u32`.
fn parse_uint(bytes: &[u8], at: usize, n: usize) -> Result<u32, TimeError> {
    if at + n > bytes.len() {
        return Err(TimeError::TooShort { got: bytes.len() });
    }
    let mut v: u32 = 0;
    for i in 0..n {
        let b = bytes[at + i];
        if !b.is_ascii_digit() {
            return Err(TimeError::BadDigit { at: at + i, got: b });
        }
        v = v.saturating_mul(10).saturating_add(u32::from(b - b'0'));
    }
    Ok(v)
}

/// Assert a single ASCII byte at the given position.
fn expect(bytes: &[u8], at: usize, expected: u8) -> Result<(), TimeError> {
    if at >= bytes.len() {
        return Err(TimeError::TooShort { got: bytes.len() });
    }
    if bytes[at] != expected {
        return Err(TimeError::BadDelimiter {
            at,
            expected,
            got: bytes[at],
        });
    }
    Ok(())
}

/// Whether the given proleptic Gregorian year is a leap year.
fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in the given month/year. Caller is responsible for the month range.
fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Convert civil (year, month, day) to days since 1970-01-01 using Howard
/// Hinnant's algorithm. Public-domain reference implementation; correct for
/// any proleptic Gregorian date that fits in i64.
fn days_from_civil(y: i64, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + (d as u64) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

/// Inverse: days since 1970-01-01 → (year, month, day) per Hinnant.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Convert civil (Y/M/D HH:MM:SS) to Unix epoch seconds.
fn components_to_unix_secs(y: i64, m: i32, d: i32, hh: i32, mm: i32, ss: i32) -> i64 {
    let days = days_from_civil(y, m, d);
    days * 86_400 + (hh as i64) * 3_600 + (mm as i64) * 60 + ss as i64
}

/// Convert Unix epoch seconds (with milliseconds carried separately) to
/// civil components.
fn unix_to_components(secs: i64, millis: u32) -> DateComponents {
    // Handle negative seconds by floor-dividing toward minus infinity.
    let day_secs = 86_400_i64;
    let mut days = secs.div_euclid(day_secs);
    let mut sod = secs.rem_euclid(day_secs);
    if sod < 0 {
        sod += day_secs;
        days -= 1;
    }
    let (year, month, day) = civil_from_days(days);
    let hour = (sod / 3_600) as u32;
    let minute = ((sod % 3_600) / 60) as u32;
    let second = (sod % 60) as u32;
    DateComponents {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_known_dates() {
        // Spot-checked via independent epoch-converter; values are the
        // canonical Unix seconds for each civil UTC instant.
        for (s, secs) in [
            ("1970-01-01T00:00:00Z", 0_i64),
            ("2026-04-30T12:00:00Z", 1_777_550_400),
            ("2026-01-01T00:00:00Z", 1_767_225_600),
            ("2024-02-29T00:00:00Z", 1_709_164_800), // leap day
        ] {
            let got = parse_rfc3339_to_unix_secs(s).expect(s);
            assert_eq!(got, secs, "{s} => {got}, expected {secs}");
        }
    }

    #[test]
    fn fractional_seconds_are_accepted_and_truncated() {
        let a = parse_rfc3339_to_unix_secs("2026-04-30T12:00:00.123Z").unwrap();
        let b = parse_rfc3339_to_unix_secs("2026-04-30T12:00:00Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn explicit_zero_offset_accepted() {
        let a = parse_rfc3339_to_unix_secs("2026-04-30T12:00:00+00:00").unwrap();
        let b = parse_rfc3339_to_unix_secs("2026-04-30T12:00:00Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn nonzero_offset_rejected() {
        assert!(parse_rfc3339_to_unix_secs("2026-04-30T12:00:00+05:00").is_err());
    }

    #[test]
    fn lowercase_z_rejected() {
        assert!(parse_rfc3339_to_unix_secs("2026-04-30T12:00:00z").is_err());
    }

    #[test]
    fn now_rfc3339_round_trips_through_parser() {
        let s = now_rfc3339();
        // `s` length is 24: YYYY-MM-DDTHH:MM:SS.mmmZ
        assert_eq!(s.len(), 24, "got {s:?}");
        assert!(s.ends_with('Z'));
        let parsed = parse_rfc3339_to_unix_secs(&s).expect("self-emit must parse");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(
            (now - parsed).abs() <= 2,
            "drift {} too large",
            now - parsed
        );
    }

    #[test]
    fn now_compact_is_filename_safe_and_sortable() {
        let s = now_compact_rfc3339();
        // Format: YYYYMMDDTHHMMSSmmmZ → 4+2+2+1+2+2+2+3+1 = 19 chars.
        assert_eq!(s.len(), 19, "got {s:?}");
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()), "got {s:?}");
        assert!(s.ends_with('Z'));
    }

    #[test]
    fn invalid_month_is_out_of_range() {
        assert!(matches!(
            parse_rfc3339_to_unix_secs("2026-13-01T00:00:00Z"),
            Err(TimeError::OutOfRange { field: "month", .. })
        ));
    }

    #[test]
    fn invalid_day_is_out_of_range() {
        assert!(matches!(
            parse_rfc3339_to_unix_secs("2026-02-30T00:00:00Z"),
            Err(TimeError::OutOfRange { field: "day", .. })
        ));
    }

    #[test]
    fn age_days_since_returns_zero_for_empty_input() {
        assert_eq!(age_days_since(""), 0);
    }

    #[test]
    fn age_days_since_returns_zero_for_garbage_input() {
        assert_eq!(age_days_since("not a date"), 0);
    }

    #[test]
    fn age_seconds_is_nonnegative_for_past() {
        let s = "2020-01-01T00:00:00Z";
        let age = age_seconds_since(s);
        assert!(age > 0, "expected positive age, got {age}");
    }

    #[test]
    fn days_in_month_handles_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2025, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29); // century divisible by 400
        assert_eq!(days_in_month(1900, 2), 28); // century not divisible by 400
    }

    #[test]
    fn civil_from_days_round_trip() {
        // 2026-04-30 → days_from_civil → civil_from_days → (2026, 4, 30).
        let d = days_from_civil(2026, 4, 30);
        let (y, m, day) = civil_from_days(d);
        assert_eq!((y, m, day), (2026, 4, 30));
    }

    #[test]
    fn too_short_input_is_rejected() {
        assert!(parse_rfc3339_to_unix_secs("2026-04-30").is_err());
    }
}
