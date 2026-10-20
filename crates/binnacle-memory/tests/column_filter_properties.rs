//! Seam 2 properties for filtering both axes: `MemorySource<R>` over random
//! rows of short texts, put through View Specs whose rows part filters the
//! rows by their columns' values and whose columns part filters the columns
//! by their rows' values (quick filter and Text and OneOf conditions, with
//! `not`), with appends, data changes, the user's edits and replacements
//! between, against a naive model that tests every cell one by one.

mod support;

use std::borrow::Cow;
use std::collections::BTreeSet;

use binnacle_core::{
    Axis, Blanks, ChangeNotice, Condition, ConditionKind, Distinct, DistinctQuery, DistinctValue,
    FilterSpec, Key, Searched, TextQuery, Value, ViewSpec,
};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};
use regex::RegexBuilder;

/// A row: one text (blank if `None`) per text column.
type Row = Vec<Option<String>>;

/// How many columns have a text form; one more, the last, has none.
const TEXTS: usize = 3;
const COLS: usize = TEXTS + 1;

#[hegel::composite]
fn text(tc: &TestCase, min: usize, max: usize) -> String {
    // Mostly a few letters, so queries often match; any character besides.
    let letters = hegel::one_of!(
        gs::sampled_from(vec!['a', 'A', 'b', 'é', 'É', '1', ' ']),
        gs::sampled_from(vec!['a', 'A', 'b', 'é', 'É', '1', ' ']),
        gs::characters(),
    );
    let chars: Vec<char> = tc.draw(gs::vecs(letters).min_size(min).max_size(max));
    chars.into_iter().collect()
}

#[hegel::composite]
fn row(tc: &TestCase) -> Row {
    (0..TEXTS)
        .map(|_| tc.draw(gs::optional(text(0, 3))))
        .collect()
}

/// An item of `axis`'s other axis: for the rows a column (the bare one
/// included), for the columns a row, sometimes one past the last.
#[hegel::composite]
fn across(tc: &TestCase, axis: Axis) -> Key {
    let n = match axis {
        Axis::Rows => COLS,
        Axis::Cols => MAX_ROWS + 1,
    };
    Key(tc.draw(gs::integers::<u64>().max_value(n as u64 - 1)))
}

const MAX_ROWS: usize = 6;

/// A Text Query on `axis`: plain text or a pattern (sometimes invalid),
/// searching the shown items of the other axis or some of them.
#[hegel::composite]
fn query(tc: &TestCase, axis: Axis) -> TextQuery {
    let regex = tc.draw(gs::booleans());
    let text = if regex {
        let pieces = gs::sampled_from(vec!["a", "b", "1", ".", "^", "$", "|", "(", "[ab]"]);
        tc.draw(gs::vecs(pieces).min_size(1).max_size(3)).concat()
    } else {
        tc.draw(text(1, 2))
    };
    let searches = match tc.draw(gs::booleans()) {
        true => Searched::Shown,
        false => Searched::Keys(tc.draw(gs::vecs(across(axis)).max_size(3))),
    };
    TextQuery::new(text)
        .case_sensitive(tc.draw(gs::booleans()))
        .regex(regex)
        .searching(searches)
}

#[hegel::composite]
fn condition(tc: &TestCase, axis: Axis) -> Condition {
    let key = tc.draw(across(axis));
    let condition = if tc.draw(gs::booleans()) {
        Condition::text(key, tc.draw(query(axis)))
    } else {
        let value = hegel::one_of!(gs::just(Value::Blank), text(0, 2).map(Value::Text));
        Condition::one_of(key, tc.draw(gs::vecs(value).max_size(3)))
    };
    condition.not(tc.draw(gs::booleans()))
}

#[hegel::composite]
fn filter(tc: &TestCase, axis: Axis) -> FilterSpec {
    FilterSpec::default()
        .with_quick(tc.draw(gs::optional(query(axis))))
        .with_conditions(tc.draw(gs::vecs(condition(axis)).max_size(4)).into())
}

fn source(rows: usize) -> MemorySource<Row> {
    (0..TEXTS).fold(MemorySource::<Row>::new(rows, COLS), |s, c| {
        s.text_column(Key(c as u64), move |r: &Row| {
            r[c].as_deref().map(Cow::Borrowed)
        })
    })
}

/// Whether `text` matches `query`, or `None` if its pattern is invalid.
fn matches(query: &TextQuery, text: &str) -> Option<bool> {
    if query.case_sensitive && !query.regex {
        return Some(text.contains(&query.text));
    }
    let pattern = match query.regex {
        true => query.text.clone(),
        false => regex::escape(&query.text),
    };
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .build()
        .ok()?;
    Some(regex.is_match(text))
}

fn valid(query: &TextQuery) -> bool {
    matches(query, "").is_some()
}

/// The text of the cell in row `r`, column `c`: blank past the last row and
/// in the bare column.
fn cell(rows: &[Row], r: u64, c: u64) -> Option<String> {
    let row = rows.get(r as usize)?;
    row.get(c as usize).cloned().flatten()
}

/// What the model applies on one axis.
#[derive(Clone, Debug, Default)]
struct Applied {
    quick: Option<TextQuery>,
    conditions: Vec<Condition>,
}

impl Applied {
    /// The part of `wanted` the source applies: a quick filter with an
    /// invalid pattern leaves the one in force, a condition with one is left
    /// out.
    fn take(&mut self, wanted: &FilterSpec) {
        if wanted.quick.as_ref().is_none_or(valid) {
            self.quick = wanted.quick.clone();
        }
        let ok = |c: &&Condition| match &c.kind {
            ConditionKind::Text(q) => valid(q),
            _ => true,
        };
        self.conditions = wanted
            .conditions
            .conditions()
            .iter()
            .filter(ok)
            .cloned()
            .collect();
    }

    fn filter(&self) -> FilterSpec {
        FilterSpec::default()
            .with_quick(self.quick.clone())
            .with_conditions(self.conditions.clone().into())
    }
}

/// Whether the model keeps item `i` of `axis`: `text(other)` is the cell
/// where it meets item `other` of the other axis, of which there are `n`.
/// A quick filter searching the shown items searches all of them.
fn keeps(applied: &Applied, n: u64, text: impl Fn(u64) -> Option<String>) -> bool {
    let quick = applied.quick.as_ref().is_none_or(|q| {
        let searched: Vec<u64> = match &q.searches {
            Searched::Shown => (0..n).collect(),
            Searched::Keys(keys) => keys.iter().map(|k| k.0).collect(),
        };
        (searched.iter()).any(|&o| text(o).is_some_and(|t| matches(q, &t) == Some(true)))
    });
    quick
        && applied.conditions.iter().all(|c| {
            let t = text(c.key.0);
            let matched = match &c.kind {
                ConditionKind::Text(q) => t.is_some_and(|t| matches(q, &t) == Some(true)),
                ConditionKind::OneOf(values) => values.contains(&Value::of(t.as_deref())),
                _ => unreachable!("only Text and OneOf conditions are drawn"),
            };
            matched != c.not
        })
}

/// The naive model: what is applied on each axis and the unmatched items.
#[derive(Clone, Debug, Default)]
struct Model {
    axes: [Applied; 2],
    unmatched: [BTreeSet<u64>; 2],
}

impl Model {
    fn keeps(&self, rows: &[Row], axis: Axis, i: u64) -> bool {
        match axis {
            Axis::Rows => keeps(&self.axes[0], COLS as u64, |c| cell(rows, i, c)),
            Axis::Cols => keeps(&self.axes[1], rows.len() as u64, |r| cell(rows, r, i)),
        }
    }

    fn base(rows: &[Row], axis: Axis) -> u64 {
        match axis {
            Axis::Rows => rows.len() as u64,
            Axis::Cols => COLS as u64,
        }
    }

    /// The items shown on `axis`, in base order: those kept, and the
    /// unmatched.
    fn shown(&self, rows: &[Row], axis: Axis) -> Vec<u64> {
        (0..Model::base(rows, axis))
            .filter(|&i| self.unmatched[axis.index()].contains(&i) || self.keeps(rows, axis, i))
            .collect()
    }

    fn spec(&self) -> ViewSpec {
        ViewSpec::default()
            .with_filter(Axis::Rows, self.axes[0].filter())
            .with_filter(Axis::Cols, self.axes[1].filter())
    }
}

fn shown(source: &MemorySource<Row>, axis: Axis) -> Vec<u64> {
    let count = source.count(axis);
    (source.window(axis, 0..count).into_iter())
        .map(|k| k.0)
        .collect()
}

#[derive(Clone, Debug)]
enum Op {
    View(FilterSpec, FilterSpec),
    Reapply,
    Append(Vec<Row>),
    /// The data changes cell (`row % len`, `col`).
    Change(usize, usize, Option<String>),
    /// The user edits cell (`row % len`, `col`).
    Edit(usize, usize, Option<String>),
    Replace(Vec<Row>),
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let cell = || {
        hegel::tuples!(
            gs::integers::<usize>(),
            gs::integers::<usize>().max_value(TEXTS - 1),
            gs::optional(text(0, 3)),
        )
    };
    let view = || hegel::tuples!(filter(Axis::Rows), filter(Axis::Cols));
    tc.draw(hegel::one_of!(
        view().map(|(r, c)| Op::View(r, c)),
        view().map(|(r, c)| Op::View(r, c)),
        gs::just(Op::Reapply),
        gs::vecs(row()).min_size(1).max_size(2).map(Op::Append),
        cell().map(|(r, c, t)| Op::Change(r, c, t)),
        cell().map(|(r, c, t)| Op::Edit(r, c, t)),
        cell().map(|(r, c, t)| Op::Edit(r, c, t)),
        gs::vecs(row()).max_size(MAX_ROWS).map(Op::Replace),
    ))
}

/// Whatever View Specs, appends, changes, edits and replacements arrive,
/// each axis shows the model's items: those its applied filter keeps,
/// tested against the other axis's cells, and its unmatched items. A
/// column the user's edit stops matching stays, unmatched, until the next
/// View Spec; a change from the data, or rows arriving, filters it at once.
/// Whenever the columns shown change, the source returns a `Refiltered`
/// for the columns with their new count.
#[hegel::test(test_cases = 1000)]
fn each_axis_shows_the_models_items(tc: TestCase) {
    let mut rows = tc.draw(gs::vecs(row()).max_size(MAX_ROWS));
    let mut source = source(rows.len());
    let mut model = Model::default();
    let mut wanted = ViewSpec::default();
    for op in tc.draw(gs::vecs(op()).max_size(8)) {
        tc.note(&format!("{op:?}"));
        let before = shown(&source, Axis::Cols);
        let notices = match op {
            Op::View(r, c) => {
                wanted = ViewSpec::default()
                    .with_filter(Axis::Rows, r.clone())
                    .with_filter(Axis::Cols, c.clone());
                model.axes[0].take(&r);
                model.axes[1].take(&c);
                model.unmatched = Default::default();
                source.view(&rows, &wanted).unwrap()
            }
            Op::Reapply => {
                model.unmatched = Default::default();
                source.view(&rows, &wanted).unwrap()
            }
            Op::Append(more) => {
                let old: Vec<bool> = (0..COLS as u64)
                    .map(|c| model.keeps(&rows, Axis::Cols, c))
                    .collect();
                rows.extend(more);
                let changed = (0..COLS as u64)
                    .filter(|&c| model.keeps(&rows, Axis::Cols, c) != old[c as usize]);
                for c in changed.collect::<Vec<_>>() {
                    model.unmatched[1].remove(&c);
                }
                source.appended(&rows, &[])
            }
            Op::Replace(new) => {
                let old: Vec<bool> = (0..COLS as u64)
                    .map(|c| model.keeps(&rows, Axis::Cols, c))
                    .collect();
                rows = new;
                let changed = (0..COLS as u64)
                    .filter(|&c| model.keeps(&rows, Axis::Cols, c) != old[c as usize]);
                for c in changed.collect::<Vec<_>>() {
                    model.unmatched[1].remove(&c);
                }
                model.unmatched[0].clear();
                source.replaced(&rows, None)
            }
            Op::Change(r, c, text) | Op::Edit(r, c, text) if rows.is_empty() => {
                let _ = (r, c, text);
                continue;
            }
            Op::Change(r, c, text) => {
                let r = r % rows.len();
                rows[r][c] = text;
                model.unmatched[0].remove(&(r as u64));
                model.unmatched[1].remove(&(c as u64));
                source.changed(&rows, r, Key(c as u64))
            }
            Op::Edit(r, c, text) => {
                let r = r % rows.len();
                let was = [
                    shown(&source, Axis::Rows).contains(&(r as u64)),
                    before.contains(&(c as u64)),
                ];
                rows[r][c] = text;
                for (axis, i) in [(Axis::Rows, r as u64), (Axis::Cols, c as u64)] {
                    let stays = was[axis.index()] && !model.keeps(&rows, axis, i);
                    match stays {
                        true => model.unmatched[axis.index()].insert(i),
                        false => model.unmatched[axis.index()].remove(&i),
                    };
                }
                source.edited(&rows, r, Key(c as u64))
            }
        };
        assert_eq!(source.view_spec(), model.spec());
        for axis in [Axis::Rows, Axis::Cols] {
            let items = shown(&source, axis);
            assert_eq!(items, model.shown(&rows, axis), "{axis:?}");
            let keys: Vec<Key> = items.iter().map(|&i| Key(i)).collect();
            let flags: Vec<bool> = (items.iter())
                .map(|i| model.unmatched[axis.index()].contains(i))
                .collect();
            assert_eq!(source.unmatched(axis, &keys), flags, "{axis:?}");
        }
        let after = shown(&source, Axis::Cols);
        if after != before {
            let told = ChangeNotice::Refiltered(after.len() as u64);
            assert!(
                notices.contains(&(Axis::Cols, told)),
                "{notices:?} for {before:?} to {after:?}"
            );
        }
    }
}

/// The model's distinct values of row `row` across the columns `narrow`
/// keeps: each cell's text, counted, in text order, searched (ignoring
/// case), capped at `limit`; blanks counted unless searching.
fn model_distinct(rows: &[Row], row: u64, query: &DistinctQuery) -> Distinct {
    let mut narrow = Applied::default();
    narrow.take(&query.narrow);
    let search = TextQuery::new(query.search.clone());
    let mut blanks = 0;
    let mut found: Vec<String> = Vec::new();
    for c in 0..COLS as u64 {
        if !keeps(&narrow, rows.len() as u64, |r| cell(rows, r, c)) {
            continue;
        }
        match cell(rows, row, c) {
            None => blanks += 1,
            Some(t) if query.search.is_empty() || matches(&search, &t) == Some(true) => {
                found.push(t)
            }
            Some(_) => {}
        }
    }
    found.sort();
    let mut values: Vec<DistinctValue> = Vec::new();
    for text in found {
        match values.iter_mut().find(|v| v.text == text) {
            Some(v) => v.count = v.count.map(|n| n + 1),
            None => values.push(DistinctValue {
                text,
                count: Some(1),
            }),
        }
    }
    let partial = values.len() > query.limit;
    values.truncate(query.limit);
    Distinct {
        values,
        blanks: (query.search.is_empty() && blanks > 0).then_some(Blanks::Counted(blanks)),
        partial,
        narrowed: true,
    }
}

/// Whatever the rows, the narrowing, the search and the limit, a row's
/// distinct values across the columns are the model's.
#[hegel::test(test_cases = 1000)]
fn a_rows_distinct_values_across_the_columns_are_the_models(tc: TestCase) {
    let rows = tc.draw(gs::vecs(row()).max_size(MAX_ROWS));
    let mut source = source(rows.len());
    let row = tc.draw(across(Axis::Cols));
    let mut narrow = Applied::default();
    narrow.take(&tc.draw(filter(Axis::Cols)));
    let query = DistinctQuery::new(tc.draw(gs::integers::<usize>().max_value(5)))
        .search(tc.draw(hegel::one_of!(gs::just(String::new()), text(1, 2))))
        .narrow(narrow.filter());
    tc.note(&format!("{query:?}"));
    assert_eq!(
        source.distinct(&rows, Axis::Cols, row, &query),
        model_distinct(&rows, row.0, &query)
    );
}
