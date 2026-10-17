//! The one place inputs from TypeScript are decoded: small enums arrive as
//! integers, counts and Positions as `f64`. `web/src/codes.ts` is the other
//! side.

use std::fmt;

use binnacle_core::{
    Axis, Button, ChangeNotice, EditEnd, FocusStep, Key, Located, Modifiers, PointerKind, Position,
    SortColumn, SortSpec,
};

/// The largest count or Position that crosses as an `f64` exactly
/// (`Number.MAX_SAFE_INTEGER`).
pub const MAX_COUNT: u64 = (1 << 53) - 1;

pub fn axis(code: u8) -> Axis {
    match code {
        0 => Axis::Rows,
        _ => Axis::Cols,
    }
}

/// A pointer event's kind: 0 down, 1 move, anything else up.
pub fn pointer_kind(code: u8) -> PointerKind {
    match code {
        0 => PointerKind::Down,
        1 => PointerKind::Move,
        _ => PointerKind::Up,
    }
}

/// A DOM `PointerEvent.button`: 0 primary, 1 middle, anything else secondary.
pub fn button(code: u8) -> Button {
    match code {
        0 => Button::Primary,
        1 => Button::Middle,
        _ => Button::Secondary,
    }
}

pub const SHIFT: u8 = 1;
pub const CTRL: u8 = 2;
pub const ALT: u8 = 4;
pub const META: u8 = 8;

/// The modifier bitmask: [`SHIFT`], [`CTRL`], [`ALT`] and [`META`] or'd.
pub fn modifiers(bits: u8) -> Modifiers {
    Modifiers {
        shift: bits & SHIFT != 0,
        ctrl: bits & CTRL != 0,
        alt: bits & ALT != 0,
        meta: bits & META != 0,
    }
}

/// How far a focus move goes: 0 an item, 1 a page, anything else the edge.
pub fn focus_step(code: u8) -> FocusStep {
    match code {
        0 => FocusStep::Item,
        1 => FocusStep::Page,
        _ => FocusStep::Edge,
    }
}

/// How an edit ended: 0 commit, anything else cancel.
pub fn edit_end(code: u8) -> EditEnd {
    match code {
        0 => EditEnd::Commit,
        _ => EditEnd::Cancel,
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

/// A Change Notice from its code and up to two Positions (`NaN` where the
/// notice takes fewer): Reordered 0, Reset 1 (count), Inserted 2 (at, n),
/// Removed 3 (at, n), ContentChanged 4 (start, end).
pub fn notice(code: u8, a: f64, b: f64) -> ChangeNotice {
    match code {
        0 => ChangeNotice::Reordered,
        1 => ChangeNotice::Reset(position(a)),
        2 => ChangeNotice::Inserted {
            at: position(a),
            n: position(b),
        },
        3 => ChangeNotice::Removed {
            at: position(a),
            n: position(b),
        },
        _ => ChangeNotice::ContentChanged(position(a)..position(b)),
    }
}

/// A `Locate` answer: Found 0 (at `position`) or Gone 1 (suggesting
/// `position`, `NaN` for no suggestion).
pub fn located(code: u8, position: f64) -> Located {
    match code {
        0 => Located::Found(self::position(position)),
        _ => Located::Gone {
            suggest: (!position.is_nan()).then(|| self::position(position)),
        },
    }
}

/// A Sort Spec from parallel lists of column Keys and directions
/// (ascending 0, descending 1).
pub fn sort_spec(keys: &[u64], directions: &[u8]) -> SortSpec {
    SortSpec::new(
        keys.iter()
            .zip(directions)
            .map(|(&k, &d)| match d {
                0 => SortColumn::ascending(Key(k)),
                _ => SortColumn::descending(Key(k)),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_decode_with_their_positions() {
        assert_eq!(notice(0, f64::NAN, f64::NAN), ChangeNotice::Reordered);
        assert_eq!(notice(1, 12.0, f64::NAN), ChangeNotice::Reset(12));
        assert_eq!(notice(2, 3.0, 4.0), ChangeNotice::Inserted { at: 3, n: 4 });
        assert_eq!(notice(3, 3.0, 4.0), ChangeNotice::Removed { at: 3, n: 4 });
        assert_eq!(notice(4, 3.0, 7.0), ChangeNotice::ContentChanged(3..7));
    }

    #[test]
    fn located_answers_decode_with_an_optional_suggestion() {
        assert_eq!(located(0, 9.0), Located::Found(9));
        assert_eq!(located(1, 4.0), Located::Gone { suggest: Some(4) });
        assert_eq!(located(1, f64::NAN), Located::Gone { suggest: None });
    }

    #[test]
    fn sort_specs_pair_keys_with_directions() {
        let spec = sort_spec(&[7, 2], &[1, 0]);
        assert_eq!(
            spec.columns(),
            &[
                SortColumn::descending(Key(7)),
                SortColumn::ascending(Key(2))
            ]
        );
    }

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
    fn each_modifier_bit_sets_its_modifier() {
        assert_eq!(modifiers(0), Modifiers::NONE);
        assert_eq!(modifiers(SHIFT), Modifiers::SHIFT);
        assert_eq!(modifiers(CTRL), Modifiers::CTRL);
        assert_eq!(modifiers(ALT), Modifiers::ALT);
        assert_eq!(modifiers(META), Modifiers::META);
        assert_eq!(modifiers(SHIFT | META), Modifiers::SHIFT | Modifiers::META);
    }

    #[test]
    fn edits_end_in_a_commit_or_a_cancel() {
        assert_eq!(edit_end(0), EditEnd::Commit);
        assert_eq!(edit_end(1), EditEnd::Cancel);
    }

    #[test]
    fn dom_buttons_map_to_grid_buttons() {
        assert_eq!(button(0), Button::Primary);
        assert_eq!(button(1), Button::Middle);
        assert_eq!(button(2), Button::Secondary);
    }

    #[test]
    fn negative_fractional_and_nan_counts_are_rejected() {
        assert!(count(-1.0).is_err());
        assert!(count(2.5).is_err());
        assert!(count(f64::NAN).is_err());
    }
}
