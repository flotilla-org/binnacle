//! Seam 2 properties for comparison conditions: `MemorySource<R>` over
//! random rows of an integer, a float, a date, a byte string and a bare
//! column, put through View Specs whose rows part compares the rows'
//! values (with `not`) and whose columns part compares the columns' values
//! in a row, with data changes between, against a naive model that compares
//! every cell one by one.

use std::borrow::Cow;

use binnacle_core::{
    Axis, Compared, Comparison, Condition, ConditionKind, FilterSpec, Key, Scalar, ViewSpec,
};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// A row: an integer, a float, a date (milliseconds) and a byte string,
/// each blank if `None`; a fifth, bare column has no closure.
#[derive(Clone, Debug)]
struct Row {
    int: Option<i64>,
    float: Option<f64>,
    date: Option<i64>,
    text: Option<String>,
}

const COLS: u64 = 5;
const MAX_ROWS: usize = 6;
const DAY: i64 = 86_400_000;

/// Mostly a few small numbers, so comparisons often meet equal values.
fn small() -> impl hegel::PrintableGenerator<i64> {
    hegel::one_of!(
        gs::integers::<i64>().min_value(-2).max_value(2),
        gs::integers::<i64>(),
    )
}

/// A float, with NaN, the infinities and both zeros among them.
fn float() -> impl hegel::PrintableGenerator<f64> {
    hegel::one_of!(
        small().map(|n| n as f64 / 2.0),
        gs::sampled_from(vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 0.0]),
        gs::floats::<f64>(),
    )
}

/// A date: mostly midnights a few days apart, sometimes a time of day,
/// sometimes any instant at all.
fn date() -> impl hegel::PrintableGenerator<i64> {
    hegel::one_of!(
        gs::integers::<i64>()
            .min_value(-2)
            .max_value(2)
            .map(|d| d * DAY),
        gs::integers::<i64>().min_value(-3 * DAY).max_value(3 * DAY),
        gs::integers::<i64>(),
    )
}

#[hegel::composite]
fn row(tc: &TestCase) -> Row {
    Row {
        int: tc.draw(gs::optional(small())),
        float: tc.draw(gs::optional(float())),
        date: tc.draw(gs::optional(date())),
        text: tc.draw(gs::optional(
            gs::sampled_from(vec!["1", "a", ""]).map(String::from),
        )),
    }
}

fn scalar() -> impl hegel::PrintableGenerator<Scalar> {
    hegel::one_of!(
        float().map(Scalar::Number),
        float().map(Scalar::Number),
        date().map(Scalar::Date),
    )
}

#[hegel::composite]
fn comparison(tc: &TestCase) -> Comparison {
    let s = tc.draw(scalar());
    match tc.draw(gs::integers::<u8>().max_value(6)) {
        0 => Comparison::Equal(s),
        1 => Comparison::NotEqual(s),
        2 => Comparison::Less(s),
        3 => Comparison::AtMost(s),
        4 => Comparison::Greater(s),
        5 => Comparison::AtLeast(s),
        // Mostly the same kind at both ends, sometimes not.
        _ => {
            let high = match (s, tc.draw(gs::booleans())) {
                (Scalar::Number(_), true) => Scalar::Number(tc.draw(float())),
                (Scalar::Date(_), true) => Scalar::Date(tc.draw(date())),
                (_, false) => tc.draw(scalar()),
            };
            Comparison::Between(s, high)
        }
    }
}

/// A comparison condition read in an item of `n` (sometimes one past them).
#[hegel::composite]
fn condition(tc: &TestCase, n: u64) -> Condition {
    let key = Key(tc.draw(gs::integers::<u64>().max_value(n)));
    Condition::compare(key, tc.draw(comparison())).not(tc.draw(gs::booleans()))
}

#[hegel::composite]
fn filter(tc: &TestCase, n: u64) -> FilterSpec {
    FilterSpec::default().with_conditions(tc.draw(gs::vecs(condition(n)).max_size(3)).into())
}

fn source(rows: usize) -> MemorySource<Row> {
    MemorySource::<Row>::new(rows, COLS as usize)
        .i64_column(Key(0), |r: &Row| r.int)
        .f64_column(Key(1), |r: &Row| r.float)
        .date_column(Key(2), |r: &Row| r.date)
        .bytes_column(Key(3), |r: &Row| {
            r.text.as_deref().map(|t| Cow::Borrowed(t.as_bytes()))
        })
}

/// A cell as the model reads it.
#[derive(Clone, Copy, Debug)]
enum Cell {
    Number(f64),
    Date(i64),
    /// Text, or a bare column: never compared.
    Other,
    Blank,
}

fn cell(rows: &[Row], r: u64, c: u64) -> Cell {
    let Some(row) = rows.get(r as usize) else {
        return Cell::Blank;
    };
    let blank_or = |v: Option<Cell>| v.unwrap_or(Cell::Blank);
    match c {
        0 => blank_or(row.int.map(|v| Cell::Number(v as f64))),
        1 => blank_or(row.float.map(Cell::Number)),
        2 => blank_or(row.date.map(Cell::Date)),
        3 => blank_or(row.text.as_ref().map(|_| Cell::Other)),
        _ => Cell::Other,
    }
}

/// `v` against `s`, as -1, 0 or 1, or `None` when they don't compare: a NaN
/// on either side, or a number against a date.
fn order(v: Cell, s: Scalar) -> Option<i8> {
    let sign = |less: bool, more: bool| match (less, more) {
        (true, _) => Some(-1),
        (_, true) => Some(1),
        _ => Some(0),
    };
    match (v, s) {
        (Cell::Number(a), Scalar::Number(b)) if a.is_nan() || b.is_nan() => None,
        (Cell::Number(a), Scalar::Number(b)) => sign(a < b, a > b),
        (Cell::Date(a), Scalar::Date(b)) => sign(a < b, a > b),
        _ => None,
    }
}

/// Whether the model's `comparison` keeps a cell, before `not`.
fn passes(comparison: &Comparison, v: Cell) -> bool {
    let is = |s: Scalar, wanted: &[i8]| order(v, s).is_some_and(|o| wanted.contains(&o));
    match *comparison {
        Comparison::Equal(s) => is(s, &[0]),
        Comparison::NotEqual(s) => is(s, &[-1, 1]),
        Comparison::Less(s) => is(s, &[-1]),
        Comparison::AtMost(s) => is(s, &[-1, 0]),
        Comparison::Greater(s) => is(s, &[1]),
        Comparison::AtLeast(s) => is(s, &[0, 1]),
        Comparison::Between(low, high) => is(low, &[0, 1]) && is(high, &[-1, 0]),
    }
}

fn kind(s: Scalar) -> Compared {
    match s {
        Scalar::Number(_) => Compared::Numbers,
        Scalar::Date(_) => Compared::Dates,
    }
}

/// The kind both ends of `comparison` share, if they do.
fn kind_of(comparison: &Comparison) -> Option<Compared> {
    match *comparison {
        Comparison::Between(low, high) => (kind(low) == kind(high)).then_some(kind(low)),
        Comparison::Equal(s)
        | Comparison::NotEqual(s)
        | Comparison::Less(s)
        | Comparison::AtMost(s)
        | Comparison::Greater(s)
        | Comparison::AtLeast(s) => Some(kind(s)),
    }
}

fn comparison_of(c: &Condition) -> &Comparison {
    match &c.kind {
        ConditionKind::Compare(comparison) => comparison,
        _ => unreachable!("only comparisons are drawn"),
    }
}

/// The conditions of `wanted` the model applies: those on one of the `n`
/// items of the other axis whose kind `compared` says that item compares.
fn applied(
    wanted: &FilterSpec,
    n: u64,
    compared: impl Fn(u64) -> Option<Compared>,
) -> Vec<Condition> {
    (wanted.conditions.conditions().iter())
        .filter(|c| c.key.0 < n)
        .filter(|c| compared(c.key.0).is_some_and(|k| kind_of(comparison_of(c)) == Some(k)))
        .cloned()
        .collect()
}

fn keeps(conditions: &[Condition], cell: impl Fn(u64) -> Cell) -> bool {
    (conditions.iter()).all(|c| passes(comparison_of(c), cell(c.key.0)) != c.not)
}

fn shown(source: &MemorySource<Row>, axis: Axis) -> Vec<u64> {
    let count = source.count(axis);
    (source.window(axis, 0..count).into_iter())
        .map(|k| k.0)
        .collect()
}

/// The kind of value column `c` compares, for the rows.
fn column_kind(c: u64) -> Option<Compared> {
    match c {
        0 | 1 => Some(Compared::Numbers),
        2 => Some(Compared::Dates),
        _ => None,
    }
}

#[derive(Clone, Debug)]
enum Op {
    View(FilterSpec),
    /// The data changes row `row % len`.
    Change(usize, Row),
}

/// Whatever comparison conditions and data changes arrive, the rows shown
/// are those whose values pass every applied condition, compared one by
/// one: numbers on the integer and float columns, dates on the date column,
/// never a blank or a NaN (so `not` keeps them). A condition on the byte
/// string or bare column, or comparing with the wrong kind, is left out of
/// the spec the source applies.
#[hegel::test(test_cases = 1000)]
fn the_rows_shown_are_those_every_applied_comparison_keeps(tc: TestCase) {
    let mut rows = tc.draw(gs::vecs(row()).max_size(MAX_ROWS));
    let mut source = source(rows.len());
    let mut conditions = Vec::new();
    let ops = hegel::one_of!(
        filter(COLS).map(Op::View),
        filter(COLS).map(Op::View),
        hegel::tuples!(gs::integers::<usize>(), row()).map(|(r, row)| Op::Change(r, row)),
    );
    for op in tc.draw(gs::vecs(ops).min_size(1).max_size(6)) {
        tc.note(&format!("{op:?}"));
        match op {
            Op::View(wanted) => {
                conditions = applied(&wanted, COLS, column_kind);
                source
                    .view(&rows, &ViewSpec::default().with_filter(Axis::Rows, wanted))
                    .unwrap();
            }
            Op::Change(_, _) if rows.is_empty() => continue,
            Op::Change(r, row) => {
                let r = r % rows.len();
                rows[r] = row;
                for c in 0..4 {
                    source.changed(&rows, r, Key(c));
                }
            }
        }
        let spec = FilterSpec::default().with_conditions(conditions.clone().into());
        assert_eq!(source.view_spec().rows.filter, spec);
        let model: Vec<u64> = (0..rows.len() as u64)
            .filter(|&r| keeps(&conditions, |c| cell(&rows, r, c)))
            .collect();
        assert_eq!(shown(&source, Axis::Rows), model);
    }
}

/// A table of float columns only, or of date columns only.
fn uniform(rows: usize, dates: bool) -> MemorySource<Row> {
    let source = MemorySource::<Row>::new(rows, 3);
    match dates {
        false => source
            .i64_column(Key(0), |r: &Row| r.int)
            .f64_column(Key(1), |r: &Row| r.float)
            .i64_column(Key(2), |r: &Row| r.date),
        true => source
            .date_column(Key(0), |r: &Row| r.int)
            .date_column(Key(1), |r: &Row| r.float.map(|f| f as i64))
            .date_column(Key(2), |r: &Row| r.date),
    }
}

/// The model's cell of a uniform table: its value in row `r`, column `c`.
fn uniform_cell(rows: &[Row], r: u64, c: u64, dates: bool) -> Cell {
    let Some(row) = rows.get(r as usize) else {
        return Cell::Blank;
    };
    let v: Option<f64> = match c {
        0 => row.int.map(|v| v as f64),
        1 => row.float.map(|f| if dates { f as i64 as f64 } else { f }),
        _ => row.date.map(|v| v as f64),
    };
    match (v, dates) {
        (None, _) => Cell::Blank,
        (Some(v), false) => Cell::Number(v),
        (Some(v), true) => Cell::Date(v as i64),
    }
}

/// On a table whose columns all compare one kind, the columns shown are
/// those whose value in each condition's row passes it; on the mixed table
/// every such condition is left out.
#[hegel::test(test_cases = 1000)]
fn the_columns_shown_are_those_every_applied_comparison_keeps(tc: TestCase) {
    let rows = tc.draw(gs::vecs(row()).max_size(MAX_ROWS));
    let n = rows.len() as u64;
    let table = tc.draw(gs::sampled_from(vec![Some(false), Some(true), None]));
    let wanted = tc.draw(filter(n));
    let spec = ViewSpec::default().with_filter(Axis::Cols, wanted.clone());
    let (mut source, cols, kind) = match table {
        Some(dates) => {
            let kind = [Compared::Numbers, Compared::Dates][dates as usize];
            (uniform(rows.len(), dates), 3, Some(kind))
        }
        None => (source(rows.len()), COLS, None),
    };
    source.view(&rows, &spec).unwrap();
    let conditions = applied(&wanted, n, |_| kind);
    let spec = FilterSpec::default().with_conditions(conditions.clone().into());
    assert_eq!(source.view_spec().cols.filter, spec);
    let model: Vec<u64> = (0..cols)
        .filter(|&c| {
            keeps(&conditions, |r| match table {
                Some(dates) => uniform_cell(&rows, r, c, dates),
                None => cell(&rows, r, c),
            })
        })
        .collect();
    assert_eq!(shown(&source, Axis::Cols), model);
}

/// Every row compares the kind its columns share: numbers on a table of
/// numbers, dates on one of dates, and none on the mixed table.
#[test]
fn the_rows_compare_the_kind_every_column_shares() {
    let mixed = source(2).support(Axis::Cols, Key(0)).compare;
    let numbers = uniform(2, false).support(Axis::Cols, Key(1)).compare;
    let dates = uniform(2, true).support(Axis::Cols, Key(1)).compare;
    assert_eq!(
        (mixed, numbers, dates),
        (None, Some(Compared::Numbers), Some(Compared::Dates))
    );
}

/// The rows declare comparisons on numbers for the integer and float
/// columns and on dates for the date column, and none for the others.
#[test]
fn the_rows_declare_comparisons_by_column_kind() {
    let source = source(1);
    for c in 0..COLS {
        assert_eq!(
            source.support(Axis::Rows, Key(c)).compare,
            column_kind(c),
            "column {c}"
        );
    }
}
