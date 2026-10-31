//! The Described Table source sorts random schemas and rows (nulls, invalid
//! values, NaN, large integers, datetimes to the nanosecond, nested values
//! where a column expects another kind) exactly as a naive comparator
//! written from the Sort Order with its invalid tier: nulls lowest, then
//! invalid values by their text, then typed values; descending reverses
//! all of it, and ties break on the primary key, ascending. A column of a
//! type the source does not read keeps integers and floats as they came,
//! and compares them exactly, a float before an integer of its value.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use binnacle_core::{Axis, Key, SortColumn, SortSpec, ViewSpec};
use binnacle_described::{
    DescribedSource, Field, FieldType, Message, RowsMessage, Schema, Scope, TableMessage, Wire,
};
use binnacle_memory::PumpSource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    String,
    Integer,
    Number,
    Boolean,
    Datetime,
    /// A type the source does not read: integers, floats and text mixed.
    Other,
}

const KINDS: [Kind; 6] = [
    Kind::String,
    Kind::Integer,
    Kind::Number,
    Kind::Boolean,
    Kind::Datetime,
    Kind::Other,
];

impl Kind {
    fn field_type(self) -> FieldType {
        match self {
            Kind::String => FieldType::String,
            Kind::Integer => FieldType::Integer,
            Kind::Number => FieldType::Number,
            Kind::Boolean => FieldType::Boolean,
            Kind::Datetime => FieldType::Datetime,
            Kind::Other => FieldType::Other("any".into()),
        }
    }
}

/// A cell as the model reads it.
#[derive(Clone, Debug)]
enum Model {
    Null,
    Invalid(String),
    Text(String),
    Int(i128),
    Float(f64),
    Bool(bool),
    /// Seconds and nanoseconds since the epoch.
    Instant(i64, u32),
}

/// The days since the epoch of a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn rfc3339(secs: i64, nanos: u32) -> String {
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{nanos:09}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// A cell of a `kind` column, on the wire and as the model reads it.
#[hegel::composite]
fn cell(tc: &TestCase, kind: Kind) -> (Wire, Model) {
    let invalid = hegel::one_of!(
        gs::sampled_from(vec!["", "soon", "a", "B", "é", "\u{10000}", "x y"]).map(String::from),
        gs::text().max_size(3),
    );
    match tc.draw(gs::integers::<u8>().max_value(9)) {
        0 | 1 => return (Wire::Null, Model::Null),
        _ if kind == Kind::Other => return tc.draw(mixed()),
        2 if kind == Kind::String => {
            let n = tc.draw(gs::integers::<i64>().min_value(-3).max_value(3));
            return (Wire::Int(n.into()), Model::Invalid(n.to_string()));
        }
        2 => {
            let text = tc.draw(invalid);
            return (Wire::Text(text.clone()), Model::Invalid(text));
        }
        3 if kind == Kind::Integer => {
            // A float where an integer belongs, as JSON text.
            return (Wire::Float(0.5), Model::Invalid("0.5".into()));
        }
        3 if kind != Kind::String => {
            // A nested value where a scalar belongs.
            let wire = Wire::Map(BTreeMap::from([("a".into(), Wire::Int(1))]));
            return (wire, Model::Invalid("{\"a\":1}".into()));
        }
        _ => {}
    }
    match kind {
        Kind::String => {
            let text = tc.draw(hegel::one_of!(
                gs::sampled_from(vec!["", "a", "b", "ab", "é", "\u{10000}", "\u{ffff}"])
                    .map(String::from),
                gs::text().max_size(3),
            ));
            (Wire::Text(text.clone()), Model::Text(text))
        }
        Kind::Integer => {
            let n: i128 = tc.draw(hegel::one_of!(
                gs::integers::<i64>()
                    .min_value(-2)
                    .max_value(2)
                    .map(i128::from),
                gs::sampled_from(vec![
                    i64::MIN.into(),
                    i64::MAX.into(),
                    i128::from(i64::MAX) + 1,
                    u64::MAX.into(),
                    1 << 53,
                    (1 << 53) + 1,
                ]),
                gs::integers::<i64>().map(i128::from),
                gs::integers::<u64>().map(i128::from),
            ));
            (Wire::Int(n), Model::Int(n))
        }
        Kind::Number => {
            let x = tc.draw(hegel::one_of!(
                gs::sampled_from(vec![
                    0.0,
                    -0.0,
                    1.0,
                    -1.0,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NAN,
                    -f64::NAN,
                    9007199254740993.0,
                ]),
                gs::floats::<f64>(),
            ));
            (Wire::Float(x), Model::Float(x))
        }
        Kind::Boolean => {
            let b = tc.draw(gs::booleans());
            (Wire::Bool(b), Model::Bool(b))
        }
        Kind::Datetime => {
            let secs = tc.draw(hegel::one_of!(
                gs::sampled_from(vec![0i64, -1, 1_785_189_639]),
                gs::integers::<i64>()
                    .min_value(-62_135_596_800)
                    .max_value(253_402_300_799),
            ));
            let nanos = tc.draw(hegel::one_of!(
                gs::sampled_from(vec![0u32, 1, 2, 999_999_999]),
                gs::integers::<u32>().max_value(999_999_999),
            ));
            (
                Wire::Text(rfc3339(secs, nanos)),
                Model::Instant(secs, nanos),
            )
        }
        Kind::Other => tc.draw(mixed()),
    }
}

/// A cell of a column of a type the source does not read.
#[hegel::composite]
fn mixed(tc: &TestCase) -> (Wire, Model) {
    let big = 1i128 << 53;
    match tc.draw(gs::integers::<u8>().max_value(2)) {
        0 => {
            let n: i128 = tc.draw(hegel::one_of!(
                gs::sampled_from(vec![
                    0,
                    -1,
                    1,
                    big - 1,
                    big,
                    big + 1,
                    big + 2,
                    u64::MAX.into()
                ]),
                gs::integers::<i64>().map(i128::from),
                gs::integers::<u64>().map(i128::from),
            ));
            (Wire::Int(n), Model::Int(n))
        }
        1 => {
            let x = tc.draw(hegel::one_of!(
                gs::sampled_from(vec![
                    0.0,
                    -0.0,
                    0.5,
                    -1.0,
                    9007199254740992.0,
                    9007199254740994.0,
                    18446744073709551616.0,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NAN,
                ]),
                gs::floats::<f64>(),
            ));
            (Wire::Float(x), Model::Float(x))
        }
        _ => {
            let text = tc.draw(gs::text().max_size(2));
            (Wire::Text(text.clone()), Model::Text(text))
        }
    }
}

/// An integer against a float, exactly: the float's floor written out in
/// full decimal and read back, a float before an integer of its value.
fn int_float(i: i128, x: f64) -> Ordering {
    if x.is_nan() || x == f64::INFINITY {
        return Ordering::Less;
    }
    if x == f64::NEG_INFINITY {
        return Ordering::Greater;
    }
    let floor = format!("{:.0}", x.floor());
    match floor.parse::<i128>() {
        Ok(f) if i != f => i.cmp(&f),
        Ok(_) if x.fract() != 0.0 => Ordering::Less,
        Ok(_) => Ordering::Greater,
        Err(_) if x > 0.0 => Ordering::Less,
        Err(_) => Ordering::Greater,
    }
}

fn compare(a: &Model, b: &Model) -> Ordering {
    let tier = |m: &Model| match m {
        Model::Null => 0,
        Model::Invalid(_) => 1,
        // Text after numbers, in a mixed column.
        Model::Text(_) => 3,
        _ => 2,
    };
    match (a, b) {
        _ if tier(a) != tier(b) => tier(a).cmp(&tier(b)),
        (Model::Null, Model::Null) => Ordering::Equal,
        (Model::Invalid(x), Model::Invalid(y)) => x.chars().cmp(y.chars()),
        (Model::Text(x), Model::Text(y)) => x.chars().cmp(y.chars()),
        (Model::Int(x), Model::Int(y)) => x.cmp(y),
        (Model::Int(i), Model::Float(x)) => int_float(*i, *x),
        (Model::Float(x), Model::Int(i)) => int_float(*i, *x).reverse(),
        (Model::Bool(x), Model::Bool(y)) => x.cmp(y),
        (Model::Instant(s, n), Model::Instant(t, m)) => (s, n).cmp(&(t, m)),
        (Model::Float(x), Model::Float(y)) => match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) if x == y => y.is_sign_negative().cmp(&x.is_sign_negative()),
            (false, false) => x.partial_cmp(y).unwrap(),
        },
        _ => unreachable!("a column holds one kind: {a:?} {b:?}"),
    }
}

#[derive(Debug)]
struct Table {
    kinds: Vec<Kind>,
    /// Each row's id and cells.
    rows: Vec<(u64, Vec<(Wire, Model)>)>,
}

#[hegel::composite]
fn table(tc: &TestCase) -> Table {
    let kinds = tc.draw(
        gs::vecs(gs::sampled_from(KINDS.to_vec()))
            .min_size(1)
            .max_size(3),
    );
    let mut ids = tc.draw(gs::vecs(gs::integers::<u64>().max_value(40)).max_size(14));
    ids.sort();
    ids.dedup();
    let rows = (ids.into_iter())
        .map(|id| (id, kinds.iter().map(|&k| tc.draw(cell(k))).collect()))
        .collect();
    Table { kinds, rows }
}

#[hegel::test(test_cases = 1000)]
fn the_rows_sort_as_the_naive_total_order_with_the_invalid_tier(tc: TestCase) {
    let table = tc.draw(table());
    let mut fields = vec![Field::new("id", FieldType::Integer)];
    for (i, kind) in table.kinds.iter().enumerate() {
        fields.push(Field::new(format!("c{i}"), kind.field_type()));
    }
    let schema = Schema {
        fields,
        primary_key: vec!["id".into()],
        foreign_keys: Vec::new(),
    };
    let mut source = DescribedSource::new("t", Scope::new("n"));
    source.receive(Message::Table(TableMessage {
        schema,
        incarnation: None,
        table: None,
        sequence: None,
    }));
    let rows = (table.rows.iter())
        .map(|(id, cells)| {
            let mut row = BTreeMap::from([("id".to_string(), Wire::Int((*id).into()))]);
            for (i, (wire, _)) in cells.iter().enumerate() {
                row.insert(format!("c{i}"), wire.clone());
            }
            row
        })
        .collect();
    source.receive(Message::Rows(RowsMessage {
        rows,
        cursor: None,
        partial: false,
        sequence: None,
    }));
    let terms = tc.draw(
        gs::vecs(hegel::tuples!(
            gs::integers::<usize>().max_value(table.kinds.len() - 1),
            gs::booleans()
        ))
        .min_size(1)
        .max_size(3),
    );
    let key_of = |id: u64| source.row_key(&[Wire::Int(id.into())]).unwrap();
    let spec = SortSpec::new(
        (terms.iter())
            .map(|&(c, descending)| {
                let key = source.column_key(&format!("c{c}")).unwrap();
                match descending {
                    true => SortColumn::descending(key),
                    false => SortColumn::ascending(key),
                }
            })
            .collect(),
    );
    let mut want: Vec<&(u64, Vec<(Wire, Model)>)> = table.rows.iter().collect();
    want.sort_by(|(a, ca), (b, cb)| {
        for &(c, descending) in &terms {
            let ordering = compare(&ca[c].1, &cb[c].1);
            let ordering = if descending {
                ordering.reverse()
            } else {
                ordering
            };
            if ordering.is_ne() {
                return ordering;
            }
        }
        a.cmp(b)
    });
    let want: Vec<Key> = want.iter().map(|(id, _)| key_of(*id)).collect();
    let viewed = source.view(&ViewSpec::default().with_sort(Axis::Rows, spec.clone()));
    assert_eq!(viewed.applied.axis(Axis::Rows).sort, spec, "applied whole");
    let count = source.count(Axis::Rows);
    assert_eq!(source.window(Axis::Rows, 0..count).keys, want);
}
