//! Test-only exports for the agreement tests in `web/`: a dump of a Snapshot
//! built straight from the core's accessors (not from the flat encoding), in
//! the shape `web/src/snapshot.ts` decodes to, and one of every Effect.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Compared, Comparison, Condition, ConditionState,
    Cursor, DropTarget, Edge, Effect, FilterSpec, FindDirection, FindMatch, FindQuery, FocusStep,
    Grid, HeaderPlacement, Key, Mode, Modifiers, PointerEvent, Rect, Scalar, Searched, Snapshot,
    SortColumn, SortSpec, Stamped, Support, TextQuery, TextSupport, Totals, Value, ViewEdit,
    ViewSpec, Viewport,
};
use serde::Serialize;

use crate::effect::{
    WireAxis, WireCondition, WireDirection, WireFindMatch, WireFindQuery, WireStamped,
    WireViewEdit, WireViewSpec,
};
use crate::snapshot::cell_bits;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dump {
    generation: f64,
    damage: DumpDamage,
    rows: DumpAxis,
    cols: DumpAxis,
    focus_rect: Option<DumpRect>,
    editor_rect: Option<DumpRect>,
    mode: DumpMode,
    capturing: bool,
    cursor: &'static str,
    resize_guide: Option<DumpGuide>,
    drop_indicator: Option<DumpDrop>,
    pin_zones: Vec<DumpZone>,
    hovered_header: Option<DumpHovered>,
    selection_range: Option<DumpRange>,
    selection_rects: Vec<DumpRect>,
    picked: DumpPicked,
    moving_keys: Vec<u64>,
    applied_view: WireViewSpec,
    requested_view: Option<WireViewSpec>,
    view_pending: bool,
    pending_move: Option<DumpPendingMove>,
    move_refusal: Option<DumpRefusal>,
    find: Option<WireFindQuery>,
    find_match: Option<WireFindMatch>,
    find_support: Option<DumpTextSupport>,
    cells: Vec<u8>,
}

#[derive(Serialize)]
struct DumpPendingMove {
    axis: WireAxis,
    keys: Vec<u64>,
}

#[derive(Serialize)]
struct DumpRefusal {
    axis: WireAxis,
    reason: String,
}

#[derive(Serialize)]
struct DumpPicked {
    rows: Vec<u64>,
    cols: Vec<u64>,
}

#[derive(Serialize)]
struct DumpDamage {
    layout: bool,
    selection: bool,
    positions: bool,
}

#[derive(Serialize)]
struct DumpRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl From<Rect> for DumpRect {
    fn from(r: Rect) -> Self {
        DumpRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }
}

#[derive(Serialize)]
struct DumpRegions {
    leading: DumpSpan,
    trailing: DumpSpan,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpAxis {
    count: f64,
    extent: f32,
    header: f32,
    gap: f32,
    anchor: DumpAnchor,
    thumb: DumpThumb,
    focus: Option<DumpFocus>,
    locating: bool,
    strip: Vec<DumpItem>,
    pinned: f64,
    trailing: f64,
    scrolling: DumpSpan,
    regions: DumpRegions,
    headers: Vec<DumpHeader>,
    total: Option<f64>,
    quick_filter: Option<DumpTextSupport>,
    conditions: Vec<DumpListed>,
    movable: bool,
    hide_support: bool,
    hide_cap: f64,
    pins: DumpPins,
}

#[derive(Serialize)]
struct DumpPins {
    leading: Vec<u64>,
    trailing: Vec<u64>,
}

#[derive(Serialize)]
struct DumpTextSupport {
    regex: bool,
}

#[derive(Serialize)]
struct DumpListed {
    condition: WireCondition,
    state: &'static str,
}

#[derive(Serialize)]
struct DumpSupport {
    sort: bool,
    text: Option<DumpTextSupport>,
    #[serde(rename = "oneOf")]
    one_of: bool,
    compare: Option<&'static str>,
}

impl From<Support> for DumpSupport {
    fn from(s: Support) -> Self {
        DumpSupport {
            sort: s.sort,
            text: s.text.map(|t| DumpTextSupport { regex: t.regex }),
            one_of: s.one_of,
            compare: s.compare.map(|c| match c {
                Compared::Numbers => "numbers",
                Compared::Dates => "dates",
            }),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DumpHeaderFilter {
    filtered: bool,
    pending: bool,
    not_applied: bool,
}

#[derive(Serialize)]
struct DumpAnchor {
    position: f64,
    offset: f32,
}

#[derive(Serialize)]
struct DumpThumb {
    fraction: f64,
    size: f64,
}

#[derive(Serialize)]
struct DumpFocus {
    position: f64,
    key: Option<u64>,
}

#[derive(Serialize)]
struct DumpItem {
    position: f64,
    key: Option<u64>,
    offset: f32,
    size: f32,
}

#[derive(Serialize)]
struct DumpHeader {
    position: f64,
    key: Option<u64>,
    rect: DumpRect,
    sort: DumpHeaderSort,
    selected: bool,
    hovered: bool,
    filter: DumpHeaderFilter,
    /// The support of the item's Key, or none (all off) for an unfilled one.
    support: DumpSupport,
    #[serde(rename = "movePending")]
    move_pending: bool,
}

#[derive(Serialize)]
struct DumpHeaderSort {
    applied: Option<DumpApplied>,
    pending: bool,
}

#[derive(Serialize)]
struct DumpApplied {
    direction: WireDirection,
    index: f64,
}

impl DumpHeader {
    fn new(h: HeaderPlacement, support: Support) -> Self {
        DumpHeader {
            position: h.position as f64,
            key: h.key.map(|k| k.0),
            rect: h.rect.into(),
            sort: DumpHeaderSort {
                applied: h.sort.applied.map(|(direction, index)| DumpApplied {
                    direction: direction.into(),
                    index: index as f64,
                }),
                pending: h.sort.pending,
            },
            selected: h.selected,
            hovered: h.hovered,
            filter: DumpHeaderFilter {
                filtered: h.filter.filtered,
                pending: h.filter.pending,
                not_applied: h.filter.not_applied,
            },
            support: support.into(),
            move_pending: h.move_pending,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
enum DumpMode {
    Idle,
    DraggingSelection,
    Resizing {
        axis: WireAxis,
        key: u64,
        start_size: f32,
    },
    ResizingBand {
        axis: WireAxis,
        start_thickness: f32,
    },
    MovingHeaders {
        axis: WireAxis,
        key: u64,
        target: DumpTarget,
    },
    Editing {
        row: u64,
        col: u64,
    },
}

impl From<Mode> for DumpMode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Idle => DumpMode::Idle,
            Mode::DraggingSelection => DumpMode::DraggingSelection,
            Mode::Resizing {
                axis,
                key,
                start_size,
            } => DumpMode::Resizing {
                axis: axis.into(),
                key: key.0,
                start_size,
            },
            Mode::ResizingBand {
                axis,
                start_thickness,
            } => DumpMode::ResizingBand {
                axis: axis.into(),
                start_thickness,
            },
            Mode::MovingHeaders { axis, key, target } => DumpMode::MovingHeaders {
                axis: axis.into(),
                key: key.0,
                target: target.into(),
            },
            Mode::Editing { row, col } => DumpMode::Editing {
                row: row.0,
                col: col.0,
            },
        }
    }
}

#[derive(Serialize)]
struct DumpGuide {
    axis: WireAxis,
    key: Option<u64>,
    offset: f32,
    size: f32,
}

#[derive(Serialize)]
struct DumpDrop {
    axis: WireAxis,
    target: DumpTarget,
    offset: f32,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum DumpTarget {
    Move {
        to: f64,
        unpin: bool,
    },
    Unpin {
        at: f64,
    },
    Pin {
        edge: &'static str,
        before: Option<u64>,
        reorder: bool,
    },
    Stay,
}

fn edge_name(edge: Edge) -> &'static str {
    match edge {
        Edge::Leading => "leading",
        Edge::Trailing => "trailing",
    }
}

impl From<DropTarget> for DumpTarget {
    fn from(target: DropTarget) -> Self {
        match target {
            DropTarget::Move { to, unpin } => DumpTarget::Move {
                to: to as f64,
                unpin,
            },
            DropTarget::Unpin { at } => DumpTarget::Unpin { at: at as f64 },
            DropTarget::Pin {
                edge,
                before,
                reorder,
            } => DumpTarget::Pin {
                edge: edge_name(edge),
                before: before.map(|k| k.0),
                reorder,
            },
            DropTarget::Stay => DumpTarget::Stay,
        }
    }
}

#[derive(Serialize)]
struct DumpZone {
    axis: WireAxis,
    edge: &'static str,
    offset: f32,
    size: f32,
    hot: bool,
}

#[derive(Serialize)]
struct DumpHovered {
    axis: WireAxis,
    position: f64,
}

#[derive(Serialize)]
struct DumpSpan {
    start: f64,
    end: f64,
}

#[derive(Serialize)]
struct DumpRange {
    rows: Vec<DumpSpan>,
    cols: Vec<DumpSpan>,
}

fn dump_axis(s: &Snapshot<'_>, axis: Axis) -> DumpAxis {
    let anchor = s.scroll_anchor(axis);
    let thumb = s.thumb(axis);
    DumpAxis {
        count: s.count(axis) as f64,
        extent: s.extent(axis),
        header: s.header(axis),
        gap: s.gap(axis),
        anchor: DumpAnchor {
            position: anchor.position as f64,
            offset: anchor.offset,
        },
        thumb: DumpThumb {
            fraction: thumb.fraction,
            size: thumb.size,
        },
        focus: s.focus(axis).map(|f| DumpFocus {
            position: f.position as f64,
            key: f.key.map(|k| k.0),
        }),
        locating: s.locating(axis),
        total: s.total(axis).map(|t| t as f64),
        quick_filter: s
            .quick_filter(axis)
            .map(|q| DumpTextSupport { regex: q.regex }),
        strip: s
            .drawn(axis)
            .map(|i| DumpItem {
                position: i.position as f64,
                key: i.key.map(|k| k.0),
                offset: i.offset,
                size: i.size,
            })
            .collect(),
        pinned: s.pinned(axis, Edge::Leading).len() as f64,
        trailing: s.pinned(axis, Edge::Trailing).len() as f64,
        scrolling: DumpSpan {
            start: s.scrolling(axis).start.into(),
            end: s.scrolling(axis).end.into(),
        },
        regions: {
            let [leading, trailing] = Edge::ALL.map(|edge| {
                let r = s.pinned_region(axis, edge);
                DumpSpan {
                    start: r.start.into(),
                    end: r.end.into(),
                }
            });
            DumpRegions { leading, trailing }
        },
        headers: s
            .headers(axis)
            .map(|h| {
                let none = Support::default().with_sort(false).with_text(None);
                let support = h.key.map_or(none, |k| s.support(axis.other(), k));
                DumpHeader::new(h, support)
            })
            .collect(),
        conditions: s
            .conditions(axis)
            .iter()
            .map(|(c, state)| DumpListed {
                condition: c.into(),
                state: match state {
                    ConditionState::Applied => "applied",
                    ConditionState::Adding => "adding",
                    ConditionState::Removing => "removing",
                    ConditionState::NotApplied => "notApplied",
                },
            })
            .collect(),
        movable: s.movable(axis),
        hide_support: s.hide_support(axis),
        hide_cap: s.hide_cap(axis) as f64,
        pins: DumpPins {
            leading: s.pins(axis, Edge::Leading).map(|k| k.0).collect(),
            trailing: s.pins(axis, Edge::Trailing).map(|k| k.0).collect(),
        },
    }
}

/// The Snapshot `s` in the shape the TypeScript decoder produces.
pub fn dump(s: &Snapshot<'_>) -> Dump {
    let keys = |ks: &[Key]| ks.iter().map(|k| k.0).collect();
    let mut cells = Vec::new();
    for row in s.drawn(Axis::Rows) {
        for col in s.drawn(Axis::Cols) {
            cells.push(cell_bits(s.cell_state(row, col)));
        }
    }
    Dump {
        generation: s.generation() as f64,
        damage: DumpDamage {
            layout: s.damage().layout,
            selection: s.damage().selection,
            positions: s.damage().positions,
        },
        rows: dump_axis(s, Axis::Rows),
        cols: dump_axis(s, Axis::Cols),
        focus_rect: s.focus_rect().map(DumpRect::from),
        editor_rect: s.editor_rect().map(DumpRect::from),
        mode: s.mode().into(),
        capturing: s.capturing(),
        cursor: match s.cursor() {
            Cursor::Default => "default",
            Cursor::ResizeCol => "resizeCol",
            Cursor::ResizeRow => "resizeRow",
            Cursor::Move => "move",
        },
        resize_guide: s.resize_guide().map(|g| DumpGuide {
            axis: g.axis.into(),
            key: g.key.map(|k| k.0),
            offset: g.offset,
            size: g.size,
        }),
        drop_indicator: s.drop_indicator().map(|d| DumpDrop {
            axis: d.axis.into(),
            target: d.target.into(),
            offset: d.offset,
        }),
        pin_zones: (s.pin_zones().into_iter())
            .map(|z| DumpZone {
                axis: z.axis.into(),
                edge: edge_name(z.edge),
                offset: z.offset,
                size: z.size,
                hot: z.hot,
            })
            .collect(),
        hovered_header: s.hovered_header().map(|(axis, item)| DumpHovered {
            axis: axis.into(),
            position: item.position as f64,
        }),
        selection_range: s.selection_range().map(|r| {
            let runs = |runs: &[std::ops::Range<u64>]| {
                (runs.iter())
                    .map(|r| DumpSpan {
                        start: r.start as f64,
                        end: r.end as f64,
                    })
                    .collect()
            };
            DumpRange {
                rows: runs(&r.rows),
                cols: runs(&r.cols),
            }
        }),
        selection_rects: s.selection_rects().iter().map(|r| (*r).into()).collect(),
        picked: DumpPicked {
            rows: keys(s.picked(Axis::Rows)),
            cols: keys(s.picked(Axis::Cols)),
        },
        moving_keys: keys(s.moving_keys()),
        applied_view: s.applied_view().into(),
        requested_view: s.requested_view().map(WireViewSpec::from),
        view_pending: s.view_pending(),
        pending_move: s.pending_move().map(|(axis, ks)| DumpPendingMove {
            axis: axis.into(),
            keys: keys(ks),
        }),
        move_refusal: s.move_refusal().map(|(axis, reason)| DumpRefusal {
            axis: axis.into(),
            reason: reason.to_string(),
        }),
        find: s.find().map(WireFindQuery::from),
        find_match: s.find_match().map(WireFindMatch::from),
        find_support: s.find_support().map(|f| DumpTextSupport { regex: f.regex }),
        cells,
    }
}

/// One of every Effect variant, with Keys past 2^53 where an Effect has them.
pub fn every_effect() -> Vec<WireStamped> {
    let (row, col) = (Key(u64::MAX), Key((1 << 53) + 1));
    let spec = ViewSpec::default()
        .with_sort(
            Axis::Rows,
            SortSpec::new(vec![
                SortColumn::ascending(col),
                SortColumn::descending(Key(7)),
            ]),
        )
        .with_sort(Axis::Cols, SortSpec::new(vec![SortColumn::descending(row)]));
    let conditions = vec![
        Condition::text(col, TextQuery::new("Ø🚢").case_sensitive(true)).not(true),
        Condition::text(Key(7), TextQuery::new("^a").regex(true)),
        Condition::one_of(
            Key(7),
            vec![
                Value::Text("Bér 🚢".into()),
                Value::Blank,
                Value::Text("".into()),
            ],
        )
        .not(true),
    ];
    let spec = spec.with_filter(
        Axis::Rows,
        FilterSpec::default()
            .with_quick(Some(
                TextQuery::new("Bér 🚢")
                    .regex(true)
                    .searching(Searched::Keys(vec![col])),
            ))
            .with_conditions(conditions.into()),
    );
    let edit = ViewEdit::CycleSort {
        axis: Axis::Rows,
        key: col,
        multi: true,
    };
    [
        Effect::TextTyped { text: "é".into() },
        Effect::Activated { row, col },
        Effect::EditRequested { row, col },
        Effect::EditCancelled { row, col },
        Effect::FocusChanged { row, col },
        Effect::SelectionChanged,
        Effect::ViewRequested { spec, edit },
        Effect::Locate {
            axis: Axis::Rows,
            key: row,
        },
        Effect::SizeChanged {
            axis: Axis::Cols,
            key: col,
            size: 96.5,
        },
        Effect::AutoSizeRequested {
            axis: Axis::Cols,
            key: col,
        },
        Effect::PinsChanged {
            axis: Axis::Cols,
            edge: Edge::Trailing,
            keys: vec![col, row],
        },
        Effect::BandSizeChanged {
            axis: Axis::Rows,
            thickness: 30.0,
        },
        Effect::BandAutoSizeRequested { axis: Axis::Cols },
        Effect::MoveRequested {
            axis: Axis::Cols,
            keys: vec![col, row],
            to: 3,
        },
        Effect::FindRequested {
            query: Some(FindQuery::new(TextQuery::new("Ø🚢").case_sensitive(true)).by(Axis::Cols)),
        },
        Effect::FindStepRequested {
            query: FindQuery::new(TextQuery::new("a").searching(Searched::Keys(vec![col]))),
            from: Some(FindMatch::new(row, col)),
            direction: FindDirection::Previous,
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(i, effect)| {
        Stamped {
            generation: i as u64,
            effect,
        }
        .into()
    })
    .collect()
}

/// One of every View Edit, as `ViewRequested` carries them.
pub fn every_view_edit() -> Vec<WireViewEdit> {
    let query = TextQuery::new("ber").case_sensitive(true);
    let col = Key((1 << 53) + 1);
    let not_ber = Condition::text(col, query.clone()).not(true);
    let regex = Condition::text(Key(2), TextQuery::new("^b").regex(true));
    let listed = Condition::one_of(col, vec![Value::Blank, Value::Text("Ø".into())]);
    [
        ViewEdit::CycleSort {
            axis: Axis::Rows,
            key: Key(u64::MAX),
            multi: false,
        },
        ViewEdit::SetQuickFilter {
            axis: Axis::Rows,
            query: Some(query),
        },
        ViewEdit::SetQuickFilter {
            axis: Axis::Cols,
            query: None,
        },
        ViewEdit::Replace,
        ViewEdit::ShownChanged { axis: Axis::Cols },
        ViewEdit::Reapply,
        ViewEdit::AddCondition {
            axis: Axis::Rows,
            condition: not_ber.clone(),
        },
        ViewEdit::RemoveCondition {
            axis: Axis::Rows,
            condition: regex.clone(),
        },
        ViewEdit::ReplaceCondition {
            axis: Axis::Rows,
            from: not_ber,
            to: regex,
        },
        ViewEdit::AddCondition {
            axis: Axis::Rows,
            condition: listed,
        },
        ViewEdit::AddCondition {
            axis: Axis::Cols,
            condition: Condition::compare(
                Key(4),
                Comparison::Between(Scalar::Date(-86_400_000), Scalar::date(2024, 3, 1)),
            ),
        },
        ViewEdit::ReplaceCondition {
            axis: Axis::Rows,
            from: Condition::compare(col, Comparison::NotEqual(Scalar::Number(0.5))),
            to: Condition::compare(col, Comparison::Less(Scalar::Number(-3.0))).not(true),
        },
        ViewEdit::Hide {
            axis: Axis::Cols,
            keys: vec![col, Key(u64::MAX)],
        },
        ViewEdit::Show {
            axis: Axis::Rows,
            keys: vec![Key(3)],
        },
    ]
    .into_iter()
    .map(WireViewEdit::from)
    .collect()
}

/// The scenarios [`scenario`] knows, for the agreement tests to loop over.
pub const SCENARIOS: [&str; 22] = [
    "plain",
    "busy",
    "resizing",
    "band",
    "moving",
    "movePending",
    "moveRefused",
    "editing",
    "locating",
    "scrolled",
    "dragging",
    "sorted",
    "filtered",
    "found",
    "rejected",
    "pinned",
    "hidden",
    "pinDrag",
    "unpinDrag",
    "clipped",
    "restored",
    "endGap",
];

/// Keys far past 2^53, so every Key in a scenario tests the `bigint` path.
const KEY_BASE: u64 = 1 << 60;

/// The viewport every scenario lays out in.
pub const VIEWPORT: Viewport = Viewport {
    width: 400.0,
    height: 200.0,
};

/// Fills every Window the frame asks for with Keys `KEY_BASE + Position`,
/// up to `limit` Positions on the rows.
fn fill(grid: &mut Grid, row_limit: u64) {
    for (axis, range) in grid.begin_frame(VIEWPORT) {
        let end = match axis {
            Axis::Rows => range.end.min(row_limit),
            Axis::Cols => range.end,
        };
        let keys: Vec<Key> = (range.start..end.max(range.start))
            .map(|p| Key(KEY_BASE + p))
            .collect();
        grid.set_window(axis, range.start, &keys);
    }
}

fn frame(grid: &mut Grid) {
    fill(grid, u64::MAX);
    grid.snapshot();
}

/// A Grid put into the state `name` names (see [`SCENARIOS`]), with the next
/// frame begun and its Windows filled but its Snapshot not yet taken. Rows
/// are 20 px with a Gap of 1 under a 24 px header band; columns 80 px with a
/// Gap of 1 after a 40 px row header band, so column p spans
/// `40 + 81p .. 120 + 81p`.
pub fn scenario(name: &str) -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(40, 20.0).header(24.0).gap(1.0),
        AxisConfig::new(6, 80.0).header(40.0).gap(1.0),
    );
    frame(&mut grid);
    let key = |p: u64| Key(KEY_BASE + p);
    let mut row_limit = u64::MAX;
    match name {
        "plain" => {}
        "busy" => {
            grid.set_focus(key(2), key(1));
            frame(&mut grid);
            for axis in Axis::ALL {
                grid.action(Action::MoveFocus {
                    axis,
                    by: FocusStep::Item,
                    forward: true,
                    extend: true,
                });
            }
            // A sort click on column 0's header, still unanswered.
            grid.pointer(PointerEvent::down(60.0, 10.0));
            grid.pointer(PointerEvent::up(60.0, 10.0));
            // Ctrl-click on row 5's header picks it.
            let y = 24.0 + 21.0 * 5.0 + 5.0;
            grid.pointer(PointerEvent::down(10.0, y).modifiers(Modifiers::CTRL));
            grid.pointer(PointerEvent::up(10.0, y).modifiers(Modifiers::CTRL));
            // Ctrl-click on column 2's header picks it.
            let x = 40.0 + 81.0 * 2.0 + 40.0;
            grid.pointer(PointerEvent::down(x, 10.0).modifiers(Modifiers::CTRL));
            grid.pointer(PointerEvent::up(x, 10.0).modifiers(Modifiers::CTRL));
            grid.pointer(PointerEvent::moved(220.0, 10.0));
            grid.set_size(Axis::Cols, key(3), 120.0);
        }
        "resizing" => {
            grid.pointer(PointerEvent::down(120.5, 10.0));
            grid.pointer(PointerEvent::moved(150.0, 10.0));
        }
        "band" => {
            grid.pointer(PointerEvent::down(39.5, 10.0));
            grid.pointer(PointerEvent::moved(60.0, 10.0));
        }
        "moving" => {
            // Column 2 over column 1's first half, clear of the pin zones.
            grid.set_movable(Axis::Cols, true);
            grid.pointer(PointerEvent::down(220.0, 10.0));
            grid.pointer(PointerEvent::moved(140.0, 10.0));
        }
        "pinDrag" => {
            // Columns 4 and 5 pinned leading; 5 dragged before 4, reordering
            // the pins, the empty trailing zone lit.
            grid.pin(Axis::Cols, Edge::Leading, key(4));
            grid.pin(Axis::Cols, Edge::Leading, key(5));
            frame(&mut grid);
            grid.pointer(PointerEvent::down(160.0, 10.0));
            grid.pointer(PointerEvent::moved(50.0, 10.0));
        }
        "unpinDrag" => {
            // Row 6 pinned to the bottom, dragged up into the body of rows
            // that can't move: it returns to its own Position.
            grid.pin(Axis::Rows, Edge::Trailing, key(6));
            frame(&mut grid);
            grid.pointer(PointerEvent::down(20.0, 190.0));
            grid.pointer(PointerEvent::moved(20.0, 80.0));
        }
        "movePending" | "moveRefused" => {
            // Row 1's header dropped below row 3, the source yet to answer.
            grid.set_movable(Axis::Rows, true);
            grid.pointer(PointerEvent::down(20.0, 24.0 + 21.0 + 10.0));
            grid.pointer(PointerEvent::moved(20.0, 24.0 + 21.0 * 3.0 + 15.0));
            grid.pointer(PointerEvent::up(20.0, 24.0 + 21.0 * 3.0 + 15.0));
            while grid.poll_effect().is_some() {}
            if name == "moveRefused" {
                grid.move_answered(Some("Can't move while sorted".into()));
            }
        }
        "editing" => {
            grid.set_focus(key(1), key(2));
            frame(&mut grid);
            grid.edit_started(key(1), key(2));
        }
        "locating" => {
            grid.set_focus(key(3), key(1));
            frame(&mut grid);
            grid.notify(Axis::Rows, ChangeNotice::Reordered);
            row_limit = 2;
        }
        "pinned" => {
            // Column 4 and row 3 pinned leading, column 1 and row 8
            // trailing, an Edge Gap on the rows, scrolled past, and a range
            // from row 5 of the pinned column 4 down two rows.
            grid.set_edge_gap(Axis::Rows, 6.0);
            grid.pin(Axis::Cols, Edge::Leading, key(4));
            grid.pin(Axis::Rows, Edge::Leading, key(3));
            grid.pin(Axis::Cols, Edge::Trailing, key(1));
            grid.pin(Axis::Rows, Edge::Trailing, key(8));
            frame(&mut grid);
            grid.action(Action::Scroll { dx: 90.0, dy: 50.0 });
            frame(&mut grid);
            grid.set_focus(key(5), key(4));
            frame(&mut grid);
            grid.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: true,
            });
        }
        "restored" => {
            // A saved layout put back: column 2 and one the source does not
            // have (dormant) pinned left, row 4 to the bottom, and sizes,
            // one for a column the source does not have.
            grid.set_pins(Axis::Cols, Edge::Leading, &[key(2), key(900)]);
            grid.set_pins(Axis::Rows, Edge::Trailing, &[key(4)]);
            grid.set_size(Axis::Cols, key(2), 44.0);
            grid.set_size(Axis::Cols, key(900), 61.0);
        }
        "clipped" => {
            // Four columns pinned left and one right, and rows 0 to 5
            // pinned to the bottom, all too long to leave what each axis
            // keeps to scroll; focus on column 3, clipped, slides it in.
            grid.set_min_scrolling(Axis::Cols, 100.0);
            grid.set_min_scrolling(Axis::Rows, 60.0);
            for k in 0..4 {
                grid.pin(Axis::Cols, Edge::Leading, key(k));
            }
            grid.pin(Axis::Cols, Edge::Trailing, key(5));
            for k in 0..6 {
                grid.pin(Axis::Rows, Edge::Trailing, key(k));
            }
            frame(&mut grid);
            grid.set_focus(key(2), key(3));
        }
        "scrolled" => {
            grid.action(Action::Scroll {
                dx: 130.0,
                dy: 333.0,
            });
        }
        "endGap" => {
            // End Gaps of 6 on the rows and 9 on the columns, scrolled to
            // the end of both, with row 4 pinned to the bottom.
            grid.set_end_gap(Axis::Rows, 6.0);
            grid.set_end_gap(Axis::Cols, 9.0);
            grid.pin(Axis::Rows, Edge::Trailing, key(4));
            frame(&mut grid);
            grid.action(Action::Scroll {
                dx: 1.0e4,
                dy: 1.0e4,
            });
            frame(&mut grid);
        }
        "dragging" => {
            // Pressed on row 1, column 0 and dragged to row 4, column 2.
            grid.pointer(PointerEvent::down(60.0, 24.0 + 21.0 + 5.0));
            grid.pointer(PointerEvent::moved(220.0, 24.0 + 21.0 * 4.0 + 5.0));
        }
        "sorted" => {
            // Applied: rows by column 2 descending, then column 0, and
            // columns by row 1; column 1 pending.
            grid.view_applied(
                ViewSpec::default()
                    .with_sort(
                        Axis::Rows,
                        SortSpec::new(vec![
                            SortColumn::descending(key(2)),
                            SortColumn::ascending(key(0)),
                        ]),
                    )
                    .with_sort(
                        Axis::Cols,
                        SortSpec::new(vec![SortColumn::ascending(key(1))]),
                    ),
                Totals::unknown(),
            );
            grid.pointer(PointerEvent::down(150.0, 10.0).modifiers(Modifiers::SHIFT));
            grid.pointer(PointerEvent::up(150.0, 10.0).modifiers(Modifiers::SHIFT));
        }
        "filtered" => {
            // The rows' quick filter applied with a total of 900 rows, and a
            // new one (with a surrogate pair) requested. Column 1 has a
            // condition applied and another being added; column 2 one being
            // removed, and a OneOf with a blank being added. Column 0 cannot
            // be filtered, column 1 takes regexes, column 2 lists values.
            grid.set_quick_filter_support(Axis::Rows, Some(TextSupport::new(true)));
            grid.set_support(Axis::Rows, key(0), Support::default().with_text(None));
            let regex = Some(TextSupport::new(true));
            grid.set_support(Axis::Rows, key(1), Support::default().with_text(regex));
            grid.set_support(Axis::Rows, key(2), Support::default().with_one_of(true));
            let dates = Support::default().with_compare(Some(Compared::Dates));
            grid.set_support(Axis::Rows, key(3), dates);
            let numbers = Support::default().with_compare(Some(Compared::Numbers));
            grid.set_support(Axis::Rows, key(4), numbers);
            let applied = TextQuery::new("ber").searching(Searched::Keys(vec![key(0), key(2)]));
            let kept = Condition::text(key(1), TextQuery::new("Ø"));
            let dropped = Condition::text(key(2), TextQuery::new("x")).not(true);
            let filter = FilterSpec::default()
                .with_quick(Some(applied))
                .with_conditions(vec![kept.clone(), dropped.clone()].into());
            grid.view_applied(
                ViewSpec::default().with_filter(Axis::Rows, filter),
                Totals::unknown().with(Axis::Rows, Some(900)),
            );
            let requested = TextQuery::new("Bé🚢").case_sensitive(true).regex(true);
            grid.set_quick_filter(Axis::Rows, Some(requested));
            grid.add_condition(
                Axis::Rows,
                Condition::text(key(1), TextQuery::new("^b").regex(true)),
            );
            grid.remove_condition(Axis::Rows, &dropped);
            let values = vec![Value::Text("Bé🚢".into()), Value::Blank];
            grid.add_condition(Axis::Rows, Condition::one_of(key(2), values).not(true));
            // Column 3 between two dates, one past midnight; column 4 at
            // least a fraction.
            let range = Comparison::Between(Scalar::date(2024, 3, 1), Scalar::Date(-1_500));
            grid.add_condition(Axis::Rows, Condition::compare(key(3), range));
            let at_least = Comparison::AtLeast(Scalar::Number(-2.5));
            grid.add_condition(Axis::Rows, Condition::compare(key(4), at_least).not(true));
        }
        "found" => {
            // Find "Bé🚢" by columns, regexes supported, stepped to row 3,
            // column 1 with rows 1 to 2 of column 0 selected.
            grid.set_focus(key(1), key(0));
            frame(&mut grid);
            grid.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: true,
            });
            grid.set_find_support(Some(TextSupport::new(true)));
            let query = TextQuery::new("Bé🚢").searching(Searched::Keys(vec![key(1)]));
            grid.set_find(Some(FindQuery::new(query).by(Axis::Cols)));
            grid.action(Action::FindNext);
            grid.find_stepped(Some(FindMatch::new(key(3), key(1))));
        }
        "rejected" => {
            // The source answered a quick filter and a condition on column 1
            // with neither (an invalid regex, say), and later stopped
            // applying the condition on column 2.
            let kept = Condition::text(key(2), TextQuery::new("x"));
            grid.add_condition(Axis::Rows, kept.clone());
            let applied = FilterSpec::default().with_conditions(vec![kept].into());
            grid.view_applied(
                ViewSpec::default().with_filter(Axis::Rows, applied),
                Totals::unknown(),
            );
            grid.set_quick_filter(Axis::Rows, Some(TextQuery::new("(").regex(true)));
            grid.add_condition(
                Axis::Rows,
                Condition::text(key(1), TextQuery::new("[").regex(true)),
            );
            let applied = grid.snapshot().applied_view().clone();
            grid.view_applied(applied.clone(), Totals::unknown());
            grid.view_applied(applied, Totals::unknown());
            grid.view_changed(ViewSpec::default(), Totals::unknown());
        }
        "hidden" => {
            // Both axes hide: rows 2 and 5 and column 4 hidden as applied,
            // column 1 being hidden and row 5 being shown.
            for axis in Axis::ALL {
                grid.set_hide_support(axis, true);
            }
            grid.set_hide_cap(Axis::Cols, 2);
            grid.hide(Axis::Rows, &[key(2), key(5)]).unwrap();
            grid.hide(Axis::Cols, &[key(4)]).unwrap();
            let applied = grid.snapshot().requested_view().cloned().unwrap();
            grid.view_applied(applied.clone(), Totals::unknown());
            grid.view_applied(applied, Totals::unknown());
            grid.hide(Axis::Cols, &[key(1)]).unwrap();
            // Past the columns' cap of 2: nothing more is hidden.
            grid.hide(Axis::Cols, &[key(0)]).unwrap_err();
            grid.show(Axis::Rows, &[key(5)]);
            grid.set_hide_support(Axis::Rows, false);
        }
        other => panic!("no scenario {other}"),
    }
    fill(&mut grid, row_limit);
    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_scenario_reaches_its_state() {
        let mut modes = Vec::new();
        for name in SCENARIOS {
            let mut grid = scenario(name);
            let s = grid.snapshot();
            let mode = match s.mode() {
                Mode::Idle => "Idle",
                Mode::DraggingSelection => "DraggingSelection",
                Mode::Resizing { .. } => "Resizing",
                Mode::ResizingBand { .. } => "ResizingBand",
                Mode::MovingHeaders { .. } => "MovingHeaders",
                Mode::Editing { .. } => "Editing",
            };
            modes.push(mode);
            match name {
                "busy" => {
                    assert!(s.selection_range().is_some());
                    assert_eq!(s.picked(Axis::Rows).len(), 1);
                    assert_eq!(s.picked(Axis::Cols).len(), 1);
                    assert!(s.requested_view().is_some());
                    assert!(s.hovered_header().is_some());
                }
                "resizing" => assert!(s.resize_guide().is_some() && s.capturing()),
                "moving" => assert!(s.drop_indicator().is_some_and(|d| matches!(
                    d.target,
                    DropTarget::Move {
                        to: 1,
                        unpin: false
                    }
                ))),
                "pinDrag" => {
                    let target = s.drop_indicator().map(|d| d.target);
                    let before = Some(Key(KEY_BASE + 4));
                    assert_eq!(
                        target,
                        Some(DropTarget::Pin {
                            edge: Edge::Leading,
                            before,
                            reorder: true
                        })
                    );
                    assert_eq!(s.pin_zones().len(), 1);
                }
                "unpinDrag" => {
                    let target = s.drop_indicator().map(|d| d.target);
                    assert_eq!(target, Some(DropTarget::Unpin { at: 6 }));
                }
                "movePending" => assert!(s.headers(Axis::Rows).any(|h| h.move_pending)),
                "moveRefused" => assert!(s.move_refusal().is_some() && s.pending_move().is_none()),
                "editing" => assert!(s.editor_rect().is_some()),
                "locating" => {
                    assert!(s.locating(Axis::Rows));
                    assert!(s.rows().iter().any(|i| i.key.is_none()));
                }
                "scrolled" => assert!(s.scroll_anchor(Axis::Rows).position > 0),
                "pinned" => {
                    for axis in Axis::ALL {
                        for edge in Edge::ALL {
                            assert_eq!(s.pinned(axis, edge).len(), 1);
                        }
                        assert!(!s.strip(axis).is_empty());
                    }
                    assert!(s.scroll_anchor(Axis::Cols).position > 0);
                    let range = s.selection_range().unwrap();
                    let (rows, cols) = (5..7, 4..5);
                    assert_eq!(
                        (&range.rows[..], &range.cols[..]),
                        (&[rows][..], &[cols][..])
                    );
                }
                "dragging" => {
                    let range = s.selection_range().unwrap();
                    let (rows, cols) = (1..5, 0..3);
                    assert_eq!(
                        (&range.rows[..], &range.cols[..]),
                        (&[rows][..], &[cols][..])
                    );
                }
                "sorted" => {
                    assert_eq!(s.applied_view().rows.sort.columns().len(), 2);
                    assert_eq!(s.applied_view().cols.sort.columns().len(), 1);
                    let requested = s.requested_view().map(|r| r.rows.sort.columns().len());
                    assert_eq!(requested, Some(3));
                }
                "filtered" => {
                    assert!(s.applied_view().rows.filter.quick.is_some());
                    let requested = s.requested_view().and_then(|r| r.rows.filter.quick.clone());
                    assert!(requested.is_some_and(|q| q.regex));
                    assert_eq!(s.total(Axis::Rows), Some(900));
                    assert!(s.quick_filter(Axis::Rows).is_some());
                    let states: Vec<ConditionState> = s
                        .conditions(Axis::Rows)
                        .into_iter()
                        .map(|(_, st)| st)
                        .collect();
                    assert_eq!(
                        states,
                        [
                            ConditionState::Applied,
                            ConditionState::Adding,
                            ConditionState::Adding,
                            // The between of dates and the not at least.
                            ConditionState::Adding,
                            ConditionState::Adding,
                            ConditionState::Removing
                        ]
                    );
                }
                "restored" => {
                    let key = |p: u64| Key(KEY_BASE + p);
                    let pins: Vec<Key> = s.pins(Axis::Cols, Edge::Leading).collect();
                    assert_eq!(pins, [key(2), key(900)]);
                    assert_eq!(s.pinned(Axis::Cols, Edge::Leading).len(), 1);
                    assert_eq!(
                        s.pins(Axis::Rows, Edge::Trailing).collect::<Vec<_>>(),
                        [key(4)]
                    );
                }
                "endGap" => {
                    let width = VIEWPORT.width;
                    assert_eq!(
                        s.cols().last().map(|c| (c.position, c.end())),
                        Some((5, width - 9.0))
                    );
                    let bottom = s.pinned(Axis::Rows, Edge::Trailing)[0];
                    assert_eq!(bottom.end(), VIEWPORT.height - 6.0);
                    assert_eq!(
                        s.rows().last().map(|r| r.end()),
                        Some(s.scrolling(Axis::Rows).end)
                    );
                }
                "clipped" => {
                    // Every pin is kept; the leading columns clip, slid to
                    // show the focused column 3, and the trailing ones
                    // clip whole, as do the bottom rows in part.
                    assert_eq!(s.pinned(Axis::Cols, Edge::Leading).len(), 4);
                    let lead = s.pinned_region(Axis::Cols, Edge::Leading);
                    let focused = s.pinned(Axis::Cols, Edge::Leading)[3];
                    assert!(focused.offset >= lead.start && focused.end() <= lead.end);
                    assert!(s.pinned(Axis::Cols, Edge::Leading)[0].offset < lead.start);
                    assert!(s.pinned_region(Axis::Cols, Edge::Trailing).is_empty());
                    assert_eq!(s.pinned(Axis::Rows, Edge::Trailing).len(), 6);
                    let rows = s.pinned_region(Axis::Rows, Edge::Trailing);
                    let first = s.pinned(Axis::Rows, Edge::Trailing)[0];
                    assert!(first.offset < rows.start && !rows.is_empty());
                }
                "rejected" => {
                    assert!(!s.view_pending());
                    let states: Vec<ConditionState> = s
                        .conditions(Axis::Rows)
                        .into_iter()
                        .map(|(_, st)| st)
                        .collect();
                    assert_eq!(states, [ConditionState::NotApplied; 2]);
                    let wanted = s.requested_view().and_then(|r| r.rows.filter.quick.clone());
                    assert!(wanted.is_some_and(|q| q.regex));
                    assert_eq!(s.applied_view(), &ViewSpec::default());
                }
                "hidden" => {
                    let key = |p: u64| Key(KEY_BASE + p);
                    assert!(!s.hide_support(Axis::Rows) && s.hide_support(Axis::Cols));
                    assert_eq!(s.applied_view().rows.hidden, [key(2), key(5)]);
                    assert_eq!(s.hidden(Axis::Rows), [key(2)]);
                    assert_eq!(s.hidden(Axis::Cols), [key(4), key(1)]);
                    assert_eq!(s.hide_cap(Axis::Cols), 2);
                    assert!(s.view_pending());
                }
                "found" => {
                    assert!(s.find().is_some_and(|f| f.by == Axis::Cols));
                    assert!(s.find_support().is_some());
                    let current = s.find_match().unwrap();
                    let rows = 1..3;
                    assert_eq!(s.selection_range().map(|r| &r.rows[..]), Some(&[rows][..]));
                    let row = s.rows().iter().find(|i| i.key == Some(current.row));
                    let col = s.cols().iter().find(|i| i.key == Some(current.col));
                    assert!(s.cell_state(row.unwrap(), col.unwrap()).current_match);
                }
                _ => {}
            }
        }
        assert_eq!(
            modes,
            [
                "Idle",
                "Idle",
                "Resizing",
                "ResizingBand",
                "MovingHeaders",
                "Idle",
                "Idle",
                "Editing",
                "Idle",
                "Idle",
                "DraggingSelection",
                "Idle",
                "Idle",
                "Idle",
                "Idle",
                "Idle",
                "Idle",
                "MovingHeaders",
                "MovingHeaders",
                "Idle",
                "Idle",
                "Idle"
            ]
        );
    }

    #[test]
    fn every_effect_covers_every_variant() {
        // A new Effect variant fails to compile here until it is added to
        // `every_effect`, and so reaches the agreement test.
        let seen: Vec<&str> = every_effect()
            .iter()
            .map(|s| match &s.effect {
                crate::effect::WireEffect::TextTyped { .. } => "TextTyped",
                crate::effect::WireEffect::Activated { .. } => "Activated",
                crate::effect::WireEffect::EditRequested { .. } => "EditRequested",
                crate::effect::WireEffect::EditCancelled { .. } => "EditCancelled",
                crate::effect::WireEffect::FocusChanged { .. } => "FocusChanged",
                crate::effect::WireEffect::SelectionChanged => "SelectionChanged",
                crate::effect::WireEffect::ViewRequested { .. } => "ViewRequested",
                crate::effect::WireEffect::Locate { .. } => "Locate",
                crate::effect::WireEffect::SizeChanged { .. } => "SizeChanged",
                crate::effect::WireEffect::AutoSizeRequested { .. } => "AutoSizeRequested",
                crate::effect::WireEffect::PinsChanged { .. } => "PinsChanged",
                crate::effect::WireEffect::BandSizeChanged { .. } => "BandSizeChanged",
                crate::effect::WireEffect::BandAutoSizeRequested { .. } => "BandAutoSizeRequested",
                crate::effect::WireEffect::MoveRequested { .. } => "MoveRequested",
                crate::effect::WireEffect::FindRequested { .. } => "FindRequested",
                crate::effect::WireEffect::FindStepRequested { .. } => "FindStepRequested",
            })
            .collect();
        let mut unique = seen.clone();
        unique.dedup();
        assert_eq!(unique.len(), 16, "{seen:?}");
    }
}
