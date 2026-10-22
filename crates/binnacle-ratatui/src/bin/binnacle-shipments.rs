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
//! crew column "Group rows by this column" adds a level. Click the weight
//! header to sort: regions and ports order by their total weight, and
//! shipments by theirs within their port. Arrows move through Summary Items
//! and shipments alike, and focus stays on its shipment through grouping
//! and ungrouping. `t` cycles the Theme presets, `q` quits.

use std::borrow::Cow;
use std::io;
use std::time::Instant;

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Grid, Grouping, ItemKind, Key, Level, Outline, Placement,
    Value, ViewSpec, Viewport,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::{MemorySource, PumpSource, pump};
use binnacle_ratatui::{
    ColorSupport, Content, HeaderMenu, Keymap, Layout, Mouse, Preset, RenderContext, Theme,
    begin_frame, render_with, wheel,
};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;

const SHIPMENTS: usize = 240;
const HEADERS: [&str; 5] = ["id", "region", "port", "weight", "crew"];
const REGION: Key = Key(1);
const PORT: Key = Key(2);
const WEIGHT: Key = Key(3);

const REGIONS: [&str; 4] = ["North", "South", "East", "West"];
const PORTS: [&str; 6] = [
    "Aberdeen", "Bergen", "Cádiz", "Dunedin", "Esbjerg", "Funchal",
];

struct Shipment {
    id: i64,
    /// Missing for every seventeenth shipment, to show a blank group.
    region: Option<&'static str>,
    port: &'static str,
    weight: f64,
    crew: i64,
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
        .text_column(Key(4), |s| Some(Cow::Owned(s.crew.to_string())));
    GroupedMemory::new(source)
}

/// Region, then port, each with a Subtotal after it; the sum of the
/// weights and the count; the Grand Total after everything.
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
    grouping
}

/// What the table draws: shipments by their fields, Summary Items by their
/// group, count and figures.
struct Text<'a> {
    rows: &'a [Shipment],
    source: &'a GroupedMemory<Shipment>,
    grouping: Option<&'a Grouping>,
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
            _ => s.crew.to_string(),
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
    for axis in Axis::ALL {
        grid.set_hide_support(axis, over.hide_support(axis));
    }
}

fn main() -> io::Result<()> {
    let rows: Vec<Shipment> = (0..SHIPMENTS).map(Shipment::generate).collect();
    let mut source = source();
    let mut grid = Grid::new(
        AxisConfig::new(SHIPMENTS as u64, 1.0).header(1.0),
        AxisConfig::new(HEADERS.len() as u64, 10.0)
            .header(24.0)
            .gap(1.0),
    );
    declare(&mut grid, &mut source, &rows);
    grid.request_view(ViewSpec::default().with_grouping(Some(grouping())));
    let keymap = Keymap::preset(Preset::Chords);
    let mut menu = HeaderMenu::new();
    let mut mouse = Mouse::default();
    let mut layout = Layout::default();
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
                area.height = full.height.saturating_sub(1);
                // Answers the requests in the queue; moves are refused while
                // grouped, and nothing else is the demo's to act on.
                pump(&mut grid, &mut source.over(&rows), |_, _| {});
                let ctx = RenderContext::new(&themes[theme]).colors(colors);
                layout = begin_frame(&mut grid, &ctx, area, |grid, v| {
                    fill(grid, &mut source, &rows, v)
                });
                let snapshot = grid.snapshot();
                let text = Text {
                    rows: &rows,
                    source: &source,
                    grouping: snapshot.applied_view().grouping.as_ref(),
                };
                render_with(&snapshot, &text, &ctx, area, frame.buffer_mut());
                menu.render(
                    &snapshot,
                    &text,
                    &themes[theme],
                    layout.grid,
                    frame.buffer_mut(),
                );
                let grouped = match source.grouped(Axis::Rows) {
                    true => "grouped",
                    false => "plain",
                };
                let line = ratatui::layout::Rect {
                    y: full.bottom().saturating_sub(1),
                    height: 1,
                    ..full
                };
                let status = format!(
                    "m column menu  r row menu  t theme  q quit | {grouped} | {}",
                    themes[theme].name
                );
                frame.render_widget(ratatui::text::Line::from(status), line);
            })?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let ctrl = key.modifiers == KeyModifiers::CONTROL;
                    if ctrl && matches!(key.code, KeyCode::Char('c' | 'q')) {
                        return Ok(());
                    }
                    if menu.key(&key, &mut grid, |axis, range| {
                        source.window(&rows, axis, range).keys
                    }) {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('t') => theme = (theme + 1) % themes.len(),
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
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
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
