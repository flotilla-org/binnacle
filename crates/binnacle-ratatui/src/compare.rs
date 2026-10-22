//! The column filter popup's comparison row: its steps from "Any value"
//! through "Between", the comparison its fields hold, and a comparison
//! written for its fields or its chip.

use binnacle_core::{Compared, Comparison, Scalar};

/// The row's steps, Left and Right moving between them: none, then each
/// comparison.
pub(crate) const STEPS: [&str; 8] = [
    "Any value",
    "Equal to",
    "Not equal to",
    "Less than",
    "At most",
    "Greater than",
    "At least",
    "Between",
];

/// The step `between` is.
pub(crate) const BETWEEN: usize = 7;

/// The comparison step `step` asks for with `low` (and `high`, for
/// Between) typed in, or none while a field holds no `compared` value.
pub(crate) fn comparison(
    step: usize,
    compared: Compared,
    low: &str,
    high: &str,
) -> Option<Comparison> {
    let a = scalar(compared, low);
    Some(match step {
        1 => Comparison::Equal(a?),
        2 => Comparison::NotEqual(a?),
        3 => Comparison::Less(a?),
        4 => Comparison::AtMost(a?),
        5 => Comparison::Greater(a?),
        6 => Comparison::AtLeast(a?),
        BETWEEN => Comparison::Between(a?, scalar(compared, high)?),
        _ => return None,
    })
}

/// The step, low and high fields that show `c`.
pub(crate) fn fields(c: &Comparison) -> (usize, String, String) {
    let (step, low) = match c {
        Comparison::Equal(s) => (1, s),
        Comparison::NotEqual(s) => (2, s),
        Comparison::Less(s) => (3, s),
        Comparison::AtMost(s) => (4, s),
        Comparison::Greater(s) => (5, s),
        Comparison::AtLeast(s) => (6, s),
        Comparison::Between(low, high) => {
            return (BETWEEN, low.to_string(), high.to_string());
        }
    };
    (step, low.to_string(), String::new())
}

/// A comparison for a chip: `> 100`, `≤ 2024-03-01`, or `between 1 and 5`.
pub(crate) fn text(c: &Comparison) -> String {
    let (sign, s) = match c {
        Comparison::Equal(s) => ("=", s),
        Comparison::NotEqual(s) => ("≠", s),
        Comparison::Less(s) => ("<", s),
        Comparison::AtMost(s) => ("≤", s),
        Comparison::Greater(s) => (">", s),
        Comparison::AtLeast(s) => ("≥", s),
        Comparison::Between(low, high) => return format!("between {low} and {high}"),
    };
    format!("{sign} {s}")
}

/// The `compared` value `text` writes: a finite number, or a date as
/// `YYYY-MM-DD`, with `THH:MM`, seconds and milliseconds and `Z` (UTC)
/// optional.
pub(crate) fn scalar(compared: Compared, text: &str) -> Option<Scalar> {
    match compared {
        Compared::Numbers => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Scalar::Number),
        Compared::Dates => date(text.trim()),
    }
}

fn date(text: &str) -> Option<Scalar> {
    let (day, time) = match text.split_once('T') {
        Some((day, time)) => (day, Some(time.strip_suffix('Z')?)),
        None => (text, None),
    };
    let number = |s: &str, len: usize| {
        (s.len() == len && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse::<u32>().ok())?
    };
    let mut parts = day.splitn(3, '-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    let (year, month, d) = (number(y, 4)?, number(m, 2)?, number(d, 2)?);
    if !(1..=12).contains(&month) || d == 0 {
        return None;
    }
    let midnight = Scalar::date(i64::from(year), month, d);
    // A day past the month's end lands in the next month.
    if !midnight.to_string().starts_with(day) {
        return None;
    }
    let Scalar::Date(ms) = midnight else {
        return None;
    };
    let Some(time) = time else {
        return Some(midnight);
    };
    let (clock, milli) = match time.split_once('.') {
        Some((clock, milli)) => (clock, number(milli, 3)?),
        None => (time, 0),
    };
    let mut parts = clock.split(':');
    let h = number(parts.next()?, 2)?;
    let min = number(parts.next()?, 2)?;
    let s = parts.next().map_or(Some(0), |s| number(s, 2))?;
    if parts.next().is_some() || h > 23 || min > 59 || s > 59 {
        return None;
    }
    let past = ((h * 60 + min) * 60 + s) * 1000 + milli;
    Some(Scalar::Date(ms + i64::from(past)))
}
