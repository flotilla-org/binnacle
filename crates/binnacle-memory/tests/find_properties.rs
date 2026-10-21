//! Seam 2 properties for find: `MemorySource<R>` over random records of a
//! name and a quantity, under random sorts, quick filters and column moves,
//! answers marks, steps and counts that agree with each other and with a
//! naive model, which lists every searched cell in the view's order (by rows
//! or by columns) and matches each value's text form one by one.

use binnacle_core::{
    Axis, FilterSpec, FindDirection, FindMatch, FindQuery, Key, Searched, SortColumn, SortSpec,
    TextQuery, ViewSpec,
};
use binnacle_memory::{FindCount, FindStep, MemorySource};
use hegel::generators as gs;
use hegel::{Generator, TestCase};
use regex::RegexBuilder;

/// A name (blank if `None`) and a quantity (blank if `None`).
type Record = (Option<String>, Option<i64>);

const NAME: Key = Key(0);
const QTY: Key = Key(1);
/// A column with no closure, so no text form.
const BARE: Key = Key(2);

/// Short text mostly from a few letters in both cases and digits, so queries
/// often match more than one cell and case matters, and any character besides.
#[hegel::composite]
fn text(tc: &TestCase, min: usize, max: usize) -> String {
    let letters = gs::sampled_from(vec!['a', 'A', 'b', 'B', 'é', 'É', '1', '2', ' ']);
    let any = hegel::one_of!(letters, gs::characters());
    let chars: Vec<char> = tc.draw(gs::vecs(any).min_size(min).max_size(max));
    chars.into_iter().collect()
}

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    let qty = gs::optional(gs::integers::<i64>().min_value(-12).max_value(25));
    (tc.draw(gs::optional(text(0, 4))), tc.draw(qty))
}

/// Plain text or a pattern (sometimes invalid), searching the shown columns
/// or some of them.
#[hegel::composite]
fn query(tc: &TestCase) -> TextQuery {
    let regex = tc.draw(gs::booleans());
    let text = if regex {
        let pieces = gs::sampled_from(vec!["a", "B", "1", ".", "^", "$", "|", "(", "[ab]"]);
        tc.draw(gs::vecs(pieces).min_size(1).max_size(3)).concat()
    } else {
        tc.draw(text(1, 2))
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

#[hegel::composite]
fn find_query(tc: &TestCase) -> FindQuery {
    let by = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    FindQuery::new(tc.draw(query())).by(by)
}

/// A view and column order: unsorted or sorted by quantity, maybe a quick
/// filter, and the columns in any order.
#[derive(Debug)]
struct View {
    sort: SortSpec,
    quick: Option<TextQuery>,
    moves: Vec<(Vec<Key>, u64)>,
}

#[hegel::composite]
fn view(tc: &TestCase) -> View {
    let sort = match tc.draw(gs::integers::<u8>().max_value(2)) {
        0 => SortSpec::unsorted(),
        1 => SortSpec::new(vec![SortColumn::ascending(QTY)]),
        _ => SortSpec::new(vec![SortColumn::descending(QTY)]),
    };
    let quick = tc.draw(gs::optional(query()));
    let keys = gs::vecs(gs::sampled_from(vec![NAME, QTY, BARE])).max_size(2);
    let moves =
        tc.draw(gs::vecs(hegel::tuples!(keys, gs::integers::<u64>().max_value(3))).max_size(3));
    View { sort, quick, moves }
}

fn source(rows: &[Record], view: &View) -> MemorySource<Record> {
    let mut source = MemorySource::<Record>::new(rows.len(), 3)
        .bytes_column(NAME, |r: &Record| r.0.as_ref().map(|n| n.as_bytes().into()))
        .i64_column(QTY, |r: &Record| r.1);
    let filter = FilterSpec::default().with_quick(view.quick.clone());
    let spec = ViewSpec::default()
        .with_sort(Axis::Rows, view.sort.clone())
        .with_filter(Axis::Rows, filter);
    source.view(rows, &spec).unwrap();
    for (keys, to) in &view.moves {
        source.move_items(Axis::Cols, keys, *to);
    }
    source
}

/// Whether `text` matches `query`, or `None` if its pattern is invalid.
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

fn text_of(record: &Record, c: Key) -> Option<String> {
    match c {
        NAME => record.0.clone(),
        QTY => record.1.map(|q| q.to_string()),
        _ => None,
    }
}

/// Whether the cell of `record` in column `c` matches `query`.
fn found(query: &TextQuery, record: &Record, c: Key) -> bool {
    let searched = match &query.searches {
        Searched::Shown => true,
        Searched::Keys(keys) => keys.contains(&c),
    };
    searched && text_of(record, c).is_some_and(|t| matches(query, &t) == Some(true))
}

/// The view as the source shows it: row Keys (Logical Indices) and column
/// Keys, each in Position order.
fn shown(source: &MemorySource<Record>) -> (Vec<Key>, Vec<Key>) {
    let all = |axis| source.window(axis, 0..source.count(axis));
    (all(Axis::Rows), all(Axis::Cols))
}

/// Every cell in the Find Query's order, as (row Position, column Position).
fn order(rows: usize, cols: usize, by: Axis) -> Vec<(usize, usize)> {
    match by {
        Axis::Rows => (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .collect(),
        Axis::Cols => (0..cols)
            .flat_map(|c| (0..rows).map(move |r| (r, c)))
            .collect(),
    }
}

/// The model's matches, in the Find Query's order.
fn model(records: &[Record], source: &MemorySource<Record>, query: &FindQuery) -> Vec<FindMatch> {
    let (rows, cols) = shown(source);
    order(rows.len(), cols.len(), query.by)
        .into_iter()
        .filter(|&(r, c)| found(&query.query, &records[rows[r].0 as usize], cols[c]))
        .map(|(r, c)| FindMatch::new(rows[r], cols[c]))
        .collect()
}

/// The model's step: the first match after `from` in the order (or the last
/// before it), wrapping; a `from` the view does not show steps from the
/// start (or the end).
fn model_step(
    source: &MemorySource<Record>,
    query: &FindQuery,
    matches: &[FindMatch],
    from: Option<FindMatch>,
    direction: FindDirection,
) -> Option<FindStep> {
    let (rows, cols) = shown(source);
    let place = |m: FindMatch| {
        let r = rows.iter().position(|&k| k == m.row)?;
        let c = cols.iter().position(|&k| k == m.col)?;
        Some(match query.by {
            Axis::Rows => (r, c),
            Axis::Cols => (c, r),
        })
    };
    let from = from.and_then(place);
    let at = |i: usize| (i, place(matches[i]).expect("a match is shown"));
    let (i, wrapped) = match direction {
        FindDirection::Next => match (0..matches.len())
            .map(at)
            .find(|&(_, p)| from.is_none_or(|f| p > f))
        {
            Some((i, _)) => (i, false),
            None if matches.is_empty() => return None,
            None => (0, from.is_some()),
        },
        FindDirection::Previous => {
            match (0..matches.len())
                .rev()
                .map(at)
                .find(|&(_, p)| from.is_none_or(|f| p < f))
            {
                Some((i, _)) => (i, false),
                None if matches.is_empty() => return None,
                None => (matches.len() - 1, from.is_some()),
            }
        }
    };
    Some(FindStep {
        at: matches[i],
        index: Some(i as u64),
        wrapped,
    })
}

/// A cell to step from: a shown one, one of a row the view leaves out or
/// that never was, or none.
#[hegel::composite]
fn from(tc: &TestCase, rows: usize) -> Option<FindMatch> {
    let row = gs::integers::<u64>().max_value(rows as u64 + 1).map(Key);
    let col = gs::sampled_from(vec![NAME, QTY, BARE, Key(9)]);
    tc.draw(gs::optional(
        hegel::tuples!(row, col).map(|(r, c)| FindMatch::new(r, c)),
    ))
}

/// Under any view, the count is the model's number of matches and the marks
/// of the whole view are the model's matches.
#[hegel::test(test_cases = 1000)]
fn the_marks_and_the_count_agree_with_the_model(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(10));
    let mut source = source(&rows, &tc.draw(view()));
    let query = tc.draw(find_query());
    let matches = model(&rows, &source, &query);
    assert_eq!(
        source.find_count(&rows, &query),
        FindCount {
            total: matches.len() as u64,
            partial: false
        }
    );
    let (row_keys, col_keys) = shown(&source);
    let marks = source.find_marks(&rows, &row_keys, &col_keys, &query);
    let marked: Vec<FindMatch> = order(row_keys.len(), col_keys.len(), Axis::Rows)
        .into_iter()
        .filter(|&(r, c)| marks[r * col_keys.len() + c])
        .map(|(r, c)| FindMatch::new(row_keys[r], col_keys[c]))
        .collect();
    let mut expected = matches.clone();
    expected.sort_by_key(|m| {
        let at = |keys: &[Key], k| keys.iter().position(|&x| x == k);
        (at(&row_keys, m.row), at(&col_keys, m.col))
    });
    assert_eq!(marked, expected, "the marks of the whole view");
}

/// Under any view, a step from any cell, either way, lands where the model's
/// does, with its index and whether it wrapped.
#[hegel::test(test_cases = 1000)]
fn a_step_agrees_with_the_model(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(10));
    let mut source = source(&rows, &tc.draw(view()));
    let query = tc.draw(find_query());
    let matches = model(&rows, &source, &query);
    let from = tc.draw(from(rows.len()));
    let direction = tc.draw(gs::sampled_from(vec![
        FindDirection::Next,
        FindDirection::Previous,
    ]));
    assert_eq!(
        source.find_next(&rows, &query, from, direction),
        model_step(&source, &query, &matches, from, direction),
        "{direction:?} from {from:?}"
    );
}

/// Stepping next from the start visits every match once, in order, then
/// wraps to the first; stepping back from each match returns to the one
/// before it.
#[hegel::test(test_cases = 500)]
fn stepping_visits_every_match_and_previous_undoes_next(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(10));
    let mut source = source(&rows, &tc.draw(view()));
    let query = tc.draw(find_query());
    let total = source.find_count(&rows, &query).total;
    let mut at = None;
    let mut visited = Vec::new();
    for i in 0..total {
        let step = source
            .find_next(&rows, &query, at, FindDirection::Next)
            .expect("a match while the count is not 0");
        assert!(!step.wrapped, "wrapped before visiting every match");
        assert_eq!(step.index, Some(i));
        visited.push(step.at);
        at = Some(step.at);
    }
    assert_eq!(visited, model(&rows, &source, &query));
    let last = source.find_next(&rows, &query, at, FindDirection::Next);
    assert_eq!(
        last.map(|s| (s.at, s.wrapped)),
        visited.first().map(|&m| (m, at.is_some()))
    );
    for pair in visited.windows(2) {
        let back = source.find_next(&rows, &query, Some(pair[1]), FindDirection::Previous);
        assert_eq!(back.map(|s| s.at), Some(pair[0]));
    }
}
