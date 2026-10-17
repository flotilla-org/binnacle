//! Test-only exports for the agreement tests in `web/`: a dump of a Snapshot
//! built straight from the core's accessors (not from the flat encoding), in
//! the shape `web/src/snapshot.ts` decodes to, and one of every Effect.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Cursor, Effect, FocusStep, Grid, HeaderPlacement, Key,
    Mode, Modifiers, PointerEvent, Rect, Snapshot, SortColumn, SortSpec, Stamped, Viewport,
};
use serde::Serialize;

use crate::effect::{WireAxis, WireDirection, WireSortColumn, WireStamped, sort_columns};
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
    hovered_header: Option<DumpHovered>,
    selection_range: Option<DumpRange>,
    selection_rects: Vec<DumpRect>,
    picked: Vec<u64>,
    moving_keys: Vec<u64>,
    applied_sort: Vec<WireSortColumn>,
    requested_sort: Option<Vec<WireSortColumn>>,
    cells: Vec<u8>,
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
    headers: Vec<DumpHeader>,
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

impl From<HeaderPlacement> for DumpHeader {
    fn from(h: HeaderPlacement) -> Self {
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
        to: f64,
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
            Mode::MovingHeaders { axis, key, to } => DumpMode::MovingHeaders {
                axis: axis.into(),
                key: key.0,
                to: to as f64,
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
    to: f64,
    offset: f32,
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
    rows: DumpSpan,
    cols: DumpSpan,
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
        strip: s
            .strip(axis)
            .iter()
            .map(|i| DumpItem {
                position: i.position as f64,
                key: i.key.map(|k| k.0),
                offset: i.offset,
                size: i.size,
            })
            .collect(),
        headers: s.headers(axis).map(DumpHeader::from).collect(),
    }
}

/// The Snapshot `s` in the shape the TypeScript decoder produces.
pub fn dump(s: &Snapshot<'_>) -> Dump {
    let keys = |ks: &[Key]| ks.iter().map(|k| k.0).collect();
    let mut cells = Vec::new();
    for row in s.rows() {
        for col in s.cols() {
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
            to: d.to as f64,
            offset: d.offset,
        }),
        hovered_header: s.hovered_header().map(|(axis, item)| DumpHovered {
            axis: axis.into(),
            position: item.position as f64,
        }),
        selection_range: s.selection_range().map(|r| DumpRange {
            rows: DumpSpan {
                start: r.rows.start as f64,
                end: r.rows.end as f64,
            },
            cols: DumpSpan {
                start: r.cols.start as f64,
                end: r.cols.end as f64,
            },
        }),
        selection_rects: s.selection_rects().iter().map(|r| (*r).into()).collect(),
        picked: keys(s.picked()),
        moving_keys: keys(s.moving_keys()),
        applied_sort: sort_columns(s.applied_sort()),
        requested_sort: s.requested_sort().map(sort_columns),
        cells,
    }
}

/// One of every Effect variant, with Keys past 2^53 where an Effect has them.
pub fn every_effect() -> Vec<WireStamped> {
    let (row, col) = (Key(u64::MAX), Key((1 << 53) + 1));
    let spec = SortSpec::new(vec![
        SortColumn::ascending(col),
        SortColumn::descending(row),
    ]);
    [
        Effect::TextTyped { text: "é".into() },
        Effect::Activated { row, col },
        Effect::EditRequested { row, col },
        Effect::EditCancelled { row, col },
        Effect::FocusChanged { row, col },
        Effect::SelectionChanged,
        Effect::SortRequested { spec },
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

/// The scenarios [`scenario`] knows, for the agreement tests to loop over.
pub const SCENARIOS: [&str; 9] = [
    "plain", "busy", "resizing", "band", "moving", "editing", "locating", "scrolled", "dragging",
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
            grid.pointer(PointerEvent::down(220.0, 10.0));
            grid.pointer(PointerEvent::moved(60.0, 10.0));
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
        "scrolled" => {
            grid.action(Action::Scroll {
                dx: 130.0,
                dy: 333.0,
            });
        }
        "dragging" => {
            // Pressed on row 1, column 0 and dragged to row 4, column 2.
            grid.pointer(PointerEvent::down(60.0, 24.0 + 21.0 + 5.0));
            grid.pointer(PointerEvent::moved(220.0, 24.0 + 21.0 * 4.0 + 5.0));
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
                    assert_eq!(s.picked().len(), 1);
                    assert!(s.requested_sort().is_some());
                    assert!(s.hovered_header().is_some());
                }
                "resizing" => assert!(s.resize_guide().is_some() && s.capturing()),
                "moving" => assert!(s.drop_indicator().is_some()),
                "editing" => assert!(s.editor_rect().is_some()),
                "locating" => {
                    assert!(s.locating(Axis::Rows));
                    assert!(s.rows().iter().any(|i| i.key.is_none()));
                }
                "scrolled" => assert!(s.scroll_anchor(Axis::Rows).position > 0),
                "dragging" => {
                    let range = s.selection_range().unwrap();
                    assert_eq!((range.rows, range.cols), (1..5, 0..3));
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
                "Editing",
                "Idle",
                "Idle",
                "DraggingSelection"
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
                crate::effect::WireEffect::SortRequested { .. } => "SortRequested",
                crate::effect::WireEffect::Locate { .. } => "Locate",
                crate::effect::WireEffect::SizeChanged { .. } => "SizeChanged",
                crate::effect::WireEffect::AutoSizeRequested { .. } => "AutoSizeRequested",
                crate::effect::WireEffect::BandSizeChanged { .. } => "BandSizeChanged",
                crate::effect::WireEffect::BandAutoSizeRequested { .. } => "BandAutoSizeRequested",
                crate::effect::WireEffect::MoveRequested { .. } => "MoveRequested",
            })
            .collect();
        let mut unique = seen.clone();
        unique.dedup();
        assert_eq!(unique.len(), 13, "{seen:?}");
    }
}
