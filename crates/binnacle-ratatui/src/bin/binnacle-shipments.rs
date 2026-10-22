//! The binnacle grouping demo: 240 shipments behind the In-Memory Table
//! Source under the grouping layer, grouped by region, then port, with the
//! sum of their weight and their count.
//!
//! Each region's Group Item heads its ports, and each port's its shipments,
//! with a disclosure mark and the count in the row Header Band; the region
//! and port columns leave the body while the rows group by them. A Subtotal
//! follows each group with its weight, so an open Group Item stays blank
//! apart from its label, and the Grand Total is pinned at the bottom (`r`
//! on it opens the row menu, which unpins it). Press `m` for the focused column's menu:
//! "Ungroup rows" brings back the plain table, and on the region, port or
//! crew column "Group rows by this column" adds a level. "Slice by this
//! column" (`s` in the menu) opens the column's filter popup in slicer
//! mode: what it asks for keeps only the shipments that pass before they
//! group, so the figures change, and the slicers show on the bottom line
//! after "Sliced by". A comparison on the weight from the popup filtering
//! the rows (Alt+Down) acts on the groups instead, leaving out those whose
//! total fails it. Click the weight
//! header to sort: regions and ports order by their total weight, and
//! shipments by theirs within their port. Arrows move through Summary Items
//! and shipments alike, and focus stays on its shipment through grouping
//! and ungrouping. A click on a Group Item's label selects its group, and
//! Shift-click stretches by whole groups. Ctrl+C copies what is shown,
//! labels first, to the terminal's clipboard (OSC 52), and Enter on a
//! Summary Item's cell shows the drill-down it carries on the status line.
//! `f` switches the rows between outline and header form: in header form
//! the region and port are columns of the row Header Band, each group one
//! span over its rows with its mark and label, and Left and Right step
//! between those columns. The corner names each level: click a name to
//! sort by its field, right-click it to filter by it. `t` cycles the Theme
//! presets, `q` quits.
//!
//! Ctrl+F opens the find bar: it matches the regions' and ports' labels and
//! the shipments' cells, counts the matches the closed ports hide ("3 of 12,
//! 5 in collapsed groups"), and a step to one (Enter, or Ctrl+N and Ctrl+P)
//! opens its port first. Drag a shipment's row number to move it within its
//! port; a drop in another port is refused, and the status line says so.
//! F2 edits the weight or crew in place; `p` sends the focused shipment to
//! the next port. It stays where it is, dimmed, while its groups' figures
//! change at once, and the next sort or grouping puts it in its new port.
//! Group rows and totals never edit or move.
//!
//! `v` pivots: ports by region down the rows and quarters across, each
//! quarter spanning its total weight and count columns in the column Header
//! Band, the All columns, one per value, pinned at the right and the Grand
//! Total row at the bottom. `v` again goes back to the grouped rows.

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{self, Write};
use std::ops::Range;
use std::time::{Duration, Instant};

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Condition, ConditionKind, Depth, DrillDown, EditEnd, Effect,
    Expand, FindQuery, Form, Grid, Grouping, HeaderSpan, ItemKind, Key, Level, Outline, Placement,
    Position, Value, ViewSpec, Viewport,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::{FindStep, MemorySource, PumpSource, pump};
use binnacle_ratatui::{
    ColorSupport, ColumnFilter, Content, Control, Controls, CopiedRow, FindBar, FindStatus,
    HeaderMenu, Keymap, Layout, Mouse, PendingMarks, Preset, RenderContext, Theme, begin_frame,
    copy_text, corner_level_at, disclosure_at, render_conditions, render_editor, render_with,
    status, wheel,
};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;

const SHIPMENTS: usize = 240;
const HEADERS: [&str; 6] = ["id", "region", "port", "weight", "crew", "quarter"];
const REGION: Key = Key(1);
const PORT: Key = Key(2);
const WEIGHT: Key = Key(3);
const CREW: Key = Key(4);
const QUARTER: Key = Key(5);

const REGIONS: [&str; 4] = ["North", "South", "East", "West"];
const PORTS: [&str; 6] = [
    "Aberdeen", "Bergen", "Cádiz", "Dunedin", "Esbjerg", "Funchal",
];
const QUARTERS: [&str; 4] = ["Q1", "Q2", "Q3", "Q4"];

struct Shipment {
    id: i64,
    /// Missing for every seventeenth shipment, to show a blank group.
    region: Option<&'static str>,
    port: &'static str,
    weight: f64,
    crew: i64,
    quarter: &'static str,
}

impl Shipment {
    fn generate(i: usize) -> Shipment {
        // A cheap scramble so the columns are not already in order.
        let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
        Shipment {
            id: i as i64,
            region: (!i.is_multiple_of(17)).then(|| REGIONS[(h % 4) as usize]),
            port: PORTS[((h >> 3) % 6) as usize],
            weight: (h % 10_000) as f64 / 10.0,
            crew: (h % 13) as i64,
            quarter: QUARTERS[((h >> 7) % 4) as usize],
        }
    }
}

fn source() -> GroupedMemory<Shipment> {
    let source = MemorySource::<Shipment>::new(SHIPMENTS, HEADERS.len())
        .i64_column(Key(0), |s| Some(s.id))
        .bytes_column(REGION, |s| s.region.map(|r| Cow::Borrowed(r.as_bytes())))
        .text_column(REGION, |s| s.region.map(Cow::Borrowed))
        .bytes_column(Key(2), |s| Some(Cow::Borrowed(s.port.as_bytes())))
        .text_column(Key(2), |s| Some(Cow::Borrowed(s.port)))
        .f64_column(Key(3), |s| Some(s.weight))
        .i64_column(Key(4), |s| Some(s.crew))
        .text_column(Key(4), |s| Some(Cow::Owned(s.crew.to_string())))
        .bytes_column(QUARTER, |s| Some(Cow::Borrowed(s.quarter.as_bytes())))
        .text_column(QUARTER, |s| Some(Cow::Borrowed(s.quarter)));
    GroupedMemory::new(source)
}

/// Region, then port, each with a Subtotal after it; the sum of the
/// weights and the count; the Grand Total after everything. The regions
/// start open and the ports closed, so both show.
fn grouping() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = [REGION, PORT]
        .map(|field| Level::Field {
            field,
            subtotal: Placement::After,
        })
        .to_vec();
    grouping.values = vec![AggregateValue::of(WEIGHT, "sum"), AggregateValue::count()];
    grouping.rows.grand_total = Placement::After;
    grouping.rows.expand = Expand::to(Depth::Levels(1));
    grouping
}

/// Ports by region down the rows, quarters across, each by the sum of the
/// weights and the count, with a Grand Total after each axis.
fn pivot() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.col_levels = vec![Level::field(QUARTER), Level::Values];
    grouping.values = vec![AggregateValue::of(WEIGHT, "sum"), AggregateValue::count()];
    grouping.rows.grand_total = Placement::After;
    grouping.cols.grand_total = Placement::After;
    grouping
}

/// What the table draws: shipments by their fields, Summary Items by their
/// group, count and figures.
struct Text<'a> {
    rows: &'a [Shipment],
    source: &'a GroupedMemory<Shipment>,
    grouping: Option<&'a Grouping>,
    /// The cells the Find Query matches, and the shipments left unmatched
    /// in their old group after an edit.
    marked: &'a HashSet<(Key, Key)>,
    unmatched: &'a HashSet<Key>,
}

impl Text<'_> {
    /// The Summary Item `row`'s figure for the value `aggregate` over
    /// `field`, if the grouping shows one.
    fn figure(&self, row: Key, field: Option<Key>, aggregate: Option<&str>) -> Option<Figure> {
        let values = &self.grouping?.values;
        let i = values
            .iter()
            .position(|v| v.field == field && aggregate.is_none_or(|a| a == v.aggregate))?;
        self.source.summary(row)?.aggregates.get(i).copied()
    }
}

impl Content for Text<'_> {
    fn cell(&self, row: Key, col: Key) -> String {
        if self.grouping.is_some_and(Grouping::pivot) {
            // Both axes are the layer's: the figure where they cross.
            return match self.source.cell(row, col) {
                Some(Figure::Number(n)) => format!("{n:.1}"),
                Some(Figure::Count(n)) => n.to_string(),
                _ => String::new(),
            };
        }
        let Some(s) = self.rows.get(row.0 as usize) else {
            // A Summary Item shows its group's figure for the column.
            return match self.figure(row, Some(col), None) {
                Some(Figure::Number(n)) => format!("{n:.1}"),
                Some(Figure::Count(n)) => n.to_string(),
                _ => String::new(),
            };
        };
        match col.0 {
            0 => s.id.to_string(),
            1 => s.region.unwrap_or_default().to_string(),
            2 => s.port.to_string(),
            3 => format!("{:.1}", s.weight),
            4 => s.crew.to_string(),
            _ => s.quarter.to_string(),
        }
    }

    fn column_header(&self, col: Key) -> String {
        HEADERS[col.0 as usize].to_string()
    }

    fn row_header(&self, row: Key, _: u64) -> String {
        format!("#{}", row.0)
    }

    fn group_label(&self, row: Key, outline: &Outline) -> String {
        let label = match outline.group.as_ref().map(|g| &g.value) {
            Some(Value::Text(t)) => t.clone(),
            _ => "(none)".into(),
        };
        let label = match outline.kind {
            ItemKind::Subtotal => format!("{label} total"),
            ItemKind::GrandTotal => "Grand total".into(),
            _ => label,
        };
        match self.figure(row, None, Some("count")) {
            Some(Figure::Count(n)) => format!("{label} ({n})"),
            _ => label,
        }
    }

    fn span_label(&self, span: &HeaderSpan) -> String {
        let summary = self.source.summary(span.key);
        match summary.as_ref().and_then(|s| s.path.last()) {
            Some((_, Value::Text(t))) => t.clone(),
            _ => "blank".into(),
        }
    }

    fn marked(&self, row: Key, col: Key) -> bool {
        self.marked.contains(&(row, col))
    }

    fn unmatched(&self, axis: Axis, key: Key) -> bool {
        axis == Axis::Rows && self.unmatched.contains(&key)
    }
}

/// The host's own editor: the edited cell and the text so far.
struct Editor {
    row: Key,
    col: Key,
    text: String,
}

/// Keeps the editor's text in its shipment if it reads as the column's
/// number, and tells the Grid what changed.
fn commit(
    editor: Editor,
    grid: &mut Grid,
    source: &mut GroupedMemory<Shipment>,
    rows: &mut [Shipment],
) {
    let row = editor.row.0 as usize;
    let text = editor.text.trim();
    let kept = match editor.col {
        WEIGHT => text.parse().ok().map(|w| rows[row].weight = w),
        _ => text.parse().ok().map(|c| rows[row].crew = c),
    };
    let end = match kept {
        Some(()) => EditEnd::Commit,
        None => EditEnd::Cancel,
    };
    grid.edit_ended(end);
    if kept.is_some() {
        for (axis, notice) in source.edited(rows, row, editor.col) {
            grid.notify(axis, notice);
        }
    }
}

/// Asks for the Windows the next Snapshot needs and fills them, outlines
/// and all.
fn fill(grid: &mut Grid, source: &mut GroupedMemory<Shipment>, rows: &[Shipment], v: Viewport) {
    for (axis, range) in grid.begin_frame(v) {
        let window = source.window(rows, axis, range.clone());
        grid.set_window(axis, range.start, &window.keys);
        if let Some(outlines) = window.outlines {
            grid.set_outlines(axis, range.start, &outlines);
        }
    }
}

/// Declares what each column supports, which says the rows group by the
/// text columns.
fn declare(grid: &mut Grid, source: &mut GroupedMemory<Shipment>, rows: &[Shipment]) {
    let mut over = source.over(rows);
    for c in 0..HEADERS.len() as u64 {
        grid.set_support(Axis::Rows, Key(c), over.support(Axis::Rows, Key(c)));
    }
    drop(over);
    for c in 0..HEADERS.len() as u64 {
        grid.set_aggregates(Axis::Rows, Key(c), source.aggregates(Axis::Rows, Key(c)));
    }
    let mut over = source.over(rows);
    for axis in Axis::ALL {
        grid.set_hide_support(axis, over.hide_support(axis));
    }
}

/// What a copy names: the rows and columns at its runs of Positions.
struct Copy {
    rows: Vec<Range<Position>>,
    cols: Vec<Range<Position>>,
    labels: bool,
}

/// The text of `copy` as the table draws it.
fn copied(
    copy: &Copy,
    source: &mut GroupedMemory<Shipment>,
    rows: &[Shipment],
    grouping: Option<&Grouping>,
) -> (usize, String) {
    let mut items = Vec::new();
    for run in &copy.rows {
        let window = source.window(rows, Axis::Rows, run.clone());
        let mut outlines = window.outlines.map(Vec::into_iter);
        for (key, position) in window.keys.into_iter().zip(run.clone()) {
            let outline = outlines.as_mut().and_then(Iterator::next);
            items.push(CopiedRow {
                key,
                position,
                outline,
            });
        }
    }
    let cols: Vec<Key> = (copy.cols.iter())
        .flat_map(|run| source.window(rows, Axis::Cols, run.clone()).keys)
        .collect();
    let text = Text {
        rows,
        source,
        grouping,
        marked: &HashSet::new(),
        unmatched: &HashSet::new(),
    };
    (items.len(), copy_text(&text, &items, &cols, copy.labels))
}

/// Puts `text` on the terminal's clipboard with OSC 52.
fn clipboard(text: &str) -> io::Result<()> {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            match i <= chunk.len() {
                true => out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char),
                false => out.push('='),
            }
        }
    }
    let mut stdout = io::stdout();
    write!(stdout, "\x1b]52;c;{out}\x07")?;
    stdout.flush()
}

/// A drill-down as the status line shows it: each condition's column and values.
fn described(drill_down: &DrillDown) -> String {
    let condition = |c: &Condition| {
        let values = match &c.kind {
            ConditionKind::OneOf(values) => (values.iter())
                .map(|v| match v {
                    Value::Text(t) => t.clone(),
                    Value::Blank => "(none)".into(),
                })
                .collect::<Vec<_>>()
                .join(" or "),
            other => format!("{other:?}"),
        };
        format!("{} {values}", HEADERS[c.key.0 as usize])
    };
    match drill_down.conditions.is_empty() {
        true => "drill-down: every shipment".into(),
        false => format!(
            "drill-down: {}",
            drill_down
                .conditions
                .iter()
                .map(condition)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The Group Item whose disclosure mark a left press in `m` lands on.
fn mark_clicked(m: &MouseEvent, grid: &mut Grid, area: ratatui::layout::Rect) -> Option<Key> {
    if m.kind != MouseEventKind::Down(MouseButton::Left) {
        return None;
    }
    let x = m.column.checked_sub(area.x)?;
    let y = m.row.checked_sub(area.y)?;
    disclosure_at(&grid.snapshot(), x, y)
}

/// A press in `m` on a level name in the corner: a left press sorts by its
/// field (Shift keeps the rest of the sort), a right press opens the filter
/// popup on it. Whether it landed on one.
fn corner_clicked(
    m: &MouseEvent,
    grid: &mut Grid,
    text: &Text,
    popup: &mut ColumnFilter,
    area: ratatui::layout::Rect,
) -> bool {
    let (Some(x), Some(y)) = (m.column.checked_sub(area.x), m.row.checked_sub(area.y)) else {
        return false;
    };
    let Some((axis, field)) = corner_level_at(&grid.snapshot(), text, x, y)
        .and_then(|(axis, level)| level.field.map(|f| (axis, f)))
    else {
        return false;
    };
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let shift = m.modifiers.contains(KeyModifiers::SHIFT);
            grid.cycle_sort(axis, field, shift);
            true
        }
        MouseEventKind::Down(MouseButton::Right) => popup.open_on(&grid.snapshot(), axis, field),
        _ => false,
    }
}

fn main() -> io::Result<()> {
    let mut rows: Vec<Shipment> = (0..SHIPMENTS).map(Shipment::generate).collect();
    let mut source = source();
    let mut grid = Grid::new(
        AxisConfig::new(SHIPMENTS as u64, 1.0).header(1.0),
        AxisConfig::new(HEADERS.len() as u64, 10.0)
            .header(24.0)
            .gap(1.0),
    );
    declare(&mut grid, &mut source, &rows);
    grid.set_find_support(source.over(&rows).find_support());
    grid.set_tiers(Axis::Rows, &[8.0, 11.0]);
    grid.request_view(ViewSpec::default().with_grouping(Some(grouping())));
    let keymap = Keymap::preset(Preset::Chords);
    let controls = Controls::preset(Preset::Chords);
    let mut find = FindBar::new().pause(Duration::ZERO);
    // The last step answered, for the find bar's "3 of 12".
    let mut step: Option<(FindQuery, FindStep)> = None;
    let mut editor: Option<Editor> = None;
    let mut menu = HeaderMenu::new();
    let mut popup = ColumnFilter::new();
    let mut mouse = Mouse::default();
    let mut marks = PendingMarks::default();
    let start = Instant::now();
    let mut layout = Layout::default();
    // The last copy or drill-down, for the status line.
    let mut note = String::new();
    let themes = Theme::presets();
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let mut theme = if no_color { themes.len() - 1 } else { 0 };
    let colors = ColorSupport::from_colorterm(std::env::var("COLORTERM").ok().as_deref());
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                let full = frame.area();
                let mut area = full;
                area.height = full.height.saturating_sub(2 + u16::from(find.is_open()));
                popup.tick(&mut grid, Instant::now());
                // Answers the requests in the queue, then acts on what the
                // demo owns: its editor, the step's place among the matches,
                // copies and drill-downs.
                let mut effects = Vec::new();
                pump(&mut grid, &mut source.over(&rows), |_, e| effects.push(e.effect));
                let mut copy = None;
                for effect in effects {
                    match effect {
                        Effect::FindStepRequested {
                            query,
                            from,
                            direction,
                        } => {
                            step = (source.find_next(&rows, &query, from, direction))
                                .map(|s| (query, s));
                        }
                        Effect::CopyRequested {
                            rows,
                            cols,
                            row_labels,
                            ..
                        } => {
                            copy = Some(Copy {
                                rows,
                                cols,
                                labels: row_labels,
                            })
                        }
                        Effect::Activated { row, col, .. } | Effect::EditRequested { row, col }
                            if matches!(col, WEIGHT | CREW) && (row.0 as usize) < SHIPMENTS =>
                        {
                            grid.edit_started(row, col);
                            let text = Text {
                                rows: &rows,
                                source: &source,
                                grouping: None,
                                marked: &HashSet::new(),
                                unmatched: &HashSet::new(),
                            }
                            .cell(row, col);
                            editor = Some(Editor { row, col, text });
                        }
                        Effect::Activated {
                            drill_down: Some(d),
                            ..
                        } => note = described(&d),
                        Effect::EditCancelled { .. } => editor = None,
                        _ => {}
                    }
                }
                if let Some(copy) = copy {
                    let grouping = grid.snapshot().applied_view().grouping.clone();
                    let (n, text) = copied(&copy, &mut source, &rows, grouping.as_ref());
                    note = match clipboard(&text) {
                        Ok(()) => format!("copied {n} rows"),
                        Err(e) => format!("copy failed: {e}"),
                    };
                }
                let now = start.elapsed();
                let ctx = RenderContext::new(&themes[theme]).colors(colors).at(now);
                layout = begin_frame(&mut grid, &ctx, area, |grid, v| {
                    fill(grid, &mut source, &rows, v)
                });
                let snapshot = grid.snapshot();
                marks.seen(&snapshot, now);
                let ctx = ctx.marks(&marks);
                popup.fetch(&snapshot, |axis, key, query| {
                    Some(source.source_mut().distinct(&rows, axis, key, query))
                });
                let shown = |axis| -> Vec<Key> { snapshot.drawn(axis).filter_map(|i| i.key).collect() };
                let (row_keys, col_keys) = (shown(Axis::Rows), shown(Axis::Cols));
                let mut found = FindStatus::default();
                let mut marked = HashSet::new();
                if let Some(query) = snapshot.find() {
                    let flags = source.find_marks(&rows, &row_keys, &col_keys, query);
                    let cells = row_keys.iter().flat_map(|&r| col_keys.iter().map(move |&c| (r, c)));
                    marked = cells.zip(flags).filter_map(|(cell, m)| m.then_some(cell)).collect();
                    let step = step.as_ref().filter(|(q, _)| q == query).map(|(_, s)| s);
                    found = FindStatus {
                        count: Some(source.find_count(&rows, query)),
                        index: step.and_then(|s| s.index),
                        wrapped: step.is_some_and(|s| s.wrapped),
                        ..FindStatus::default()
                    };
                }
                let flags = source.unmatched(&rows, Axis::Rows, &row_keys);
                let unmatched = (row_keys.iter().zip(flags))
                    .filter_map(|(&k, u)| u.then_some(k))
                    .collect();
                let text = Text {
                    rows: &rows,
                    source: &source,
                    grouping: snapshot.applied_view().grouping.as_ref(),
                    marked: &marked,
                    unmatched: &unmatched,
                };
                render_with(&snapshot, &text, &ctx, area, frame.buffer_mut());
                if let Some(editor) = &editor {
                    render_editor(&snapshot, &editor.text, layout.grid, frame.buffer_mut());
                }
                if find.is_open() {
                    let bar = ratatui::layout::Rect {
                        y: area.bottom(),
                        height: 1,
                        ..full
                    };
                    find.render(&snapshot, &found, &themes[theme], bar, frame.buffer_mut());
                }
                popup.render(
                    &snapshot,
                    &text,
                    &themes[theme],
                    layout.grid,
                    frame.buffer_mut(),
                );
                menu.render(
                    &snapshot,
                    &text,
                    &themes[theme],
                    layout.grid,
                    frame.buffer_mut(),
                );
                let applied = snapshot.applied_view().grouping.as_ref();
                let grouped = match applied.map(|g| (g.row_levels.is_empty(), g.rows.form)) {
                    _ if applied.is_some_and(Grouping::pivot) => "pivot",
                    Some((false, Form::Header)) => "grouped, header form",
                    Some((false, Form::Outline)) => "grouped, outline form",
                    _ => "plain",
                };
                let chips = ratatui::layout::Rect {
                    y: full.bottom().saturating_sub(2),
                    height: 1,
                    ..full
                };
                render_conditions(&snapshot, &text, &themes[theme], chips, frame.buffer_mut());
                let line = ratatui::layout::Rect {
                    y: full.bottom().saturating_sub(1),
                    height: 1,
                    ..full
                };
                // What a drag or a refused move says, else the keys.
                let said = status(&snapshot);
                let line_text = match said.is_empty() {
                    false => said,
                    true => format!(
                        "^F find  F2 edit  p reroute  m column menu  r row menu  f form  v pivot  C-c copy  t theme  q quit | {grouped} | {} | {note}",
                        themes[theme].name
                    ),
                };
                frame.render_widget(ratatui::text::Line::from(line_text), line);
            })?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let ctrl = key.modifiers == KeyModifiers::CONTROL;
                    if ctrl && key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                    if let Some(open) = &mut editor {
                        match key.code {
                            KeyCode::Char(c) => open.text.push(c),
                            KeyCode::Backspace => {
                                open.text.pop();
                            }
                            KeyCode::Enter => {
                                let open = editor.take().unwrap();
                                commit(open, &mut grid, &mut source, &mut rows);
                            }
                            KeyCode::Esc => {
                                editor = None;
                                grid.edit_ended(EditEnd::Cancel);
                            }
                            _ => {}
                        }
                        continue;
                    }
                    if popup.key(&key, &mut grid) {
                        continue;
                    }
                    if menu.key(&key, &mut grid, |axis, range| {
                        source.window(&rows, axis, range).keys
                    }) {
                        if let Some(key) = menu.take_slicer() {
                            popup.open_slicer(&grid.snapshot(), key);
                        }
                        continue;
                    }
                    if find.key(&key, &mut grid) {
                        continue;
                    }
                    let took = match controls.control(&key) {
                        Some(Control::ColumnFilter) => {
                            popup.open(&grid.snapshot());
                            true
                        }
                        Some(Control::Find) => {
                            if find.open(&grid.snapshot()) {
                                find.apply(&mut grid);
                            }
                            true
                        }
                        Some(Control::FindNext) => find.repeat(false, &mut grid),
                        Some(Control::FindPrevious) => find.repeat(true, &mut grid),
                        Some(Control::Close) if find.is_open() => {
                            find.close(&mut grid);
                            true
                        }
                        _ => false,
                    };
                    if took {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('p') => {
                            // The focused shipment goes to the next port.
                            let snapshot = grid.snapshot();
                            let focus = snapshot.focus(Axis::Rows).and_then(|f| f.key);
                            if let Some(row) =
                                focus.map(|k| k.0 as usize).filter(|&r| r < SHIPMENTS)
                            {
                                let next = PORTS.iter().position(|&p| p == rows[row].port);
                                rows[row].port = PORTS[next.map_or(0, |i| (i + 1) % PORTS.len())];
                                for (axis, notice) in source.edited(&rows, row, PORT) {
                                    grid.notify(axis, notice);
                                }
                            }
                        }
                        KeyCode::Char('t') => theme = (theme + 1) % themes.len(),
                        KeyCode::Char('f') => {
                            let snapshot = grid.snapshot();
                            let latest = snapshot.requested_view();
                            let latest = latest.unwrap_or(snapshot.applied_view());
                            let form = match latest.grouping.as_ref().map(|g| g.rows.form) {
                                Some(Form::Header) => Form::Outline,
                                _ => Form::Header,
                            };
                            grid.set_form(Axis::Rows, form);
                        }
                        KeyCode::Char('v') => {
                            let snapshot = grid.snapshot();
                            let latest = snapshot.requested_view();
                            let latest = latest.unwrap_or(snapshot.applied_view());
                            let pivoted = latest.grouping.as_ref().is_some_and(Grouping::pivot);
                            let mut view = latest.clone();
                            if pivoted {
                                view.grouping = Some(grouping());
                                // The column band back to one line.
                                grid.set_tiers(Axis::Cols, &[]);
                                grid.set_header(Axis::Rows, 1.0);
                            } else {
                                view.grouping = Some(pivot());
                                // The quarters' line, then the value columns' own.
                                grid.set_tiers(Axis::Cols, &[1.0, 0.0]);
                                grid.set_header(Axis::Rows, 2.0);
                            }
                            grid.request_view(view);
                        }
                        KeyCode::Char('m') => {
                            menu.open(&grid.snapshot(), Axis::Cols);
                        }
                        KeyCode::Char('r') => {
                            menu.open(&grid.snapshot(), Axis::Rows);
                        }
                        _ => {
                            if let Some(action) = keymap.action(&key) {
                                grid.action(action);
                            }
                        }
                    }
                }
                Event::Mouse(m) => {
                    let text = Text {
                        rows: &rows,
                        source: &source,
                        grouping: None,
                        marked: &HashSet::new(),
                        unmatched: &HashSet::new(),
                    };
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
                    } else if corner_clicked(&m, &mut grid, &text, &mut popup, layout.grid) {
                    } else if let Some(key) = mark_clicked(&m, &mut grid, layout.grid) {
                        let _ = grid.toggle(Axis::Rows, key);
                    } else if let Some(pointer) = mouse.event(&m, layout.grid, Instant::now()) {
                        grid.pointer(pointer);
                    }
                }
                _ => {}
            }
        }
    })();
    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}
