//! The list-of-values checklist's rules (#107): which conditions on a column
//! a tick writes. A capped list can't be written out, so each change writes
//! whichever condition is exact: unticking from all gives `not OneOf`,
//! ticking from none gives `OneOf`, and "select all matching" a search gives
//! a Text condition on it, which later unticks AND a `not OneOf` onto. The
//! ticks are read back from the conditions against the values shown.
//!
//! The checklist owns a column's first Text condition and its first OneOf;
//! the [comparison row](super::compare) owns its first comparison; any
//! others stay as they are. A value a Text condition rejects can't be
//! ticked: a menu shows it dimmed. A column read from the grouping's
//! slicers writes the same changes as slicer edits.

use binnacle_core::{Axis, Condition, ConditionKind, Grid, Key, TextQuery, Value};

use crate::TextMatcher;

/// The conditions on one column the checklist reads and writes: on the
/// rows, or on a row filtering the columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// The axis the conditions filter.
    pub axis: Axis,
    pub key: Key,
    /// Its first Text condition.
    pub text: Option<Condition>,
    /// Its first OneOf.
    pub one_of: Option<Condition>,
    /// Its first comparison.
    pub compare: Option<Condition>,
    /// Every condition on the column, those two included, in order.
    pub all: Vec<Condition>,
    /// Whether the conditions are the grouping's slicers on the column,
    /// written as slicer edits, rather than conditions filtering `axis`.
    pub slicing: bool,
}

/// How the checklist shows a value: ticked, and whether a Text condition
/// rejects it, so it can't be ticked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tick {
    /// Every condition on the column keeps the value.
    pub ticked: bool,
    /// A Text condition on the column rejects it.
    pub excluded: bool,
}

/// The values the checklist shows: the blank first if listed, whether there
/// were more, and the search they answer.
pub struct Shown<'a> {
    pub values: &'a [Value],
    pub partial: bool,
    pub search: &'a str,
}

impl Column {
    /// The conditions on `key` among `conditions`, which filter `axis`.
    pub fn of<'a>(
        conditions: impl IntoIterator<Item = &'a Condition>,
        axis: Axis,
        key: Key,
    ) -> Column {
        let all: Vec<Condition> = (conditions.into_iter())
            .filter(|c| c.key == key)
            .cloned()
            .collect();
        let first = |kind: fn(&ConditionKind) -> bool| all.iter().find(|c| kind(&c.kind)).cloned();
        Column {
            axis,
            key,
            text: first(|k| matches!(k, ConditionKind::Text(_))),
            one_of: first(|k| matches!(k, ConditionKind::OneOf(_))),
            compare: first(|k| matches!(k, ConditionKind::Compare(_))),
            all,
            slicing: false,
        }
    }

    /// The slicers on `key` among `slicers`.
    pub fn sliced<'a>(slicers: impl IntoIterator<Item = &'a Condition>, key: Key) -> Column {
        Column {
            slicing: true,
            ..Column::of(slicers, Axis::Rows, key)
        }
    }

    /// Whether the column's Text conditions reject `value`, so it can't be
    /// ticked. A list reads through [`ticks`](Column::ticks), which builds
    /// the matchers once.
    pub fn excluded(&self, value: &Value) -> bool {
        Keepers::of(self).excluded(value)
    }

    /// Whether `value` is ticked: every condition on the column keeps it.
    /// A list reads through [`ticks`](Column::ticks), which builds the
    /// matchers once.
    pub fn ticked(&self, value: &Value) -> bool {
        Keepers::of(self).ticked(value)
    }

    /// How each of `values` shows, as [`ticked`](Column::ticked) and
    /// [`excluded`](Column::excluded) say, with each Text condition's
    /// matcher built once for the list.
    pub fn ticks(&self, values: &[Value]) -> Vec<Tick> {
        let keepers = Keepers::of(self);
        (values.iter())
            .map(|v| Tick {
                ticked: keepers.ticked(v),
                excluded: keepers.excluded(v),
            })
            .collect()
    }

    /// The column with `text` and `one_of` in place of its Text condition
    /// and OneOf (each added at the end if new), and the edits that ask the
    /// Grid for it.
    pub fn write(
        &self,
        grid: &mut Grid,
        text: Option<Condition>,
        one_of: Option<Condition>,
    ) -> Column {
        self.swap(grid, [(&self.text, text), (&self.one_of, one_of)])
    }

    /// The column with `compare` in place of its comparison (added at the
    /// end if new), and the edit that asks the Grid for it.
    pub fn write_compare(&self, grid: &mut Grid, compare: Option<Condition>) -> Column {
        self.swap(grid, [(&self.compare, compare)])
    }

    /// The column with each `to` in place of its `from`, and the edits.
    fn swap<const N: usize>(
        &self,
        grid: &mut Grid,
        swaps: [(&Option<Condition>, Option<Condition>); N],
    ) -> Column {
        let mut all = self.all.clone();
        for (from, to) in swaps {
            let at = from.as_ref().and_then(|f| all.iter().position(|c| c == f));
            match (from, at, to) {
                (Some(f), Some(i), Some(t)) => {
                    match self.slicing {
                        true => grid.replace_slicer(f, t.clone()),
                        false => grid.replace_condition(self.axis, f, t.clone()),
                    }
                    all[i] = t;
                }
                (Some(f), Some(i), None) => {
                    match self.slicing {
                        true => grid.remove_slicer(f),
                        false => grid.remove_condition(self.axis, f),
                    }
                    all.remove(i);
                }
                (_, _, Some(t)) => {
                    match self.slicing {
                        true => grid.add_slicer(t.clone()),
                        false => grid.add_condition(self.axis, t.clone()),
                    }
                    all.push(t);
                }
                _ => {}
            }
        }
        Column {
            slicing: self.slicing,
            ..Column::of(&all, self.axis, self.key)
        }
    }

    /// Ticks (`on`) or unticks `value`: with no OneOf, unticking writes `not
    /// OneOf([value])`; a OneOf lists the ticked values, a `not OneOf` the
    /// unticked ones. A `not OneOf` left with none, or a OneOf holding every
    /// value shown of a whole unsearched list, is removed.
    pub fn tick(&self, grid: &mut Grid, value: &Value, on: bool, shown: &Shown) -> Column {
        let key = self.key;
        if self.excluded(value) {
            return self.clone();
        }
        let text = self.text.clone();
        let Some(Condition {
            kind: ConditionKind::OneOf(listed),
            not,
            ..
        }) = &self.one_of
        else {
            let one_of = (!on).then(|| Condition::one_of(key, vec![value.clone()]).not(true));
            return self.write(grid, text, one_of);
        };
        let mut listed: Vec<Value> = listed.iter().filter(|v| *v != value).cloned().collect();
        // A OneOf lists the ticked values; a `not OneOf` the unticked ones.
        if on != *not {
            listed.push(value.clone());
        }
        let whole = !shown.partial && shown.search.is_empty();
        let keepers = Keepers::of(self);
        let all = (shown.values.iter()).all(|v| keepers.excluded(v) || listed.contains(v));
        if (*not && listed.is_empty()) || (!*not && whole && all) {
            return self.write(grid, text, None);
        }
        let one_of = Condition::one_of(key, listed).not(*not);
        self.write(grid, text, Some(one_of))
    }

    /// Whether the head of the list is ticked: "(Select all)" when every
    /// value shown that can be ticked is; "(Select all matching …)" while
    /// searching, when the column's Text condition is that search and no
    /// OneOf narrows it.
    pub fn head_ticked(&self, shown: &Shown) -> bool {
        if !shown.search.is_empty() {
            let searched = self
                .text
                .as_ref()
                .is_some_and(|c| c.kind == search_condition(c.key, shown.search).kind && !c.not);
            return searched && self.one_of.is_none();
        }
        let keepers = Keepers::of(self);
        let mut open = (shown.values.iter())
            .filter(|v| !keepers.excluded(v))
            .peekable();
        open.peek().is_some() && open.all(|v| keepers.ticked(v))
    }

    /// Ticks (`on`) or unticks the head. Without a search, ticking removes
    /// the OneOf and unticking writes `OneOf([])`, so later ticks start from
    /// none. With one, ticking writes a Text condition on the search in
    /// place of the column's (dropping the OneOf), and unticking unticks each
    /// value shown.
    pub fn tick_head(&self, grid: &mut Grid, on: bool, shown: &Shown) -> Column {
        let key = self.key;
        if shown.search.is_empty() {
            let one_of = (!on).then(|| Condition::one_of(key, Vec::new()));
            return self.write(grid, self.text.clone(), one_of);
        }
        if on {
            return self.write(grid, Some(search_condition(key, shown.search)), None);
        }
        (shown.values.iter()).fold(self.clone(), |now, v| now.tick(grid, v, false, shown))
    }
}

/// "Select all matching" `search`: contains it, ignoring case.
fn search_condition(key: Key, search: &str) -> Condition {
    Condition::text(key, TextQuery::new(search))
}

/// A column's conditions read against values, each Text condition's
/// matcher built once.
struct Keepers<'c>(Vec<(&'c Condition, Option<TextMatcher>)>);

impl<'c> Keepers<'c> {
    fn of(column: &'c Column) -> Self {
        let matcher = |c: &Condition| match &c.kind {
            ConditionKind::Text(q) => TextMatcher::new(q),
            _ => None,
        };
        Keepers(column.all.iter().map(|c| (c, matcher(c))).collect())
    }

    fn excluded(&self, value: &Value) -> bool {
        (self.0.iter())
            .any(|(c, m)| matches!(c.kind, ConditionKind::Text(_)) && !keeps(c, m.as_ref(), value))
    }

    fn ticked(&self, value: &Value) -> bool {
        self.0.iter().all(|(c, m)| keeps(c, m.as_ref(), value))
    }
}

/// Whether `condition` (with `matcher`, its Text matcher) keeps an item
/// whose value is `value`, read here as the In-Memory Table Source reads
/// it. A pattern the `regex` crate rejects, which no source applies, keeps
/// everything.
fn keeps(condition: &Condition, matcher: Option<&TextMatcher>, value: &Value) -> bool {
    let matched = match (&condition.kind, value) {
        (ConditionKind::OneOf(values), _) => values.contains(value),
        (ConditionKind::Text(_), value) => match (matcher, value) {
            (None, _) => return true,
            (Some(matcher), Value::Text(t)) => matcher.is_match(t),
            (Some(_), Value::Blank) => false,
        },
        _ => return true,
    };
    matched != condition.not
}
