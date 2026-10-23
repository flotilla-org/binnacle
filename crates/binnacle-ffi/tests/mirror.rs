//! The C API mirrors the Rust Grid: a handle and a Grid driven by the same
//! inputs request the same Windows, describe the same Snapshot and drain the
//! same Effects. Tags are encoded from the header's constants, which are the
//! independent source here; inputs with unknown tags must change nothing.

use std::slice;

use binnacle_core::{
    Action, Aggregate, AggregateValue, Axis, AxisConfig, Button, ChangeNotice, Compared,
    Comparison, Condition, ConditionKind, ConditionState, Depth, DropTarget, Edge, EditEnd, Effect,
    FilterSpec, FindDirection, FindMatch, FindQuery, FocusStep, Form, Grid, GroupOf, Grouping,
    HeaderClick, ItemKind, Key, Level, Located, Mode, Modifiers, MoveRefusal, OpenState, Outline,
    PinZone, Placement, Placements, PointerEvent, PointerKind, Scalar, Searched, SelectionUnit,
    SetPosition, Snapshot, SortColumn, SortDirection, SortSpec, Support, TextQuery, TextSupport,
    Totals, VALUES_FIELD, Value, ViewEdit, ViewSpec, Viewport,
};
use binnacle_ffi::*;
use hegel::Generator;
use hegel::TestCase;
use hegel::generators as gs;

const ROWS: u32 = 0;
const COLS: u32 = 1;

fn tag(axis: Axis) -> u32 {
    match axis {
        Axis::Rows => ROWS,
        Axis::Cols => COLS,
    }
}

#[derive(Clone, Debug)]
enum Op {
    /// A frame of this viewport, each Window answered with these outlines
    /// from its start if there are any.
    Frame(f32, f32, [Vec<Outline>; 2]),
    /// Outlines from this far into the last Window set on the axis.
    SetOutlines(Axis, u64, Vec<Outline>),
    /// Grouping the axis by a Key of the other axis, first declaring that
    /// the Table Source groups by it if the flag is set.
    GroupBy(Axis, Key, bool),
    /// Removing or moving the level of the axis on a field (`None`: the
    /// values pseudo-level).
    RemoveLevel(Axis, Option<Key>),
    MoveLevel(Axis, Option<Key>),
    /// Adding a value, first declaring its aggregate offered over its field
    /// if the flag is set; removing one.
    AddValue(AggregateValue, bool),
    RemoveValue(AggregateValue),
    CycleSort(Axis, Key, bool),
    Ungroup(Axis),
    /// Expand (0), collapse (1), toggle (2) or expand everything under (3)
    /// the group headed by a Key.
    Toggle(u8, Axis, Key),
    ExpandTo(Axis, Depth),
    SetTiers(Axis, Vec<f32>),
    SetForm(Axis, Form),
    Pointer(PointerEvent),
    /// A primary press, then a move away (no release) or the release.
    Gesture([PointerEvent; 2]),
    Action(Action),
    SetSize(Axis, u64, f32),
    SetHeader(Axis, f32),
    SetEdgeGap(Axis, f32),
    SetPinZone(Axis, f32),
    SetMinScrolling(Axis, f32),
    SetEndGap(Axis, f32),
    Pin(Axis, Edge, u64),
    SetPins(Axis, Edge, Vec<u64>),
    Unpin(Axis, u64),
    /// Lists the Keys held on the axis, then forgets these, or every one
    /// held.
    Forget(Axis, Vec<u64>, bool),
    Freeze(Axis, u64),
    /// A press `inset` inside the end of the band leading the axis (where its
    /// edge is, given a Gap), halfway across the other band as of the last
    /// frame, with this click count; then a move by `travel` along the axis.
    BandDrag {
        axis: Axis,
        inset: f32,
        clicks: u32,
        travel: f32,
    },
    /// A press on the middle of the Gap after the item at index `index` of
    /// the last frame's strip on the axis (wrapped), where its border is,
    /// halfway across the other band, with this click count; then a move by
    /// `travel` along the axis, and the release if `release`.
    BorderDrag {
        axis: Axis,
        index: usize,
        clicks: u32,
        travel: f32,
        release: bool,
    },
    Unit(SelectionUnit),
    HeaderClick(Axis, HeaderClick),
    Threshold(f32),
    ViewApplied(ViewSpec, Totals),
    ViewChanged(ViewSpec, Totals),
    RequestView(ViewSpec),
    ReapplyView,
    SetQuickFilter(Axis, Option<TextQuery>),
    QuickFilterSupport(Axis, Option<TextSupport>),
    SetSupport(Axis, Key, Support),
    AddCondition(Axis, Condition),
    RemoveCondition(Axis, Condition),
    ReplaceCondition(Axis, Condition, Condition),
    AddSlicer(Condition),
    RemoveSlicer(Condition),
    ReplaceSlicer(Condition, Condition),
    SetAggregates(Axis, Key, Vec<Aggregate>),
    HideSupport(Axis, bool),
    Hide(Axis, Vec<Key>),
    HideCap(Axis, usize),
    Show(Axis, Vec<Key>),
    SetFind(Option<FindQuery>),
    FindSupport(Option<TextSupport>),
    /// The answer to a find step: this match, or with `Some((r, c))` the
    /// cell of the last frame's strips at those indices (wrapped), within
    /// these groups on each axis if any.
    FindStepped(Option<FindMatch>, Option<(usize, usize)>, [Vec<Key>; 2]),
    /// A `located` answer about this Key, or (`None`) about the Key of the
    /// last `Locate` drained, if any.
    Located(Option<(Axis, Key)>, Located),
    Notify(Axis, ChangeNotice),
    SetMovable(Axis, bool),
    /// A press on the header of the item at index `from` of the last
    /// frame's strip on the axis (wrapped), a move over item `to`, and the
    /// release if `release`.
    HeaderDrag {
        axis: Axis,
        from: usize,
        to: usize,
        release: bool,
    },
    /// The answer to a move: moved, or refused with this reason.
    MoveAnswered(Option<String>),
    /// The host opened its editor on this cell.
    EditStarted(Key, Key),
    /// The host closed its editor.
    EditEnded(EditEnd),
    /// Text that is not UTF-8: refused.
    BadText(Vec<u8>),
    /// An input with an unknown tag: refused.
    BadTag(u32),
}

/// A size inside the Grid's per-axis size contract (default min 1.0, max
/// 2^20), usually a few units.
#[hegel::composite]
fn sane_size(tc: &TestCase) -> f32 {
    tc.draw(hegel::one_of!(
        gs::floats::<f32>().min_value(1.0).max_value(12.0),
        gs::floats::<f32>().min_value(1.0).max_value(1_048_576.0),
    ))
}

#[hegel::composite]
fn axis_config(tc: &TestCase) -> AxisConfig {
    let count = tc.draw(hegel::one_of!(
        gs::integers::<u64>().max_value(40),
        gs::integers::<u64>().min_value(1).max_value(40),
        gs::integers::<u64>(),
    ));
    let size = tc.draw(sane_size());
    let header = tc.draw(hegel::one_of!(
        gs::floats::<f32>().min_value(0.0).max_value(4.0),
        gs::floats::<f32>(),
    ));
    let gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(2.0),
        gs::floats::<f32>().min_value(0.0).allow_infinity(false),
    ));
    AxisConfig::new(count, size).header(header).gap(gap)
}

#[hegel::composite]
fn axes(tc: &TestCase) -> Axis {
    tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]))
}

#[hegel::composite]
fn modifiers(tc: &TestCase) -> Modifiers {
    Modifiers {
        shift: tc.draw(gs::booleans()),
        ctrl: tc.draw(gs::booleans()),
        alt: tc.draw(gs::booleans()),
        meta: tc.draw(gs::booleans()),
    }
}

#[hegel::composite]
fn pointer(tc: &TestCase) -> PointerEvent {
    let kind = tc.draw(gs::sampled_from(vec![
        PointerKind::Down,
        PointerKind::Move,
        PointerKind::Up,
    ]));
    let coord = || {
        hegel::one_of!(
            gs::floats::<f32>().min_value(-5.0).max_value(80.0),
            gs::floats::<f32>(),
        )
    };
    PointerEvent {
        kind,
        x: tc.draw(coord()),
        y: tc.draw(coord()),
        button: tc.draw(gs::sampled_from(vec![
            Button::Primary,
            Button::Primary,
            Button::Secondary,
            Button::Middle,
        ])),
        modifiers: tc.draw(modifiers()),
        clicks: tc.draw(gs::integers::<u32>().min_value(1).max_value(3)),
    }
}

#[hegel::composite]
fn action(tc: &TestCase) -> Action {
    match tc.draw(gs::integers::<u8>().max_value(13)) {
        13 => Action::Copy,
        12 => Action::LeaveOutline,
        10 => Action::Activate,
        11 => Action::BeginEdit,
        8 => Action::FindNext,
        9 => Action::FindPrevious,
        0 => Action::TypeText(tc.draw(gs::text().max_size(8))),
        1 => Action::MoveFocus {
            axis: tc.draw(axes()),
            by: tc.draw(gs::sampled_from(vec![
                FocusStep::Item,
                FocusStep::Page,
                FocusStep::Edge,
            ])),
            forward: tc.draw(gs::booleans()),
            extend: tc.draw(gs::booleans()),
        },
        2 => Action::SelectAll,
        3 => match tc.draw(gs::booleans()) {
            true => Action::SelectColumn,
            false => Action::SelectRow,
        },
        4 => Action::Cancel,
        5 => Action::Scroll {
            dx: tc.draw(gs::floats::<f32>().min_value(-50.0).max_value(50.0)),
            dy: tc.draw(gs::floats::<f32>().min_value(-50.0).max_value(50.0)),
        },
        6 => Action::ScrollTo {
            axis: tc.draw(axes()),
            fraction: tc.draw(gs::floats::<f64>().min_value(-0.5).max_value(1.5)),
        },
        _ => Action::ScrollBy {
            axis: tc.draw(axes()),
            items: tc.draw(gs::integers::<i64>().min_value(-20).max_value(20)),
        },
    }
}

/// A viewport length: usually one that shows a few items.
#[hegel::composite]
fn len(tc: &TestCase) -> f32 {
    tc.draw(hegel::one_of!(
        gs::floats::<f32>().min_value(10.0).max_value(60.0),
        gs::floats::<f32>().min_value(0.0).max_value(60.0),
        gs::sampled_from(vec![-1.0f32, f32::NAN, f32::INFINITY, f32::NEG_INFINITY]),
    ))
}

/// A Key the Window on `axis` may hold (see [`keys`]).
#[hegel::composite]
fn key(tc: &TestCase, axis: Axis) -> Key {
    let base: u64 = if axis == Axis::Rows { 1000 } else { 5000 };
    Key(base + tc.draw(gs::integers::<u64>().max_value(12)))
}

/// A base field to group by: one of a few column Keys, so that levels,
/// group edits and level edits meet.
#[hegel::composite]
fn field(tc: &TestCase) -> Key {
    Key(5000 + tc.draw(gs::integers::<u64>().max_value(2)))
}

/// A Position or a number of items, usually small enough to touch the
/// Window.
#[hegel::composite]
fn position(tc: &TestCase) -> u64 {
    tc.draw(hegel::one_of!(
        gs::integers::<u64>().max_value(12),
        gs::integers::<u64>().max_value(12),
        gs::integers::<u64>()
    ))
}

/// A Sort Spec for `axis`, by Keys of the other axis.
#[hegel::composite]
fn sort_spec(tc: &TestCase, axis: Axis) -> SortSpec {
    let by = tc.draw(gs::vecs(hegel::tuples!(key(axis.other()), gs::booleans())).max_size(3));
    SortSpec::new(
        by.into_iter()
            .map(|(key, descending)| SortColumn {
                key,
                direction: if descending {
                    SortDirection::Descending
                } else {
                    SortDirection::Ascending
                },
            })
            .collect(),
    )
}

/// A Text Query for `axis`, searching the shown items or Keys of the other
/// axis; sometimes with no text.
#[hegel::composite]
fn text_query(tc: &TestCase, axis: Axis) -> TextQuery {
    let searches = match tc.draw(gs::booleans()) {
        true => Searched::Shown,
        false => Searched::Keys(tc.draw(gs::vecs(key(axis.other())).max_size(3))),
    };
    let text = tc.draw(hegel::one_of!(
        gs::text().min_size(1).max_size(6),
        gs::just(String::new())
    ));
    TextQuery::new(text)
        .case_sensitive(tc.draw(gs::booleans()))
        .regex(tc.draw(gs::booleans()))
        .searching(searches)
}

/// A value for a OneOf condition: a blank, or text (empty or not ASCII
/// sometimes).
#[hegel::composite]
fn value(tc: &TestCase) -> Value {
    let text = gs::sampled_from(vec!["", "a", "Ø b"]).map(|t| Value::Text(t.into()));
    tc.draw(hegel::one_of!(gs::just(Value::Blank), text))
}

/// A number (NaN and -0 among them) or a date for a comparison.
#[hegel::composite]
fn scalar(tc: &TestCase) -> Scalar {
    let number = hegel::one_of!(
        gs::sampled_from(vec![1.5, -0.0, f64::NAN, f64::INFINITY]),
        gs::floats::<f64>(),
    )
    .map(Scalar::Number);
    let date = gs::integers::<i64>().map(Scalar::Date);
    tc.draw(hegel::one_of!(number, date))
}

/// Any comparison, between included.
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
        _ => Comparison::Between(s, tc.draw(scalar())),
    }
}

/// A Text, OneOf, comparison or Native condition on `axis`, read in a Key
/// of the other axis, maybe with `not`; sometimes with no text or no values.
#[hegel::composite]
fn condition(tc: &TestCase, axis: Axis) -> Condition {
    let key = tc.draw(key(axis.other()));
    let condition = match tc.draw(gs::integers::<u8>().max_value(3)) {
        0 => Condition::text(key, tc.draw(text_query(axis))),
        1 => Condition::one_of(key, tc.draw(gs::vecs(value()).max_size(3))),
        2 => Condition::compare(key, tc.draw(comparison())),
        _ => Condition::native(key, tc.draw(gs::text().max_size(8))),
    };
    condition.not(tc.draw(gs::booleans()))
}

/// A Find Query, by rows or by columns; now and then with no text.
#[hegel::composite]
fn find_query(tc: &TestCase) -> FindQuery {
    let mut query = tc.draw(text_query(Axis::Rows));
    if query.text.is_empty() && tc.draw(gs::booleans()) {
        query = TextQuery::new("a").searching(query.searches);
    }
    FindQuery::new(query).by(tc.draw(axes()))
}

#[hegel::composite]
fn placement(tc: &TestCase) -> Placement {
    tc.draw(gs::sampled_from(vec![
        Placement::Before,
        Placement::After,
        Placement::Off,
    ]))
}

/// A level grouping by a column Key, or now and then the values
/// pseudo-level.
#[hegel::composite]
fn level(tc: &TestCase) -> Level {
    match tc.draw(gs::weighted_booleans(0.2)) {
        true => Level::Values,
        false => Level::Field {
            field: tc.draw(field()),
            subtotal: tc.draw(placement()),
        },
    }
}

/// A value as a View Spec's grouping holds it: counting or over a column,
/// with parameters now and then.
#[hegel::composite]
fn aggregate_value(tc: &TestCase) -> AggregateValue {
    let field = tc.draw(gs::optional(key(Axis::Cols)));
    let name = tc.draw(gs::sampled_from(vec!["count", "sum", "Ø mean", ""]));
    let params = tc.draw(gs::vecs(gs::text().max_size(6)).max_size(2));
    AggregateValue::new(field, name).with_params(params)
}

/// An aggregate a Table Source might offer, with parameter names.
#[hegel::composite]
fn aggregate(tc: &TestCase) -> Aggregate {
    let name = tc.draw(gs::sampled_from(vec!["count", "sum", "Ø mean", ""]));
    let params = tc.draw(gs::vecs(gs::text().max_size(6)).max_size(2));
    Aggregate::new(name).with_params(params)
}

/// A View Spec's grouping: levels on either axis or none, values counting
/// or over a column, slicers, and each axis's layout.
#[hegel::composite]
fn grouping(tc: &TestCase) -> Grouping {
    let mut grouping = Grouping::default();
    for axis in Axis::ALL {
        *grouping.levels_mut(axis) = tc.draw(gs::vecs(level()).max_size(3));
        let g = grouping.axis_mut(axis);
        g.grand_total = tc.draw(placement());
        g.form = tc.draw(gs::sampled_from(vec![Form::Outline, Form::Header]));
        g.expand.depth = match tc.draw(gs::booleans()) {
            true => Depth::All,
            false => Depth::Levels(tc.draw(hegel::one_of!(
                gs::integers::<u32>().max_value(3),
                gs::integers::<u32>()
            ))),
        };
        g.expand.except = tc.draw(gs::vecs(key(axis)).max_size(3));
    }
    let aggregate = gs::sampled_from(vec!["count", "sum", "Ø mean", ""]);
    let params = gs::vecs(gs::text().max_size(6)).max_size(2);
    grouping.values = tc
        .draw(
            gs::vecs(hegel::tuples!(
                gs::optional(key(Axis::Cols)),
                aggregate,
                params
            ))
            .max_size(3),
        )
        .into_iter()
        .map(|(field, aggregate, params)| AggregateValue::new(field, aggregate).with_params(params))
        .collect();
    grouping.slicers = tc.draw(gs::vecs(condition(Axis::Rows)).max_size(2));
    grouping
}

/// An outline a grouped Table Source might send for an item of `axis`: any
/// kind and open state, ancestors among the axis's Keys, now and then a
/// depth that is not their count, and sometimes a group.
#[hegel::composite]
fn outline(tc: &TestCase, axis: Axis) -> Outline {
    let mut outline = Outline::leaf(tc.draw(gs::vecs(key(axis)).max_size(3)));
    outline.kind = tc.draw(gs::sampled_from(vec![
        ItemKind::Leaf,
        ItemKind::Group,
        ItemKind::Subtotal,
        ItemKind::GrandTotal,
    ]));
    if tc.draw(gs::weighted_booleans(0.2)) {
        outline.depth = tc.draw(gs::integers::<u32>());
    }
    outline.open = tc.draw(gs::sampled_from(vec![
        OpenState::Open,
        OpenState::Closed,
        OpenState::CantOpen,
    ]));
    outline.group = tc.draw(gs::booleans()).then(|| GroupOf {
        // Now and then a group of the values pseudo-level.
        field: match tc.draw(gs::weighted_booleans(0.2)) {
            true => VALUES_FIELD,
            false => tc.draw(key(axis.other())),
        },
        value: tc.draw(value()),
    });
    let path = tc.draw(gs::vecs(hegel::tuples!(key(axis.other()), value())).max_size(3));
    let path = path
        .into_iter()
        .map(|(field, value)| GroupOf { field, value });
    if tc.draw(gs::booleans()) {
        let size = tc.draw(gs::integers::<u64>());
        outline = outline.with_set(tc.draw(gs::integers::<u64>()), size);
    }
    outline
        .with_span(tc.draw(gs::integers::<u64>().max_value(20)))
        .with_path(path.collect())
}

/// Outlines to send with each axis's Window in a frame: none most of the
/// time.
#[hegel::composite]
fn frame_outlines(tc: &TestCase) -> [Vec<Outline>; 2] {
    Axis::ALL.map(|axis| match tc.draw(gs::weighted_booleans(0.3)) {
        true => tc.draw(gs::vecs(outline(axis)).min_size(1).max_size(12)),
        false => Vec::new(),
    })
}

/// A View Spec: usually sorting and filtering only the rows, now and then
/// grouped.
#[hegel::composite]
fn view_spec(tc: &TestCase) -> ViewSpec {
    let mut spec = ViewSpec::default();
    for axis in Axis::ALL {
        if axis == Axis::Rows || tc.draw(gs::weighted_booleans(0.25)) {
            spec = spec.with_sort(axis, tc.draw(sort_spec(axis)));
            let quick = tc.draw(gs::optional(text_query(axis)));
            let conditions = tc.draw(gs::vecs(condition(axis)).max_size(3));
            let filter = FilterSpec::default()
                .with_quick(quick)
                .with_conditions(conditions.into());
            spec = spec.with_filter(axis, filter);
        }
        if tc.draw(gs::weighted_booleans(0.25)) {
            spec = spec.with_hidden(axis, tc.draw(gs::vecs(key(axis)).max_size(3)));
        }
    }
    if tc.draw(gs::weighted_booleans(0.3)) {
        spec = spec.with_grouping(Some(tc.draw(grouping())));
    }
    spec
}

#[hegel::composite]
fn totals(tc: &TestCase) -> Totals {
    Totals {
        rows: tc.draw(gs::optional(position())),
        cols: tc.draw(gs::optional(position())),
    }
}

/// An answer the Table Source might send: an applied View Spec (usually
/// sorting only the rows), what it supports, a `located` (often for the Key
/// the Grid asked about), or a Change Notice; or a host's request for a View
/// Spec or a quick filter.
#[hegel::composite]
fn answer(tc: &TestCase) -> Op {
    let axis = tc.draw(axes());
    match tc.draw(gs::integers::<u8>().max_value(23)) {
        20 => Op::AddSlicer(tc.draw(condition(Axis::Rows))),
        // Often one the latest request has, so it goes.
        21 => Op::RemoveSlicer(tc.draw(condition(Axis::Rows))),
        22 => Op::ReplaceSlicer(
            tc.draw(condition(Axis::Rows)),
            tc.draw(condition(Axis::Rows)),
        ),
        23 => {
            let aggregate = hegel::tuples!(
                gs::text().max_size(6),
                gs::vecs(gs::text().max_size(4)).max_size(2)
            )
            .map(|(name, params)| Aggregate::new(name).with_params(params));
            Op::SetAggregates(
                axis,
                tc.draw(key(axis.other())),
                tc.draw(gs::vecs(aggregate).max_size(3)),
            )
        }
        // Mostly small, so some hides pass it; past HIDE_CAP it clamps.
        19 => Op::HideCap(
            axis,
            tc.draw(hegel::one_of!(
                gs::integers::<usize>().max_value(4),
                gs::integers::<usize>().min_value(990).max_value(5000),
                gs::just(usize::MAX)
            )),
        ),
        15 => Op::MoveAnswered(tc.draw(gs::optional(gs::text().max_size(8)))),
        16 => Op::HideSupport(axis, tc.draw(gs::weighted_booleans(0.75))),
        17 => Op::Hide(axis, tc.draw(gs::vecs(key(axis)).max_size(3))),
        18 => Op::Show(axis, tc.draw(gs::vecs(key(axis)).max_size(3))),
        10 => Op::SetFind(
            tc.draw(gs::weighted_booleans(0.9))
                .then(|| tc.draw(find_query())),
        ),
        11 => Op::FindSupport(tc.draw(gs::optional(gs::booleans().map(TextSupport::new)))),
        // Often, so a match lands while find is open and a frame shows it.
        12 | 13 => {
            let cell = hegel::tuples!(key(Axis::Rows), key(Axis::Cols));
            let found = tc
                .draw(gs::optional(cell))
                .map(|(r, c)| FindMatch::new(r, c));
            let shown = hegel::tuples!(gs::integers::<usize>(), gs::integers::<usize>());
            // Half the time within groups, by Keys outlines may name.
            let within = |axis| match tc.draw(gs::booleans()) {
                true => tc.draw(gs::vecs(key(axis)).max_size(2)),
                false => Vec::new(),
            };
            let within = [within(Axis::Rows), within(Axis::Cols)];
            Op::FindStepped(found, tc.draw(gs::optional(shown)), within)
        }
        0 => Op::ViewApplied(tc.draw(view_spec()), tc.draw(totals())),
        14 => Op::ViewChanged(tc.draw(view_spec()), tc.draw(totals())),
        7 => Op::AddCondition(axis, tc.draw(condition(axis))),
        // Often one the latest request has, so it goes.
        8 => Op::RemoveCondition(axis, tc.draw(condition(axis))),
        9 => Op::ReplaceCondition(axis, tc.draw(condition(axis)), tc.draw(condition(axis))),
        4 if tc.draw(gs::weighted_booleans(0.25)) => Op::ReapplyView,
        4 => Op::RequestView(tc.draw(view_spec())),
        5 => {
            let query = tc
                .draw(gs::weighted_booleans(0.8))
                .then(|| tc.draw(text_query(axis)));
            Op::SetQuickFilter(axis, query)
        }
        6 => Op::QuickFilterSupport(
            axis,
            tc.draw(gs::optional(gs::booleans().map(TextSupport::new))),
        ),
        3 => {
            let sort = tc.draw(gs::booleans());
            let text = tc.draw(gs::optional(gs::booleans().map(TextSupport::new)));
            Op::SetSupport(
                axis,
                tc.draw(key(axis.other())),
                Support::default()
                    .with_sort(sort)
                    .with_text(text)
                    .with_one_of(tc.draw(gs::booleans()))
                    .with_compare(tc.draw(gs::optional(gs::sampled_from(vec![
                        Compared::Numbers,
                        Compared::Dates,
                    ]))))
                    .with_group(tc.draw(gs::booleans()))
                    .with_native(tc.draw(gs::booleans()))
                    .with_pivot(tc.draw(gs::booleans()))
                    .with_subtotals(Placements {
                        before: tc.draw(gs::booleans()),
                        after: tc.draw(gs::booleans()),
                    }),
            )
        }
        1 => {
            let answer = match tc.draw(gs::integers::<u8>().max_value(2)) {
                0 => Located::Found(tc.draw(position())),
                1 => Located::Gone { suggest: None },
                _ => Located::Gone {
                    suggest: Some(tc.draw(position())),
                },
            };
            let about = tc
                .draw(gs::weighted_booleans(0.25))
                .then(|| (axis, tc.draw(key(axis))));
            Op::Located(about, answer)
        }
        _ => {
            let (at, n) = (tc.draw(position()), tc.draw(position()));
            let notice = match tc.draw(gs::integers::<u8>().max_value(7)) {
                6 => ChangeNotice::Collapsed { at, n },
                7 => ChangeNotice::Expanded { at, n },
                0 => ChangeNotice::Reset(n),
                5 => ChangeNotice::Refiltered(n),
                1 => ChangeNotice::Reordered,
                2 => ChangeNotice::Inserted { at, n },
                3 => ChangeNotice::Removed { at, n },
                _ => ChangeNotice::ContentChanged(at..at.saturating_add(n)),
            };
            Op::Notify(axis, notice)
        }
    }
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let coord = || gs::floats::<f32>().min_value(0.0).max_value(40.0);
    // The last value is the unknown tags', so new ops go before it.
    match tc.draw(gs::integers::<u8>().max_value(41)) {
        38 => {
            let axis = tc.draw(axes());
            // Mostly a field the groupings use, sometimes any column.
            let field = tc.draw(gs::optional(hegel::one_of!(
                field(),
                field(),
                key(Axis::Cols)
            )));
            match tc.draw(gs::booleans()) {
                true => Op::RemoveLevel(axis, field),
                false => Op::MoveLevel(axis, field),
            }
        }
        39 => {
            let value = tc.draw(aggregate_value());
            match tc.draw(gs::booleans()) {
                true => Op::AddValue(value, tc.draw(gs::booleans())),
                false => Op::RemoveValue(value),
            }
        }
        40 => {
            let axis = tc.draw(axes());
            let keys = tc.draw(gs::vecs(key(axis)).max_size(3));
            Op::Forget(
                axis,
                keys.iter().map(|k| k.0).collect(),
                tc.draw(gs::booleans()),
            )
        }
        37 => {
            let axis = tc.draw(axes());
            Op::CycleSort(axis, tc.draw(key(axis.other())), tc.draw(gs::booleans()))
        }
        33 => Op::Toggle(
            tc.draw(gs::integers::<u8>().max_value(3)),
            Axis::Rows,
            tc.draw(key(Axis::Rows)),
        ),
        34 => Op::ExpandTo(
            tc.draw(axes()),
            match tc.draw(gs::integers::<u32>().max_value(3)) {
                3 => Depth::All,
                n => Depth::Levels(n),
            },
        ),
        31 => {
            let axis = tc.draw(axes());
            let outlines = tc.draw(gs::vecs(outline(axis)).max_size(12));
            Op::SetOutlines(axis, tc.draw(position()), outlines)
        }
        32 => match tc.draw(gs::weighted_booleans(0.7)) {
            true => {
                let axis = tc.draw(axes());
                Op::GroupBy(axis, tc.draw(field()), tc.draw(gs::booleans()))
            }
            false => Op::Ungroup(tc.draw(axes())),
        },
        29 => Op::BorderDrag {
            axis: tc.draw(axes()),
            index: tc.draw(gs::integers::<usize>().max_value(20)),
            clicks: tc.draw(gs::integers::<u32>().min_value(1).max_value(2)),
            travel: tc.draw(gs::floats::<f32>().min_value(-6.0).max_value(6.0)),
            release: tc.draw(gs::weighted_booleans(0.8)),
        },
        30 => match tc.draw(gs::weighted_booleans(0.6)) {
            true => Op::EditStarted(tc.draw(key(Axis::Rows)), tc.draw(key(Axis::Cols))),
            false => {
                Op::EditEnded(tc.draw(gs::sampled_from(vec![EditEnd::Commit, EditEnd::Cancel])))
            }
        },
        28 => Op::SetEndGap(
            tc.draw(axes()),
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![-1.0f32, f32::NAN, f32::INFINITY, 1.0]),
                gs::floats::<f32>().min_value(0.0).max_value(8.0),
            )),
        ),
        27 => Op::SetPins(
            tc.draw(axes()),
            tc.draw(gs::sampled_from(Edge::ALL.to_vec())),
            tc.draw(gs::vecs(gs::integers::<u64>().max_value(10)).max_size(4)),
        ),
        25 => Op::SetPinZone(
            tc.draw(axes()),
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![-1.0f32, f32::NAN, f32::INFINITY]),
                gs::floats::<f32>().min_value(0.0).max_value(8.0),
            )),
        ),
        26 => Op::SetMinScrolling(
            tc.draw(axes()),
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![-1.0f32, f32::NAN, f32::INFINITY]),
                gs::floats::<f32>().min_value(0.0).max_value(40.0),
            )),
        ),
        24 => Op::SetEdgeGap(
            tc.draw(axes()),
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![-1.0f32, f32::NAN, f32::INFINITY]),
                gs::floats::<f32>().min_value(0.0).max_value(3.0),
            )),
        ),
        18 => Op::Pin(
            tc.draw(axes()),
            tc.draw(gs::sampled_from(Edge::ALL.to_vec())),
            tc.draw(gs::integers::<u64>().max_value(10)),
        ),
        23 => Op::Freeze(
            tc.draw(axes()),
            tc.draw(gs::integers::<u64>().max_value(10)),
        ),
        19 => Op::Unpin(
            tc.draw(axes()),
            tc.draw(gs::integers::<u64>().max_value(10)),
        ),
        // Often, so moves are answered while a refusal or a frame can show it.
        22 => Op::MoveAnswered(tc.draw(gs::optional(gs::text().max_size(8)))),
        16 => Op::SetHeader(
            tc.draw(axes()),
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![0.0f32, -1.0, f32::NAN, f32::INFINITY]),
                gs::floats::<f32>().min_value(0.0).max_value(8.0),
            )),
        ),
        20 | 21 => Op::HeaderDrag {
            axis: tc.draw(axes()),
            from: tc.draw(gs::integers::<usize>().max_value(20)),
            to: tc.draw(gs::integers::<usize>().max_value(20)),
            release: tc.draw(gs::weighted_booleans(0.8)),
        },
        17 => Op::BandDrag {
            axis: tc.draw(axes()),
            inset: tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)),
            clicks: tc.draw(gs::integers::<u32>().min_value(1).max_value(2)),
            travel: tc.draw(gs::floats::<f32>().min_value(-6.0).max_value(6.0)),
        },
        10 | 14 | 15 => tc.draw(answer()),
        0 | 1 | 11 => Op::Frame(tc.draw(len()), tc.draw(len()), tc.draw(frame_outlines())),
        2 => Op::Pointer(tc.draw(pointer())),
        3 | 12 | 13 => {
            let held = tc.draw(hegel::one_of!(gs::just(Modifiers::NONE), modifiers()));
            let down = PointerEvent::down(tc.draw(coord()), tc.draw(coord())).modifiers(held);
            let (x, y) = (tc.draw(coord()), tc.draw(coord()));
            let next = if tc.draw(gs::booleans()) {
                PointerEvent::moved(x, y)
            } else {
                PointerEvent::up(down.x, down.y)
            };
            Op::Gesture([down, next])
        }
        4 | 5 => Op::Action(tc.draw(action())),
        6 => Op::SetSize(
            tc.draw(axes()),
            tc.draw(gs::integers::<u64>().max_value(10)),
            // Ignored sizes, or ones inside the size contract.
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![0.0f32, -1.0, f32::NAN, f32::INFINITY]),
                sane_size(),
            )),
        ),
        7 => match tc.draw(gs::integers::<u8>().max_value(3)) {
            3 => Op::SetMovable(tc.draw(axes()), tc.draw(gs::weighted_booleans(0.75))),
            0 => Op::Unit(tc.draw(gs::sampled_from(vec![
                SelectionUnit::Cells,
                SelectionUnit::Rows,
                SelectionUnit::Cols,
            ]))),
            1 => Op::HeaderClick(
                tc.draw(axes()),
                tc.draw(gs::sampled_from(vec![
                    HeaderClick::Sort,
                    HeaderClick::Select,
                    HeaderClick::None,
                ])),
            ),
            _ => Op::Threshold(tc.draw(gs::floats::<f32>().min_value(-1.0).max_value(4.0))),
        },
        8 => Op::BadText(
            tc.draw(gs::vecs(gs::integers::<u8>()).max_size(6))
                .into_iter()
                .chain([0xff])
                .collect(),
        ),
        9 => Op::Pointer(PointerEvent::up(tc.draw(coord()), tc.draw(coord()))),
        35 => Op::SetTiers(
            tc.draw(axes()),
            tc.draw(
                gs::vecs(hegel::one_of!(
                    gs::sampled_from(vec![-1.0f32, f32::NAN, f32::INFINITY]),
                    gs::floats::<f32>().min_value(0.0).max_value(8.0),
                ))
                .max_size(4),
            ),
        ),
        36 => Op::SetForm(
            tc.draw(axes()),
            tc.draw(gs::sampled_from(vec![Form::Outline, Form::Header])),
        ),
        _ => Op::BadTag(tc.draw(gs::integers::<u32>().max_value(99))),
    }
}

fn pointer_event(e: PointerEvent) -> BinnaclePointerEvent {
    let bit = |held, b| if held { b } else { 0 };
    BinnaclePointerEvent {
        kind: match e.kind {
            PointerKind::Down => 0,
            PointerKind::Move => 1,
            PointerKind::Up => 2,
        },
        x: e.x,
        y: e.y,
        button: match e.button {
            Button::Primary => 0,
            Button::Secondary => 1,
            Button::Middle => 2,
        },
        modifiers: bit(e.modifiers.shift, 1)
            | bit(e.modifiers.ctrl, 2)
            | bit(e.modifiers.alt, 4)
            | bit(e.modifiers.meta, 8),
        clicks: e.clicks,
    }
}

/// Sends an Action through the C API.
unsafe fn send(h: *mut BinnacleGrid, action: &Action) -> i32 {
    unsafe {
        match action {
            Action::TypeText(t) => binnacle_grid_type_text(h, t.as_ptr(), t.len()),
            Action::MoveFocus {
                axis,
                by,
                forward,
                extend,
            } => {
                let by = match by {
                    FocusStep::Item => 0,
                    FocusStep::Page => 1,
                    FocusStep::Edge => 2,
                };
                binnacle_grid_move_focus(h, tag(*axis), by, u32::from(*forward), u32::from(*extend))
            }
            Action::SelectAll => binnacle_grid_select_all(h),
            Action::SelectColumn => binnacle_grid_select_column(h),
            Action::SelectRow => binnacle_grid_select_row(h),
            Action::Cancel => binnacle_grid_cancel(h),
            Action::Scroll { dx, dy } => binnacle_grid_scroll(h, *dx, *dy),
            Action::ScrollTo { axis, fraction } => {
                binnacle_grid_scroll_to(h, tag(*axis), *fraction)
            }
            Action::ScrollBy { axis, items } => binnacle_grid_scroll_by(h, tag(*axis), *items),
            Action::FindNext => binnacle_grid_find_next(h),
            Action::FindPrevious => binnacle_grid_find_previous(h),
            Action::Activate => binnacle_grid_activate(h),
            Action::BeginEdit => binnacle_grid_begin_edit(h),
            Action::LeaveOutline => binnacle_grid_leave_outline(h),
            Action::Copy => binnacle_grid_copy(h),
            #[allow(unreachable_patterns)] // `action` draws only the Actions above.
            _ => unreachable!(),
        }
    }
}

/// Sends an input with an unknown tag in slot `which`; returns what the call
/// returned.
unsafe fn send_bad_tag(h: *mut BinnacleGrid, which: u32) -> i32 {
    let ok = BinnaclePointerEvent {
        kind: 0,
        x: 10.0,
        y: 10.0,
        button: 0,
        modifiers: 0,
        clicks: 1,
    };
    unsafe {
        match which {
            0 => binnacle_grid_pointer(h, BinnaclePointerEvent { kind: 3, ..ok }),
            1 => binnacle_grid_pointer(h, BinnaclePointerEvent { button: 3, ..ok }),
            2 => binnacle_grid_pointer(
                h,
                BinnaclePointerEvent {
                    modifiers: 16,
                    ..ok
                },
            ),
            3 => binnacle_grid_move_focus(h, 2, 0, 1, 0),
            4 => binnacle_grid_move_focus(h, ROWS, 3, 1, 0),
            5 => binnacle_grid_scroll_to(h, 2, 1.0),
            6 => binnacle_grid_set_selection_unit(h, 3),
            7 => binnacle_grid_set_header_click(h, ROWS, 3),
            8 => binnacle_grid_set_size(h, 2, 0, 3.0),
            9 => {
                let unknown = BinnacleSortColumn {
                    key: 5000,
                    direction: 2,
                };
                let mut spec = BinnacleViewSpec::default();
                spec.axes[ROWS as usize] = BinnacleAxisView {
                    sort: &unknown,
                    sort_len: 1,
                    ..BinnacleAxisView::default()
                };
                binnacle_grid_view_applied(h, &spec, std::ptr::null())
            }
            10 => binnacle_grid_located(h, ROWS, 1000, 3, 0),
            11 => binnacle_grid_located(h, 2, 1000, 0, 0),
            12 => binnacle_grid_notify(h, ROWS, 8, 0, 0),
            13 => binnacle_grid_set_header(h, 2, 1.0),
            14 => binnacle_grid_set_support(h, 2, 5000, 0),
            15 => binnacle_grid_notify(h, 2, 1, 0, 0),
            16 => binnacle_grid_set_quick_filter_support(h, ROWS, 2),
            17 => binnacle_grid_set_quick_filter_support(h, 2, 1),
            18 => {
                let query = BinnacleTextQuery {
                    flags: 4,
                    ..BinnacleTextQuery::default()
                };
                binnacle_grid_set_quick_filter(h, ROWS, &query)
            }
            19 => {
                let query = BinnacleTextQuery {
                    searched: 2,
                    ..BinnacleTextQuery::default()
                };
                binnacle_grid_set_quick_filter(h, COLS, &query)
            }
            20 => {
                let bad = [b'a', 0xff];
                let query = BinnacleTextQuery {
                    text: BinnacleText {
                        data: bad.as_ptr(),
                        len: bad.len(),
                    },
                    ..BinnacleTextQuery::default()
                };
                let mut spec = BinnacleViewSpec::default();
                spec.axes[ROWS as usize].has_quick = 1;
                spec.axes[ROWS as usize].quick = query;
                binnacle_grid_request_view(h, &spec)
            }
            21 => {
                let mut spec = BinnacleViewSpec::default();
                spec.axes[COLS as usize].has_quick = 2;
                binnacle_grid_view_applied(h, &spec, std::ptr::null())
            }
            22 => binnacle_grid_set_support(h, ROWS, 5000, 4),
            23 => binnacle_grid_set_support(h, ROWS, 5000, 2048),
            24 => {
                let unknown = BinnacleCondition {
                    kind: 5,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_condition(h, ROWS, &unknown)
            }
            25 => {
                let negated = BinnacleCondition {
                    kind: BINNACLE_CONDITION_TEXT,
                    negated: 2,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_remove_condition(h, ROWS, &negated)
            }
            26 => {
                let other = BinnacleCondition::default();
                let mut spec = BinnacleViewSpec::default();
                spec.axes[ROWS as usize].conditions = &other;
                spec.axes[ROWS as usize].conditions_len = 1;
                binnacle_grid_request_view(h, &spec)
            }
            27 => binnacle_grid_replace_condition(h, ROWS, std::ptr::null(), std::ptr::null()),
            28 => {
                let text = BinnacleCondition {
                    kind: BINNACLE_CONDITION_TEXT,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_condition(h, 2, &text)
            }
            29 => {
                let value = BinnacleValue {
                    blank: 2,
                    ..BinnacleValue::default()
                };
                let one_of = BinnacleCondition {
                    kind: BINNACLE_CONDITION_ONE_OF,
                    values: &value,
                    values_len: 1,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_condition(h, ROWS, &one_of)
            }
            30 => {
                let bad = [b'a', 0xff];
                let value = BinnacleValue {
                    blank: 0,
                    text: BinnacleText {
                        data: bad.as_ptr(),
                        len: bad.len(),
                    },
                };
                let one_of = BinnacleCondition {
                    kind: BINNACLE_CONDITION_ONE_OF,
                    values: &value,
                    values_len: 1,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_condition(h, ROWS, &one_of)
            }
            31 => binnacle_grid_set_find_support(h, BINNACLE_FIND_REGEX),
            32 => binnacle_grid_set_find_support(h, 4),
            33 => {
                let find = BinnacleFindQuery {
                    by: 2,
                    ..BinnacleFindQuery::default()
                };
                binnacle_grid_set_find(h, &find)
            }
            34 => {
                let find = BinnacleFindQuery {
                    query: BinnacleTextQuery {
                        flags: 4,
                        ..BinnacleTextQuery::default()
                    },
                    by: ROWS,
                };
                binnacle_grid_set_find(h, &find)
            }
            35 => binnacle_grid_pin(h, ROWS, 2, 0),
            36 => binnacle_grid_pin(h, 2, BINNACLE_EDGE_LEADING, 0),
            37 => binnacle_grid_unpin(h, 2, 0),
            38 => binnacle_grid_set_movable(h, 2, 1),
            39 => binnacle_grid_set_movable(h, COLS, 2),
            40 => binnacle_grid_set_hide_support(h, ROWS, 2),
            41 => binnacle_grid_set_hide_support(h, 2, 1),
            42 => binnacle_grid_hide(h, 2, &5000, 1),
            43 => binnacle_grid_show(h, 2, &5000, 1),
            44 => binnacle_grid_pin(h, COLS, 2, 0),
            45 => binnacle_grid_freeze(h, 2, 0),
            46 => binnacle_grid_set_edge_gap(h, 2, 1.0),
            47 => binnacle_grid_set_hide_cap(h, 2, 1),
            48 => binnacle_grid_set_pin_zone(h, 2, 1.0),
            49 => binnacle_grid_set_min_scrolling(h, 2, 1.0),
            50 => binnacle_grid_set_pins(h, 2, BINNACLE_EDGE_LEADING, &5000, 1),
            51 => binnacle_grid_set_pins(h, COLS, 2, &5000, 1),
            52 => {
                let mut out = BinnacleSizeOverrides {
                    keys: std::ptr::null(),
                    sizes: std::ptr::null(),
                    len: 0,
                };
                binnacle_grid_size_overrides(h, 2, &mut out)
            }
            53 => binnacle_grid_set_end_gap(h, 2, 1.0),
            54 => binnacle_grid_edit_ended(h, 2),
            55 => binnacle_grid_set_support(
                h,
                ROWS,
                5000,
                BINNACLE_SUPPORT_COMPARE_NUMBERS | BINNACLE_SUPPORT_COMPARE_DATES,
            ),
            56 => {
                let compare = BinnacleCondition {
                    kind: BINNACLE_CONDITION_COMPARE,
                    compare: 7,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_condition(h, ROWS, &compare)
            }
            57 => {
                let compare = BinnacleCondition {
                    kind: BINNACLE_CONDITION_COMPARE,
                    compare: BINNACLE_COMPARE_BETWEEN,
                    high: BinnacleScalar {
                        kind: 2,
                        ..BinnacleScalar::default()
                    },
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_condition(h, ROWS, &compare)
            }
            58 => binnacle_grid_group_by(h, 2, 5000),
            59 => binnacle_grid_ungroup(h, 2),
            60 => binnacle_grid_set_outlines(h, 2, 0, std::ptr::null(), 0),
            75 => binnacle_grid_expand(h, 2, 0),
            76 => binnacle_grid_collapse(h, 2, 0),
            77 => binnacle_grid_toggle(h, 2, 0),
            78 => binnacle_grid_expand_all(h, 2, 0),
            79 => binnacle_grid_expand_to(h, 2, 0, 1),
            80 => binnacle_grid_expand_to(h, ROWS, 2, 1),
            87 => binnacle_grid_set_tiers(h, 2, std::ptr::null(), 0),
            88 => binnacle_grid_set_form(h, ROWS, 2),
            89 => binnacle_grid_cycle_sort(h, 2, 0, 0),
            90 => binnacle_grid_cycle_sort(h, ROWS, 0, 2),
            98 => binnacle_grid_forget(h, 2, std::ptr::null(), 0),
            99 => {
                let (mut keys, mut len) = (std::ptr::null(), 0usize);
                binnacle_grid_held_keys(h, 2, &mut keys, &mut len)
            }
            61..=64 | 91 => {
                let mut bad = BinnacleOutline::default();
                match which {
                    61 => bad.kind = 4,
                    62 => bad.open = 3,
                    63 => bad.has_group = 2,
                    91 => bad.has_set = 2,
                    _ => {
                        bad.has_group = 1;
                        bad.value.blank = 2;
                    }
                }
                binnacle_grid_set_outlines(h, ROWS, 0, &bad, 1)
            }
            65..=74 => {
                let levels = [BinnacleLevel {
                    kind: if which == 66 {
                        BINNACLE_LEVEL_OTHER
                    } else {
                        BINNACLE_LEVEL_FIELD
                    },
                    field: 5000,
                    subtotal: if which == 67 {
                        3
                    } else {
                        BINNACLE_PLACEMENT_OFF
                    },
                }];
                let bad = [b'a', 0xff];
                let param = BinnacleText {
                    data: bad.as_ptr(),
                    len: if which == 74 { bad.len() } else { 1 },
                };
                let value = BinnacleAggregateValue {
                    has_field: if which == 71 { 2 } else { 0 },
                    field: 5000,
                    aggregate: BinnacleText {
                        data: bad.as_ptr(),
                        len: if which == 72 { bad.len() } else { 1 },
                    },
                    params: &param,
                    params_len: 1,
                };
                let slicer = BinnacleCondition {
                    kind: if which == 73 {
                        5
                    } else {
                        BINNACLE_CONDITION_ONE_OF
                    },
                    ..BinnacleCondition::default()
                };
                let mut spec = BinnacleViewSpec {
                    has_grouping: if which == 65 { 2 } else { 1 },
                    ..BinnacleViewSpec::default()
                };
                let g = &mut spec.grouping;
                g.axes[ROWS as usize].levels = levels.as_ptr();
                g.axes[ROWS as usize].levels_len = levels.len();
                g.axes[COLS as usize] = BinnacleGroupedAxis {
                    grand_total: if which == 68 {
                        3
                    } else {
                        BINNACLE_PLACEMENT_OFF
                    },
                    form: if which == 69 { 2 } else { BINNACLE_FORM_HEADER },
                    expand_all: if which == 70 { 2 } else { 1 },
                    ..BinnacleGroupedAxis::default()
                };
                (g.values, g.values_len) = (&value, 1);
                (g.slicers, g.slicers_len) = (&slicer, 1);
                binnacle_grid_request_view(h, &spec)
            }
            81 => {
                let unknown = BinnacleCondition {
                    kind: 5,
                    ..BinnacleCondition::default()
                };
                binnacle_grid_add_slicer(h, &unknown)
            }
            82 => binnacle_grid_replace_slicer(h, std::ptr::null(), std::ptr::null()),
            83 | 84 => {
                let bad = [b'a', 0xff];
                let param = BinnacleText {
                    data: bad.as_ptr(),
                    len: bad.len(),
                };
                let aggregate = BinnacleAggregate {
                    name: BinnacleText {
                        data: bad.as_ptr(),
                        len: if which == 83 { bad.len() } else { 1 },
                    },
                    params: &param,
                    params_len: 1,
                };
                binnacle_grid_set_aggregates(h, ROWS, 0, &aggregate, 1)
            }
            85 => binnacle_grid_set_aggregates(h, 2, 0, std::ptr::null(), 0),
            92 => binnacle_grid_remove_level(h, ROWS, std::ptr::null()),
            93 => binnacle_grid_move_level(h, 2, &level_to_c(None)),
            94 => {
                let level = BinnacleLevel {
                    kind: BINNACLE_LEVEL_OTHER,
                    ..BinnacleLevel::default()
                };
                binnacle_grid_move_level(h, ROWS, &level)
            }
            95 | 96 => {
                let bad = [0xff];
                let value = BinnacleAggregateValue {
                    has_field: if which == 95 { 2 } else { 0 },
                    aggregate: BinnacleText {
                        data: bad.as_ptr(),
                        len: usize::from(which == 96),
                    },
                    ..BinnacleAggregateValue::default()
                };
                match which {
                    95 => binnacle_grid_add_value(h, &value),
                    _ => binnacle_grid_remove_value(h, &value),
                }
            }
            97 => binnacle_grid_add_value(h, std::ptr::null()),
            _ => binnacle_grid_set_quick_filter(h, 2, std::ptr::null()),
        }
    }
}

fn same_f32(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits()
}

/// Every field of the C Snapshot matches the Rust one.
unsafe fn check_snapshot(c: &BinnacleSnapshot, rust: &Snapshot<'_>) {
    let d = rust.damage();
    let damage = u32::from(d.layout) | u32::from(d.selection) << 1 | u32::from(d.positions) << 2;
    assert_eq!(c.generation, rust.generation());
    assert_eq!(c.damage, damage);
    let range = rust.selection_range();
    for axis in Axis::ALL {
        let a = &c.axes[tag(axis) as usize];
        assert_eq!(a.count, rust.count(axis));
        assert!(same_f32(a.extent, rust.extent(axis)));
        assert!(same_f32(a.header, rust.header(axis)));
        assert!(same_f32(a.gap, rust.gap(axis)));
        let anchor = rust.scroll_anchor(axis);
        assert_eq!(a.anchor.position, anchor.position);
        assert!(same_f32(a.anchor.offset, anchor.offset));
        let thumb = rust.thumb(axis);
        assert_eq!(a.thumb.fraction.to_bits(), thumb.fraction.to_bits());
        assert_eq!(a.thumb.size.to_bits(), thumb.size.to_bits());
        let strip = unsafe { slice::from_raw_parts(a.strip, a.strip_len) };
        let expected: Vec<_> = rust.drawn(axis).collect();
        assert_eq!(a.pinned_len, rust.pinned(axis, Edge::Leading).len());
        assert_eq!(a.trailing_len, rust.pinned(axis, Edge::Trailing).len());
        let scrolling = rust.scrolling(axis);
        assert!(same_f32(a.scrolling_start, scrolling.start));
        assert!(same_f32(a.scrolling_end, scrolling.end));
        let [leading, trailing] = Edge::ALL.map(|edge| rust.pinned_region(axis, edge));
        assert!(same_f32(a.leading_start, leading.start));
        assert!(same_f32(a.leading_end, leading.end));
        assert!(same_f32(a.trailing_start, trailing.start));
        assert!(same_f32(a.trailing_end, trailing.end));
        assert_eq!(strip.len(), expected.len(), "{axis:?} strip length");
        for (got, want) in strip.iter().zip(expected) {
            assert_eq!(got.position, want.position);
            assert_eq!(got.has_key != 0, want.key.is_some());
            if let Some(Key(k)) = want.key {
                assert_eq!(got.key, k);
            }
            assert!(same_f32(got.offset, want.offset));
            assert!(same_f32(got.size, want.size));
            assert!(got.has_outline <= 1);
            let pending = want.key.is_some_and(|k| rust.toggle_pending(axis, k));
            assert_eq!(
                got.toggle_pending != 0,
                pending && rust.outline(axis, want.position).is_some(),
                "{axis:?} toggle pending at {}",
                want.position
            );
            let outline = (got.has_outline != 0).then(|| unsafe { outline_from_c(&got.outline) });
            assert_eq!(
                outline.as_ref(),
                rust.outline(axis, want.position),
                "{axis:?} outline at {}",
                want.position
            );
        }
        let headers = unsafe { slice::from_raw_parts(a.headers, a.headers_len) };
        let expected: Vec<_> = rust.headers(axis).collect();
        assert_eq!(headers.len(), expected.len(), "{axis:?} headers length");
        for (got, want) in headers.iter().zip(&expected) {
            assert_eq!(got.position, want.position);
            assert_eq!(got.has_key != 0, want.key.is_some());
            assert_eq!(got.key, want.key.map_or(0, |k| k.0));
            let r = &want.rect;
            assert!(
                same_f32(got.rect.x, r.x)
                    && same_f32(got.rect.y, r.y)
                    && same_f32(got.rect.width, r.width)
                    && same_f32(got.rect.height, r.height)
            );
            let (direction, place) = want.sort.applied.unzip();
            let flags = u32::from(direction.is_some())
                | u32::from(direction == Some(SortDirection::Descending)) << 1
                | u32::from(want.sort.pending) << 2
                | u32::from(want.selected) << 3
                | u32::from(want.hovered) << 4
                | u32::from(want.filter.filtered) << 5
                | u32::from(want.filter.pending) << 6
                | u32::from(want.filter.not_applied) << 7
                | u32::from(want.move_pending) << 8;
            assert_eq!(got.flags, flags, "{want:?}");
            assert_eq!(got.sort_place as usize, place.unwrap_or(0));
            let support = want
                .key
                .map_or(0, |k| support_bits(rust.support(axis.other(), k)));
            assert_eq!(got.support, support, "{want:?} support");
            let offered = want
                .key
                .map_or(&[][..], |k| rust.aggregates(axis.other(), k));
            assert_eq!(
                unsafe { aggregates_from_c(got) },
                offered,
                "{want:?} aggregates"
            );
        }
        assert_eq!(c.has_focus != 0, rust.focus(axis).is_some());
        if let Some(f) = rust.focus(axis) {
            assert_eq!(a.focus.position, f.position);
            assert_eq!(a.focus.has_key != 0, f.key.is_some());
            if let Some(Key(k)) = f.key {
                assert_eq!(a.focus.key, k);
            }
        }
        let runs = unsafe { slice::from_raw_parts(a.selection_runs, a.selection_runs_len) };
        let runs: Vec<_> = runs.iter().map(|r| r.start..r.end).collect();
        assert_eq!(
            runs,
            range.map_or(&[][..], |r| r.span(axis)),
            "{axis:?} runs"
        );
        assert_eq!(
            (a.has_total != 0).then_some(a.total),
            rust.total(axis),
            "{axis:?} total"
        );
        let quick = rust
            .quick_filter(axis)
            .map_or(0, |s| BINNACLE_QUICK_FILTER | u32::from(s.regex) << 1);
        assert_eq!(a.quick_filter, quick, "{axis:?} quick filter support");
        let listed = unsafe { slice::from_raw_parts(a.conditions, a.conditions_len) };
        let got: Vec<(Condition, ConditionState)> = listed
            .iter()
            .map(|l| {
                let state = match l.state {
                    0 => ConditionState::Applied,
                    1 => ConditionState::Adding,
                    2 => ConditionState::Removing,
                    _ => ConditionState::NotApplied,
                };
                (unsafe { condition_from_c(&l.condition) }, state)
            })
            .collect();
        assert_eq!(got, rust.conditions(axis), "{axis:?} conditions");
        let listed = unsafe { c_slice(a.levels, a.levels_len) };
        let got: Vec<(Level, ConditionState)> = listed
            .iter()
            .map(|l| (level_from_c(&l.level), state_from_c(l.state)))
            .collect();
        assert_eq!(got, rust.levels(axis), "{axis:?} levels");
        let listed = unsafe { c_slice(a.sorts, a.sorts_len) };
        let got: Vec<(SortColumn, ConditionState)> = listed
            .iter()
            .map(|l| {
                let direction = match l.column.direction {
                    BINNACLE_SORT_ASCENDING => SortDirection::Ascending,
                    _ => SortDirection::Descending,
                };
                let column = SortColumn {
                    key: Key(l.column.key),
                    direction,
                };
                (column, state_from_c(l.state))
            })
            .collect();
        assert_eq!(got, rust.sorts(axis), "{axis:?} sorts");
        let picked = unsafe { slice::from_raw_parts(a.picked, a.picked_len) };
        let keys: Vec<u64> = rust.picked(axis).iter().map(|k| k.0).collect();
        assert_eq!(picked, keys, "{axis:?} picks");
        assert_eq!(a.movable != 0, rust.movable(axis), "{axis:?} movable");
        assert_eq!(
            a.hide_support != 0,
            rust.hide_support(axis),
            "{axis:?} hides"
        );
        let hidden = unsafe { slice::from_raw_parts(a.hidden, a.hidden_len) };
        let want: Vec<u64> = rust.hidden(axis).iter().map(|k| k.0).collect();
        assert_eq!(hidden, want, "{axis:?} hidden");
        assert_eq!(a.hide_cap, rust.hide_cap(axis), "{axis:?} hide cap");
        let pins = [
            (a.leading_pins, a.leading_pins_len, Edge::Leading),
            (a.trailing_pins, a.trailing_pins_len, Edge::Trailing),
        ];
        for (keys, len, edge) in pins {
            let got = unsafe { slice::from_raw_parts(keys, len) };
            let want: Vec<u64> = rust.pins(axis, edge).map(|k| k.0).collect();
            assert_eq!(got, want, "{axis:?} {edge:?} pins");
        }
        assert_eq!(
            a.band_focused != 0,
            rust.band_focused(axis),
            "{axis:?} band focused"
        );
        let tiers = unsafe { slice::from_raw_parts(a.tiers, a.tiers_len) };
        let got: Vec<_> = tiers.iter().map(|t| t.start..t.end).collect();
        assert_eq!(got, rust.tiers(axis), "{axis:?} tiers");
        let corner = unsafe { slice::from_raw_parts(a.corner_tiers, a.corner_tiers_len) };
        let levels = unsafe { slice::from_raw_parts(a.corner_levels, a.corner_levels_len) };
        let want = rust.corner().tiers(axis);
        assert_eq!(corner.len(), want.len(), "{axis:?} corner tiers");
        for (got, want) in corner.iter().zip(want) {
            assert!(same_f32(got.across.start, want.across.start));
            assert!(same_f32(got.across.end, want.across.end));
            let named = &levels[got.levels_start..got.levels_start + got.levels_len];
            let named: Vec<(u32, Option<Key>)> = (named.iter())
                .map(|l| match l.kind {
                    BINNACLE_LEVEL_FIELD => (l.index, Some(Key(l.field))),
                    kind => {
                        assert_eq!((kind, l.field), (BINNACLE_LEVEL_VALUES, 0));
                        (l.index, None)
                    }
                })
                .collect();
            let want: Vec<(u32, Option<Key>)> =
                want.levels.iter().map(|l| (l.index, l.field)).collect();
            assert_eq!(named, want, "{axis:?} corner levels");
        }
        let spans = unsafe { slice::from_raw_parts(a.spans, a.spans_len) };
        assert_eq!(spans.len(), rust.spans(axis).len(), "{axis:?} spans");
        for (got, want) in spans.iter().zip(rust.spans(axis)) {
            let open = match want.open {
                OpenState::Open => BINNACLE_OPEN_STATE_OPEN,
                OpenState::Closed => BINNACLE_OPEN_STATE_CLOSED,
                OpenState::CantOpen => BINNACLE_OPEN_STATE_CANT_OPEN,
            };
            assert_eq!(
                (got.axis, got.tier, got.key, got.open, got.first, got.last),
                (
                    tag(want.axis),
                    want.tier,
                    want.key.0,
                    open,
                    want.first,
                    want.last
                ),
                "{want:?}"
            );
            assert_eq!((got.has_field != 0).then_some(Key(got.field)), want.field);
            assert_eq!(
                (got.cut_before != 0, got.cut_after != 0),
                (want.cut_before, want.cut_after)
            );
            let same = |r: &BinnacleRect, w: &binnacle_core::Rect| {
                same_f32(r.x, w.x)
                    && same_f32(r.y, w.y)
                    && same_f32(r.width, w.width)
                    && same_f32(r.height, w.height)
            };
            assert!(same(&got.rect, &want.rect) && same(&got.visible, &want.visible));
        }
    }
    let rect = |r: &BinnacleRect, w: &binnacle_core::Rect| {
        same_f32(r.x, w.x)
            && same_f32(r.y, w.y)
            && same_f32(r.width, w.width)
            && same_f32(r.height, w.height)
    };
    assert_eq!(c.has_focus_rect != 0, rust.focus_rect().is_some());
    if let Some(r) = rust.focus_rect() {
        assert!(rect(&c.focus_rect, &r));
    }
    assert_eq!(c.has_selection_range != 0, range.is_some());
    let got: Vec<(Condition, ConditionState)> = unsafe { c_slice(c.slicers, c.slicers_len) }
        .iter()
        .map(|l| {
            (
                unsafe { condition_from_c(&l.condition) },
                state_from_c(l.state),
            )
        })
        .collect();
    assert_eq!(got, rust.slicers(), "slicers");
    let got: Vec<(AggregateValue, ConditionState)> = unsafe { c_slice(c.values, c.values_len) }
        .iter()
        .map(|l| {
            (
                unsafe { aggregate_value_from_c(&l.value) },
                state_from_c(l.state),
            )
        })
        .collect();
    assert_eq!(got, rust.values(), "values");

    let rects = unsafe { slice::from_raw_parts(c.selection_rects, c.selection_rects_len) };
    assert_eq!(rects.len(), rust.selection_rects().len());
    assert!(
        rects
            .iter()
            .zip(rust.selection_rects())
            .all(|(g, w)| rect(g, w))
    );
    let cells = unsafe { slice::from_raw_parts(c.cell_states, c.cell_states_len) };
    let rows: Vec<_> = rust.drawn(Axis::Rows).collect();
    let cols: Vec<_> = rust.drawn(Axis::Cols).collect();
    assert_eq!(cells.len(), rows.len() * cols.len());
    for (r, row) in rows.iter().enumerate() {
        for (k, col) in cols.iter().enumerate() {
            let s = rust.cell_state(row, col);
            let want =
                u8::from(s.focused) | u8::from(s.selected) << 1 | u8::from(s.current_match) << 2;
            assert_eq!(cells[r * cols.len() + k], want, "cell {r},{k}");
        }
    }
    #[allow(unreachable_patterns)] // Modes added to the Grid later read as OTHER.
    let mode = match rust.mode() {
        Mode::Idle => 1,
        Mode::DraggingSelection => 2,
        Mode::ResizingBand { .. } => 3,
        Mode::MovingHeaders { .. } => 4,
        _ => 0,
    };
    assert_eq!(c.mode, mode);
    assert_eq!(c.capturing != 0, rust.capturing());
    let find = (c.has_find != 0).then(|| unsafe { find_from_c(&c.find) });
    assert_eq!(find.as_ref(), rust.find(), "find");
    let found = (c.has_find_match != 0)
        .then(|| FindMatch::new(Key(c.find_match.row), Key(c.find_match.col)));
    assert_eq!(found, rust.find_match(), "find match");
    let support = rust.find_support().map_or(0, |s| {
        BINNACLE_FIND | (u32::from(s.regex) * BINNACLE_FIND_REGEX)
    });
    assert_eq!(c.find_support, support, "find support");
    assert_eq!(c.view_pending != 0, rust.view_pending(), "view pending");
    assert_eq!(
        c.outline_focused != 0,
        rust.outline_focused(),
        "outline focused"
    );
    assert_eq!(c.focused_tier, rust.focused_tier(), "focused tier");
    let pending = (c.has_pending_move != 0).then(|| {
        let keys = unsafe { slice::from_raw_parts(c.pending_move_keys, c.pending_move_keys_len) };
        (
            c.pending_move_axis,
            keys.iter().map(|&k| Key(k)).collect::<Vec<_>>(),
        )
    });
    let want = rust.pending_move().map(|(a, k)| (tag(a), k.to_vec()));
    assert_eq!(pending, want, "pending move");
    let refusal = (c.has_move_refusal != 0).then(|| {
        let text = unsafe { slice::from_raw_parts(c.move_refusal.data, c.move_refusal.len) };
        (
            c.move_refusal_axis,
            std::str::from_utf8(text).unwrap().to_owned(),
        )
    });
    let want = rust.move_refusal().map(|(a, r)| (tag(a), r.to_owned()));
    assert_eq!(refusal, want, "move refusal");
    let edge_of = |t: u32| match t {
        BINNACLE_EDGE_LEADING => Edge::Leading,
        BINNACLE_EDGE_TRAILING => Edge::Trailing,
        _ => panic!("edge tag {t}"),
    };
    let drop = (c.has_drop != 0).then(|| {
        let target = match c.drop_target {
            BINNACLE_DROP_MOVE => DropTarget::Move {
                to: c.drop_position,
                unpin: c.drop_unpin != 0,
            },
            BINNACLE_DROP_UNPIN => DropTarget::Unpin {
                at: c.drop_position,
            },
            BINNACLE_DROP_PIN => DropTarget::Pin {
                edge: edge_of(c.drop_edge),
                before: (c.has_drop_before != 0).then_some(Key(c.drop_before)),
                reorder: c.drop_reorder != 0,
            },
            BINNACLE_DROP_REFUSED => DropTarget::Refused {
                to: c.drop_position,
                why: match c.drop_refusal {
                    BINNACLE_MOVE_REFUSAL_BETWEEN_GROUPS => MoveRefusal::BetweenGroups,
                    BINNACLE_MOVE_REFUSAL_SUMMARY => MoveRefusal::Summary,
                    t => panic!("move refusal tag {t}"),
                },
            },
            t => panic!("drop target tag {t}"),
        };
        (c.drop_axis, target, c.drop_offset.to_bits())
    });
    let want = (rust.drop_indicator()).map(|d| (tag(d.axis), d.target, d.offset.to_bits()));
    assert_eq!(drop, want, "drop indicator");
    let zones: Vec<PinZone> = c.pin_zones[..c.pin_zones_len]
        .iter()
        .map(|z| PinZone {
            axis: if z.axis == ROWS {
                Axis::Rows
            } else {
                Axis::Cols
            },
            edge: edge_of(z.edge),
            offset: z.offset,
            size: z.size,
            hot: z.hot != 0,
        })
        .collect();
    assert_eq!(zones, rust.pin_zones(), "pin zones");
    match rust.hovered_header() {
        None => assert_eq!(c.has_hovered_header, 0),
        Some((axis, item)) => {
            assert_eq!(c.has_hovered_header, 1);
            assert_eq!(c.hovered_axis, tag(axis));
            assert_eq!(rust.drawn(axis).nth(c.hovered_index), Some(item));
        }
    }
}

/// Drains both queues and checks they carried the same Effects; returns
/// the last `Locate` drained.
unsafe fn check_effects(h: *mut BinnacleGrid, grid: &mut Grid) -> Option<(Axis, Key)> {
    let mut asked = None;
    loop {
        let mut c = BinnacleEffect::default();
        let polled = unsafe { binnacle_grid_poll_effect(h, &mut c) };
        let Some(want) = grid.poll_effect() else {
            assert_eq!(polled, 0, "an extra Effect: {c:?}");
            return asked;
        };
        assert_eq!(polled, 1, "missing {want:?}");
        assert_eq!(c.generation, want.generation);
        let mut view = ViewSpec::default();
        let mut edit = BinnacleViewEdit::default();
        let mut edit_keys: Vec<u64> = Vec::new();
        let mut edit_value: Option<AggregateValue> = None;
        let mut size = 0.0;
        let mut find = None;
        let mut from = None;
        let mut direction = BINNACLE_FIND_NEXT;
        let mut moved: (Vec<u64>, u64) = (Vec::new(), 0);
        let mut edge = BINNACLE_EDGE_LEADING;
        let mut drill_down = None;
        let mut copied = ([Vec::new(), Vec::new()], [0, 0]);
        let (kind, row, col, axis, key) = match &want.effect {
            Effect::TextTyped { text } => {
                let got = unsafe { slice::from_raw_parts(c.text.data, c.text.len) };
                assert_eq!(got, text.as_bytes());
                (1, 0, 0, 0, 0)
            }
            Effect::FocusChanged { row, col } => (2, row.0, col.0, 0, 0),
            Effect::Activated {
                row,
                col,
                drill_down: d,
            } => {
                drill_down = d.as_ref().map(|d| d.conditions.clone());
                (12, row.0, col.0, 0, 0)
            }
            Effect::CopyRequested {
                rows,
                cols,
                row_labels,
                col_labels,
            } => {
                copied = (
                    [rows.clone(), cols.clone()],
                    [u32::from(*row_labels), u32::from(*col_labels)],
                );
                (17, 0, 0, 0, 0)
            }
            Effect::EditRequested { row, col } => (13, row.0, col.0, 0, 0),
            Effect::EditCancelled { row, col } => (14, row.0, col.0, 0, 0),
            Effect::SizeChanged { axis, key, size: s } => {
                size = *s;
                (15, 0, 0, tag(*axis), key.0)
            }
            Effect::AutoSizeRequested { axis, key } => (16, 0, 0, tag(*axis), key.0),
            Effect::SelectionChanged => (3, 0, 0, 0, 0),
            Effect::ViewRequested { spec, edit: e } => {
                view = spec.clone();
                let on = |kind, axis| BinnacleViewEdit {
                    kind,
                    axis: tag(axis),
                    ..BinnacleViewEdit::default()
                };
                edit = match *e {
                    ViewEdit::CycleSort { axis, key, multi } => BinnacleViewEdit {
                        kind: 1,
                        axis: tag(axis),
                        key: key.0,
                        multi: u32::from(multi),
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::Hide { axis, ref keys } => {
                        edit_keys = keys.iter().map(|k| k.0).collect();
                        on(9, axis)
                    }
                    ViewEdit::Show { axis, ref keys } => {
                        edit_keys = keys.iter().map(|k| k.0).collect();
                        on(10, axis)
                    }
                    ViewEdit::SetQuickFilter { axis, .. } => on(2, axis),
                    ViewEdit::Replace => on(3, Axis::Rows),
                    ViewEdit::ShownChanged { axis } => on(4, axis),
                    ViewEdit::Reapply => on(8, Axis::Rows),
                    ViewEdit::AddCondition {
                        axis,
                        ref condition,
                    } => BinnacleViewEdit {
                        key: condition.key.0,
                        ..on(5, axis)
                    },
                    ViewEdit::RemoveCondition {
                        axis,
                        ref condition,
                    } => BinnacleViewEdit {
                        key: condition.key.0,
                        ..on(6, axis)
                    },
                    ViewEdit::ReplaceCondition { axis, ref to, .. } => BinnacleViewEdit {
                        key: to.key.0,
                        ..on(7, axis)
                    },
                    ViewEdit::Group { axis, field } => BinnacleViewEdit {
                        key: field.0,
                        ..on(11, axis)
                    },
                    ViewEdit::Ungroup { axis } => on(12, axis),
                    ViewEdit::Expand { axis, key } => BinnacleViewEdit {
                        key: key.0,
                        ..on(13, axis)
                    },
                    ViewEdit::Collapse { axis, key } => BinnacleViewEdit {
                        key: key.0,
                        ..on(14, axis)
                    },
                    ViewEdit::ExpandAll { axis, key } => BinnacleViewEdit {
                        key: key.0,
                        ..on(15, axis)
                    },
                    ViewEdit::ExpandTo { axis, depth } => BinnacleViewEdit {
                        expand_all: u32::from(depth == Depth::All),
                        expand_depth: match depth {
                            Depth::All => 0,
                            Depth::Levels(n) => n,
                        },
                        ..on(16, axis)
                    },
                    // A slicer is on no axis: the edit names its Key, the
                    // axis left as its zero.
                    ViewEdit::AddSlicer { ref condition } => BinnacleViewEdit {
                        key: condition.key.0,
                        ..on(17, Axis::Rows)
                    },
                    ViewEdit::RemoveSlicer { ref condition } => BinnacleViewEdit {
                        key: condition.key.0,
                        ..on(18, Axis::Rows)
                    },
                    ViewEdit::ReplaceSlicer { ref to, .. } => BinnacleViewEdit {
                        key: to.key.0,
                        ..on(19, Axis::Rows)
                    },
                    ViewEdit::SetForm { axis, form } => BinnacleViewEdit {
                        form: u32::from(form == Form::Header),
                        ..on(20, axis)
                    },
                    ViewEdit::RemoveLevel { axis, field } => BinnacleViewEdit {
                        level: level_to_c(field),
                        ..on(21, axis)
                    },
                    ViewEdit::MoveLevel { axis, field } => BinnacleViewEdit {
                        level: level_to_c(field),
                        ..on(22, axis)
                    },
                    ViewEdit::AddValue { ref value } => {
                        edit_value = Some(value.clone());
                        on(23, Axis::Rows)
                    }
                    ViewEdit::RemoveValue { ref value } => {
                        edit_value = Some(value.clone());
                        on(24, Axis::Rows)
                    }
                    #[allow(unreachable_patterns)] // Edits added to the Grid later are OTHER.
                    _ => BinnacleViewEdit::default(),
                };
                (4, 0, 0, 0, 0)
            }
            Effect::Locate { axis, key } => {
                asked = Some((*axis, *key));
                (5, 0, 0, tag(*axis), key.0)
            }
            Effect::BandSizeChanged { axis, thickness } => {
                size = *thickness;
                (6, 0, 0, tag(*axis), 0)
            }
            Effect::BandAutoSizeRequested { axis } => (7, 0, 0, tag(*axis), 0),
            Effect::FindRequested { query } => {
                find = query.clone();
                (8, 0, 0, 0, 0)
            }
            Effect::FindStepRequested {
                query,
                from: f,
                direction: d,
            } => {
                find = Some(query.clone());
                from = *f;
                if *d == FindDirection::Previous {
                    direction = BINNACLE_FIND_PREVIOUS;
                }
                (9, 0, 0, 0, 0)
            }
            Effect::MoveRequested { axis, keys, to } => {
                moved = (keys.iter().map(|k| k.0).collect(), *to);
                (10, 0, 0, tag(*axis), 0)
            }
            Effect::PinsChanged {
                axis,
                edge: e,
                keys,
            } => {
                moved = (keys.iter().map(|k| k.0).collect(), 0);
                if *e == Edge::Trailing {
                    edge = BINNACLE_EDGE_TRAILING;
                }
                (11, 0, 0, tag(*axis), 0)
            }
            #[allow(unreachable_patterns)] // Effects added to the Grid later are OTHER.
            _ => (0, 0, 0, 0, 0),
        };
        let keys = match c.keys.is_null() {
            true => &[][..],
            false => unsafe { slice::from_raw_parts(c.keys, c.keys_len) },
        };
        assert_eq!((keys.to_vec(), c.to), moved, "move for {want:?}");
        assert_eq!(c.edge, edge, "edge for {want:?}");
        let got_drill_down = (c.has_drill_down == 1).then(|| {
            let conditions = unsafe { c_slice(c.drill_down, c.drill_down_len) };
            (conditions.iter())
                .map(|c| unsafe { condition_from_c(c) })
                .collect::<Vec<_>>()
        });
        assert!(c.has_drill_down <= 1);
        assert_eq!(got_drill_down, drill_down, "drill-down for {want:?}");
        let runs = [0, 1].map(|i| {
            (unsafe { c_slice(c.copy_runs[i], c.copy_runs_len[i]) }.iter())
                .map(|r| r.start..r.end)
                .collect::<Vec<_>>()
        });
        assert_eq!((runs, c.copy_labels), copied, "copy for {want:?}");
        assert_eq!(
            (c.kind, c.row, c.col, c.axis, c.key),
            (kind, row, col, axis, key)
        );
        assert!(same_f32(c.size, size), "size {} for {want:?}", c.size);
        let got_keys = match c.edit.keys.is_null() {
            true => &[][..],
            false => unsafe { slice::from_raw_parts(c.edit.keys, c.edit.keys_len) },
        };
        assert_eq!(got_keys, edit_keys, "edit Keys for {want:?}");
        let got_value = (!c.edit.value.aggregate.data.is_null() || c.edit.value.has_field != 0)
            .then(|| unsafe { aggregate_value_from_c(&c.edit.value) });
        assert_eq!(got_value, edit_value, "edit value for {want:?}");
        let bare = BinnacleViewEdit {
            keys: std::ptr::null(),
            keys_len: 0,
            value: BinnacleAggregateValue::default(),
            ..c.edit
        };
        assert_eq!(bare, edit);
        let got = (c.has_find != 0).then(|| unsafe { find_from_c(&c.find) });
        assert_eq!(got, find, "find for {want:?}");
        let got = (c.has_from != 0).then(|| FindMatch::new(Key(c.from.row), Key(c.from.col)));
        assert_eq!((got, c.direction), (from, direction), "step for {want:?}");
        for axis in Axis::ALL {
            let got = c.view.axes[tag(axis) as usize];
            let sort = match got.sort.is_null() {
                true => &[][..],
                false => unsafe { slice::from_raw_parts(got.sort, got.sort_len) },
            };
            assert!(!got.sort.is_null() || got.sort_len == 0);
            let sort: Vec<(u64, u32)> = sort.iter().map(|s| (s.key, s.direction)).collect();
            let want: Vec<(u64, u32)> = view
                .axis(axis)
                .sort
                .columns()
                .iter()
                .map(|s| (s.key.0, u32::from(s.direction == SortDirection::Descending)))
                .collect();
            assert_eq!(sort, want);
            let quick = (got.has_quick != 0).then(|| unsafe { query_from_c(&got.quick) });
            assert_eq!(quick, view.axis(axis).filter.quick, "{axis:?} quick filter");
            assert!(!got.conditions.is_null() || got.conditions_len == 0);
            let conditions = match got.conditions.is_null() {
                true => &[][..],
                false => unsafe { slice::from_raw_parts(got.conditions, got.conditions_len) },
            };
            let conditions: Vec<Condition> = conditions
                .iter()
                .map(|c| unsafe { condition_from_c(c) })
                .collect();
            assert_eq!(
                conditions,
                view.axis(axis).filter.conditions.conditions(),
                "{axis:?} conditions"
            );
            let hidden = match got.hidden.is_null() {
                true => &[][..],
                false => unsafe { slice::from_raw_parts(got.hidden, got.hidden_len) },
            };
            let want: Vec<u64> = view.axis(axis).hidden.iter().map(|k| k.0).collect();
            assert_eq!(hidden, want, "{axis:?} hidden");
        }
        assert!(c.view.has_grouping <= 1);
        let grouping =
            (c.view.has_grouping != 0).then(|| unsafe { grouping_from_c(&c.view.grouping) });
        assert_eq!(grouping, view.grouping, "grouping for {want:?}");
    }
}

/// A C array as a slice, as a host would read it: NULL only when empty.
unsafe fn c_slice<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    assert!(!ptr.is_null() || len == 0, "NULL with length {len}");
    match len {
        0 => &[],
        n => unsafe { slice::from_raw_parts(ptr, n) },
    }
}

fn placement_from_c(tag: u32) -> Placement {
    match tag {
        BINNACLE_PLACEMENT_OFF => Placement::Off,
        BINNACLE_PLACEMENT_BEFORE => Placement::Before,
        BINNACLE_PLACEMENT_AFTER => Placement::After,
        t => panic!("placement tag {t}"),
    }
}

fn placement_to_c(placement: Placement) -> u32 {
    match placement {
        Placement::Off => BINNACLE_PLACEMENT_OFF,
        Placement::Before => BINNACLE_PLACEMENT_BEFORE,
        Placement::After => BINNACLE_PLACEMENT_AFTER,
    }
}

/// A grouping read back from C, as a host would.
unsafe fn grouping_from_c(g: &BinnacleGrouping) -> Grouping {
    let mut grouping = Grouping::default();
    for axis in Axis::ALL {
        let a = &g.axes[tag(axis) as usize];
        *grouping.levels_mut(axis) = unsafe { c_slice(a.levels, a.levels_len) }
            .iter()
            .map(level_from_c)
            .collect();
        let got = grouping.axis_mut(axis);
        got.grand_total = placement_from_c(a.grand_total);
        got.form = match a.form {
            BINNACLE_FORM_OUTLINE => Form::Outline,
            BINNACLE_FORM_HEADER => Form::Header,
            f => panic!("form {f}"),
        };
        got.expand.depth = match a.expand_all {
            0 => Depth::Levels(a.expand_depth),
            1 => Depth::All,
            e => panic!("expand_all {e}"),
        };
        got.expand.except = unsafe { c_slice(a.except, a.except_len) }
            .iter()
            .map(|&k| Key(k))
            .collect();
    }
    grouping.values = unsafe { c_slice(g.values, g.values_len) }
        .iter()
        .map(|v| unsafe { aggregate_value_from_c(v) })
        .collect();
    grouping.slicers = unsafe { c_slice(g.slicers, g.slicers_len) }
        .iter()
        .map(|c| unsafe { condition_from_c(c) })
        .collect();
    grouping
}

/// An outline read back from C, as a host would.
unsafe fn outline_from_c(o: &BinnacleOutline) -> Outline {
    let ancestors = unsafe { c_slice(o.ancestors, o.ancestors_len) };
    let path = unsafe { c_slice(o.path, o.path_len) };
    let path = (path.iter()).map(|g| GroupOf {
        field: Key(g.field),
        value: unsafe { value_from_c(&g.value) },
    });
    let mut outline = Outline::leaf(ancestors.iter().map(|&k| Key(k)).collect())
        .with_span(o.span)
        .with_path(path.collect());
    outline.kind = match o.kind {
        BINNACLE_ITEM_LEAF => ItemKind::Leaf,
        BINNACLE_ITEM_GROUP => ItemKind::Group,
        BINNACLE_ITEM_SUBTOTAL => ItemKind::Subtotal,
        BINNACLE_ITEM_GRAND_TOTAL => ItemKind::GrandTotal,
        k => panic!("item kind {k}"),
    };
    outline.depth = o.depth;
    outline.open = match o.open {
        BINNACLE_OPEN_STATE_OPEN => OpenState::Open,
        BINNACLE_OPEN_STATE_CLOSED => OpenState::Closed,
        BINNACLE_OPEN_STATE_CANT_OPEN => OpenState::CantOpen,
        s => panic!("open state {s}"),
    };
    outline.group = match o.has_group {
        0 => None,
        1 => Some(GroupOf {
            field: Key(o.field),
            value: unsafe { value_from_c(&o.value) },
        }),
        g => panic!("has_group {g}"),
    };
    outline.set = match o.has_set {
        0 => None,
        1 => Some(SetPosition {
            index: o.set_index,
            size: o.set_size,
        }),
        h => panic!("has_set {h}"),
    };
    outline
}

/// Runs `f` with `outlines` in C form, their arrays alive for the call.
fn with_c_outlines<T>(outlines: &[Outline], f: impl FnOnce(&[BinnacleOutline]) -> T) -> T {
    let ancestors: Vec<Vec<u64>> = (outlines.iter())
        .map(|o| o.ancestors.iter().map(|k| k.0).collect())
        .collect();
    let paths: Vec<Vec<BinnacleGroupOf>> = (outlines.iter())
        .map(|o| {
            (o.path.iter())
                .map(|g| BinnacleGroupOf {
                    field: g.field.0,
                    value: value_to_c(&g.value),
                })
                .collect()
        })
        .collect();
    let c: Vec<BinnacleOutline> = (outlines.iter().zip(&ancestors).zip(&paths))
        .map(|((o, ancestors), path)| BinnacleOutline {
            kind: match o.kind {
                ItemKind::Leaf => BINNACLE_ITEM_LEAF,
                ItemKind::Group => BINNACLE_ITEM_GROUP,
                ItemKind::Subtotal => BINNACLE_ITEM_SUBTOTAL,
                ItemKind::GrandTotal => BINNACLE_ITEM_GRAND_TOTAL,
            },
            depth: o.depth,
            open: match o.open {
                OpenState::Open => BINNACLE_OPEN_STATE_OPEN,
                OpenState::Closed => BINNACLE_OPEN_STATE_CLOSED,
                OpenState::CantOpen => BINNACLE_OPEN_STATE_CANT_OPEN,
            },
            ancestors: ancestors.as_ptr(),
            ancestors_len: ancestors.len(),
            has_group: u32::from(o.group.is_some()),
            field: o.group.as_ref().map_or(0, |g| g.field.0),
            value: o
                .group
                .as_ref()
                .map_or_else(BinnacleValue::default, |g| value_to_c(&g.value)),
            span: o.span,
            path: path.as_ptr(),
            path_len: path.len(),
            has_set: u32::from(o.set.is_some()),
            set_index: o.set.map_or(0, |s| s.index),
            set_size: o.set.map_or(0, |s| s.size),
        })
        .collect();
    f(&c)
}

/// A Text Query read back from C, as a host would.
unsafe fn query_from_c(q: &BinnacleTextQuery) -> TextQuery {
    let text = unsafe { slice::from_raw_parts(q.text.data, q.text.len) };
    let searches = match q.searched {
        BINNACLE_SEARCHED_SHOWN => Searched::Shown,
        _ => Searched::Keys(
            unsafe { slice::from_raw_parts(q.keys, q.keys_len) }
                .iter()
                .map(|&k| Key(k))
                .collect(),
        ),
    };
    TextQuery::new(std::str::from_utf8(text).unwrap())
        .case_sensitive(q.flags & BINNACLE_TEXT_CASE_SENSITIVE != 0)
        .regex(q.flags & BINNACLE_TEXT_REGEX != 0)
        .searching(searches)
}

/// A Find Query read back from C, as a host would.
unsafe fn find_from_c(f: &BinnacleFindQuery) -> FindQuery {
    let by = match f.by {
        BINNACLE_ROWS => Axis::Rows,
        _ => Axis::Cols,
    };
    FindQuery::new(unsafe { query_from_c(&f.query) }).by(by)
}

/// The level a View Edit names by its field, `None` the values
/// pseudo-level, in C.
fn level_to_c(field: Option<Key>) -> BinnacleLevel {
    BinnacleLevel {
        kind: match field {
            Some(_) => BINNACLE_LEVEL_FIELD,
            None => BINNACLE_LEVEL_VALUES,
        },
        field: field.map_or(0, |k| k.0),
        subtotal: BINNACLE_PLACEMENT_OFF,
    }
}

fn with_c_value<T>(v: &AggregateValue, f: impl FnOnce(&BinnacleAggregateValue) -> T) -> T {
    let params: Vec<BinnacleText> = v.params.iter().map(|p| text_to_c(p)).collect();
    f(&BinnacleAggregateValue {
        has_field: u32::from(v.field.is_some()),
        field: v.field.map_or(0, |k| k.0),
        aggregate: text_to_c(&v.aggregate),
        params: params.as_ptr(),
        params_len: params.len(),
    })
}

fn with_c_aggregates<T>(list: &[Aggregate], f: impl FnOnce(&[BinnacleAggregate]) -> T) -> T {
    let params: Vec<Vec<BinnacleText>> = (list.iter())
        .map(|a| a.params.iter().map(|p| text_to_c(p)).collect())
        .collect();
    let c: Vec<BinnacleAggregate> = (list.iter().zip(&params))
        .map(|(a, ps)| BinnacleAggregate {
            name: text_to_c(&a.name),
            params: ps.as_ptr(),
            params_len: ps.len(),
        })
        .collect();
    f(&c)
}

/// The support bits from the header's constants.
fn support_bits(support: Support) -> u32 {
    let text = support.text.map_or(0, |t| {
        BINNACLE_SUPPORT_TEXT
            | if t.regex {
                BINNACLE_SUPPORT_TEXT_REGEX
            } else {
                0
            }
    });
    let compare = match support.compare {
        None => 0,
        Some(Compared::Numbers) => BINNACLE_SUPPORT_COMPARE_NUMBERS,
        Some(Compared::Dates) => BINNACLE_SUPPORT_COMPARE_DATES,
    };
    (u32::from(support.sort) * BINNACLE_SUPPORT_SORT)
        | text
        | (u32::from(support.one_of) * BINNACLE_SUPPORT_ONE_OF)
        | compare
        | (u32::from(support.group) * BINNACLE_SUPPORT_GROUP)
        | (u32::from(support.native) * BINNACLE_SUPPORT_NATIVE)
        | (u32::from(support.pivot) * BINNACLE_SUPPORT_PIVOT)
        | (u32::from(support.subtotals.before) * BINNACLE_SUPPORT_SUBTOTAL_BEFORE)
        | (u32::from(support.subtotals.after) * BINNACLE_SUPPORT_SUBTOTAL_AFTER)
}

/// Where a listed part stands, read back from C.
fn state_from_c(state: u32) -> ConditionState {
    match state {
        0 => ConditionState::Applied,
        1 => ConditionState::Adding,
        2 => ConditionState::Removing,
        3 => ConditionState::NotApplied,
        s => panic!("state {s}"),
    }
}

/// A level read back from C, as a host would.
fn level_from_c(l: &BinnacleLevel) -> Level {
    match l.kind {
        BINNACLE_LEVEL_FIELD => Level::Field {
            field: Key(l.field),
            subtotal: placement_from_c(l.subtotal),
        },
        BINNACLE_LEVEL_VALUES => Level::Values,
        k => panic!("level kind {k}"),
    }
}

/// A value read back from C, as a host would.
unsafe fn aggregate_value_from_c(v: &BinnacleAggregateValue) -> AggregateValue {
    let field = match v.has_field {
        0 => None,
        1 => Some(Key(v.field)),
        f => panic!("has_field {f}"),
    };
    let params = unsafe { c_slice(v.params, v.params_len) }
        .iter()
        .map(|&p| unsafe { text_from_c(p) })
        .collect();
    AggregateValue::new(field, unsafe { text_from_c(v.aggregate) }).with_params(params)
}

/// The aggregates a header offers, read back from C, as a host would.
unsafe fn aggregates_from_c(h: &BinnacleHeader) -> Vec<Aggregate> {
    unsafe { c_slice(h.aggregates, h.aggregates_len) }
        .iter()
        .map(|a| {
            let params = unsafe { c_slice(a.params, a.params_len) }
                .iter()
                .map(|&p| unsafe { text_from_c(p) })
                .collect();
            Aggregate::new(unsafe { text_from_c(a.name) }).with_params(params)
        })
        .collect()
}

/// `text` in C form, borrowed.
fn text_to_c(text: &str) -> BinnacleText {
    BinnacleText {
        data: text.as_ptr(),
        len: text.len(),
    }
}

/// The Keys a Text condition's query names, if any.
fn condition_keys(condition: &Condition) -> Vec<u64> {
    match &condition.kind {
        ConditionKind::Text(query) => query_keys(query),
        _ => Vec::new(),
    }
}

/// A condition read back from C, as a host would: its query searching the
/// Keys it names.
unsafe fn condition_from_c(c: &BinnacleCondition) -> Condition {
    let condition = match c.kind {
        BINNACLE_CONDITION_TEXT => {
            let query = unsafe { query_from_c(&c.text) };
            assert_eq!(query.searches, Searched::Keys(vec![Key(c.key)]));
            Condition::text(Key(c.key), query)
        }
        BINNACLE_CONDITION_ONE_OF => {
            let values = match c.values_len {
                0 => &[][..],
                n => unsafe { slice::from_raw_parts(c.values, n) },
            };
            let values = (values.iter())
                .map(|v| unsafe { value_from_c(v) })
                .collect();
            Condition::one_of(Key(c.key), values)
        }
        BINNACLE_CONDITION_COMPARE => {
            let (s, high) = (scalar_from_c(c.scalar), scalar_from_c(c.high));
            let comparison = match c.compare {
                BINNACLE_COMPARE_EQUAL => Comparison::Equal(s),
                BINNACLE_COMPARE_NOT_EQUAL => Comparison::NotEqual(s),
                BINNACLE_COMPARE_LESS => Comparison::Less(s),
                BINNACLE_COMPARE_AT_MOST => Comparison::AtMost(s),
                BINNACLE_COMPARE_GREATER => Comparison::Greater(s),
                BINNACLE_COMPARE_AT_LEAST => Comparison::AtLeast(s),
                BINNACLE_COMPARE_BETWEEN => Comparison::Between(s, high),
                op => panic!("comparison {op}"),
            };
            Condition::compare(Key(c.key), comparison)
        }
        BINNACLE_CONDITION_NATIVE => {
            Condition::native(Key(c.key), unsafe { text_from_c(c.text.text) })
        }
        kind => panic!("condition kind {kind}"),
    };
    condition.not(c.negated != 0)
}

/// A value read back from C, as a host would.
unsafe fn value_from_c(v: &BinnacleValue) -> Value {
    match v.blank {
        0 => Value::Text(unsafe { text_from_c(v.text) }),
        1 => Value::Blank,
        b => panic!("blank is {b}"),
    }
}

/// The text `t` points at, as a host would read it.
unsafe fn text_from_c(t: BinnacleText) -> String {
    let bytes = match t.len {
        0 => &[][..],
        n => unsafe { slice::from_raw_parts(t.data, n) },
    };
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

fn scalar_from_c(s: BinnacleScalar) -> Scalar {
    match s.kind {
        BINNACLE_SCALAR_NUMBER => Scalar::Number(s.number),
        BINNACLE_SCALAR_DATE => Scalar::Date(s.date),
        kind => panic!("scalar kind {kind}"),
    }
}

fn scalar_to_c(s: Scalar) -> BinnacleScalar {
    match s {
        Scalar::Number(number) => BinnacleScalar {
            kind: BINNACLE_SCALAR_NUMBER,
            number,
            date: 0,
        },
        Scalar::Date(date) => BinnacleScalar {
            kind: BINNACLE_SCALAR_DATE,
            number: 0.0,
            date,
        },
    }
}

/// The values of a OneOf `condition` in C form, borrowing their texts; none
/// for other kinds.
fn values_to_c(condition: &Condition) -> Vec<BinnacleValue> {
    let ConditionKind::OneOf(values) = &condition.kind else {
        return Vec::new();
    };
    values.iter().map(value_to_c).collect()
}

/// `value` in C form, borrowing its text.
fn value_to_c(value: &Value) -> BinnacleValue {
    match value {
        Value::Text(t) => BinnacleValue {
            blank: 0,
            text: BinnacleText {
                data: t.as_ptr(),
                len: t.len(),
            },
        },
        Value::Blank => BinnacleValue {
            blank: 1,
            text: BinnacleText::default(),
        },
    }
}

/// `condition` in C form, borrowing its text, `keys` and `values` (from
/// [`values_to_c`]).
fn condition_to_c(
    condition: &Condition,
    keys: &[u64],
    values: &[BinnacleValue],
) -> BinnacleCondition {
    let mut c = BinnacleCondition {
        key: condition.key.0,
        negated: u32::from(condition.not),
        values: values.as_ptr(),
        values_len: values.len(),
        ..BinnacleCondition::default()
    };
    match &condition.kind {
        ConditionKind::Text(query) => {
            c.kind = BINNACLE_CONDITION_TEXT;
            c.text = query_to_c(query, keys);
        }
        ConditionKind::OneOf(_) => c.kind = BINNACLE_CONDITION_ONE_OF,
        ConditionKind::Native(text) => {
            c.kind = BINNACLE_CONDITION_NATIVE;
            c.text.text = text_to_c(text);
        }
        ConditionKind::Compare(comparison) => {
            c.kind = BINNACLE_CONDITION_COMPARE;
            let (op, s, high) = match *comparison {
                Comparison::Equal(s) => (BINNACLE_COMPARE_EQUAL, s, None),
                Comparison::NotEqual(s) => (BINNACLE_COMPARE_NOT_EQUAL, s, None),
                Comparison::Less(s) => (BINNACLE_COMPARE_LESS, s, None),
                Comparison::AtMost(s) => (BINNACLE_COMPARE_AT_MOST, s, None),
                Comparison::Greater(s) => (BINNACLE_COMPARE_GREATER, s, None),
                Comparison::AtLeast(s) => (BINNACLE_COMPARE_AT_LEAST, s, None),
                Comparison::Between(low, high) => (BINNACLE_COMPARE_BETWEEN, low, Some(high)),
            };
            c.compare = op;
            c.scalar = scalar_to_c(s);
            if let Some(high) = high {
                c.high = scalar_to_c(high);
            }
        }
        _ => unreachable!("only Text, OneOf, comparison and Native conditions are drawn"),
    }
    c
}

/// A Text Query in C form, borrowing `query`'s text and `keys`.
fn query_to_c(query: &TextQuery, keys: &[u64]) -> BinnacleTextQuery {
    BinnacleTextQuery {
        text: BinnacleText {
            data: query.text.as_ptr(),
            len: query.text.len(),
        },
        flags: u32::from(query.case_sensitive) | u32::from(query.regex) << 1,
        searched: u32::from(matches!(query.searches, Searched::Keys(_))),
        keys: keys.as_ptr(),
        keys_len: keys.len(),
    }
}

/// The Keys a Text Query searches, if it names them.
fn query_keys(query: &TextQuery) -> Vec<u64> {
    match &query.searches {
        Searched::Shown => Vec::new(),
        Searched::Keys(keys) => keys.iter().map(|k| k.0).collect(),
    }
}

/// Runs `f` with `spec` in C form, its arrays alive for the call.
fn with_c_view<T>(spec: &ViewSpec, f: impl FnOnce(&BinnacleViewSpec) -> T) -> T {
    let sorts = Axis::ALL.map(|axis| {
        spec.axis(axis)
            .sort
            .columns()
            .iter()
            .map(|s| BinnacleSortColumn {
                key: s.key.0,
                direction: u32::from(s.direction == SortDirection::Descending),
            })
            .collect::<Vec<_>>()
    });
    let keys = Axis::ALL.map(|axis| {
        spec.axis(axis)
            .filter
            .quick
            .as_ref()
            .map_or_else(Vec::new, query_keys)
    });
    let values = Axis::ALL.map(|axis| {
        (spec.axis(axis).filter.conditions.conditions().iter())
            .map(values_to_c)
            .collect::<Vec<_>>()
    });
    // No Keys: a condition's query searches its own column whatever it names.
    let conditions = Axis::ALL.map(|axis| {
        (spec.axis(axis).filter.conditions.conditions().iter())
            .zip(&values[axis.index()])
            .map(|(c, v)| condition_to_c(c, &[], v))
            .collect::<Vec<_>>()
    });
    let hidden = Axis::ALL.map(|axis| {
        (spec.axis(axis).hidden.iter())
            .map(|k| k.0)
            .collect::<Vec<u64>>()
    });
    let grouping = spec.grouping.as_ref();
    // A VALUES level's field and subtotal are ignored: send junk in them.
    let levels = Axis::ALL.map(|axis| {
        (grouping.map_or(&[][..], |g| g.levels(axis)).iter())
            .map(|l| match l {
                Level::Field { field, subtotal } => BinnacleLevel {
                    kind: BINNACLE_LEVEL_FIELD,
                    field: field.0,
                    subtotal: placement_to_c(*subtotal),
                },
                Level::Values => BinnacleLevel {
                    kind: BINNACLE_LEVEL_VALUES,
                    field: 1,
                    subtotal: 99,
                },
                _ => unreachable!("only Field and Values levels are drawn"),
            })
            .collect::<Vec<_>>()
    });
    let excepts = Axis::ALL.map(|axis| {
        grouping.map_or_else(Vec::new, |g| {
            g.axis(axis).expand.except.iter().map(|k| k.0).collect()
        })
    });
    let text = |t: &String| BinnacleText {
        data: t.as_ptr(),
        len: t.len(),
    };
    let values = grouping.map_or(&[][..], |g| &g.values[..]);
    let params: Vec<Vec<BinnacleText>> = (values.iter())
        .map(|v| v.params.iter().map(text).collect())
        .collect();
    let aggregates: Vec<BinnacleAggregateValue> = (values.iter().zip(&params))
        .map(|(v, params)| BinnacleAggregateValue {
            has_field: u32::from(v.field.is_some()),
            field: v.field.map_or(0, |k| k.0),
            aggregate: text(&v.aggregate),
            params: params.as_ptr(),
            params_len: params.len(),
        })
        .collect();
    let slicers = grouping.map_or(&[][..], |g| &g.slicers[..]);
    let slicer_values: Vec<_> = slicers.iter().map(values_to_c).collect();
    let slicers: Vec<_> = (slicers.iter().zip(&slicer_values))
        .map(|(c, v)| condition_to_c(c, &[], v))
        .collect();
    let mut c = BinnacleViewSpec::default();
    if let Some(g) = grouping {
        c.has_grouping = 1;
        for axis in Axis::ALL {
            let (levels, except) = (&levels[axis.index()], &excepts[axis.index()]);
            let a = g.axis(axis);
            let (expand_all, expand_depth) = match a.expand.depth {
                Depth::All => (1, 7),
                Depth::Levels(n) => (0, n),
            };
            c.grouping.axes[tag(axis) as usize] = BinnacleGroupedAxis {
                levels: levels.as_ptr(),
                levels_len: levels.len(),
                grand_total: placement_to_c(a.grand_total),
                form: match a.form {
                    Form::Outline => BINNACLE_FORM_OUTLINE,
                    Form::Header => BINNACLE_FORM_HEADER,
                },
                expand_all,
                expand_depth,
                except: except.as_ptr(),
                except_len: except.len(),
            };
        }
        c.grouping.values = aggregates.as_ptr();
        c.grouping.values_len = aggregates.len();
        c.grouping.slicers = slicers.as_ptr();
        c.grouping.slicers_len = slicers.len();
    }
    for axis in Axis::ALL {
        let sort = &sorts[axis.index()];
        let hidden = &hidden[axis.index()];
        let quick = spec.axis(axis).filter.quick.as_ref();
        let conds = &conditions[axis.index()];
        c.axes[tag(axis) as usize] = BinnacleAxisView {
            sort: sort.as_ptr(),
            sort_len: sort.len(),
            has_quick: u32::from(quick.is_some()),
            quick: quick.map_or_else(BinnacleTextQuery::default, |q| {
                query_to_c(q, &keys[axis.index()])
            }),
            conditions: conds.as_ptr(),
            conditions_len: conds.len(),
            hidden: hidden.as_ptr(),
            hidden_len: hidden.len(),
        };
    }
    f(&c)
}

/// The Window answer: Keys 1000 + Position for rows, 5000 + Position for
/// columns (wrapping on huge axes; only equality matters).
fn keys(axis: Axis, range: std::ops::Range<u64>) -> Vec<u64> {
    let base: u64 = if axis == Axis::Rows { 1000 } else { 5000 };
    range.map(|p| p.wrapping_add(base)).collect()
}

#[hegel::test(test_cases = 2000)]
fn the_c_api_mirrors_the_grid(tc: TestCase) {
    let configs = [tc.draw(axis_config()), tc.draw(axis_config())];
    let c_config = |a: AxisConfig| BinnacleAxisConfig {
        count: a.count,
        default_size: a.default_size,
        header: a.header,
        gap: a.gap,
    };
    let h = binnacle_grid_new(c_config(configs[0]), c_config(configs[1]));
    assert!(!h.is_null());
    let mut grid = Grid::new(configs[0], configs[1]);
    // Start with a frame so pointer input can hit the laid-out Grid.
    let first = Op::Frame(tc.draw(len()), tc.draw(len()), tc.draw(frame_outlines()));
    let mut asked = None;
    // The start of the last Window set on each axis.
    let mut windows = [0u64; 2];
    // Each band's thickness and each strip's Keys as of the last frame.
    let mut bands = [0.0f32; 2];
    let mut shown: [Vec<Key>; 2] = Default::default();
    // Each strip item's middle along its axis, as of the last frame.
    let mut middles: [Vec<f32>; 2] = Default::default();
    // And the middle of the Gap between each strip item and the next.
    let mut borders: [Vec<f32>; 2] = Default::default();
    // Find open from the start half the time, so steps land while it is.
    let find = tc
        .draw(gs::booleans())
        .then(|| Op::SetFind(Some(FindQuery::new(TextQuery::new("a")))));
    // Both axes movable from the start half the time, so header drags move.
    let movable = tc
        .draw(gs::booleans())
        .then(|| Axis::ALL.map(|a| Op::SetMovable(a, true)))
        .into_iter()
        .flatten();
    let ops = [first]
        .into_iter()
        .chain(find)
        .chain(movable)
        .chain(tc.draw(gs::vecs(op()).min_size(5).max_size(60)));
    for op in ops {
        unsafe {
            let ok = match op {
                Op::Frame(width, height, outlines) => {
                    let mut requests = BinnacleWindowRequests::default();
                    assert_eq!(
                        binnacle_grid_begin_frame(h, width, height, &mut requests),
                        1
                    );
                    let want = grid.begin_frame(Viewport::new(width, height));
                    for axis in Axis::ALL {
                        let r = requests.axes[tag(axis) as usize];
                        let got = (r.requested != 0).then_some(r.start..r.end);
                        assert_eq!(got, want.get(axis), "{axis:?} request");
                        if let Some(range) = got {
                            // Cap what we answer: a Window may cover a huge axis.
                            let range = range.start..range.end.min(range.start.saturating_add(500));
                            let k = keys(axis, range.clone());
                            assert_eq!(
                                binnacle_grid_set_window(
                                    h,
                                    tag(axis),
                                    range.start,
                                    k.as_ptr(),
                                    k.len()
                                ),
                                1
                            );
                            let k: Vec<Key> = k.into_iter().map(Key).collect();
                            grid.set_window(axis, range.start, &k);
                            windows[axis.index()] = range.start;
                            let outlines = &outlines[axis.index()];
                            if !outlines.is_empty() {
                                let start = range.start;
                                let ok = with_c_outlines(outlines, |c| {
                                    binnacle_grid_set_outlines(
                                        h,
                                        tag(axis),
                                        start,
                                        c.as_ptr(),
                                        c.len(),
                                    )
                                });
                                assert_eq!(ok, 1);
                                grid.set_outlines(axis, start, outlines);
                            }
                        }
                    }
                    let mut snap = std::mem::zeroed::<BinnacleSnapshot>();
                    assert_eq!(binnacle_grid_snapshot(h, &mut snap), 1);
                    let rust = grid.snapshot();
                    check_snapshot(&snap, &rust);
                    bands = Axis::ALL.map(|a| rust.header(a));
                    shown = Axis::ALL.map(|a| rust.strip(a).iter().filter_map(|i| i.key).collect());
                    middles = Axis::ALL.map(|a| {
                        rust.strip(a)
                            .iter()
                            .map(|i| i.offset + i.size / 2.0)
                            .collect()
                    });
                    borders = Axis::ALL.map(|a| {
                        rust.strip(a)
                            .windows(2)
                            .map(|w| (w[0].offset + w[0].size + w[1].offset) / 2.0)
                            .collect()
                    });
                    1
                }
                Op::Pointer(e) => {
                    grid.pointer(e);
                    binnacle_grid_pointer(h, pointer_event(e))
                }
                Op::SetOutlines(axis, offset, outlines) => {
                    let start = windows[axis.index()].saturating_add(offset);
                    grid.set_outlines(axis, start, &outlines);
                    with_c_outlines(&outlines, |c| {
                        binnacle_grid_set_outlines(h, tag(axis), start, c.as_ptr(), c.len())
                    })
                }
                Op::GroupBy(axis, field, declare) => {
                    if declare {
                        let support = Support::default().with_group(true).with_pivot(true);
                        grid.set_support(Axis::Rows, field, support);
                        let bits = support_bits(support);
                        assert_eq!(binnacle_grid_set_support(h, ROWS, field.0, bits), 1);
                    }
                    grid.group_by(axis, field);
                    binnacle_grid_group_by(h, tag(axis), field.0)
                }
                Op::RemoveLevel(axis, field) => {
                    grid.remove_level(axis, field);
                    binnacle_grid_remove_level(h, tag(axis), &level_to_c(field))
                }
                Op::MoveLevel(axis, field) => {
                    // Declared, so that a level the spec has moves.
                    if let Some(field) = field {
                        let support = Support::default().with_group(true).with_pivot(true);
                        grid.set_support(Axis::Rows, field, support);
                        let bits = support_bits(support);
                        assert_eq!(binnacle_grid_set_support(h, ROWS, field.0, bits), 1);
                    }
                    grid.move_level(axis, field);
                    binnacle_grid_move_level(h, tag(axis), &level_to_c(field))
                }
                Op::AddValue(value, declare) => {
                    if declare && let Some(field) = value.field {
                        let offered = vec![
                            Aggregate::new(value.aggregate.clone())
                                .with_params(value.params.clone()),
                        ];
                        let ok = with_c_aggregates(&offered, |c| {
                            binnacle_grid_set_aggregates(h, ROWS, field.0, c.as_ptr(), c.len())
                        });
                        assert_eq!(ok, 1);
                        grid.set_aggregates(Axis::Rows, field, offered);
                    }
                    grid.add_value(value.clone());
                    with_c_value(&value, |c| binnacle_grid_add_value(h, c))
                }
                Op::RemoveValue(value) => {
                    grid.remove_value(&value);
                    with_c_value(&value, |c| binnacle_grid_remove_value(h, c))
                }
                Op::CycleSort(axis, key, multi) => {
                    grid.cycle_sort(axis, key, multi);
                    binnacle_grid_cycle_sort(h, tag(axis), key.0, u32::from(multi))
                }
                Op::Ungroup(axis) => {
                    grid.ungroup(axis);
                    binnacle_grid_ungroup(h, tag(axis))
                }
                Op::Toggle(which, axis, key) => {
                    let (rust, c): (fn(&mut Grid, Axis, Key) -> _, _) = match which {
                        0 => (
                            Grid::expand,
                            binnacle_grid_expand as unsafe extern "C" fn(_, _, _) -> _,
                        ),
                        1 => (Grid::collapse, binnacle_grid_collapse),
                        2 => (Grid::toggle, binnacle_grid_toggle),
                        _ => (Grid::expand_all, binnacle_grid_expand_all),
                    };
                    let want = match rust(&mut grid, axis, key) {
                        Ok(()) => 1,
                        Err(_) => BINNACLE_EXCEPT_CAPPED,
                    };
                    assert_eq!(c(h, tag(axis), key.0), want);
                    want
                }
                Op::SetTiers(axis, sizes) => {
                    grid.set_tiers(axis, &sizes);
                    binnacle_grid_set_tiers(h, tag(axis), sizes.as_ptr(), sizes.len())
                }
                Op::SetForm(axis, form) => {
                    grid.set_form(axis, form);
                    let form = match form {
                        Form::Outline => BINNACLE_FORM_OUTLINE,
                        Form::Header => BINNACLE_FORM_HEADER,
                    };
                    binnacle_grid_set_form(h, tag(axis), form)
                }
                Op::ExpandTo(axis, depth) => {
                    grid.expand_to(axis, depth);
                    let (all, n) = match depth {
                        Depth::All => (1, 0),
                        Depth::Levels(n) => (0, n),
                    };
                    binnacle_grid_expand_to(h, tag(axis), all, n)
                }
                Op::Gesture(events) => {
                    for e in events {
                        grid.pointer(e);
                        assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                    }
                    1
                }
                Op::Action(a) => {
                    let ok = send(h, &a);
                    grid.action(a);
                    ok
                }
                Op::SetSize(axis, key, size) => {
                    grid.set_size(axis, Key(key + 5000 * u64::from(axis == Axis::Cols)), size);
                    binnacle_grid_set_size(
                        h,
                        tag(axis),
                        key + 5000 * u64::from(axis == Axis::Cols),
                        size,
                    )
                }
                Op::SetHeader(axis, thickness) => {
                    grid.set_header(axis, thickness);
                    binnacle_grid_set_header(h, tag(axis), thickness)
                }
                Op::SetPinZone(axis, size) => {
                    grid.set_pin_zone(axis, size);
                    binnacle_grid_set_pin_zone(h, tag(axis), size)
                }
                Op::SetEdgeGap(axis, gap) => {
                    grid.set_edge_gap(axis, gap);
                    binnacle_grid_set_edge_gap(h, tag(axis), gap)
                }
                Op::SetMinScrolling(axis, length) => {
                    grid.set_min_scrolling(axis, length);
                    binnacle_grid_set_min_scrolling(h, tag(axis), length)
                }
                Op::SetEndGap(axis, gap) => {
                    grid.set_end_gap(axis, gap);
                    binnacle_grid_set_end_gap(h, tag(axis), gap)
                }
                Op::Pin(axis, edge, key) => {
                    let key = key + 5000 * u64::from(axis == Axis::Cols);
                    grid.pin(axis, edge, Key(key));
                    let edge = match edge {
                        Edge::Leading => BINNACLE_EDGE_LEADING,
                        Edge::Trailing => BINNACLE_EDGE_TRAILING,
                    };
                    binnacle_grid_pin(h, tag(axis), edge, key)
                }
                Op::SetPins(axis, edge, keys) => {
                    let keys: Vec<u64> = (keys.into_iter())
                        .map(|k| k + 5000 * u64::from(axis == Axis::Cols))
                        .collect();
                    let rust: Vec<Key> = keys.iter().map(|&k| Key(k)).collect();
                    grid.set_pins(axis, edge, &rust);
                    let edge = match edge {
                        Edge::Leading => BINNACLE_EDGE_LEADING,
                        Edge::Trailing => BINNACLE_EDGE_TRAILING,
                    };
                    binnacle_grid_set_pins(h, tag(axis), edge, keys.as_ptr(), keys.len())
                }
                Op::Freeze(axis, key) => {
                    let key = key + 5000 * u64::from(axis == Axis::Cols);
                    grid.freeze(axis, Key(key));
                    binnacle_grid_freeze(h, tag(axis), key)
                }
                Op::Unpin(axis, key) => {
                    let key = key + 5000 * u64::from(axis == Axis::Cols);
                    grid.unpin(axis, Key(key));
                    binnacle_grid_unpin(h, tag(axis), key)
                }
                Op::Forget(axis, keys, all) => {
                    let (mut held, mut len) = (std::ptr::null(), 0usize);
                    assert_eq!(
                        binnacle_grid_held_keys(h, tag(axis), &mut held, &mut len),
                        1
                    );
                    let held = slice::from_raw_parts(held, len).to_vec();
                    let want: Vec<u64> = grid.held_keys(axis).iter().map(|k| k.0).collect();
                    assert_eq!(held, want, "{axis:?} held Keys");
                    let keys = if all { held } else { keys };
                    let rust: Vec<Key> = keys.iter().map(|&k| Key(k)).collect();
                    grid.forget(axis, &rust);
                    binnacle_grid_forget(h, tag(axis), keys.as_ptr(), keys.len())
                }
                Op::BandDrag {
                    axis,
                    inset,
                    clicks,
                    travel,
                } => {
                    let along = bands[axis.index()] - inset;
                    let across = bands[axis.other().index()] / 2.0;
                    let at = |v: f32| match axis {
                        Axis::Rows => (across, v),
                        Axis::Cols => (v, across),
                    };
                    let (x, y) = at(along);
                    let (mx, my) = at(along + travel);
                    for e in [
                        PointerEvent::down(x, y).clicks(clicks),
                        PointerEvent::moved(mx, my),
                    ] {
                        grid.pointer(e);
                        assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                    }
                    1
                }
                Op::BorderDrag {
                    axis,
                    index,
                    clicks,
                    travel,
                    release,
                } => {
                    let along = &borders[axis.index()];
                    if along.is_empty() {
                        continue;
                    }
                    let across = bands[axis.other().index()] / 2.0;
                    let at = |v: f32| match axis {
                        Axis::Rows => (across, v),
                        Axis::Cols => (v, across),
                    };
                    let border = along[index % along.len()];
                    let (x, y) = at(border);
                    let (mx, my) = at(border + travel);
                    let up = release.then(|| PointerEvent::up(mx, my));
                    for e in [
                        PointerEvent::down(x, y).clicks(clicks),
                        PointerEvent::moved(mx, my),
                    ]
                    .into_iter()
                    .chain(up)
                    {
                        grid.pointer(e);
                        assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                    }
                    1
                }
                Op::Unit(unit) => {
                    grid.set_selection_unit(unit);
                    let unit = match unit {
                        SelectionUnit::Cells => 0,
                        SelectionUnit::Rows => 1,
                        SelectionUnit::Cols => 2,
                    };
                    binnacle_grid_set_selection_unit(h, unit)
                }
                Op::HeaderClick(axis, click) => {
                    grid.set_header_click(axis, click);
                    let click = match click {
                        HeaderClick::Sort => 0,
                        HeaderClick::Select => 1,
                        HeaderClick::None => 2,
                    };
                    binnacle_grid_set_header_click(h, tag(axis), click)
                }
                Op::Threshold(t) => {
                    grid.set_drag_threshold(t);
                    binnacle_grid_set_drag_threshold(h, t)
                }
                Op::ViewApplied(spec, totals) => {
                    let known = |t: Option<u64>| BinnacleTotal {
                        known: u32::from(t.is_some()),
                        count: t.unwrap_or(0),
                    };
                    let c_totals = BinnacleTotals {
                        axes: [known(totals.rows), known(totals.cols)],
                    };
                    let ok = with_c_view(&spec, |c| binnacle_grid_view_applied(h, c, &c_totals));
                    grid.view_applied(spec, totals);
                    ok
                }
                Op::ViewChanged(spec, totals) => {
                    let known = |t: Option<u64>| BinnacleTotal {
                        known: u32::from(t.is_some()),
                        count: t.unwrap_or(0),
                    };
                    let c_totals = BinnacleTotals {
                        axes: [known(totals.rows), known(totals.cols)],
                    };
                    let ok = with_c_view(&spec, |c| binnacle_grid_view_changed(h, c, &c_totals));
                    grid.view_changed(spec, totals);
                    ok
                }
                Op::RequestView(spec) => {
                    let ok = with_c_view(&spec, |c| binnacle_grid_request_view(h, c));
                    grid.request_view(spec);
                    ok
                }
                Op::ReapplyView => {
                    grid.reapply_view();
                    binnacle_grid_reapply_view(h)
                }
                Op::SetQuickFilter(axis, query) => {
                    let ok = match &query {
                        None => binnacle_grid_set_quick_filter(h, tag(axis), std::ptr::null()),
                        Some(q) => {
                            let keys = query_keys(q);
                            binnacle_grid_set_quick_filter(h, tag(axis), &query_to_c(q, &keys))
                        }
                    };
                    grid.set_quick_filter(axis, query);
                    ok
                }
                Op::QuickFilterSupport(axis, support) => {
                    grid.set_quick_filter_support(axis, support);
                    let bits = support.map_or(0, |s| 1 | u32::from(s.regex) << 1);
                    binnacle_grid_set_quick_filter_support(h, tag(axis), bits)
                }
                Op::HideSupport(axis, hides) => {
                    grid.set_hide_support(axis, hides);
                    binnacle_grid_set_hide_support(h, tag(axis), u32::from(hides))
                }
                Op::Hide(axis, keys) => {
                    let want = match grid.hide(axis, &keys) {
                        Ok(()) => 1,
                        Err(_) => BINNACLE_HIDE_CAPPED,
                    };
                    let keys: Vec<u64> = keys.iter().map(|k| k.0).collect();
                    let got = binnacle_grid_hide(h, tag(axis), keys.as_ptr(), keys.len());
                    assert_eq!(got, want, "hide past the cap or not");
                    1
                }
                Op::HideCap(axis, cap) => {
                    grid.set_hide_cap(axis, cap);
                    binnacle_grid_set_hide_cap(h, tag(axis), cap)
                }
                Op::Show(axis, keys) => {
                    grid.show(axis, &keys);
                    let keys: Vec<u64> = keys.iter().map(|k| k.0).collect();
                    binnacle_grid_show(h, tag(axis), keys.as_ptr(), keys.len())
                }
                Op::SetSupport(axis, key, support) => {
                    grid.set_support(axis, key, support);
                    binnacle_grid_set_support(h, tag(axis), key.0, support_bits(support))
                }
                Op::AddCondition(axis, condition) => {
                    let values = values_to_c(&condition);
                    let c = condition_to_c(&condition, &[], &values);
                    let ok = binnacle_grid_add_condition(h, tag(axis), &c);
                    grid.add_condition(axis, condition);
                    ok
                }
                Op::RemoveCondition(axis, condition) => {
                    let values = values_to_c(&condition);
                    let c = condition_to_c(&condition, &[], &values);
                    grid.remove_condition(axis, &condition);
                    binnacle_grid_remove_condition(h, tag(axis), &c)
                }
                Op::ReplaceCondition(axis, from, to) => {
                    let (from_values, to_values) = (values_to_c(&from), values_to_c(&to));
                    let c_from = condition_to_c(&from, &[], &from_values);
                    let c_to = condition_to_c(&to, &[], &to_values);
                    grid.replace_condition(axis, &from, to.clone());
                    binnacle_grid_replace_condition(h, tag(axis), &c_from, &c_to)
                }
                Op::AddSlicer(condition) => {
                    let (keys, values) = (condition_keys(&condition), values_to_c(&condition));
                    let c = condition_to_c(&condition, &keys, &values);
                    let ok = binnacle_grid_add_slicer(h, &c);
                    grid.add_slicer(condition);
                    ok
                }
                Op::RemoveSlicer(condition) => {
                    let (keys, values) = (condition_keys(&condition), values_to_c(&condition));
                    let c = condition_to_c(&condition, &keys, &values);
                    grid.remove_slicer(&condition);
                    binnacle_grid_remove_slicer(h, &c)
                }
                Op::ReplaceSlicer(from, to) => {
                    let (from_keys, to_keys) = (condition_keys(&from), condition_keys(&to));
                    let (from_values, to_values) = (values_to_c(&from), values_to_c(&to));
                    let c_from = condition_to_c(&from, &from_keys, &from_values);
                    let c_to = condition_to_c(&to, &to_keys, &to_values);
                    grid.replace_slicer(&from, to.clone());
                    binnacle_grid_replace_slicer(h, &c_from, &c_to)
                }
                Op::SetAggregates(axis, key, aggregates) => {
                    let params: Vec<Vec<BinnacleText>> = (aggregates.iter())
                        .map(|a| a.params.iter().map(|p| text_to_c(p)).collect())
                        .collect();
                    let c: Vec<BinnacleAggregate> = (aggregates.iter().zip(&params))
                        .map(|(a, ps)| BinnacleAggregate {
                            name: text_to_c(&a.name),
                            params: ps.as_ptr(),
                            params_len: ps.len(),
                        })
                        .collect();
                    grid.set_aggregates(axis, key, aggregates);
                    binnacle_grid_set_aggregates(h, tag(axis), key.0, c.as_ptr(), c.len())
                }
                Op::SetFind(query) => {
                    let keys = query
                        .as_ref()
                        .map_or_else(Vec::new, |q| query_keys(&q.query));
                    let c = query.as_ref().map(|q| BinnacleFindQuery {
                        query: query_to_c(&q.query, &keys),
                        by: tag(q.by),
                    });
                    let ok = binnacle_grid_set_find(h, c.as_ref().map_or(std::ptr::null(), |c| c));
                    grid.set_find(query);
                    ok
                }
                Op::FindSupport(support) => {
                    grid.set_find_support(support);
                    let bits = support.map_or(0, |s| BINNACLE_FIND | u32::from(s.regex) << 1);
                    binnacle_grid_set_find_support(h, bits)
                }
                Op::FindStepped(found, at, within) => {
                    let found = match at {
                        Some((r, c)) if found.is_some() && shown.iter().all(|s| !s.is_empty()) => {
                            let [rows, cols] = &shown;
                            Some(FindMatch::new(rows[r % rows.len()], cols[c % cols.len()]))
                        }
                        _ => found,
                    };
                    let c = found.map(|m| BinnacleFindMatch {
                        row: m.row.0,
                        col: m.col.0,
                    });
                    match (found, &c, within.iter().all(Vec::is_empty)) {
                        (Some(found), Some(c), false) => {
                            grid.find_stepped_within(found, [&within[0], &within[1]]);
                            let [rows, cols] = within.map(|k| k.iter().map(|k| k.0).collect());
                            let rows: Vec<u64> = rows;
                            let cols: Vec<u64> = cols;
                            binnacle_grid_find_stepped_within(
                                h,
                                c,
                                rows.as_ptr(),
                                rows.len(),
                                cols.as_ptr(),
                                cols.len(),
                            )
                        }
                        _ => {
                            grid.find_stepped(found);
                            binnacle_grid_find_stepped(
                                h,
                                c.as_ref().map_or(std::ptr::null(), |c| c),
                            )
                        }
                    }
                }
                Op::Located(about, answer) => {
                    let Some((axis, key)) = about.or(asked) else {
                        continue;
                    };
                    grid.located(axis, key, answer);
                    let (answer, position) = match answer {
                        Located::Found(p) => (0, p),
                        Located::Gone { suggest: None } => (1, 0),
                        Located::Gone { suggest: Some(p) } => (2, p),
                    };
                    binnacle_grid_located(h, tag(axis), key.0, answer, position)
                }
                Op::Notify(axis, notice) => {
                    let (kind, at, n) = match &notice {
                        ChangeNotice::Reset(count) => (0, 0, *count),
                        ChangeNotice::Reordered => (1, 0, 0),
                        ChangeNotice::Inserted { at, n } => (2, *at, *n),
                        ChangeNotice::Removed { at, n } => (3, *at, *n),
                        ChangeNotice::ContentChanged(r) => (4, r.start, r.end - r.start),
                        ChangeNotice::Refiltered(count) => (5, 0, *count),
                        ChangeNotice::Collapsed { at, n } => (6, *at, *n),
                        ChangeNotice::Expanded { at, n } => (7, *at, *n),
                    };
                    grid.notify(axis, notice);
                    binnacle_grid_notify(h, tag(axis), kind, at, n)
                }
                Op::HeaderDrag {
                    axis,
                    from,
                    to,
                    release,
                } => {
                    let along = &middles[axis.index()];
                    if along.is_empty() {
                        continue;
                    }
                    let across = bands[axis.other().index()] / 2.0;
                    let at = |v: f32| match axis {
                        Axis::Rows => (across, v),
                        Axis::Cols => (v, across),
                    };
                    let (x, y) = at(along[from % along.len()]);
                    let (mx, my) = at(along[to % along.len()]);
                    let up = release.then(|| PointerEvent::up(mx, my));
                    for e in [PointerEvent::down(x, y), PointerEvent::moved(mx, my)]
                        .into_iter()
                        .chain(up)
                    {
                        grid.pointer(e);
                        assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                    }
                    1
                }
                Op::SetMovable(axis, movable) => {
                    grid.set_movable(axis, movable);
                    binnacle_grid_set_movable(h, tag(axis), u32::from(movable))
                }
                Op::MoveAnswered(refusal) => {
                    let ok = match &refusal {
                        None => binnacle_grid_move_answered(h, std::ptr::null(), 0),
                        Some(r) => binnacle_grid_move_answered(h, r.as_ptr(), r.len()),
                    };
                    grid.move_answered(refusal);
                    ok
                }
                Op::EditStarted(row, col) => {
                    grid.edit_started(row, col);
                    binnacle_grid_edit_started(h, row.0, col.0)
                }
                Op::EditEnded(end) => {
                    grid.edit_ended(end);
                    binnacle_grid_edit_ended(h, u32::from(end == EditEnd::Cancel))
                }
                Op::BadText(bytes) => {
                    assert_eq!(binnacle_grid_type_text(h, bytes.as_ptr(), bytes.len()), 0);
                    assert_eq!(
                        binnacle_grid_move_answered(h, bytes.as_ptr(), bytes.len()),
                        0
                    );
                    1
                }
                Op::BadTag(which) => {
                    assert_eq!(send_bad_tag(h, which), 0, "unknown tag {which} accepted");
                    1
                }
            };
            assert_eq!(ok, 1);
            asked = check_effects(h, &mut grid).or(asked);
            check_size_overrides(h, &grid);
        }
    }
    unsafe { binnacle_grid_free(h) };
}

/// The handle lists every Size Override the Grid holds, on each axis.
unsafe fn check_size_overrides(h: *mut BinnacleGrid, grid: &Grid) {
    for axis in Axis::ALL {
        let mut out = BinnacleSizeOverrides {
            keys: std::ptr::null(),
            sizes: std::ptr::null(),
            len: 0,
        };
        assert_eq!(
            unsafe { binnacle_grid_size_overrides(h, tag(axis), &mut out) },
            1
        );
        let keys = unsafe { slice::from_raw_parts(out.keys, out.len) };
        let sizes = unsafe { slice::from_raw_parts(out.sizes, out.len) };
        let got: Vec<(u64, u32)> = (keys.iter().zip(sizes))
            .map(|(&k, s)| (k, s.to_bits()))
            .collect();
        let want: Vec<(u64, u32)> = (grid.size_overrides(axis))
            .map(|(k, s)| (k.0, s.to_bits()))
            .collect();
        assert_eq!(got, want, "{axis:?} Size Overrides");
    }
}

/// Runs a frame on both, answering the Windows with Position Keys, and
/// checks the Snapshots match; returns each strip item's middle along its
/// axis, each band's thickness, and the middle of the Gap after each strip
/// item but the last.
unsafe fn mirrored_frame(
    h: *mut BinnacleGrid,
    grid: &mut Grid,
) -> ([Vec<f32>; 2], [f32; 2], [Vec<f32>; 2]) {
    let mut requests = BinnacleWindowRequests::default();
    assert_eq!(
        unsafe { binnacle_grid_begin_frame(h, 40.0, 30.0, &mut requests) },
        1
    );
    for (axis, range) in grid.begin_frame(Viewport::new(40.0, 30.0)) {
        let k: Vec<u64> = range.clone().collect();
        unsafe { binnacle_grid_set_window(h, tag(axis), range.start, k.as_ptr(), k.len()) };
        let k: Vec<Key> = k.into_iter().map(Key).collect();
        grid.set_window(axis, range.start, &k);
    }
    let mut snap = unsafe { std::mem::zeroed::<BinnacleSnapshot>() };
    assert_eq!(unsafe { binnacle_grid_snapshot(h, &mut snap) }, 1);
    let rust = grid.snapshot();
    unsafe { check_snapshot(&snap, &rust) };
    let middles = Axis::ALL.map(|a| {
        (rust.strip(a).iter())
            .map(|i| i.offset + i.size / 2.0)
            .collect()
    });
    let borders = Axis::ALL.map(|a| {
        (rust.strip(a).windows(2))
            .map(|w| (w[0].offset + w[0].size + w[1].offset) / 2.0)
            .collect()
    });
    (middles, Axis::ALL.map(|a| rust.header(a)), borders)
}

#[derive(Clone, Debug)]
enum MoveOp {
    Frame,
    Drag {
        axis: Axis,
        from: usize,
        to: usize,
        release: bool,
    },
    SetMovable(Axis, bool),
    Answered(Option<String>),
    /// A click on a cell, which clears a refusal.
    Click,
    /// Outlines for the axis's Window from its start, so drags meet groups
    /// and Summary Items.
    Outlines(Axis, Vec<Outline>),
}

#[hegel::composite]
fn move_op(tc: &TestCase) -> MoveOp {
    match tc.draw(gs::integers::<u8>().max_value(7)) {
        7 => {
            let axis = tc.draw(axes());
            MoveOp::Outlines(axis, tc.draw(gs::vecs(move_outline()).max_size(8)))
        }
        0 | 1 => MoveOp::Frame,
        2 | 3 => MoveOp::Drag {
            axis: tc.draw(axes()),
            from: tc.draw(gs::integers::<usize>().max_value(8)),
            to: tc.draw(gs::integers::<usize>().max_value(8)),
            release: tc.draw(gs::weighted_booleans(0.8)),
        },
        4 => MoveOp::SetMovable(tc.draw(axes()), tc.draw(gs::weighted_booleans(0.8))),
        5 => MoveOp::Answered(
            tc.draw(gs::booleans())
                .then(|| tc.draw(gs::text().max_size(8))),
        ),
        _ => MoveOp::Click,
    }
}

/// An outline for a move: a Leaf or a Summary Item under one of two
/// groups, so neighbours often share a parent.
#[hegel::composite]
fn move_outline(tc: &TestCase) -> Outline {
    let parent = tc.draw(gs::sampled_from(vec![Key(90), Key(91)]));
    let mut outline = Outline::leaf(vec![parent]);
    outline.kind = tc.draw(gs::sampled_from(vec![
        ItemKind::Leaf,
        ItemKind::Leaf,
        ItemKind::Group,
        ItemKind::Subtotal,
        ItemKind::GrandTotal,
    ]));
    outline
}

/// Moves through the C API: header drags on either axis, movability, the
/// pending move and its answer, and refusals mirror the Grid's.
#[hegel::test(test_cases = 500)]
fn moves_through_the_c_api_mirror_the_grid(tc: TestCase) {
    let config = || AxisConfig::new(8, 3.0).header(2.0).gap(1.0);
    let c = BinnacleAxisConfig {
        count: 8,
        default_size: 3.0,
        header: 2.0,
        gap: 1.0,
    };
    let h = binnacle_grid_new(c, c);
    let mut grid = Grid::new(config(), config());
    unsafe {
        let (mut middles, mut bands, _) = mirrored_frame(h, &mut grid);
        for op in tc.draw(gs::vecs(move_op()).min_size(1).max_size(30)) {
            let ok = match op {
                MoveOp::Frame => {
                    (middles, bands, _) = mirrored_frame(h, &mut grid);
                    1
                }
                MoveOp::Drag {
                    axis,
                    from,
                    to,
                    release,
                } => {
                    let along = &middles[axis.index()];
                    let across = bands[axis.other().index()] / 2.0;
                    let at = |v: f32| match axis {
                        Axis::Rows => (across, v),
                        Axis::Cols => (v, across),
                    };
                    let (x, y) = at(along[from % along.len()]);
                    let (mx, my) = at(along[to % along.len()]);
                    let up = release.then(|| PointerEvent::up(mx, my));
                    for e in [PointerEvent::down(x, y), PointerEvent::moved(mx, my)]
                        .into_iter()
                        .chain(up)
                    {
                        grid.pointer(e);
                        assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                    }
                    1
                }
                MoveOp::SetMovable(axis, movable) => {
                    grid.set_movable(axis, movable);
                    binnacle_grid_set_movable(h, tag(axis), u32::from(movable))
                }
                MoveOp::Answered(refusal) => {
                    let ok = match &refusal {
                        None => binnacle_grid_move_answered(h, std::ptr::null(), 0),
                        Some(r) => binnacle_grid_move_answered(h, r.as_ptr(), r.len()),
                    };
                    grid.move_answered(refusal);
                    ok
                }
                MoveOp::Click => {
                    for e in [PointerEvent::down(20.0, 20.0), PointerEvent::up(20.0, 20.0)] {
                        grid.pointer(e);
                        assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                    }
                    1
                }
                MoveOp::Outlines(axis, outlines) => {
                    grid.set_outlines(axis, 0, &outlines);
                    with_c_outlines(&outlines, |c| {
                        binnacle_grid_set_outlines(h, tag(axis), 0, c.as_ptr(), c.len())
                    })
                }
            };
            assert_eq!(ok, 1);
            check_effects(h, &mut grid);
        }
        mirrored_frame(h, &mut grid);
        binnacle_grid_free(h);
    }
}

#[derive(Clone, Debug)]
enum CellOp {
    Frame,
    /// A press and release on the cell at these indices of the last frame's
    /// strips (wrapped), with this click count.
    Click(usize, usize, u32),
    /// A press on the border after the item at this index of the last
    /// frame's strip (wrapped), with this click count, then a move by
    /// `travel` and the release if `release`.
    Border {
        axis: Axis,
        index: usize,
        clicks: u32,
        travel: f32,
        release: bool,
    },
    Action(Action),
    /// The host opened its editor on the cell at these Positions.
    EditStarted(u64, u64),
    EditEnded(EditEnd),
}

#[hegel::composite]
fn cell_op(tc: &TestCase) -> CellOp {
    let index = || gs::integers::<usize>().max_value(12);
    let clicks = || gs::integers::<u32>().min_value(1).max_value(2);
    match tc.draw(gs::integers::<u8>().max_value(6)) {
        0 => CellOp::Frame,
        1 => CellOp::Click(tc.draw(index()), tc.draw(index()), tc.draw(clicks())),
        2 => CellOp::Border {
            axis: tc.draw(axes()),
            index: tc.draw(index()),
            clicks: tc.draw(clicks()),
            travel: tc.draw(gs::floats::<f32>().min_value(-4.0).max_value(4.0)),
            release: tc.draw(gs::weighted_booleans(0.8)),
        },
        3 => CellOp::EditStarted(
            tc.draw(gs::integers::<u64>().max_value(9)),
            tc.draw(gs::integers::<u64>().max_value(9)),
        ),
        4 => CellOp::EditEnded(tc.draw(gs::sampled_from(vec![EditEnd::Commit, EditEnd::Cancel]))),
        _ => CellOp::Action(tc.draw(hegel::one_of!(
            gs::sampled_from(vec![Action::Activate, Action::BeginEdit, Action::Cancel]),
            action(),
        ))),
    }
}

/// Activation, editing and resizing through the C API: double-clicks and
/// the Activate and Begin Edit Actions, the host's editor opening and
/// closing, and drags and double-clicks on borders send the Grid's Effects.
#[hegel::test(test_cases = 300)]
fn cell_gestures_through_the_c_api_mirror_the_grid(tc: TestCase) {
    // Configs with borders and bands to press on; the_c_api_mirrors_the_grid
    // covers the rest.
    let [rows, cols] = [(); 2].map(|_| BinnacleAxisConfig {
        count: tc.draw(gs::integers::<u64>().min_value(1).max_value(12)),
        default_size: tc.draw(gs::floats::<f32>().min_value(1.0).max_value(6.0)),
        header: tc.draw(gs::floats::<f32>().min_value(1.0).max_value(4.0)),
        gap: tc.draw(gs::floats::<f32>().min_value(0.25).max_value(2.0)),
    });
    let config = |c: BinnacleAxisConfig| {
        AxisConfig::new(c.count, c.default_size)
            .header(c.header)
            .gap(c.gap)
    };
    let h = binnacle_grid_new(rows, cols);
    let mut grid = Grid::new(config(rows), config(cols));
    unsafe {
        let (mut middles, mut bands, mut borders) = mirrored_frame(h, &mut grid);
        for op in tc.draw(gs::vecs(cell_op()).min_size(1).max_size(30)) {
            let mut press = |events: &[PointerEvent]| {
                for &e in events {
                    grid.pointer(e);
                    assert_eq!(binnacle_grid_pointer(h, pointer_event(e)), 1);
                }
                1
            };
            let ok = match op {
                CellOp::Frame => {
                    (middles, bands, borders) = mirrored_frame(h, &mut grid);
                    1
                }
                CellOp::Click(r, c, clicks) => {
                    let [rows, cols] = &middles;
                    let (x, y) = (cols[c % cols.len()], rows[r % rows.len()]);
                    press(&[
                        PointerEvent::down(x, y).clicks(clicks),
                        PointerEvent::up(x, y),
                    ])
                }
                CellOp::Border {
                    axis,
                    index,
                    clicks,
                    travel,
                    release,
                } => {
                    let along = &borders[axis.index()];
                    if along.is_empty() {
                        continue;
                    }
                    let along = along[index % along.len()];
                    let across = bands[axis.other().index()] / 2.0;
                    let at = |v: f32| match axis {
                        Axis::Rows => (across, v),
                        Axis::Cols => (v, across),
                    };
                    let ((x, y), (mx, my)) = (at(along), at(along + travel));
                    let down = PointerEvent::down(x, y).clicks(clicks);
                    match release {
                        true => {
                            press(&[down, PointerEvent::moved(mx, my), PointerEvent::up(mx, my)])
                        }
                        false => press(&[down, PointerEvent::moved(mx, my)]),
                    }
                }
                CellOp::Action(a) => {
                    let ok = send(h, &a);
                    grid.action(a);
                    ok
                }
                CellOp::EditStarted(row, col) => {
                    grid.edit_started(Key(row), Key(col));
                    binnacle_grid_edit_started(h, row, col)
                }
                CellOp::EditEnded(end) => {
                    grid.edit_ended(end);
                    binnacle_grid_edit_ended(h, u32::from(end == EditEnd::Cancel))
                }
            };
            assert_eq!(ok, 1);
            check_effects(h, &mut grid);
        }
        mirrored_frame(h, &mut grid);
        binnacle_grid_free(h);
    }
}

fn valid(c: &BinnacleAxisConfig) -> bool {
    c.default_size.is_finite() && c.default_size > 0.0 && c.gap.is_finite() && c.gap >= 0.0
}

#[hegel::composite]
fn any_config(tc: &TestCase) -> BinnacleAxisConfig {
    BinnacleAxisConfig {
        count: tc.draw(gs::integers::<u64>()),
        default_size: tc.draw(gs::floats::<f32>()),
        header: tc.draw(gs::floats::<f32>()),
        gap: tc.draw(gs::floats::<f32>()),
    }
}

/// The header's contract for `binnacle_grid_new`: a Grid for valid configs,
/// NULL (not a panic) when either is invalid.
#[hegel::test(test_cases = 1000)]
fn a_grid_is_made_only_from_valid_configs(tc: TestCase) {
    let (rows, cols) = (tc.draw(any_config()), tc.draw(any_config()));
    let h = binnacle_grid_new(rows, cols);
    assert_eq!(!h.is_null(), valid(&rows) && valid(&cols));
    unsafe { binnacle_grid_free(h) };
}

/// Support declared for items in view, comparisons included, reaches the
/// C headers as it reaches the Rust Snapshot's. The main property's Keys
/// rarely land in view, so this one declares support on the Keys shown.
#[hegel::test(test_cases = 300)]
fn support_on_items_in_view_through_the_c_api_mirrors_the_grid(tc: TestCase) {
    let config = || AxisConfig::new(8, 3.0).header(2.0).gap(1.0);
    let c = BinnacleAxisConfig {
        count: 8,
        default_size: 3.0,
        header: 2.0,
        gap: 1.0,
    };
    let h = binnacle_grid_new(c, c);
    let mut grid = Grid::new(config(), config());
    unsafe {
        mirrored_frame(h, &mut grid);
        for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(6)) {
            let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
            let key = Key(tc.draw(gs::integers::<u64>().max_value(7)));
            let support = Support::default()
                .with_sort(tc.draw(gs::booleans()))
                .with_text(tc.draw(gs::optional(gs::booleans().map(TextSupport::new))))
                .with_one_of(tc.draw(gs::booleans()))
                .with_compare(tc.draw(gs::optional(gs::sampled_from(vec![
                    Compared::Numbers,
                    Compared::Dates,
                ]))));
            grid.set_support(axis, key, support);
            assert_eq!(
                binnacle_grid_set_support(h, tag(axis), key.0, support_bits(support)),
                1
            );
            mirrored_frame(h, &mut grid);
        }
        binnacle_grid_free(h);
    }
}

/// Activating and copying on a grouped table, where drill-downs and copy
/// labels arise, with outlines on both axes: the C API mirrors the Grid.
#[hegel::test(test_cases = 200)]
fn summary_items_through_the_c_api_mirror_the_grid(tc: TestCase) {
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(8));
    let config = AxisConfig::new(count, 2.0).header(2.0);
    let c_config = BinnacleAxisConfig {
        count,
        default_size: 2.0,
        header: 2.0,
        gap: 0.0,
    };
    let h = binnacle_grid_new(c_config, c_config);
    let mut grid = Grid::new(config, config);
    let mut spec = ViewSpec::default();
    let mut grouping = tc.draw(grouping());
    for axis in Axis::ALL {
        if grouping.levels(axis).is_empty() && tc.draw(gs::weighted_booleans(0.8)) {
            grouping
                .levels_mut(axis)
                .push(Level::field(tc.draw(key(axis.other()))));
        }
    }
    spec.grouping = Some(grouping);
    unsafe {
        let ok = with_c_view(&spec, |c| {
            binnacle_grid_view_applied(h, c, &BinnacleTotals::default())
        });
        assert_eq!(ok, 1);
        grid.view_applied(spec, Totals::default());
        let mut requests = BinnacleWindowRequests::default();
        binnacle_grid_begin_frame(h, 40.0, 30.0, &mut requests);
        for (axis, range) in grid.begin_frame(Viewport::new(40.0, 30.0)) {
            let k: Vec<u64> = range.clone().collect();
            binnacle_grid_set_window(h, tag(axis), range.start, k.as_ptr(), k.len());
            let k: Vec<Key> = k.into_iter().map(Key).collect();
            grid.set_window(axis, range.start, &k);
            let outlines = tc.draw(gs::vecs(outline(axis)).max_size(count as usize));
            let ok = with_c_outlines(&outlines, |c| {
                binnacle_grid_set_outlines(h, tag(axis), range.start, c.as_ptr(), c.len())
            });
            assert_eq!(ok, 1);
            grid.set_outlines(axis, range.start, &outlines);
        }
        check_effects(h, &mut grid);
        for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(6)) {
            let (r, c) = (
                tc.draw(gs::integers::<u64>().max_value(count - 1)),
                tc.draw(gs::integers::<u64>().max_value(count - 1)),
            );
            let (y, x) = (3.0 + 2.0 * r as f32, 3.0 + 2.0 * c as f32);
            let mut press = |down: PointerEvent, up: PointerEvent| {
                for e in [down, up] {
                    grid.pointer(e);
                    binnacle_grid_pointer(h, pointer_event(e));
                }
            };
            match tc.draw(gs::integers::<u8>().max_value(4)) {
                // A click on a cell, a double-click activating it.
                0 => {
                    let clicks = tc.draw(gs::integers::<u32>().min_value(1).max_value(2));
                    let down = PointerEvent::down(x, y).modifiers(tc.draw(modifiers()));
                    press(down.clicks(clicks), PointerEvent::up(x, y));
                }
                // A click on a row header, which may select a group.
                1 => {
                    let down = PointerEvent::down(1.0, y).modifiers(tc.draw(modifiers()));
                    press(down, PointerEvent::up(1.0, y));
                }
                // A click on a column header, which may select a group.
                2 => {
                    let down = PointerEvent::down(x, 1.0).modifiers(tc.draw(modifiers()));
                    press(down, PointerEvent::up(x, 1.0));
                }
                3 => {
                    grid.action(Action::Activate);
                    binnacle_grid_activate(h);
                }
                _ => {
                    grid.action(Action::Copy);
                    binnacle_grid_copy(h);
                }
            }
            check_effects(h, &mut grid);
        }
        binnacle_grid_free(h);
    }
}

/// A find step answered within groups through the C API: the Grid asks to
/// open each closed group and focuses the match once the View Spec is
/// applied, as the Rust Grid does.
#[hegel::test(test_cases = 300)]
fn find_steps_within_groups_through_the_c_api_mirror_the_grid(tc: TestCase) {
    let config = || AxisConfig::new(8, 3.0).header(2.0).gap(1.0);
    let c = BinnacleAxisConfig {
        count: 8,
        default_size: 3.0,
        header: 2.0,
        gap: 1.0,
    };
    let h = binnacle_grid_new(c, c);
    let mut grid = Grid::new(config(), config());
    let apply = |h: *mut BinnacleGrid, grid: &mut Grid, spec: ViewSpec| {
        let totals = BinnacleTotals::default();
        let ok = with_c_view(&spec, |c| unsafe {
            binnacle_grid_view_applied(h, c, &totals)
        });
        assert_eq!(ok, 1);
        grid.view_applied(spec, Totals::default());
    };
    unsafe {
        mirrored_frame(h, &mut grid);
        let mut grouping = Grouping::rows_by(Key(100));
        grouping.row_levels.push(Level::Field {
            field: Key(101),
            subtotal: Placement::Off,
        });
        grouping.rows.expand =
            binnacle_core::Expand::to(Depth::Levels(tc.draw(gs::integers::<u32>().max_value(2))));
        if tc.draw(gs::booleans()) {
            grouping.rows.expand = grouping.rows.expand.clone().with(0, Key(7), true);
        }
        apply(
            h,
            &mut grid,
            ViewSpec::default().with_grouping(Some(grouping)),
        );
        let query = FindQuery::new(TextQuery::new("a"));
        let keys = query_keys(&query.query);
        let c = BinnacleFindQuery {
            query: query_to_c(&query.query, &keys),
            by: tag(query.by),
        };
        assert_eq!(binnacle_grid_set_find(h, &c), 1);
        grid.set_find(Some(query));
        check_effects(h, &mut grid);
        for _ in 0..tc.draw(gs::integers::<u32>().min_value(1).max_value(3)) {
            grid.action(Action::FindNext);
            assert_eq!(binnacle_grid_find_next(h), 1);
            check_effects(h, &mut grid);
            let found = FindMatch::new(
                Key(tc.draw(gs::integers::<u64>().max_value(7))),
                Key(tc.draw(gs::sampled_from(vec![0, 3, 101]))),
            );
            let rows =
                tc.draw(gs::vecs(gs::sampled_from(vec![Key(6), Key(7), Key(8)])).max_size(2));
            let cols = tc.draw(gs::vecs(gs::sampled_from(vec![Key(6), Key(7)])).max_size(1));
            grid.find_stepped_within(found, [&rows, &cols]);
            let [r, k]: [Vec<u64>; 2] = [&rows, &cols].map(|k| k.iter().map(|k| k.0).collect());
            let c = BinnacleFindMatch {
                row: found.row.0,
                col: found.col.0,
            };
            let ok =
                binnacle_grid_find_stepped_within(h, &c, r.as_ptr(), r.len(), k.as_ptr(), k.len());
            assert_eq!(ok, 1);
            check_effects(h, &mut grid);
            // Answer the expands asked, sometimes. Both take a Snapshot, as
            // reading one clears its damage.
            let mut snap = std::mem::zeroed::<BinnacleSnapshot>();
            assert_eq!(binnacle_grid_snapshot(h, &mut snap), 1);
            let rust = grid.snapshot();
            check_snapshot(&snap, &rust);
            let requested = rust.requested_view().cloned();
            if tc.draw(gs::booleans())
                && let Some(spec) = requested
            {
                apply(h, &mut grid, spec);
                check_effects(h, &mut grid);
            }
            mirrored_frame(h, &mut grid);
        }
        assert_eq!(
            binnacle_grid_find_stepped_within(
                h,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0
            ),
            0
        );
        binnacle_grid_free(h);
    }
}

/// An axis's view read back from C, as a host would.
unsafe fn axis_view_from_c(v: &BinnacleAxisView) -> (SortSpec, FilterSpec, Vec<Key>) {
    let sort = unsafe { c_slice(v.sort, v.sort_len) }
        .iter()
        .map(|s| SortColumn {
            key: Key(s.key),
            direction: match s.direction {
                0 => SortDirection::Ascending,
                1 => SortDirection::Descending,
                d => panic!("direction {d}"),
            },
        })
        .collect();
    let quick = match v.has_quick {
        0 => None,
        1 => Some(unsafe { query_from_c(&v.quick) }),
        q => panic!("has_quick {q}"),
    };
    let conditions: Vec<Condition> = unsafe { c_slice(v.conditions, v.conditions_len) }
        .iter()
        .map(|c| unsafe { condition_from_c(c) })
        .collect();
    let filter = FilterSpec::default()
        .with_quick(quick)
        .with_conditions(conditions.into());
    let hidden = unsafe { c_slice(v.hidden, v.hidden_len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    (SortSpec::new(sort), filter, hidden)
}

/// A View Spec read back from C, as a host would.
unsafe fn view_from_c(v: &BinnacleViewSpec) -> ViewSpec {
    let mut spec = ViewSpec::default();
    for axis in Axis::ALL {
        let (sort, filter, hidden) = unsafe { axis_view_from_c(&v.axes[tag(axis) as usize]) };
        spec = (spec.with_sort(axis, sort))
            .with_filter(axis, filter)
            .with_hidden(axis, hidden);
    }
    let grouping = match v.has_grouping {
        0 => None,
        1 => Some(unsafe { grouping_from_c(&v.grouping) }),
        g => panic!("has_grouping {g}"),
    };
    spec.with_grouping(grouping)
}

type LayerFigure = Option<binnacle_ffi::Figure>;

/// A path read back from C, as a host would.
unsafe fn path_from_c(steps: *const BinnacleGroupStep, len: usize) -> binnacle_group::Path {
    unsafe { c_slice(steps, len) }
        .iter()
        .map(|s| (Key(s.field), unsafe { value_from_c(&s.value) }))
        .collect()
}

/// Figures read back from C, as a host would.
unsafe fn figures_from_c(figures: *const BinnacleFigure, len: usize) -> Vec<LayerFigure> {
    unsafe { c_slice(figures, len) }
        .iter()
        .map(|f| match f.kind {
            BINNACLE_FIGURE_BLANK => None,
            BINNACLE_FIGURE_NUMBER => Some(binnacle_ffi::Figure::Number(f.number)),
            BINNACLE_FIGURE_TEXT => {
                Some(binnacle_ffi::Figure::Text(unsafe { text_from_c(f.text) }))
            }
            k => panic!("figure kind {k}"),
        })
        .collect()
}

/// A batch read back from C, as a host would.
unsafe fn batch_from_c(b: &BinnacleGroupBatch) -> binnacle_group::Batch {
    use binnacle_group::{Batch, Request, RequestId, Scope, Stamp, Want};
    let s = &b.scope;
    let (sort, filter, hidden) = unsafe { axis_view_from_c(&s.rows) };
    // Read the values back through a grouping, as grouping_from_c does.
    let values = unsafe {
        grouping_from_c(&BinnacleGrouping {
            values: s.values,
            values_len: s.values_len,
            ..BinnacleGrouping::default()
        })
    }
    .values;
    let scope = Scope {
        levels: unsafe { c_slice(s.levels, s.levels_len) }
            .iter()
            .map(|&k| Key(k))
            .collect(),
        values,
        slicers: unsafe { c_slice(s.slicers, s.slicers_len) }
            .iter()
            .map(|c| unsafe { condition_from_c(c) })
            .collect(),
        filter,
        hidden,
        sort,
        // No pivot over C: nothing crosses.
        columns: Vec::new(),
        col_sort: SortSpec::default(),
        col_filter: FilterSpec::default(),
        path_keys: Vec::new(),
    };
    let requests = unsafe { c_slice(b.requests, b.requests_len) }
        .iter()
        .map(|r| Request {
            id: RequestId(r.id),
            path: unsafe { path_from_c(r.path, r.path_len) },
            want: match r.want {
                BINNACLE_WANT_GROUPS => Want::Groups,
                BINNACLE_WANT_LEAVES => Want::Leaves,
                BINNACLE_WANT_LOCATE => Want::Locate(Key(r.key)),
                BINNACLE_WANT_TOTAL => Want::Total,
                w => panic!("want {w}"),
            },
            range: r.start..r.end,
        })
        .collect();
    Batch {
        stamp: Stamp(b.stamp),
        scope,
        requests,
    }
}

/// Runs `f` with `answer` in C form, its arrays alive for the call.
fn with_c_answer<T>(
    answer: &binnacle_group::Answer<LayerFigure>,
    f: impl FnOnce(&BinnacleGroupAnswer) -> T,
) -> T {
    use binnacle_group::Body;
    let figures = |fs: &[LayerFigure]| -> Vec<BinnacleFigure> {
        fs.iter()
            .map(|f| match f {
                None => BinnacleFigure::default(),
                Some(binnacle_ffi::Figure::Number(number)) => BinnacleFigure {
                    kind: BINNACLE_FIGURE_NUMBER,
                    number: *number,
                    ..BinnacleFigure::default()
                },
                Some(binnacle_ffi::Figure::Text(t)) => BinnacleFigure {
                    kind: BINNACLE_FIGURE_TEXT,
                    text: BinnacleText {
                        data: t.as_ptr(),
                        len: t.len(),
                    },
                    ..BinnacleFigure::default()
                },
            })
            .collect()
    };
    let path: Vec<BinnacleGroupStep> = (answer.path.iter())
        .map(|(field, value)| BinnacleGroupStep {
            field: field.0,
            value: value_to_c(value),
        })
        .collect();
    let mut c = BinnacleGroupAnswer {
        stamp: answer.stamp.0,
        has_id: u32::from(answer.id.is_some()),
        id: answer.id.map_or(0, |id| id.0),
        path: path.as_ptr(),
        path_len: path.len(),
        start: answer.start,
        body: BINNACLE_BODY_GROUPS,
        total: 0,
        groups: std::ptr::null(),
        groups_len: 0,
        keys: std::ptr::null(),
        keys_len: 0,
        key: 0,
        found: 0,
        figures: std::ptr::null(),
        figures_len: 0,
    };
    let (row_figures, rows, keys, total): (Vec<_>, Vec<_>, Vec<u64>, Vec<_>);
    match &answer.body {
        Body::Groups { total: n, groups } => {
            row_figures = groups.iter().map(|g| figures(&g.aggregates)).collect();
            rows = (groups.iter().zip(&row_figures))
                .map(|(g, fs)| BinnacleGroupRow {
                    value: value_to_c(&g.value),
                    children: g.children,
                    figures: fs.as_ptr(),
                    figures_len: fs.len(),
                })
                .collect();
            c.total = *n;
            c.groups = rows.as_ptr();
            c.groups_len = rows.len();
        }
        Body::Leaves { keys: ks } => {
            keys = ks.iter().map(|k| k.0).collect();
            c.body = BINNACLE_BODY_LEAVES;
            c.keys = keys.as_ptr();
            c.keys_len = keys.len();
        }
        Body::Located { key, found } => {
            c.body = BINNACLE_BODY_LOCATED;
            c.key = key.0;
            c.found = u32::from(*found);
        }
        Body::Total { aggregates } => {
            total = figures(aggregates);
            c.body = BINNACLE_BODY_TOTAL;
            c.figures = total.as_ptr();
            c.figures_len = total.len();
        }
        Body::Columns { .. } => unreachable!("the C wire has no pivot"),
    }
    f(&c)
}

/// The backend both layers answer from: 30 Leaves, Keys 1000 to 1029 (the
/// rows' Keys), each with a value in every field, blank now and then.
struct LayerFake;

impl LayerFake {
    fn value(row: u64, field: Key) -> Value {
        match (row + field.0) % 5 {
            0 => Value::Blank,
            v => Value::Text(format!("{}", v % 3)),
        }
    }

    fn under(scope: &binnacle_group::Scope, path: &binnacle_group::Path) -> Vec<u64> {
        (0..30)
            .filter(|row| !scope.hidden.contains(&Key(1000 + row)))
            .filter(|&row| path.iter().all(|(f, v)| Self::value(row, *f) == *v))
            .collect()
    }

    fn figures(scope: &binnacle_group::Scope, rows: &[u64]) -> Vec<LayerFigure> {
        (scope.values.iter().enumerate())
            .map(|(i, _)| match i % 3 {
                0 => Some(binnacle_ffi::Figure::Number(rows.len() as f64)),
                1 => Some(binnacle_ffi::Figure::Text(format!("{} rows", rows.len()))),
                _ => None,
            })
            .collect()
    }

    /// The whole answer to `request`, `more` past its end when that is
    /// asked for.
    fn answer(
        batch: &binnacle_group::Batch,
        request: &binnacle_group::Request,
        more: u64,
    ) -> binnacle_group::Answer<LayerFigure> {
        use binnacle_group::{Answer, Body, GroupRow, Want};
        let scope = &batch.scope;
        let mut answer = Answer {
            stamp: batch.stamp,
            id: Some(request.id),
            path: request.path.clone(),
            start: request.range.start,
            body: Body::Leaves { keys: Vec::new() },
        };
        let end = |n: u64| request.range.end.saturating_add(more).min(n);
        let rows = Self::under(scope, &request.path);
        answer.body = match request.want {
            Want::Groups => {
                let Some(&field) = scope.levels.get(request.path.len()) else {
                    return answer;
                };
                let mut values: Vec<Value> = Vec::new();
                for &row in &rows {
                    let v = Self::value(row, field);
                    if !values.contains(&v) {
                        values.push(v);
                    }
                }
                let total = values.len() as u64;
                let groups = (values.iter())
                    .skip(request.range.start as usize)
                    .take(end(total).saturating_sub(request.range.start) as usize)
                    .map(|v| {
                        let mut path = request.path.clone();
                        path.push((field, v.clone()));
                        let inner = Self::under(scope, &path);
                        let children = match scope.levels.get(path.len()) {
                            Some(&next) => {
                                let mut seen: Vec<Value> = Vec::new();
                                for &row in &inner {
                                    let v = Self::value(row, next);
                                    if !seen.contains(&v) {
                                        seen.push(v);
                                    }
                                }
                                seen.len() as u64
                            }
                            None => inner.len() as u64,
                        };
                        GroupRow {
                            value: v.clone(),
                            children,
                            aggregates: Self::figures(scope, &inner),
                            cells: Vec::new(),
                        }
                    })
                    .collect();
                Body::Groups { total, groups }
            }
            Want::Leaves => Body::Leaves {
                keys: (rows.iter())
                    .skip(request.range.start as usize)
                    .take(end(rows.len() as u64).saturating_sub(request.range.start) as usize)
                    .map(|&row| Key(1000 + row))
                    .collect(),
            },
            Want::Total => Body::Total {
                aggregates: Self::figures(scope, &rows),
            },
            Want::Locate(key) => {
                let row = key.0.wrapping_sub(1000);
                let found = row < 30 && !scope.hidden.contains(&key);
                if found {
                    answer.path = (scope.levels.iter())
                        .map(|&f| (f, Self::value(row, f)))
                        .collect();
                    let under = Self::under(scope, &answer.path);
                    answer.start = under.iter().position(|&r| r == row).unwrap_or(0) as u64;
                } else {
                    answer.path = Vec::new();
                    answer.start = 0;
                }
                Body::Located { key, found }
            }
            Want::Columns => unreachable!("the C wire has no pivot"),
        };
        answer
    }
}

#[derive(Clone, Debug)]
enum LayerOp {
    View(Box<ViewSpec>),
    Refresh,
    /// Polls a batch from both and answers each request still due, in
    /// order or reversed, with shapes taken in turn: `more` past its end,
    /// unnamed, or left for later.
    Pump(bool, Vec<(u64, bool, bool)>),
    Window(Axis, u64, u64),
    Locate(Axis, Key),
    /// The Summary Item at this index of the last Window.
    Summary(usize),
    /// The spec in force again, the group at this index of the last Window
    /// toggled through the rows' except Keys: it opens or closes.
    Toggle(usize),
    /// An answer whose body tag is unknown: refused, nothing changes.
    BadAnswer,
}

/// A View Spec for the layer: usually the rows grouped by one or two of
/// the fields the fake backend knows, otherwise any spec.
#[hegel::composite]
fn layer_spec(tc: &TestCase) -> ViewSpec {
    let spec = tc.draw(view_spec());
    if tc.draw(gs::weighted_booleans(0.2)) {
        return spec;
    }
    let mut grouping = tc.draw(grouping());
    let fields = tc.draw(
        gs::vecs(gs::integers::<u64>().max_value(3))
            .min_size(1)
            .max_size(2),
    );
    *grouping.levels_mut(Axis::Rows) = (fields.into_iter())
        .map(|f| Level::Field {
            field: Key(5000 + f),
            subtotal: tc.draw(placement()),
        })
        .collect();
    if tc.draw(gs::weighted_booleans(0.8)) {
        grouping.levels_mut(Axis::Cols).clear();
    }
    spec.with_grouping(Some(grouping))
}

#[hegel::composite]
fn layer_op(tc: &TestCase) -> LayerOp {
    match tc.draw(gs::integers::<u8>().max_value(13)) {
        0 => LayerOp::View(Box::new(tc.draw(layer_spec()))),
        1 => LayerOp::Refresh,
        2..=5 => LayerOp::Pump(
            tc.draw(gs::booleans()),
            tc.draw(
                gs::vecs(hegel::tuples!(
                    gs::sampled_from(vec![0, 0, 3, 100]),
                    gs::weighted_booleans(0.2),
                    gs::weighted_booleans(0.2)
                ))
                .max_size(5),
            ),
        ),
        6..=8 => {
            // Mostly near the top, where a few groups are.
            let start = match tc.draw(gs::weighted_booleans(0.7)) {
                true => tc.draw(gs::integers::<u64>().max_value(3)),
                false => tc.draw(gs::integers::<u64>().max_value(40)),
            };
            let len = tc.draw(gs::integers::<u64>().max_value(12));
            let axis = match tc.draw(gs::weighted_booleans(0.8)) {
                true => Axis::Rows,
                false => Axis::Cols,
            };
            LayerOp::Window(axis, start, start + len)
        }
        9 => LayerOp::Locate(
            tc.draw(axes()),
            Key(1000 + tc.draw(gs::integers::<u64>().max_value(32))),
        ),
        10 => LayerOp::Summary(tc.draw(gs::integers::<usize>().max_value(10))),
        11 | 12 => LayerOp::Toggle(tc.draw(gs::integers::<usize>().max_value(10))),
        _ => LayerOp::BadAnswer,
    }
}

/// The grouping layer through the C API mirrors the Rust layer: given the
/// same View Specs and answers, it polls the same batches, applies the same
/// specs with the same Change Notices, and shows the same counts, Windows,
/// Locates, Summary Items and holdings.
#[hegel::test(test_cases = 1000)]
fn the_c_api_mirrors_the_grouping_layer(tc: TestCase) {
    use binnacle_group::GroupingLayer;
    assert!(binnacle_group_layer_new(2, 0).is_null());
    let cache = tc.draw(gs::optional(gs::integers::<u64>().max_value(80)));
    let h = binnacle_group_layer_new(u32::from(cache.is_some()), cache.unwrap_or(0));
    assert!(!h.is_null());
    let mut layer: GroupingLayer<LayerFigure> = match cache {
        Some(n) => GroupingLayer::with_cache(n),
        None => GroupingLayer::new(),
    };
    let mut shown: Vec<Key> = Vec::new();
    let mut due: Vec<(binnacle_group::Batch, binnacle_group::Request)> = Vec::new();
    let mut current = ViewSpec::default();
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(40));
    for step in 0..steps {
        // A spec first, so most cases group.
        let op = match step {
            0 => LayerOp::View(Box::new(tc.draw(layer_spec()))),
            _ => tc.draw(layer_op()),
        };
        match &op {
            LayerOp::View(spec) => {
                // The C layer does not pivot: it drops the column levels and
                // the values pseudo-level.
                let mut dropped = (**spec).clone();
                if let Some(g) = dropped.grouping.as_mut() {
                    g.col_levels.clear();
                    g.row_levels.retain(|l| *l != Level::Values);
                }
                with_c_view(spec, |c| {
                    assert_eq!(unsafe { binnacle_group_layer_request_view(h, c) }, 1);
                    layer.request_view(dropped.clone());
                });
                current = dropped;
            }
            LayerOp::Toggle(i) => {
                let (Some(&key), Some(grouping)) = (shown.get(*i), current.grouping.as_mut())
                else {
                    continue;
                };
                let except = &mut grouping.axis_mut(Axis::Rows).expand.except;
                match except.iter().position(|&k| k == key) {
                    Some(at) => _ = except.remove(at),
                    None => except.push(key),
                }
                with_c_view(&current, |c| {
                    assert_eq!(unsafe { binnacle_group_layer_request_view(h, c) }, 1);
                    layer.request_view(current.clone());
                });
            }
            LayerOp::Refresh => {
                assert_eq!(unsafe { binnacle_group_layer_refresh(h) }, 1);
                layer.refresh();
            }
            LayerOp::Pump(reverse, shapes) => {
                let mut c = BinnacleGroupBatch::default();
                let polled = unsafe { binnacle_group_layer_poll_batch(h, &mut c) };
                let want = layer.poll_batch();
                assert_eq!(polled, i32::from(want.is_some()));
                if let Some(want) = want {
                    assert_eq!(unsafe { batch_from_c(&c) }, want);
                    for r in &want.requests {
                        due.push((want.clone(), r.clone()));
                    }
                }
                // Every request still due, some left for a later pump and
                // so answered late, perhaps after a newer spec.
                if *reverse {
                    due.reverse();
                }
                let mut kept = Vec::new();
                for (i, (batch, request)) in std::mem::take(&mut due).into_iter().enumerate() {
                    let (more, unnamed, later) = match shapes.len() {
                        0 => (0, false, false),
                        n => shapes[i % n],
                    };
                    if later {
                        kept.push((batch, request));
                        continue;
                    }
                    let mut answer = LayerFake::answer(&batch, &request, more);
                    if unnamed {
                        answer.id = None;
                    }
                    with_c_answer(&answer, |c| {
                        assert_eq!(unsafe { binnacle_group_layer_answer(h, c) }, 1);
                    });
                    layer.answer(answer);
                }
                due = kept;
            }
            LayerOp::Window(axis, start, end) => {
                let mut items: *const BinnacleGroupItem = std::ptr::null();
                let mut len = 0;
                let ok = unsafe {
                    binnacle_group_layer_window(h, tag(*axis), *start, *end, &mut items, &mut len)
                };
                assert_eq!(ok, 1);
                let got: Vec<(Key, Outline)> = unsafe { c_slice(items, len) }
                    .iter()
                    .map(|i| (Key(i.key), unsafe { outline_from_c(&i.outline) }))
                    .collect();
                let want = layer.window(*axis, *start..*end);
                assert_eq!(got, want, "Window {start}..{end} of {axis:?}");
                shown = want.into_iter().map(|(k, _)| k).collect();
            }
            LayerOp::Locate(axis, key) => {
                let (mut answer, mut position) = (99, 99);
                let got = unsafe {
                    binnacle_group_layer_locate(h, tag(*axis), key.0, &mut answer, &mut position)
                };
                let got = (got == 1).then(|| match answer {
                    BINNACLE_LOCATED_FOUND => Located::Found(position),
                    BINNACLE_LOCATED_GONE => Located::Gone { suggest: None },
                    BINNACLE_LOCATED_GONE_SUGGEST => Located::Gone {
                        suggest: Some(position),
                    },
                    a => panic!("located {a}"),
                });
                assert_eq!(got, layer.locate(*axis, *key), "locate {key:?}");
            }
            LayerOp::Summary(i) => {
                let key = shown.get(*i).copied().unwrap_or(Key(1 << 63));
                let mut c = BinnacleGroupSummary::default();
                let got = unsafe { binnacle_group_layer_summary(h, key.0, &mut c) };
                let want = layer.summary(key);
                assert_eq!(got, i32::from(want.is_some()));
                if let Some(want) = want {
                    let kind = match want.kind {
                        ItemKind::Leaf => BINNACLE_ITEM_LEAF,
                        ItemKind::Group => BINNACLE_ITEM_GROUP,
                        ItemKind::Subtotal => BINNACLE_ITEM_SUBTOTAL,
                        ItemKind::GrandTotal => BINNACLE_ITEM_GRAND_TOTAL,
                    };
                    assert_eq!((c.kind, c.children), (kind, want.children));
                    assert_eq!(unsafe { path_from_c(c.path, c.path_len) }, *want.path);
                    assert_eq!(
                        unsafe { figures_from_c(c.figures, c.figures_len) },
                        want.aggregates
                    );
                }
            }
            LayerOp::BadAnswer => {
                let answer = BinnacleGroupAnswer {
                    body: 9,
                    ..unsafe { std::mem::zeroed() }
                };
                assert_eq!(unsafe { binnacle_group_layer_answer(h, &answer) }, 0);
            }
        }
        // What every op leaves to compare: the applied specs and their
        // notices, the counts and the holdings.
        loop {
            let mut c = BinnacleGroupViewed::default();
            let polled = unsafe { binnacle_group_layer_poll_viewed(h, &mut c) };
            let want = layer.poll_viewed();
            assert_eq!(polled, i32::from(want.is_some()), "viewed after {op:?}");
            let Some(want) = want else { break };
            assert_eq!(unsafe { view_from_c(&c.applied) }, want.applied);
            let total = |t: BinnacleTotal| (t.known != 0).then_some(t.count);
            assert_eq!(
                (total(c.totals.axes[0]), total(c.totals.axes[1])),
                (want.totals.rows, want.totals.cols)
            );
            let notices: Vec<(Axis, ChangeNotice)> = unsafe { c_slice(c.notices, c.notices_len) }
                .iter()
                .map(|n| {
                    let axis = if n.axis == ROWS {
                        Axis::Rows
                    } else {
                        Axis::Cols
                    };
                    let (at, count) = (n.at, n.n);
                    let notice = match n.notice {
                        BINNACLE_NOTICE_RESET => ChangeNotice::Reset(count),
                        BINNACLE_NOTICE_REORDERED => ChangeNotice::Reordered,
                        BINNACLE_NOTICE_INSERTED => ChangeNotice::Inserted { at, n: count },
                        BINNACLE_NOTICE_REMOVED => ChangeNotice::Removed { at, n: count },
                        BINNACLE_NOTICE_CONTENT_CHANGED => {
                            ChangeNotice::ContentChanged(at..at + count)
                        }
                        BINNACLE_NOTICE_REFILTERED => ChangeNotice::Refiltered(count),
                        BINNACLE_NOTICE_COLLAPSED => ChangeNotice::Collapsed { at, n: count },
                        BINNACLE_NOTICE_EXPANDED => ChangeNotice::Expanded { at, n: count },
                        k => panic!("notice {k}"),
                    };
                    (axis, notice)
                })
                .collect();
            assert_eq!(notices, want.notices);
        }
        for axis in Axis::ALL {
            let mut n = 99;
            let got = unsafe { binnacle_group_layer_count(h, tag(axis), &mut n) };
            assert_eq!((got == 1).then_some(n), layer.count(axis), "{axis:?} count");
            let grouped = unsafe { binnacle_group_layer_grouped(h, tag(axis)) };
            assert_eq!(grouped, i32::from(layer.grouped(axis)));
            // The first Summary Keys the layer issues, and Leaf Keys.
            let keys = (0..12)
                .map(|n| n | binnacle_group::SUMMARY_KEY_BIT)
                .chain(0..3);
            for key in keys {
                let fits = unsafe { binnacle_group_layer_fits(h, tag(axis), key) };
                assert_eq!(
                    fits,
                    i32::from(layer.fits(axis, Key(key))),
                    "{axis:?} fits {key}"
                );
            }
        }
        assert_eq!(unsafe { binnacle_group_layer_held(h) }, layer.held());
    }
    unsafe { binnacle_group_layer_free(h) };
}

/// Rows grouped in header form, with tiers set and a row pinned, give the
/// same tiers, spans, header rects and focused tier through the C API.
#[test]
fn header_form_through_the_c_api_mirrors_the_grid() {
    let (rows, cols) = (
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(12.0),
    );
    let c_config = |a: AxisConfig| BinnacleAxisConfig {
        count: a.count,
        default_size: a.default_size,
        header: a.header,
        gap: a.gap,
    };
    let h = binnacle_grid_new(c_config(rows), c_config(cols));
    let mut grid = Grid::new(rows, cols);
    let (north, aberdeen, bergen) = (Key(100), Key(110), Key(111));
    let mut grouping = Grouping::rows_by(Key(1));
    grouping.row_levels = vec![Level::field(Key(1)), Level::field(Key(2))];
    grouping.rows.form = Form::Header;
    let spec = ViewSpec::default().with_grouping(Some(grouping));
    let outlines = [
        Outline::leaf(vec![north, aberdeen]),
        Outline::leaf(vec![north, aberdeen]),
        Outline::subtotal(
            vec![north, aberdeen],
            Key(2),
            Value::Text("Aberdeen".into()),
        ),
        Outline::leaf(vec![north, bergen]),
        Outline::subtotal(vec![north, bergen], Key(2), Value::Text("Bergen".into())),
        Outline::subtotal(vec![north], Key(1), Value::Text("North".into())),
    ];
    let sizes = [4.0f32, 6.0];
    grid.view_applied(spec.clone(), Totals::unknown());
    grid.set_tiers(Axis::Rows, &sizes);
    unsafe {
        with_c_view(&spec, |c| {
            assert_eq!(
                binnacle_grid_view_applied(h, c, &BinnacleTotals::default()),
                1
            )
        });
        assert_eq!(
            binnacle_grid_set_tiers(h, ROWS, sizes.as_ptr(), sizes.len()),
            1
        );
    }
    let frame = |h, grid: &mut Grid| {
        let mut requests = BinnacleWindowRequests::default();
        assert_eq!(
            unsafe { binnacle_grid_begin_frame(h, 30.0, 13.0, &mut requests) },
            1
        );
        for (axis, range) in grid.begin_frame(Viewport::new(30.0, 13.0)) {
            let k: Vec<u64> = range.clone().collect();
            unsafe { binnacle_grid_set_window(h, tag(axis), range.start, k.as_ptr(), k.len()) };
            let keys: Vec<Key> = k.into_iter().map(Key).collect();
            grid.set_window(axis, range.start, &keys);
            if axis == Axis::Rows {
                let part = &outlines[range.start as usize..range.end as usize];
                grid.set_outlines(axis, range.start, part);
                with_c_outlines(part, |c| unsafe {
                    binnacle_grid_set_outlines(h, ROWS, range.start, c.as_ptr(), c.len())
                });
            }
        }
        let mut snap = unsafe { std::mem::zeroed::<BinnacleSnapshot>() };
        assert_eq!(unsafe { binnacle_grid_snapshot(h, &mut snap) }, 1);
        let rust = grid.snapshot();
        unsafe { check_snapshot(&snap, &rust) };
        (snap.axes[ROWS as usize].spans_len, snap.focused_tier)
    };
    assert_eq!(frame(h, &mut grid), (3, 0), "North, Aberdeen and Bergen");
    grid.pin(Axis::Rows, Edge::Leading, Key(0));
    unsafe { binnacle_grid_pin(h, ROWS, BINNACLE_EDGE_LEADING, 0) };
    frame(h, &mut grid);
    for e in [PointerEvent::down(13.0, 2.5), PointerEvent::up(13.0, 2.5)] {
        grid.pointer(e);
        assert_eq!(unsafe { binnacle_grid_pointer(h, pointer_event(e)) }, 1);
    }
    let left = Action::MoveFocus {
        axis: Axis::Cols,
        by: FocusStep::Item,
        forward: false,
        extend: false,
    };
    assert_eq!(unsafe { send(h, &left) }, 1);
    grid.action(left);
    let (spans, tier) = frame(h, &mut grid);
    assert!(spans > 3, "the pinned edge breaks spans");
    assert_eq!(tier, 1, "Left from the body enters the innermost level");
    grid.set_form(Axis::Rows, Form::Outline);
    assert_eq!(
        unsafe { binnacle_grid_set_form(h, ROWS, BINNACLE_FORM_OUTLINE) },
        1
    );
    unsafe { check_effects(h, &mut grid) };
    unsafe { binnacle_grid_free(h) };
}

#[test]
fn the_values_pseudo_levels_field_is_the_cores() {
    assert_eq!(binnacle_ffi::BINNACLE_VALUES_FIELD, VALUES_FIELD.0);
}

/// Columns grouped in header form: focus entering their tiers, and Left
/// there closing a column group, give the same band focus, tier and View
/// Edit through the C API.
#[test]
fn the_column_tiers_through_the_c_api_mirror_the_grid() {
    let (rows, cols) = (
        AxisConfig::new(3, 1.0).header(2.0),
        AxisConfig::new(3, 5.0).header(4.0),
    );
    let c_config = |a: AxisConfig| BinnacleAxisConfig {
        count: a.count,
        default_size: a.default_size,
        header: a.header,
        gap: a.gap,
    };
    let h = binnacle_grid_new(c_config(rows), c_config(cols));
    let mut grid = Grid::new(rows, cols);
    let (q1, q2) = (Key(200), Key(201));
    let mut grouping = Grouping::default();
    grouping.col_levels = vec![Level::field(Key(3))];
    let spec = ViewSpec::default().with_grouping(Some(grouping));
    let outlines = [
        Outline::leaf(vec![q1]),
        Outline::leaf(vec![q1]),
        Outline::leaf(vec![q2]),
    ];
    let sizes = [1.0f32];
    grid.view_applied(spec.clone(), Totals::unknown());
    grid.set_tiers(Axis::Cols, &sizes);
    unsafe {
        with_c_view(&spec, |c| {
            assert_eq!(
                binnacle_grid_view_applied(h, c, &BinnacleTotals::default()),
                1
            )
        });
        assert_eq!(
            binnacle_grid_set_tiers(h, COLS, sizes.as_ptr(), sizes.len()),
            1
        );
    }
    let frame = |h, grid: &mut Grid| {
        let mut requests = BinnacleWindowRequests::default();
        assert_eq!(
            unsafe { binnacle_grid_begin_frame(h, 30.0, 8.0, &mut requests) },
            1
        );
        for (axis, range) in grid.begin_frame(Viewport::new(30.0, 8.0)) {
            let k: Vec<u64> = range.clone().collect();
            unsafe { binnacle_grid_set_window(h, tag(axis), range.start, k.as_ptr(), k.len()) };
            let keys: Vec<Key> = k.into_iter().map(Key).collect();
            grid.set_window(axis, range.start, &keys);
            if axis == Axis::Cols {
                let part = &outlines[range.start as usize..range.end as usize];
                grid.set_outlines(axis, range.start, part);
                with_c_outlines(part, |c| unsafe {
                    binnacle_grid_set_outlines(h, COLS, range.start, c.as_ptr(), c.len())
                });
            }
        }
        let mut snap = unsafe { std::mem::zeroed::<BinnacleSnapshot>() };
        assert_eq!(unsafe { binnacle_grid_snapshot(h, &mut snap) }, 1);
        let rust = grid.snapshot();
        unsafe { check_snapshot(&snap, &rust) };
        snap.axes[COLS as usize].band_focused
    };
    frame(h, &mut grid);
    for e in [PointerEvent::down(11.5, 2.5), PointerEvent::up(11.5, 2.5)] {
        grid.pointer(e);
        assert_eq!(unsafe { binnacle_grid_pointer(h, pointer_event(e)) }, 1);
    }
    assert_eq!(frame(h, &mut grid), 0);
    let up = Action::MoveFocus {
        axis: Axis::Rows,
        by: FocusStep::Item,
        forward: false,
        extend: false,
    };
    let left = Action::MoveFocus {
        axis: Axis::Cols,
        by: FocusStep::Item,
        forward: false,
        extend: false,
    };
    for action in [up, left] {
        assert_eq!(unsafe { send(h, &action) }, 1);
        grid.action(action);
        assert_eq!(frame(h, &mut grid), 1, "in the column tiers");
    }
    unsafe { check_effects(h, &mut grid) };
    unsafe { binnacle_grid_free(h) };
}
