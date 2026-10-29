//! Seam 2 properties: the In-Memory Table Source's one total sort order,
//! against a naive comparator written straight from the rules over the whole
//! table: nulls lowest, NaN the greatest number, exact integers, bytes and
//! text by code point, descending reversing all of it, ties by Key ascending
//! in both directions. The initial sort and the order after random base
//! changes (appends, and replacements that update, delete and insert rows).

use std::borrow::Cow;
use std::cmp::Ordering;

use binnacle_core::{Axis, Key, SortColumn, SortDirection, SortSpec};
use binnacle_memory::{InMemoryTableSource, SortKeys};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const MAX_ROWS: usize = 14;

#[derive(Clone, Copy, Debug)]
enum Kind {
    I64,
    U64,
    F64,
    Bytes,
    Text,
}

/// One column per kind, in this order.
const KINDS: [Kind; 5] = [Kind::I64, Kind::U64, Kind::F64, Kind::Bytes, Kind::Text];

#[derive(Clone, Debug)]
enum Cell {
    Null,
    Int(i128),
    Float(f64),
    Bytes(Vec<u8>),
    Text(String),
}

#[derive(Clone, Debug)]
struct Row {
    key: u64,
    cells: Vec<Cell>,
}

#[derive(Clone, Debug)]
enum Op {
    Sort(SortSpec),
    Append(Vec<Row>),
    Replace(Vec<Row>),
}

#[hegel::composite]
fn cell(tc: &TestCase, kind: Kind) -> Cell {
    match kind {
        Kind::I64 => tc
            .draw(gs::optional(hegel::one_of!(
                gs::integers::<i64>().min_value(-2).max_value(2),
                gs::sampled_from(vec![i64::MIN, i64::MAX]),
                gs::integers::<i64>(),
            )))
            .map_or(Cell::Null, |v| Cell::Int(v.into())),
        Kind::U64 => tc
            .draw(gs::optional(hegel::one_of!(
                gs::integers::<u64>().max_value(2),
                gs::sampled_from(vec![
                    u64::MAX,
                    i64::MAX as u64,
                    i64::MAX as u64 + 1,
                    1 << 53,
                    (1 << 53) + 1,
                ]),
                gs::integers::<u64>(),
            )))
            .map_or(Cell::Null, |v| Cell::Int(v.into())),
        Kind::F64 => tc
            .draw(gs::optional(hegel::one_of!(
                gs::sampled_from(vec![
                    0.0,
                    -0.0,
                    1.0,
                    -1.0,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NAN,
                    -f64::NAN,
                ]),
                gs::floats::<f64>(),
            )))
            .map_or(Cell::Null, Cell::Float),
        Kind::Bytes => tc
            .draw(gs::optional(hegel::one_of!(
                gs::vecs(gs::integers::<u8>().max_value(2)).max_size(3),
                gs::binary().max_size(6),
            )))
            .map_or(Cell::Null, Cell::Bytes),
        Kind::Text => tc
            .draw(gs::optional(hegel::one_of!(
                gs::sampled_from(vec!["", "a", "b", "ab", "\u{e9}", "\u{10000}", "\u{ffff}"])
                    .map(String::from),
                gs::text().max_size(3),
            )))
            .map_or(Cell::Null, Cell::Text),
    }
}

/// Rows with distinct Keys drawn from a small range, so replacements update
/// and delete rows they share Keys with.
#[hegel::composite]
fn rows(tc: &TestCase, max: usize) -> Vec<Row> {
    let keys = tc.draw(
        gs::vecs(hegel::one_of!(
            gs::integers::<u64>().max_value(30),
            gs::integers::<u64>()
        ))
        .max_size(max),
    );
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for key in keys {
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let cells = KINDS.iter().map(|&k| tc.draw(cell(k))).collect();
        out.push(Row { key, cells });
    }
    out
}

#[hegel::composite]
fn spec(tc: &TestCase) -> SortSpec {
    let column = hegel::tuples!(
        gs::integers::<u64>().max_value(KINDS.len() as u64 - 1),
        gs::sampled_from(vec![SortDirection::Ascending, SortDirection::Descending]),
    )
    .map(|(key, direction)| SortColumn {
        key: Key(key),
        direction,
    });
    SortSpec::new(tc.draw(gs::vecs(column).max_size(4)))
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    tc.draw(hegel::one_of!(
        spec().map(Op::Sort),
        rows(5).map(Op::Append),
        rows(MAX_ROWS).map(Op::Replace),
    ))
}

fn compare_cells(a: &Cell, b: &Cell) -> Ordering {
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        (Cell::Int(x), Cell::Int(y)) => x.cmp(y),
        (Cell::Float(x), Cell::Float(y)) => match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) if x == y => {
                // -0 sorts below +0.
                y.is_sign_negative().cmp(&x.is_sign_negative())
            }
            (false, false) => x.partial_cmp(y).unwrap(),
        },
        (Cell::Bytes(x), Cell::Bytes(y)) => x.cmp(y),
        (Cell::Text(x), Cell::Text(y)) => x.chars().cmp(y.chars()),
        _ => unreachable!("a column holds one kind"),
    }
}

/// The naive comparator over the whole table, rules 1 to 6.
fn naive(spec: &SortSpec, a: &Row, b: &Row) -> Ordering {
    for term in spec.columns() {
        let column = term.key.0 as usize;
        let ordering = compare_cells(&a.cells[column], &b.cells[column]);
        let ordering = match term.direction {
            SortDirection::Ascending => ordering,
            SortDirection::Descending => ordering.reverse(),
        };
        if ordering.is_ne() {
            return ordering;
        }
    }
    a.key.cmp(&b.key)
}

fn sort_keys(rows: &[Row], spec: &SortSpec) -> Vec<SortKeys<'static>> {
    spec.columns()
        .iter()
        .map(|term| {
            let column = term.key.0 as usize;
            let cells = rows.iter().map(|r| &r.cells[column]);
            match KINDS[column] {
                Kind::I64 => SortKeys::I64(
                    cells
                        .map(|c| match c {
                            Cell::Int(v) => Some(*v as i64),
                            _ => None,
                        })
                        .collect(),
                ),
                Kind::U64 => SortKeys::U64(
                    cells
                        .map(|c| match c {
                            Cell::Int(v) => Some(*v as u64),
                            _ => None,
                        })
                        .collect(),
                ),
                Kind::F64 => SortKeys::F64(
                    cells
                        .map(|c| match c {
                            Cell::Float(v) => Some(*v),
                            _ => None,
                        })
                        .collect(),
                ),
                Kind::Bytes => SortKeys::Bytes(
                    cells
                        .map(|c| match c {
                            Cell::Bytes(v) => Some(Cow::Owned(v.clone())),
                            _ => None,
                        })
                        .collect(),
                ),
                Kind::Text => SortKeys::Bytes(
                    cells
                        .map(|c| match c {
                            Cell::Text(v) => Some(Cow::Owned(v.clone().into_bytes())),
                            _ => None,
                        })
                        .collect(),
                ),
            }
        })
        .collect()
}

fn keys_of(rows: &[Row]) -> Vec<Key> {
    rows.iter().map(|r| Key(r.key)).collect()
}

#[hegel::test(test_cases = 1000)]
fn the_order_is_the_naive_total_order_through_any_base_change(tc: TestCase) {
    let mut rows = tc.draw(rows(MAX_ROWS));
    let mut spec = tc.draw(spec());
    let mut source =
        InMemoryTableSource::new(rows.len(), KINDS.len()).with_keys(Axis::Rows, keys_of(&rows));
    source.apply_sort(Axis::Rows, &spec, sort_keys(&rows, &spec));
    let check = |source: &InMemoryTableSource, rows: &[Row], spec: &SortSpec, what: &str| {
        let mut want: Vec<&Row> = rows.iter().collect();
        if !spec.is_empty() {
            want.sort_by(|a, b| naive(spec, a, b));
        }
        let want: Vec<Key> = want.iter().map(|r| Key(r.key)).collect();
        let count = source.count(Axis::Rows);
        assert_eq!(
            source.window(Axis::Rows, 0..count),
            want,
            "{what}: {spec:?}"
        );
    };
    check(&source, &rows, &spec, "initial sort");
    for op in tc.draw(gs::vecs(op()).max_size(8)) {
        match &op {
            Op::Sort(s) => {
                spec = s.clone();
                source.apply_sort(Axis::Rows, &spec, sort_keys(&rows, &spec));
            }
            Op::Append(new) => {
                let new: Vec<Row> = new
                    .iter()
                    .filter(|n| rows.iter().all(|r| r.key != n.key))
                    .cloned()
                    .collect();
                rows.extend(new.iter().cloned());
                source.append_rows(new.len(), &keys_of(&new), sort_keys(&rows, &spec), &[]);
            }
            Op::Replace(new) => {
                rows = new.clone();
                source.replace_rows(
                    rows.len(),
                    Some(keys_of(&rows)),
                    sort_keys(&rows, &spec),
                    Vec::new(),
                );
            }
        }
        check(&source, &rows, &spec, &format!("{op:?}"));
    }
}
