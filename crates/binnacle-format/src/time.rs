//! UTC instants, written out as local time or relative to now.

use std::time::Duration;

const NANOS: i128 = 1_000_000_000;

/// The local zone: its offset from UTC, in seconds east, at each instant.
/// A closure `Fn(Timestamp) -> i32` is one.
pub trait Zone {
    fn offset_at(&self, at: Timestamp) -> i32;
}

/// A zone with one offset at every instant: `FixedOffset(0)` is UTC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FixedOffset(pub i32);

impl Zone for FixedOffset {
    fn offset_at(&self, _: Timestamp) -> i32 {
        self.0
    }
}

impl<F: Fn(Timestamp) -> i32> Zone for F {
    fn offset_at(&self, at: Timestamp) -> i32 {
        self(at)
    }
}

/// A UTC instant with nanosecond precision: seconds since the Unix epoch
/// and the nanoseconds past them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    secs: i64,
    nanos: u32,
}

impl Timestamp {
    pub const UNIX_EPOCH: Timestamp = Timestamp { secs: 0, nanos: 0 };

    /// Saturates outside the `i64` seconds range.
    pub fn from_unix_nanos(nanos: i128) -> Timestamp {
        let secs = nanos.div_euclid(NANOS);
        match i64::try_from(secs) {
            Ok(secs) => Timestamp {
                secs,
                nanos: nanos.rem_euclid(NANOS) as u32,
            },
            Err(_) if secs < 0 => Timestamp {
                secs: i64::MIN,
                nanos: 0,
            },
            Err(_) => Timestamp {
                secs: i64::MAX,
                nanos: (NANOS - 1) as u32,
            },
        }
    }

    pub fn from_unix_millis(millis: i64) -> Timestamp {
        Timestamp::from_unix_nanos(i128::from(millis) * 1_000_000)
    }

    pub fn from_unix_secs(secs: i64) -> Timestamp {
        Timestamp { secs, nanos: 0 }
    }

    pub fn unix_nanos(self) -> i128 {
        i128::from(self.secs) * NANOS + i128::from(self.nanos)
    }

    /// Nanoseconds since the Unix epoch as an `i64`, which spans about 1677
    /// to 2262: clamped to that range, as a C `DATETIME` takes it.
    pub fn unix_nanos_i64_saturating(self) -> i64 {
        let nanos = self.unix_nanos();
        i64::try_from(nanos).unwrap_or(if nanos < 0 { i64::MIN } else { i64::MAX })
    }

    pub fn unix_secs(self) -> i64 {
        self.secs
    }

    /// This instant `d` later, saturating.
    pub fn saturating_add(self, d: Duration) -> Timestamp {
        let d = i128::try_from(d.as_nanos()).unwrap_or(i128::MAX);
        Timestamp::from_unix_nanos(self.unix_nanos().saturating_add(d))
    }

    /// Reads an RFC 3339 instant: `2026-10-29T14:03:00Z`, with optional
    /// fractional seconds and a `Z` or `±HH:MM` offset.
    pub fn parse_rfc3339(s: &str) -> Option<Timestamp> {
        let b = s.as_bytes();
        let num = |r: std::ops::Range<usize>| -> Option<i64> {
            let part = s.get(r)?;
            part.bytes()
                .all(|c| c.is_ascii_digit())
                .then(|| part.parse().ok())?
        };
        if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
            return None;
        }
        if !matches!(b[10], b'T' | b't' | b' ') {
            return None;
        }
        let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        if hour > 23 || minute > 59 {
            return None;
        }
        if second > 60 {
            return None;
        }
        let mut at = 19;
        let mut nanos: i128 = 0;
        if b[at] == b'.' {
            let start = at + 1;
            at = start;
            while at < b.len() && b[at].is_ascii_digit() {
                at += 1;
            }
            let digits = s.get(start..at)?;
            if digits.is_empty() {
                return None;
            }
            for (i, c) in digits.bytes().take(9).enumerate() {
                nanos += i128::from(c - b'0') * 10i128.pow(8 - i as u32);
            }
        }
        let offset = match s.get(at..)? {
            "Z" | "z" => 0,
            rest if rest.len() == 6 && rest.as_bytes()[3] == b':' => {
                let sign = match rest.as_bytes()[0] {
                    b'+' => 1,
                    b'-' => -1,
                    _ => return None,
                };
                sign * (num(at + 1..at + 3)? * 3600 + num(at + 4..at + 6)? * 60)
            }
            _ => return None,
        };
        let days = days_from_civil(year, month as u32, day as u32);
        let secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset;
        Some(Timestamp::from_unix_nanos(i128::from(secs) * NANOS + nanos))
    }
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `days` after 1970-01-01 (Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `at` as local time `utc_offset` seconds east of UTC, to the minute:
/// `2026-10-29 14:03`.
pub(crate) fn absolute(at: Timestamp, utc_offset: i32) -> String {
    let secs = at.secs.saturating_add(i64::from(utc_offset));
    let (days, of_day) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        of_day / 3600,
        of_day % 3600 / 60
    )
}

/// How long ago `at` was at `now`, in its largest whole unit: `just now`
/// within a minute either way, then `3m ago` or `in 2h`, up to years.
pub(crate) fn relative(at: Timestamp, now: Timestamp) -> String {
    let ago = (now.unix_nanos() - at.unix_nanos()) / NANOS;
    let secs = ago.unsigned_abs();
    let (n, unit) = match secs {
        0..60 => return "just now".to_owned(),
        60..3600 => (secs / 60, "m"),
        3600..86_400 => (secs / 3600, "h"),
        86_400..31_536_000 => (secs / 86_400, "d"),
        _ => (secs / 31_536_000, "y"),
    };
    if ago > 0 {
        format!("{n}{unit} ago")
    } else {
        format!("in {n}{unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_round_trip() {
        for days in [-1_000_000, -719_468, -1, 0, 1, 20_000, 2_932_896] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }
}
