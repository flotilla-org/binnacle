//! The list-of-values checklist's rules (#107): which conditions on a column
//! a tick writes. A capped list can't be written out, so each change writes
//! whichever condition is exact: unticking from all gives `not OneOf`,
//! ticking from none gives `OneOf`, and "select all matching" a search gives
//! a Text condition on it, which later unticks AND a `not OneOf` onto. The
//! ticks are read back from the conditions against the values shown.
//!
//! The checklist owns a column's first Text condition and its first OneOf;
//! any others stay as they are. A value a Text condition rejects can't be
//! ticked: the popup shows it dimmed.

use binnacle_core::{Axis, Condition, ConditionKind, Grid, Key, TextQuery, Value};
use regex::RegexBuilder;

/// The conditions on one column the checklist reads and writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Column {
    pub key: Key,
    /// Its first Text condition.
    pub text: Option<Condition>,
    /// Its first OneOf.
    pub one_of: Option<Condition>,
    /// Every condition on the column, those two included, in order.
    pub all: Vec<Condition>,
}

/// The values the checklist shows: the blank first if listed, whether there
/// were more, and the search they answer.
pub(crate) struct Shown<'a> {
    pub values: &'a [Value],
    pub partial: bool,
    pub search: &'a str,
}

impl Column {
    /// The conditions on column `key` among `conditions`.
    pub fn of<'a>(conditions: impl IntoIterator<Item = &'a Condition>, key: Key) -> Column {
        let all: Vec<Condition> = (conditions.into_iter())
            .filter(|c| c.key == key)
            .cloned()
            .collect();
        let first = |one_of: bool| {
            (all.iter())
                .find(|c| matches!(c.kind, ConditionKind::OneOf(_)) == one_of)
                .cloned()
        };
        Column {
            key,
            text: first(false),
            one_of: first(true),
            all,
        }
    }

    /// Whether the column's Text conditions reject `value`, so it can't be ticked.
    pub fn excluded(&self, value: &Value) -> bool {
        (self.all.iter()).any(|c| matches!(c.kind, ConditionKind::Text(_)) && !keeps(c, value))
    }

    /// Whether `value` is ticked: every condition on the column keeps it.
    pub fn ticked(&self, value: &Value) -> bool {
        self.all.iter().all(|c| keeps(c, value))
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
        let mut all = self.all.clone();
        for (from, to) in [(&self.text, text), (&self.one_of, one_of)] {
            let at = from.as_ref().and_then(|f| all.iter().position(|c| c == f));
            match (from, at, to) {
                (Some(f), Some(i), Some(t)) => {
                    grid.replace_condition(Axis::Rows, f, t.clone());
                    all[i] = t;
                }
                (Some(f), Some(i), None) => {
                    grid.remove_condition(Axis::Rows, f);
                    all.remove(i);
                }
                (_, _, Some(t)) => {
                    grid.add_condition(Axis::Rows, t.clone());
                    all.push(t);
                }
                _ => {}
            }
        }
        Column::of(&all, self.key)
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
        let all = (shown.values.iter()).all(|v| self.excluded(v) || listed.contains(v));
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
        let mut open = shown.values.iter().filter(|v| !self.excluded(v)).peekable();
        open.peek().is_some() && open.all(|v| self.ticked(v))
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

/// Whether `condition` keeps an item whose value is `value`, read here as
/// the In-Memory Table Source reads it. A pattern the `regex` crate rejects,
/// which no source applies, keeps everything.
fn keeps(condition: &Condition, value: &Value) -> bool {
    let matched = match (&condition.kind, value) {
        (ConditionKind::OneOf(values), _) => values.contains(value),
        (ConditionKind::Text(_), Value::Blank) => false,
        (ConditionKind::Text(q), Value::Text(t)) => {
            let pattern = if q.regex {
                q.text.clone()
            } else {
                regex::escape(&q.text)
            };
            let built = RegexBuilder::new(&pattern)
                .case_insensitive(!q.case_sensitive)
                .build();
            match built {
                Ok(regex) => regex.is_match(t),
                Err(_) => return true,
            }
        }
        _ => return true,
    };
    matched != condition.not
}
