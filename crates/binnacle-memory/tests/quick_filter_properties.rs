//! Seam 2 properties for the quick filter and column conditions:
//! `MemorySource<R>` over random records of a name and a quantity, put
//! through View Specs with random quick filters and Text conditions (with
//! `not`), appends, name edits and the host's own filter, against a naive
//! model that matches each value's text form one by one (the `regex` crate
//! for patterns and for ignoring case, substrings otherwise).

mod support;

use binnacle_core::{
    Axis, Condition, ConditionKind, FilterSpec, Key, Searched, SortColumn, SortSpec, TextQuery,
    ViewSpec,
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

/// A Text condition on any column, the bare one included, maybe with `not`.
#[hegel::composite]
fn condition(tc: &TestCase) -> Condition {
    let key = tc.draw(gs::sampled_from(vec![NAME, QTY, BARE]));
    Condition::text(key, tc.draw(query())).not(tc.draw(gs::booleans()))
}

/// Up to three conditions, sometimes two on one column.
#[hegel::composite]
fn conditions(tc: &TestCase) -> Vec<Condition> {
    tc.draw(gs::vecs(condition()).max_size(3))
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
/// condition's column has text that matches, or with `not` does not (a
/// blank never matches).
fn satisfies(condition: &Condition, record: &Record) -> bool {
    let ConditionKind::Text(query) = &condition.kind else {
        unreachable!("only Text conditions are drawn");
    };
    let matched = text_of(record, condition.key).is_some_and(|t| matches(query, &t) == Some(true));
    matched != condition.not
}

/// The conditions of `wanted` the source can apply: those with a valid
/// pattern.
fn valid(wanted: &[Condition]) -> Vec<Condition> {
    let ok = |c: &&Condition| match &c.kind {
        ConditionKind::Text(q) => matches(q, "").is_some(),
        _ => false,
    };
    wanted.iter().filter(ok).cloned().collect()
}

/// The model's visible order, as Logical Indices.
fn model(
    rows: &[Record],
    sort: &SortSpec,
    quick: Option<&TextQuery>,
    conditions: &[Condition],
    min: Option<i64>,
) -> Vec<u64> {
    let rank = |_: usize, i: usize| rows[i].1.map_or((2, 0), |v| (0, i128::from(v)));
    let kept = |i: usize| {
        quick.is_none_or(|q| keeps(q, &rows[i]))
            && conditions.iter().all(|c| satisfies(c, &rows[i]))
            && min.is_none_or(|m| rows[i].1.is_some_and(|q| q >= m))
    };
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
    /// Renames base row `row % len`.
    Rename(usize, Option<String>),
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
        hegel::tuples!(gs::integers::<usize>(), gs::optional(text(0, 5)))
            .map(|(r, n)| Op::Rename(r, n)),
        gs::optional(gs::integers::<i64>().min_value(-12).max_value(12)).map(Op::Filter),
    ))
}

/// Whatever View Specs, appends, edits and host filters arrive, the rows
/// shown are the model's: those the applied quick filter, the applied
/// conditions and the host's filter keep, in the applied sort. A quick
/// filter with an invalid pattern leaves the one in force applied; a
/// condition with one is left out.
#[hegel::test(test_cases = 1000)]
fn the_rows_shown_are_the_models(tc: TestCase) {
    let mut rows = tc.draw(gs::vecs(record()).max_size(10));
    let mut source = source(rows.len());
    let (mut sort, mut quick, mut min) = (SortSpec::unsorted(), None::<TextQuery>, None);
    let mut applied = Vec::new();
    for op in tc.draw(gs::vecs(op()).max_size(8)) {
        tc.note(&format!("{op:?}"));
        match op {
            Op::View(s, q, c) => {
                let wanted = filtered(s.clone(), q.clone(), c.clone());
                source.view(&rows, &wanted).unwrap();
                sort = s;
                applied = valid(&c);
                let valid = q.as_ref().is_none_or(|q| matches(q, "").is_some());
                if valid {
                    quick = q;
                }
            }
            Op::Append(records) => {
                rows.extend(records);
                source.appended(&rows, &[]);
            }
            Op::Rename(row, name) => {
                if rows.is_empty() {
                    continue;
                }
                let row = row % rows.len();
                rows[row].0 = name;
                source.edited(&rows, row, NAME);
            }
            Op::Filter(m) => {
                min = m;
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
        assert_eq!(
            visible(&source),
            model(&rows, &sort, quick.as_ref(), &applied, min)
        );
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
    assert_eq!(applied.conditions(), valid(&wanted), "the valid ones apply");
    let shown = visible(&source);
    for (i, record) in rows.iter().enumerate() {
        let kept = applied.conditions().iter().all(|c| satisfies(c, record));
        assert_eq!(shown.contains(&(i as u64)), kept, "row {i}: {record:?}");
    }
}

/// A condition and its `not` split the rows between them: each row is shown
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
