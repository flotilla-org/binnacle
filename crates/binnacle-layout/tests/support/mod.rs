//! Random Layouts, with string IDs from a small pool (so lists repeat and
//! layers share IDs) and any text besides.

#![allow(dead_code)]

use binnacle_layout::view::{
    AggregateValue, All, AxisView, Comparison, Condition, ConditionKind, Depth, Direction, Expand,
    FilterSpec, Form, GroupedAxis, Grouping, Level, Placement, Scalar, Searched, Shown, SortColumn,
    TextMatch, TextQuery, True, ViewSpec,
};
use binnacle_layout::{Bands, EdgePins, Layout, Order, Pins, Sizes, Version};
use hegel::TestCase;
use hegel::generators as gs;

pub const POOL: [&str; 6] = ["a", "b", "c", "row 1", "é", ""];

pub fn id(tc: &TestCase) -> String {
    match tc.draw(gs::booleans()) {
        true => tc.draw(gs::sampled_from(POOL.to_vec())).to_owned(),
        false => tc.draw(gs::text().max_size(4)),
    }
}

pub fn ids(tc: &TestCase) -> Vec<String> {
    let n = tc.draw(gs::integers::<usize>().max_value(4));
    (0..n).map(|_| id(tc)).collect()
}

/// Any finite number: a JSON document holds no others.
pub fn number(tc: &TestCase) -> f64 {
    tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
}

fn text(tc: &TestCase) -> String {
    tc.draw(gs::text().max_size(6))
}

fn many<T>(tc: &TestCase, max: usize, f: impl Fn(&TestCase) -> T) -> Vec<T> {
    let n = tc.draw(gs::integers::<usize>().max_value(max));
    (0..n).map(|_| f(tc)).collect()
}

fn scalar(tc: &TestCase) -> Scalar {
    match tc.draw(gs::booleans()) {
        true => Scalar::Number { value: number(tc) },
        false => Scalar::Date {
            value: tc.draw(
                gs::integers::<i64>()
                    .min_value(-8_640_000_000_000_000)
                    .max_value(8_640_000_000_000_000),
            ) as f64,
        },
    }
}

fn condition(tc: &TestCase) -> Condition<String> {
    let kind = match tc.draw(gs::integers::<u8>().max_value(3)) {
        0 => ConditionKind::Text {
            query: TextMatch {
                text: text(tc),
                case_sensitive: tc.draw(gs::booleans()),
                regex: tc.draw(gs::booleans()),
            },
        },
        1 => ConditionKind::OneOf {
            values: many(tc, 3, |tc| tc.draw(gs::booleans()).then(|| text(tc))),
        },
        2 => ConditionKind::Compare {
            comparison: match tc.draw(gs::integers::<u8>().max_value(6)) {
                0 => Comparison::Equal { value: scalar(tc) },
                1 => Comparison::NotEqual { value: scalar(tc) },
                2 => Comparison::Less { value: scalar(tc) },
                3 => Comparison::AtMost { value: scalar(tc) },
                4 => Comparison::Greater { value: scalar(tc) },
                5 => Comparison::AtLeast { value: scalar(tc) },
                _ => Comparison::Between {
                    low: scalar(tc),
                    high: scalar(tc),
                },
            },
        },
        _ => ConditionKind::Native { text: text(tc) },
    };
    Condition {
        id: id(tc),
        kind,
        not: tc.draw(gs::booleans()),
    }
}

fn axis_view(tc: &TestCase) -> AxisView<String, String> {
    let quick = tc.draw(gs::booleans()).then(|| TextQuery {
        text: text(tc),
        case_sensitive: tc.draw(gs::booleans()),
        regex: tc.draw(gs::booleans()),
        searches: match tc.draw(gs::booleans()) {
            true => Searched::Shown(Shown::Shown),
            false => Searched::Items(ids(tc)),
        },
    });
    AxisView {
        filter: FilterSpec {
            quick,
            conditions: many(tc, 3, condition),
        },
        sort: many(tc, 3, |tc| SortColumn {
            id: id(tc),
            direction: tc.draw(gs::sampled_from(vec![
                Direction::Ascending,
                Direction::Descending,
            ])),
        }),
        hidden: ids(tc),
    }
}

fn placement(tc: &TestCase) -> Placement {
    tc.draw(gs::sampled_from(vec![
        Placement::Before,
        Placement::After,
        Placement::Off,
    ]))
}

fn level(tc: &TestCase) -> Level<String> {
    match tc.draw(gs::booleans()) {
        true => Level::Field {
            field: id(tc),
            subtotal: placement(tc),
        },
        false => Level::Values { values: True },
    }
}

fn grouped_axis(tc: &TestCase) -> GroupedAxis<String> {
    GroupedAxis {
        grand_total: placement(tc),
        form: tc.draw(gs::sampled_from(vec![Form::Outline, Form::Header])),
        expand: Expand {
            depth: match tc.draw(gs::booleans()) {
                true => Depth::Levels(tc.draw(gs::integers::<u32>())),
                false => Depth::All(All::All),
            },
            except: ids(tc),
        },
    }
}

fn grouping(tc: &TestCase) -> Grouping<String, String, String> {
    Grouping {
        row_levels: many(tc, 3, level),
        col_levels: many(tc, 2, level),
        values: many(tc, 3, |tc| AggregateValue {
            field: tc.draw(gs::booleans()).then(|| id(tc)),
            aggregate: text(tc),
            params: tc.draw(gs::booleans()).then(|| many(tc, 2, text)),
        }),
        slicers: many(tc, 2, condition),
        row_axis: grouped_axis(tc),
        col_axis: grouped_axis(tc),
    }
}

pub fn view(tc: &TestCase) -> ViewSpec<String, String> {
    ViewSpec {
        rows: axis_view(tc),
        cols: axis_view(tc),
        grouping: tc.draw(gs::booleans()).then(|| grouping(tc)),
    }
}

fn edge_pins(tc: &TestCase) -> EdgePins<String> {
    EdgePins {
        leading: ids(tc),
        trailing: ids(tc),
    }
}

pub fn sizes(tc: &TestCase) -> Vec<(String, f64)> {
    many(tc, 4, |tc| (id(tc), number(tc)))
}

/// A Layout with each part set or not.
pub fn layout(tc: &TestCase) -> Layout<String, String> {
    let set = || tc.draw(gs::booleans());
    Layout {
        version: Version,
        view: set().then(|| view(tc)),
        order: set().then(|| Order {
            rows: ids(tc),
            cols: ids(tc),
        }),
        pins: set().then(|| Pins {
            rows: edge_pins(tc),
            cols: edge_pins(tc),
        }),
        sizes: set().then(|| Sizes {
            rows: sizes(tc),
            cols: sizes(tc),
        }),
        bands: set().then(|| Bands {
            rows: number(tc),
            cols: number(tc),
        }),
    }
}
