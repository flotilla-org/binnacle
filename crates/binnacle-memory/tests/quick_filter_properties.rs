//! Seam 2 properties for the quick filter, column conditions and distinct
//! values: `MemorySource<R>` over random records of a name and a quantity,
//! put through View Specs with random quick filters, Text and OneOf
//! conditions (with `not`), appends, name changes, the user's name edits and
//! the host's own filter, and asked for distinct values, against a naive
//! model that matches each value's text form one by one (the `regex` crate
//! for patterns and for ignoring case, substrings otherwise).

mod support;

use std::collections::BTreeSet;

use binnacle_core::{
    Axis, Blanks, Condition, ConditionKind, Distinct, DistinctQuery, DistinctValue, FilterSpec,
    Key, Searched, SortColumn, SortSpec, TextQuery, Value, ViewSpec,
};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};
use regex::RegexBuilder;

/// A name (blank if `None`) and a quantity (blank if `None`).
type Record = (Option<String>, Option<i64>);

const NAME: Key = Key(0);
const QTY: Key = Key(1);
/// A column with no closure, so no text form.
const BARE: Key = Key(2);

/// Text of `min..=max` characters: mostly from a few letters in both cases,
/// digits and an accented pair, so queries often match and case matters;
/// sometimes any character at all.
#[hegel::composite]
fn text(tc: &TestCase, min: usize, max: usize) -> String {
    let letters = || {
        gs::sampled_from(vec![
            'a', 'A', 'b', 'B', 'é', 'É', 'i', 'ß', 'İ', '1', '2', ' ',
        ])
    };
    let chars = hegel::one_of!(letters(), letters(), letters(), gs::characters());
    let chars: Vec<char> = tc.draw(gs::vecs(chars).min_size(min).max_size(max));
    chars.into_iter().collect()
}

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    // Mostly small, so quick filters on digits and the host's threshold often bite.
    let small = || gs::integers::<i64>().min_value(-12).max_value(12);
    let qty = gs::optional(hegel::one_of!(small(), small(), gs::integers::<i64>()));
    (tc.draw(gs::optional(text(0, 5))), tc.draw(qty))
}

/// A quick filter: plain text, or a pattern from regex syntax and the same
/// letters (sometimes invalid), searching the shown columns or some of them.
#[hegel::composite]
fn query(tc: &TestCase) -> TextQuery {
    let regex = tc.draw(gs::booleans());
    let text = if regex {
        let pieces = gs::sampled_from(vec![
            "a", "B", "é", "1", ".", "^", "$", "|", "*", "(", ")", "[ab]",
        ]);
        tc.draw(gs::vecs(pieces).min_size(1).max_size(4)).concat()
    } else {
        tc.draw(text(1, 3))
    };
    let searches = match tc.draw(gs::booleans()) {
        true => Searched::Shown,
        false => {
            Searched::Keys(tc.draw(gs::vecs(gs::sampled_from(vec![NAME, QTY, BARE])).max_size(3)))
        }
    };
    TextQuery::new(text)
        .case_sensitive(tc.draw(gs::booleans()))
        .regex(regex)
        .searching(searches)
}

/// A value as a OneOf condition lists it: often a blank, empty text or a
/// small quantity, so it often matches; otherwise short text.
#[hegel::composite]
fn value(tc: &TestCase) -> Value {
    let small = gs::integers::<i64>().min_value(-12).max_value(12);
    tc.draw(hegel::one_of!(
        gs::just(Value::Blank),
        gs::just(Value::Text(String::new())),
        small.map(|q| Value::Text(q.to_string())),
        text(1, 2).map(Value::Text),
    ))
}

/// A Text or OneOf condition on any column, the bare one included, maybe
/// with `not`.
#[hegel::composite]
fn condition(tc: &TestCase) -> Condition {
    let key = tc.draw(gs::sampled_from(vec![NAME, QTY, BARE]));
    let condition = if tc.draw(gs::booleans()) {
        Condition::text(key, tc.draw(query()))
    } else {
        Condition::one_of(key, tc.draw(gs::vecs(value()).max_size(4)))
    };
    condition.not(tc.draw(gs::booleans()))
}

/// Some conditions, often several on one column.
#[hegel::composite]
fn conditions(tc: &TestCase) -> Vec<Condition> {
    tc.draw(gs::vecs(condition()).max_size(8))
}

/// Unsorted, or sorted by quantity either way.
#[hegel::composite]
fn sort(tc: &TestCase) -> SortSpec {
    match tc.draw(gs::integers::<u8>().max_value(2)) {
        0 => SortSpec::unsorted(),
        1 => SortSpec::new(vec![SortColumn::ascending(QTY)]),
        _ => SortSpec::new(vec![SortColumn::descending(QTY)]),
    }
}

fn spec(sort: SortSpec, quick: Option<TextQuery>) -> ViewSpec {
    filtered(sort, quick, Vec::new())
}

fn filtered(sort: SortSpec, quick: Option<TextQuery>, conditions: Vec<Condition>) -> ViewSpec {
    let filter = FilterSpec::default()
        .with_quick(quick)
        .with_conditions(conditions.into());
    ViewSpec::default()
        .with_sort(Axis::Rows, sort)
        .with_filter(Axis::Rows, filter)
}

fn source(rows: usize) -> MemorySource<Record> {
    MemorySource::<Record>::new(rows, 3)
        .bytes_column(NAME, |r: &Record| r.0.as_ref().map(|n| n.as_bytes().into()))
        .i64_column(QTY, |r: &Record| r.1)
}

/// Whether `text` matches `query`, or `None` if its pattern is invalid.
/// Ignoring case means Unicode simple case folding, as the `regex` crate
/// does it (so `i` does not match `İ`, whose lowercase is two characters).
fn matches(query: &TextQuery, text: &str) -> Option<bool> {
    if query.case_sensitive && !query.regex {
        return Some(text.contains(&query.text));
    }
    let pattern = if query.regex {
        query.text.clone()
    } else {
        regex::escape(&query.text)
    };
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .build()
        .ok()?;
    Some(regex.is_match(text))
}

/// The text form of `record`'s value in column `c`.
fn text_of(record: &Record, c: Key) -> Option<String> {
    match c {
        NAME => record.0.clone(),
        QTY => record.1.map(|q| q.to_string()),
        _ => None,
    }
}

/// Whether the model keeps `record` under `query`: some searched column with
/// a text form has text that matches.
fn keeps(query: &TextQuery, record: &Record) -> bool {
    let columns = match &query.searches {
        Searched::Shown => vec![NAME, QTY, BARE],
        Searched::Keys(keys) => keys.clone(),
    };
    columns
        .iter()
        .any(|&c| text_of(record, c).is_some_and(|t| matches(query, &t) == Some(true)))
}

/// Whether the model keeps `record` under `condition`: its value in the
/// condition's column has text that matches (a blank never does) or is
/// listed (a blank as `Blank`), or with `not` the opposite.
fn satisfies(condition: &Condition, record: &Record) -> bool {
    let text = text_of(record, condition.key);
    let matched = match &condition.kind {
        ConditionKind::Text(query) => text.is_some_and(|t| matches(query, &t) == Some(true)),
        ConditionKind::OneOf(values) => values.contains(&Value::of(text.as_deref())),
        _ => unreachable!("only Text and OneOf conditions are drawn"),
    };
    matched != condition.not
}

/// The conditions of `wanted` the source can apply: OneOf, and Text with a
/// valid pattern.
fn valid(wanted: &[Condition]) -> Vec<Condition> {
    let ok = |c: &&Condition| match &c.kind {
        ConditionKind::Text(q) => matches(q, "").is_some(),
        ConditionKind::OneOf(_) => true,
        _ => false,
    };
    wanted.iter().filter(ok).cloned().collect()
}

/// Whether the model's filters keep `record`.
fn kept(
    record: &Record,
    quick: Option<&TextQuery>,
    conditions: &[Condition],
    min: Option<i64>,
) -> bool {
    quick.is_none_or(|q| keeps(q, record))
        && conditions.iter().all(|c| satisfies(c, record))
        && min.is_none_or(|m| record.1.is_some_and(|q| q >= m))
}

/// The model's visible order, as Logical Indices: the rows its filters keep
/// and the Unmatched Rows, in its sort.
fn model(
    rows: &[Record],
    sort: &SortSpec,
    quick: Option<&TextQuery>,
    conditions: &[Condition],
    min: Option<i64>,
    unmatched: &BTreeSet<usize>,
) -> Vec<u64> {
    let rank = |_: usize, i: usize| rows[i].1.map_or((2, 0), |v| (0, i128::from(v)));
    let kept = |i: usize| unmatched.contains(&i) || kept(&rows[i], quick, conditions, min);
    support::model(rows.len(), sort, rank, kept)
        .into_iter()
        .map(|i| i as u64)
        .collect()
}

fn visible(source: &MemorySource<Record>) -> Vec<u64> {
    let count = source.count(Axis::Rows);
    source
        .window(Axis::Rows, 0..count)
        .into_iter()
        .map(|k| k.0)
        .collect()
}

#[derive(Clone, Debug)]
enum Op {
    View(SortSpec, Option<TextQuery>, Vec<Condition>),
    Append(Vec<Record>),
    /// Renames base row `row % len`: the data changed.
    Rename(usize, Option<String>),
    /// The user renames base row `row % len`.
    Edit(usize, Option<String>),
    /// The View Spec last asked for, again: a re-apply.
    Reapply,
    /// The host's own filter: quantities of at least this.
    Filter(Option<i64>),
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    tc.draw(hegel::one_of!(
        hegel::tuples!(sort(), gs::optional(query()), conditions())
            .map(|(s, q, c)| Op::View(s, q, c)),
        hegel::tuples!(sort(), gs::optional(query()), conditions())
            .map(|(s, q, c)| Op::View(s, q, c)),
        gs::vecs(record()).min_size(1).max_size(3).map(Op::Append),
        gs::just(Op::Reapply),
        hegel::tuples!(gs::integers::<usize>(), gs::optional(text(0, 5)))
            .map(|(r, n)| Op::Rename(r, n)),
        hegel::tuples!(gs::integers::<usize>(), gs::optional(text(0, 5)))
            .map(|(r, n)| Op::Edit(r, n)),
        gs::optional(gs::integers::<i64>().min_value(-12).max_value(12)).map(Op::Filter),
    ))
}

/// Whatever View Specs, appends, changes, edits and host filters arrive, the
/// rows shown are the model's: those the applied quick filter, the applied
/// conditions and the host's filter keep, in the applied sort, and the
/// Unmatched Rows. A row the user's edit stops matching stays, flagged,
/// until the next View Spec or host filter; a change from the data is
/// filtered at once. A quick filter with an invalid pattern leaves the one
/// in force applied; a condition with one is left out.
#[hegel::test(test_cases = 1000)]
fn the_rows_shown_are_the_models(tc: TestCase) {
    let mut rows = tc.draw(gs::vecs(record()).max_size(10));
    let mut source = source(rows.len());
    let (mut sort, mut quick, mut min) = (SortSpec::unsorted(), None::<TextQuery>, None);
    let mut applied = Vec::new();
    let mut unmatched = BTreeSet::new();
    let mut wanted = ViewSpec::default();
    for op in tc.draw(gs::vecs(op()).max_size(8)) {
        tc.note(&format!("{op:?}"));
        match op {
            Op::View(s, q, c) => {
                wanted = filtered(s.clone(), q.clone(), c.clone());
                source.view(&rows, &wanted).unwrap();
                sort = s;
                applied = valid(&c);
                let valid = q.as_ref().is_none_or(|q| matches(q, "").is_some());
                if valid {
                    quick = q;
                }
                unmatched.clear();
            }
            Op::Append(records) => {
                rows.extend(records);
                source.appended(&rows, &[]);
            }
            Op::Reapply => {
                source.view(&rows, &wanted).unwrap();
                unmatched.clear();
            }
            Op::Rename(row, name) => {
                if rows.is_empty() {
                    continue;
                }
                let row = row % rows.len();
                rows[row].0 = name;
                source.changed(&rows, row, NAME);
                unmatched.remove(&row);
            }
            Op::Edit(row, name) => {
                if rows.is_empty() {
                    continue;
                }
                let row = row % rows.len();
                let shown = visible(&source).contains(&(row as u64));
                rows[row].0 = name;
                source.edited(&rows, row, NAME);
                if shown && !kept(&rows[row], quick.as_ref(), &applied, min) {
                    unmatched.insert(row);
                } else {
                    unmatched.remove(&row);
                }
            }
            Op::Filter(m) => {
                min = m;
                unmatched.clear();
                match m {
                    Some(m) => source.filter(&rows, move |r| r.1.is_some_and(|q| q >= m)),
                    None => source.clear_filter(&rows),
                };
            }
        }
        assert_eq!(
            source.view_spec(),
            filtered(sort.clone(), quick.clone(), applied.clone())
        );
        let shown = visible(&source);
        assert_eq!(
            shown,
            model(&rows, &sort, quick.as_ref(), &applied, min, &unmatched)
        );
        let keys: Vec<Key> = shown.iter().map(|&i| Key(i)).collect();
        let flags: Vec<bool> = shown.iter().map(|&i| unmatched.contains(&(i as usize))).collect();
        assert_eq!(source.unmatched(Axis::Rows, &keys), flags);
    }
}

/// Every row shown has a value in a searched column that the applied quick
/// filter matches, and every row it matches is shown.
#[hegel::test(test_cases = 1000)]
fn every_row_shown_satisfies_the_applied_quick_filter(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(12));
    let mut source = source(rows.len());
    let query = tc.draw(query());
    source
        .view(&rows, &spec(SortSpec::unsorted(), Some(query)))
        .unwrap();
    let applied = source.view_spec().rows.filter.quick;
    let shown = visible(&source);
    for (i, record) in rows.iter().enumerate() {
        let kept = applied.as_ref().is_none_or(|q| keeps(q, record));
        assert_eq!(shown.contains(&(i as u64)), kept, "row {i}: {record:?}");
    }
}

/// Every row shown satisfies every applied condition, and every row that
/// satisfies them all is shown.
#[hegel::test(test_cases = 1000)]
fn every_row_shown_satisfies_the_applied_conditions(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(12));
    let mut source = source(rows.len());
    let wanted = tc.draw(conditions());
    source
        .view(&rows, &filtered(SortSpec::unsorted(), None, wanted.clone()))
        .unwrap();
    let applied = source.view_spec().rows.filter.conditions;
    let shown = visible(&source);
    for (i, record) in rows.iter().enumerate() {
        let kept = applied.conditions().iter().all(|c| satisfies(c, record));
        assert_eq!(shown.contains(&(i as u64)), kept, "row {i}: {record:?}");
    }
}

/// The source applies exactly the conditions with a valid pattern, in order.
#[hegel::test(test_cases = 500)]
fn the_conditions_applied_are_those_with_a_valid_pattern(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(12));
    let mut source = source(rows.len());
    let wanted = tc.draw(conditions());
    source
        .view(&rows, &filtered(SortSpec::unsorted(), None, wanted.clone()))
        .unwrap();
    let applied = source.view_spec().rows.filter.conditions;
    assert_eq!(applied.conditions(), valid(&wanted));
}

/// A condition and its `not` split the rows between them (a blank goes to
/// whichever lists it, for OneOf, and to the `not`, for Text): each row is shown
/// under exactly one, whichever order they come in.
#[hegel::test(test_cases = 500)]
fn a_condition_and_its_not_split_the_rows(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(12));
    let c = tc.draw(condition()).not(false);
    tc.assume(!valid(std::slice::from_ref(&c)).is_empty());
    let shown = |c: Condition| {
        let mut source = source(rows.len());
        let spec = filtered(SortSpec::unsorted(), None, vec![c]);
        source.view(&rows, &spec).unwrap();
        visible(&source)
    };
    let (yes, no) = (shown(c.clone()), shown(c.not(true)));
    let mut all: Vec<u64> = yes.iter().chain(&no).copied().collect();
    all.sort_unstable();
    assert_eq!(all, (0..rows.len() as u64).collect::<Vec<_>>());
}

/// Applying a sort then adding a quick filter shows the same rows in the same
/// order as applying the quick filter then adding the sort.
#[hegel::test(test_cases = 1000)]
fn filtering_then_sorting_equals_sorting_then_filtering(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(12));
    let (sort, query) = (tc.draw(sort()), tc.draw(query()));
    let both = spec(sort.clone(), Some(query.clone()));
    let mut sorted_first = source(rows.len());
    sorted_first.view(&rows, &spec(sort, None)).unwrap();
    sorted_first.view(&rows, &both).unwrap();
    let mut filtered_first = source(rows.len());
    filtered_first
        .view(&rows, &spec(SortSpec::unsorted(), Some(query)))
        .unwrap();
    filtered_first.view(&rows, &both).unwrap();
    assert_eq!(visible(&sorted_first), visible(&filtered_first));
}

/// The model's distinct values of `column` among the records the host's
/// filter (quantities of at least `min`) and `narrow` keep: those containing
/// `search` ignoring case, counted, in the column's natural order (names by
/// bytes, quantities numerically), capped at `limit`; blanks counted unless
/// searching.
fn model_distinct(
    rows: &[Record],
    column: Key,
    query: &DistinctQuery,
    min: Option<i64>,
) -> Distinct {
    let narrow = &query.narrow;
    let kept = rows.iter().filter(|r| {
        min.is_none_or(|m| r.1.is_some_and(|q| q >= m))
            && narrow.quick.as_ref().is_none_or(|q| keeps(q, r))
            && narrow
                .conditions
                .conditions()
                .iter()
                .all(|c| satisfies(c, r))
    });
    let search = TextQuery::new(query.search.clone());
    let mut blanks = 0;
    let mut found: Vec<(Option<i64>, String)> = Vec::new();
    for r in kept {
        match text_of(r, column) {
            None => blanks += 1,
            Some(t) if query.search.is_empty() || matches(&search, &t) == Some(true) => {
                let rank = if column == QTY { r.1 } else { None };
                found.push((rank, t));
            }
            Some(_) => {}
        }
    }
    found.sort();
    let mut values: Vec<DistinctValue> = Vec::new();
    for (_, text) in found {
        match values.iter_mut().find(|v| v.text == text) {
            Some(v) => v.count = v.count.map(|c| c + 1),
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

/// The narrowing a checklist sends: a quick filter and conditions, only the
/// valid ones (as the applied spec it comes from would hold).
#[hegel::composite]
fn narrow(tc: &TestCase) -> FilterSpec {
    let quick = tc
        .draw(gs::optional(query()))
        .filter(|q| matches(q, "").is_some());
    FilterSpec::default()
        .with_quick(quick)
        .with_conditions(valid(&tc.draw(conditions())).into())
}

/// Whatever the records, the host's filter, the narrowing, the search and
/// the limit, the distinct values are the model's: each value of the column
/// in the rows kept, counted, in natural order, searched, capped, with
/// blanks counted apart from empty text.
#[hegel::test(test_cases = 1000)]
fn distinct_values_are_the_models(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(16));
    let mut source = source(rows.len());
    let min = tc.draw(gs::optional(
        gs::integers::<i64>().min_value(-12).max_value(12),
    ));
    if let Some(m) = min {
        source.filter(&rows, move |r| r.1.is_some_and(|q| q >= m));
    }
    let column = tc.draw(gs::sampled_from(vec![NAME, QTY, BARE]));
    let search = tc.draw(hegel::one_of!(gs::just(String::new()), text(1, 2)));
    let limit = tc.draw(gs::integers::<usize>().max_value(8));
    let query = DistinctQuery::new(limit)
        .search(search)
        .narrow(tc.draw(narrow()));
    tc.note(&format!("{query:?}"));
    assert_eq!(
        source.distinct(&rows, column, &query),
        model_distinct(&rows, column, &query, min)
    );
}

/// What the source applies does not narrow the distinct values; only the
/// query's `narrow` does.
#[hegel::test(test_cases = 300)]
fn distinct_values_ignore_the_filter_in_force(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(12));
    let mut source = source(rows.len());
    let column = tc.draw(gs::sampled_from(vec![NAME, QTY, BARE]));
    let search = tc.draw(hegel::one_of!(gs::just(String::new()), text(1, 2)));
    let asked = DistinctQuery::new(100)
        .search(search)
        .narrow(tc.draw(narrow()));
    let before = source.distinct(&rows, column, &asked);
    let spec = filtered(
        tc.draw(sort()),
        tc.draw(gs::optional(query())),
        tc.draw(conditions()),
    );
    source.view(&rows, &spec).unwrap();
    assert_eq!(source.distinct(&rows, column, &asked), before);
}
