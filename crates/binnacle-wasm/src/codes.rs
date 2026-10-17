//! The one place inputs from TypeScript are decoded: small enums arrive as
//! integers, counts and Positions as `f64`. `web/src/codes.ts` is the other
//! side.

use std::fmt;

use binnacle_core::{Axis, Position};

/// The largest count or Position that crosses as an `f64` exactly
/// (`Number.MAX_SAFE_INTEGER`).
pub const MAX_COUNT: u64 = (1 << 53) - 1;

pub fn axis(code: u8) -> Axis {
    match code {
        0 => Axis::Rows,
        _ => Axis::Cols,
    }
}

/// Why a number from TypeScript is not a count or Position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotACount(pub f64);

impl fmt::Display for NotACount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is not a whole number from 0 to 2^53 - 1, so it cannot be a binnacle count or Position",
            self.0
        )
    }
}

impl std::error::Error for NotACount {}

/// A count or Position from an `f64`: whole, not negative, and at most
/// [`MAX_COUNT`]. Anything else is rejected rather than rounded.
pub fn count(n: f64) -> Result<u64, NotACount> {
    if n.fract() == 0.0 && (0.0..=MAX_COUNT as f64).contains(&n) {
        Ok(n as u64)
    } else {
        Err(NotACount(n))
    }
}

/// A Position from an `f64`, for inputs where a bad one is a bug in the
/// TypeScript side rather than the widget user's mistake.
pub fn position(n: f64) -> Position {
    count(n).unwrap_or_else(|e| panic!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_up_to_the_safe_limit_are_counts() {
        assert_eq!(count(0.0), Ok(0));
        assert_eq!(count(1e15), Ok(1_000_000_000_000_000));
        assert_eq!(count(9_007_199_254_740_991.0), Ok(MAX_COUNT));
    }

    #[test]
    fn counts_past_the_safe_limit_are_rejected_not_rounded() {
        for n in [9_007_199_254_740_992.0, 1e300, f64::INFINITY] {
            assert_eq!(count(n), Err(NotACount(n)));
        }
    }

    #[test]
    fn negative_fractional_and_nan_counts_are_rejected() {
        assert!(count(-1.0).is_err());
        assert!(count(2.5).is_err());
        assert!(count(f64::NAN).is_err());
    }
}
