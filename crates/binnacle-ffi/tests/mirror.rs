//! The C API mirrors the Rust Grid: a handle and a Grid driven by the same
//! inputs request the same Windows, describe the same Snapshot and drain the
//! same Effects. Tags are encoded from the header's constants, which are the
//! independent source here; inputs with unknown tags must change nothing.

use std::slice;

use binnacle_core::{
    Action, Axis, AxisConfig, Button, ChangeNotice, Condition, ConditionKind, ConditionState,
    Effect, FilterSpec, FocusStep, Grid, HeaderClick, Key, Located, Mode, Modifiers, PointerEvent,
    PointerKind, Searched, SelectionUnit, Snapshot, SortColumn, SortDirection, SortSpec, Support,
    TextQuery, TextSupport, Totals, Value, ViewEdit, ViewSpec, Viewport,
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
    Frame(f32, f32),
    Pointer(PointerEvent),
    /// A primary press, then a move away (no release) or the release.
    Gesture([PointerEvent; 2]),
    Action(Action),
    SetSize(Axis, u64, f32),
    SetHeader(Axis, f32),
    /// A press `inset` inside the end of the band leading the axis (where its
    /// edge is, given a Gap), halfway across the other band as of the last
    /// frame, with this click count; then a move by `travel` along the axis.
    BandDrag {
        axis: Axis,
        inset: f32,
        clicks: u32,
        travel: f32,
    },
    Unit(SelectionUnit),
    HeaderClick(Axis, HeaderClick),
    Threshold(f32),
    ViewApplied(ViewSpec, Totals),
    RequestView(ViewSpec),
    SetQuickFilter(Axis, Option<TextQuery>),
    QuickFilterSupport(Axis, Option<TextSupport>),
    SetSupport(Axis, Key, Support),
    AddCondition(Axis, Condition),
    RemoveCondition(Axis, Condition),
    ReplaceCondition(Axis, Condition, Condition),
    /// A `located` answer about this Key, or (`None`) about the Key of the
    /// last `Locate` drained, if any.
    Located(Option<(Axis, Key)>, Located),
    Notify(Axis, ChangeNotice),
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
    match tc.draw(gs::integers::<u8>().max_value(7)) {
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
        3 => Action::SelectColumn,
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

/// A Text or OneOf condition on `axis`, read in a Key of the other axis,
/// maybe with `not`; sometimes with no text or no values.
#[hegel::composite]
fn condition(tc: &TestCase, axis: Axis) -> Condition {
    let key = tc.draw(key(axis.other()));
    let condition = if tc.draw(gs::booleans()) {
        Condition::text(key, tc.draw(text_query(axis)))
    } else {
        Condition::one_of(key, tc.draw(gs::vecs(value()).max_size(3)))
    };
    condition.not(tc.draw(gs::booleans()))
}

/// A View Spec: usually sorting and filtering only the rows.
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
    match tc.draw(gs::integers::<u8>().max_value(9)) {
        0 => Op::ViewApplied(tc.draw(view_spec()), tc.draw(totals())),
        7 => Op::AddCondition(axis, tc.draw(condition(axis))),
        // Often one the latest request has, so it goes.
        8 => Op::RemoveCondition(axis, tc.draw(condition(axis))),
        9 => Op::ReplaceCondition(axis, tc.draw(condition(axis)), tc.draw(condition(axis))),
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
                    .with_one_of(tc.draw(gs::booleans())),
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
            let notice = match tc.draw(gs::integers::<u8>().max_value(5)) {
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
    match tc.draw(gs::integers::<u8>().max_value(17)) {
        16 => Op::SetHeader(
            tc.draw(axes()),
            tc.draw(hegel::one_of!(
                gs::sampled_from(vec![0.0f32, -1.0, f32::NAN, f32::INFINITY]),
                gs::floats::<f32>().min_value(0.0).max_value(8.0),
            )),
        ),
        17 => Op::BandDrag {
            axis: tc.draw(axes()),
            inset: tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)),
            clicks: tc.draw(gs::integers::<u32>().min_value(1).max_value(2)),
            travel: tc.draw(gs::floats::<f32>().min_value(-6.0).max_value(6.0)),
        },
        10 | 14 | 15 => tc.draw(answer()),
        0 | 1 | 11 => Op::Frame(tc.draw(len()), tc.draw(len())),
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
        7 => match tc.draw(gs::integers::<u8>().max_value(2)) {
            0 => Op::Unit(tc.draw(gs::sampled_from(vec![
                SelectionUnit::Cells,
                SelectionUnit::Rows,
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
        _ => Op::BadTag(tc.draw(gs::integers::<u32>().max_value(31))),
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
            Action::Cancel => binnacle_grid_cancel(h),
            Action::Scroll { dx, dy } => binnacle_grid_scroll(h, *dx, *dy),
            Action::ScrollTo { axis, fraction } => {
                binnacle_grid_scroll_to(h, tag(*axis), *fraction)
            }
            Action::ScrollBy { axis, items } => binnacle_grid_scroll_by(h, tag(*axis), *items),
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
            6 => binnacle_grid_set_selection_unit(h, 2),
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
            12 => binnacle_grid_notify(h, ROWS, 6, 0, 0),
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
            23 => binnacle_grid_set_support(h, ROWS, 5000, 16),
            24 => {
                let unknown = BinnacleCondition {
                    kind: 3,
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
        let expected = rust.strip(axis);
        assert_eq!(strip.len(), expected.len(), "{axis:?} strip length");
        for (got, want) in strip.iter().zip(expected) {
            assert_eq!(got.position, want.position);
            assert_eq!(got.has_key != 0, want.key.is_some());
            if let Some(Key(k)) = want.key {
                assert_eq!(got.key, k);
            }
            assert!(same_f32(got.offset, want.offset));
            assert!(same_f32(got.size, want.size));
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
                | u32::from(want.filter.pending) << 6;
            assert_eq!(got.flags, flags, "{want:?}");
            assert_eq!(got.sort_place as usize, place.unwrap_or(0));
            let support = want
                .key
                .map_or(0, |k| support_bits(rust.support(axis.other(), k)));
            assert_eq!(got.support, support, "{want:?} support");
        }
        assert_eq!(c.has_focus != 0, rust.focus(axis).is_some());
        if let Some(f) = rust.focus(axis) {
            assert_eq!(a.focus.position, f.position);
            assert_eq!(a.focus.has_key != 0, f.key.is_some());
            if let Some(Key(k)) = f.key {
                assert_eq!(a.focus.key, k);
            }
        }
        if let Some(r) = &range {
            assert_eq!(
                a.selection_range.start..a.selection_range.end,
                *r.span(axis)
            );
        }
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
                    _ => ConditionState::Removing,
                };
                (unsafe { condition_from_c(&l.condition) }, state)
            })
            .collect();
        assert_eq!(got, rust.conditions(axis), "{axis:?} conditions");
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
    let picked = unsafe { slice::from_raw_parts(c.picked, c.picked_len) };
    assert_eq!(
        picked,
        rust.picked().iter().map(|k| k.0).collect::<Vec<_>>()
    );
    let rects = unsafe { slice::from_raw_parts(c.selection_rects, c.selection_rects_len) };
    assert_eq!(rects.len(), rust.selection_rects().len());
    assert!(
        rects
            .iter()
            .zip(rust.selection_rects())
            .all(|(g, w)| rect(g, w))
    );
    let cells = unsafe { slice::from_raw_parts(c.cell_states, c.cell_states_len) };
    let (rows, cols) = (rust.rows(), rust.cols());
    assert_eq!(cells.len(), rows.len() * cols.len());
    for (r, row) in rows.iter().enumerate() {
        for (k, col) in cols.iter().enumerate() {
            let s = rust.cell_state(row, col);
            let want = u8::from(s.focused) | u8::from(s.selected) << 1;
            assert_eq!(cells[r * cols.len() + k], want, "cell {r},{k}");
        }
    }
    #[allow(unreachable_patterns)] // Modes added to the Grid later read as OTHER.
    let mode = match rust.mode() {
        Mode::Idle => 1,
        Mode::DraggingSelection => 2,
        Mode::ResizingBand { .. } => 3,
        _ => 0,
    };
    assert_eq!(c.mode, mode);
    assert_eq!(c.capturing != 0, rust.capturing());
    match rust.hovered_header() {
        None => assert_eq!(c.has_hovered_header, 0),
        Some((axis, item)) => {
            assert_eq!(c.has_hovered_header, 1);
            assert_eq!(c.hovered_axis, tag(axis));
            assert_eq!(rust.strip(axis)[c.hovered_index], *item);
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
        let mut size = 0.0;
        let (kind, row, col, axis, key) = match &want.effect {
            Effect::TextTyped { text } => {
                let got = unsafe { slice::from_raw_parts(c.text.data, c.text.len) };
                assert_eq!(got, text.as_bytes());
                (1, 0, 0, 0, 0)
            }
            Effect::FocusChanged { row, col } => (2, row.0, col.0, 0, 0),
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
                    },
                    ViewEdit::SetQuickFilter { axis, .. } => on(2, axis),
                    ViewEdit::Replace => on(3, Axis::Rows),
                    ViewEdit::ShownChanged { axis } => on(4, axis),
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
            #[allow(unreachable_patterns)] // Effects added to the Grid later are OTHER.
            _ => (0, 0, 0, 0, 0),
        };
        assert_eq!(
            (c.kind, c.row, c.col, c.axis, c.key),
            (kind, row, col, axis, key)
        );
        assert!(same_f32(c.size, size), "size {} for {want:?}", c.size);
        assert_eq!(c.edit, edit);
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
        }
    }
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
    (u32::from(support.sort) * BINNACLE_SUPPORT_SORT)
        | text
        | (u32::from(support.one_of) * BINNACLE_SUPPORT_ONE_OF)
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
                .map(|v| match v.blank {
                    0 => Value::Text(unsafe { text_from_c(v.text) }),
                    1 => Value::Blank,
                    b => panic!("blank is {b}"),
                })
                .collect();
            Condition::one_of(Key(c.key), values)
        }
        kind => panic!("condition kind {kind}"),
    };
    condition.not(c.negated != 0)
}

/// The text `t` points at, as a host would read it.
unsafe fn text_from_c(t: BinnacleText) -> String {
    let bytes = match t.len {
        0 => &[][..],
        n => unsafe { slice::from_raw_parts(t.data, n) },
    };
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// The values of a OneOf `condition` in C form, borrowing their texts; none
/// for other kinds.
fn values_to_c(condition: &Condition) -> Vec<BinnacleValue> {
    let ConditionKind::OneOf(values) = &condition.kind else {
        return Vec::new();
    };
    (values.iter())
        .map(|v| match v {
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
        })
        .collect()
}

/// `condition` in C form, borrowing its text, `keys` and `values` (from
/// [`values_to_c`]).
fn condition_to_c(
    condition: &Condition,
    keys: &[u64],
    values: &[BinnacleValue],
) -> BinnacleCondition {
    let (kind, text) = match &condition.kind {
        ConditionKind::Text(query) => (BINNACLE_CONDITION_TEXT, query_to_c(query, keys)),
        ConditionKind::OneOf(_) => (BINNACLE_CONDITION_ONE_OF, BinnacleTextQuery::default()),
        _ => unreachable!("only Text and OneOf conditions are drawn"),
    };
    BinnacleCondition {
        key: condition.key.0,
        kind,
        negated: u32::from(condition.not),
        text,
        values: values.as_ptr(),
        values_len: values.len(),
    }
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
    let mut c = BinnacleViewSpec::default();
    for axis in Axis::ALL {
        let sort = &sorts[axis.index()];
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
    let first = Op::Frame(tc.draw(len()), tc.draw(len()));
    let mut asked = None;
    // Each band's thickness as of the last frame.
    let mut bands = [0.0f32; 2];
    let ops = [first]
        .into_iter()
        .chain(tc.draw(gs::vecs(op()).min_size(5).max_size(60)));
    for op in ops {
        unsafe {
            let ok = match op {
                Op::Frame(width, height) => {
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
                        }
                    }
                    let mut snap = std::mem::zeroed::<BinnacleSnapshot>();
                    assert_eq!(binnacle_grid_snapshot(h, &mut snap), 1);
                    let rust = grid.snapshot();
                    check_snapshot(&snap, &rust);
                    bands = Axis::ALL.map(|a| rust.header(a));
                    1
                }
                Op::Pointer(e) => {
                    grid.pointer(e);
                    binnacle_grid_pointer(h, pointer_event(e))
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
                Op::Unit(unit) => {
                    grid.set_selection_unit(unit);
                    binnacle_grid_set_selection_unit(
                        h,
                        if unit == SelectionUnit::Rows { 1 } else { 0 },
                    )
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
                Op::RequestView(spec) => {
                    let ok = with_c_view(&spec, |c| binnacle_grid_request_view(h, c));
                    grid.request_view(spec);
                    ok
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
                    };
                    grid.notify(axis, notice);
                    binnacle_grid_notify(h, tag(axis), kind, at, n)
                }
                Op::BadText(bytes) => {
                    assert_eq!(binnacle_grid_type_text(h, bytes.as_ptr(), bytes.len()), 0);
                    1
                }
                Op::BadTag(which) => {
                    assert_eq!(send_bad_tag(h, which), 0, "unknown tag {which} accepted");
                    1
                }
            };
            assert_eq!(ok, 1);
            asked = check_effects(h, &mut grid).or(asked);
        }
    }
    unsafe { binnacle_grid_free(h) };
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
