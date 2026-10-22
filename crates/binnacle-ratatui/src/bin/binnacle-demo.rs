//! The binnacle ratatui demo.
//!
//! `binnacle-demo` shows 1000 generated records behind an In-Memory Table
//! Source: click a column header to sort ascending, descending, then
//! unsorted; Shift-click adds a column to the sort. A header ends in `▲` or
//! `▼` (with its place in a multi-column sort) and `~` while a sort is
//! pending. Focus and picks stay on their records. Drag a column header to
//! move the column (a selected column takes the other selected ones with
//! it); a cyan line shows where it will drop, and widths go with their
//! columns. Drag a row header to move the record the same way. Moving on a
//! sorted axis keeps the order shown, drops the record there and clears the
//! sort; `--refuse-sorted-moves` instead refuses while sorted, and the
//! status line says why. Press Ctrl+R to show only records with a crew of 6 or more, again
//! for a crew of 13 or more (there are none, so the body says "No rows match
//! the filter"), and again to show them all: focus stays on its record, or
//! moves to the nearest one left in sort order; the topmost record shown
//! that is left stays at the top, and picks of records filtered out are
//! dropped. `binnacle-demo --huge` shows 10^15 identity rows,
//! which neither sort, move nor filter. The records and tickers label their
//! rows with their Keys, which stay with them through sorts and filters; the
//! huge table numbers its rows. Its row-number band fits the
//! largest row number shown and refits as you scroll; dragging the band's
//! edge (the corner cell above its last column) sets it by hand, and
//! double-clicking the edge fits it again. `binnacle-demo --live` shows 200
//! tickers whose prices and loads change four times a second: the price
//! right-aligned, its recent history as a sparkline, the load as a bar, and
//! changed cells flashing, and a new ticker is listed every two seconds up
//! to 240. Each tick sends Change Notices through the In-Memory Table
//! Source, so a sort by price or load holds as values move, and a column
//! filter (Alt+Down) takes each change and each new ticker as it arrives.
//! `binnacle-demo --slow` shows the records with sorts and Locates answered
//! only after 1.5 s: a Shimmer sweeps across a header while its sort or
//! move is pending, and the focused cell pulses while its Locate is. A filter typed
//! there leaves the rows as they are while the status line says
//! "filtering…", and a newer one answers the one waiting at once with the
//! filter in force, so only the last is drawn. The demo redraws
//! on a timer only while something animates, an answer is due or the live
//! table ticks, and otherwise waits for input.
//!
//! `binnacle-demo --transposed` shows the same records on their side, as a
//! pivoted or column-major table has them: each record is a column headed
//! with its id and each field a row, over the records' own In-Memory Table
//! Source seen through `Transposed`. Each field row draws its own way (the
//! id and weight right-aligned, the crew as a bar), and Ctrl+R's crew filter
//! and find work on the records as columns. Row headers are set to sort and
//! column headers to select, the records' settings swapped: click a field's
//! row header to sort the records by it, Shift-click to add a field, and the
//! header shows `▲`, `▼` and `~` as a column header does.
//!
//! In the records (and the slow records), Ctrl+L focuses the quick filter
//! field on the top line: typing keeps only the records with that text in
//! any column, ignoring case, once typing pauses for 150 ms. Alt+C matches
//! case, Alt+R reads the text as a regex, Ctrl+U clears it, Enter keeps the
//! filter and leaves the field, and Esc puts back the filter from before. An
//! invalid regex (try `(` with Alt+R) turns the field red with the reason,
//! keeping the text and the filter from before. The status line counts the
//! records shown of the total, such as "84 of 1,000 rows". The quick filter
//! and Ctrl+R's crew filter both apply. A name edited so its record no longer
//! matches stays where it is, dimmed, until the filter changes or Alt+L (or
//! Excel's Ctrl+Alt+L) applies it again; the status line counts such records
//! in view.
//!
//! Alt+Down opens the focused column's filter popup under its header (every
//! records column but the id, and the tickers' name, price and load): type to
//! keep the records whose value in that column contains the text, Alt+N for
//! "does not contain", Alt+C to match case, Alt+R for a regex. Below it, the
//! checklist lists the column's values with their counts, narrowed by the
//! other filters: Tab moves to its search and then the list, Space ticks the
//! value under the cursor, and Ctrl+A ticks "(Select all)" or, while
//! searching, "(Select all matching …)". The port column has blanks, every
//! eleventh record. On the weight and crew (numbers) and the shipped date,
//! a comparison row sits under the text: Tab to it, Left and Right step
//! from "Any value" through "Equal to" to "At least" and "Between", and the
//! value is typed as a number or a `YYYY-MM-DD` date (Tab again for a
//! Between's high end, both ends included). Try weight "Greater than" 500,
//! or shipped "Between" 2024-04-01 and 2024-06-30. A record with no value
//! never passes a comparison. Ctrl+U clears the field, Enter keeps the
//! conditions and Esc puts back the column's conditions from before.
//! Conditions on several columns AND together and with the quick filter; a
//! filtered column's header shows `▿` (with `~` while the change is
//! pending, `!` while not applied), and a line above the table lists each
//! condition as a chip, red when the source did not apply it.
//!
//! Ctrl+F opens the find bar above the status line: typing marks every
//! matching cell in yellow, and the bar counts them. Enter (or Ctrl+N) steps
//! to the next match and Shift+Enter (or Ctrl+P) to the previous, wrapping
//! round: focus moves to the match, which is drawn in its own colour and
//! ends in `◂`, and the selection stays. Alt+C matches case, Alt+R reads a
//! regex, Alt+B steps by columns instead of rows, Alt+F shows only the rows
//! that match (again to show them all). Esc goes back to the table with find
//! still open, where F3 and Shift+F3 step, Ctrl+F returns to the bar, and Esc
//! closes find. The slow records answer each step late, and pressing Enter
//! again before the first answer moves once. An invalid regex reads "No
//! matches" in red, with the reason. Opening find again selects its text, so
//! typing replaces it.
//!
//! Those are the chords preset's keys, which also give Alt+Shift+F to find
//! backwards, Ctrl+N and Ctrl+P to step from the table where F3 does not reach
//! the demo, Alt+C for find's case, Alt+T and Alt+N to set the focused
//! column's text to contain or not contain, Alt+= to keep only the focused
//! cell's value, Ctrl+D to remove the focused column's conditions (or the
//! last one) and Ctrl+K to clear every filter. `--preset modes` picks vim
//! and VisiData keys instead, with typing off and `h j k l` moving: `/` and
//! `?` find forwards and backwards, `n` and `N` step, `|` the quick filter,
//! `\` and `&` the column's text, `F` its popup, `=` keep the value, `u` and
//! `U` remove and clear, `i` case, `m` show only matching rows, `r` re-apply.
//! `--preset bare-keys` uses single keys with typing off: `/ ? n N`, `\` the
//! quick filter, `|` and `&` the column's text, `x` and `X` remove and clear,
//! `m`, `i`, `=`, `r`, and `f` opens a filter drawer down the right that
//! applies as you tick; Enter or Esc gives the table focus back, and the
//! drawer then follows the focused column until `f` focuses it again or Esc
//! closes it. The status line names the preset's main keys while it has
//! nothing else to say.
//!
//! Alt+, pins the focused column to the left (`!` in modes, `<` in bare
//! keys): it stays after the row numbers, past a `║` edge, while the other
//! columns scroll behind it, and it keeps its place through sorts and
//! filters. Pinning more columns adds each after the last. The same key on a
//! pinned column unpins it, as does Alt+U (`u` in bare keys). Alt+. (`>`)
//! pins it right, flush with the far edge; Alt+< and Alt+> (`[` and `]`)
//! pin the focused row to the top or bottom, past a `═` edge, and
//! Alt+Shift+U (`U` in bare keys) unpins it. Alt+W (`z`) freezes the
//! columns from the first to the focused one, as many as fit, and
//! Alt+Shift+W (`Z`) the rows. The arrow keys step through pinned and
//! scrolling items in the order they are drawn. Pinned columns (or rows)
//! that would leave fewer than a few to scroll are clipped at their inside
//! end, the right ones first, with the edge where they clip; nothing unpins,
//! and focusing a clipped one slides its Strip to show it. A pinned column
//! that a filter or a hide takes out waits, and comes back to its place.
//! Alt+M (or Shift+F10) opens the focused column's header menu and
//! Alt+Shift+M the row's: `l` and `r` pin a column left and right (`t` and
//! `b` a row to the top and bottom), `u` unpins a pinned one, clipped or
//! not, and `f` freezes up to it, or the cursor and Enter do.
//!
//! In the records (slow too) and on their side, Alt+0 hides the focused
//! column and Alt+9 its row: the source leaves it out whatever the filter
//! keeps, focus moves to its neighbour, and its width stays for when it comes
//! back. There the header menu goes on: `h` hides the item, and the hidden
//! ones are listed under it, `1` to `9` (or the cursor and Enter) showing
//! one again in its place and `a` all of them; Alt+) and Alt+( open it at
//! that list. The status line
//! counts what is left, "4 of 5 columns". In the modes and bare-keys presets
//! `-` and `_` hide, `+` and `*` list the hidden ones, and `.` and `,` open
//! the menus.
//!
//! Typing jumps to the next record whose name starts with what was typed
//! (type-ahead through `set_focus`; pause a second to start over); in the
//! huge table, typing digits goes to that row number. Enter, F2 or a
//! double-click on a name opens a minimal inline editor: type, Backspace,
//! Enter to keep the new name, Esc to drop it. A click elsewhere keeps it.
//!
//! The wheel scrolls, and so do the scrollbars: one down the right edge and,
//! when the columns overflow, one along the bottom. Click a track to page;
//! drag a thumb. The adapter's keymap moves focus (arrows, PageUp and
//! PageDown, Alt+PageUp and Alt+PageDown a page of columns, Home and End the
//! first and last row, Ctrl with an arrow; Shift extends the selection) and
//! the view follows it; Ctrl+A selects all, Ctrl+Space the column and
//! Shift+Space the row (`V` too in the modes and bare keys presets, since few
//! terminals report Shift with Space), Esc cancels a drag or a resize;
//! Ctrl+G scrolls to the middle and Ctrl+R cycles the crew filter. Click to
//! focus, Shift-click or drag to select, Ctrl-click a cell to pick its row
//! and Ctrl-click a header to pick its row or column, click a row number to
//! select its row. Alt+S cycles what a click selects: the cell, its row or
//! its column (where Ctrl-click on a cell picks the column). The status line
//! counts the picks on each axis. Drag a
//! column header's border (the line in the one-cell Gap) to resize the
//! column; double-click it to fit the column to what it draws. The bottom
//! line shows the gesture in progress and, on the right, the source, the
//! Theme and the Border Style: Ctrl+T cycles the Themes (`classic`,
//! `catppuccin_mocha`, `mono`), and the demo starts in `mono` when `NO_COLOR`
//! is set. Ctrl+B cycles the Border Styles: header handles only (with or
//! without band rules), column lines, a full grid (which turns on a one-cell
//! row Gap through `set_gap`) or none, in light, heavy and double lines.
//! Column lines and the grid close the table and always draw the band rules;
//! with band rules the column header band gets a second line for its rule.
//!
//! Ctrl+S saves the layout (the sort, filters and hidden items, the pins,
//! the widths and heights set by hand and the header bands' thicknesses) and
//! Ctrl+O puts it back, in any table: the status line says which. Restored
//! in the slow records, the sort and filters show pending until the late
//! answer, while pins, sizes and bands come back at once. The saved layout
//! lasts as long as the demo runs. Press Ctrl+Q or Ctrl+C to quit.

use std::borrow::Cow;
use std::io;
use std::time::{Duration, Instant};

use std::collections::HashSet;

use binnacle_core::{
    Action, Axis, AxisConfig, ConditionKind, Distinct, DistinctQuery, Edge, EditEnd, Effect,
    FindDirection, FindMatch, FindQuery, Grid, HeaderClick, Key, Located, Mode, Position, Scalar,
    SelectionUnit, Snapshot, Stamped, Support, TextSupport, Totals, Value, ViewSpec, Viewport,
};
use binnacle_memory::{
    FindStep, IdentitySource, MemorySource, MovePolicy, Moved, PumpSource, Transposed, Viewed,
    answer_move, pump, query_error, transpose_find,
};
use binnacle_ratatui::{
    Align, BarEvent, BarRenderer, BorderExtent, BorderStyle, BorderWeight, CellCanvas,
    CellRenderer, Chooser, ColorSupport, ColumnFilter, Content, Control, Controls, FindBar,
    FindStatus, HeaderMenu, Hid, Keymap, Layout, Mouse, Preset, QuickFilter, RenderContext,
    SavedLayout, SparklineRenderer, TextRenderer, Theme, animating, begin_frame, cap_hint,
    clear_filters, count, fit_width, freeze_focused, hide_chosen, keep_value, remove_conditions,
    render_conditions, render_editor, render_with, restore_layout, row_count, save_layout, status,
    toggle_pin, unpin_focused, wheel,
};

/// The most records the demo's source hides, fewer than the Grid's own cap.
const RECORD_HIDE_CAP: usize = 100;

/// The Control that pins the focused column left, for the keys line.
const PIN_LEFT: Control = Control::Pin {
    axis: Axis::Cols,
    edge: Edge::Leading,
};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;

const RECORDS: usize = 1000;
const HUGE_ROWS: u64 = 1_000_000_000_000_000;
const HUGE_COLS: u64 = 12;

/// The Border Styles Ctrl+B cycles through, from today's look.
const BORDERS: [BorderStyle; 9] = {
    use BorderExtent::{Columns, Grid, Headers, None};
    use BorderWeight::{Double, Heavy, Light};
    [
        BorderStyle::new(Headers, Light),
        BorderStyle::new(Headers, Light).band_rules(true),
        BorderStyle::new(Columns, Light),
        BorderStyle::new(Columns, Heavy),
        BorderStyle::new(Columns, Double),
        BorderStyle::new(Grid, Light),
        BorderStyle::new(Grid, Heavy),
        BorderStyle::new(Grid, Double),
        BorderStyle::new(None, Light),
    ]
};

const NAMES: [&str; 8] = [
    "anchor", "bosun", "capstan", "davit", "ensign", "fathom", "galley", "hawser",
];

const PORTS: [&str; 9] = [
    "Aberdeen",
    "Bergen",
    "Cádiz",
    "Dunedin",
    "Esbjerg",
    "Funchal",
    "Gdańsk",
    "Halifax",
    "Île d'Yeu",
];

struct Record {
    id: i64,
    name: String,
    /// Missing for every seventh record, to show nulls sorting last.
    weight: Option<f64>,
    crew: i64,
    /// Missing for every eleventh record, to show blanks in its checklist.
    port: Option<&'static str>,
    /// When it shipped, a day in 2024 in milliseconds since the Unix epoch;
    /// missing for every thirteenth record.
    shipped: Option<i64>,
}

impl Record {
    fn generate(i: usize) -> Record {
        // A cheap scramble so the columns are not already in order.
        let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
        Record {
            id: i as i64,
            name: format!("{} {}", NAMES[(h % 8) as usize], h % 97),
            weight: (!i.is_multiple_of(7)).then(|| (h % 10_000) as f64 / 10.0),
            crew: (h % 13) as i64,
            port: (!i.is_multiple_of(11)).then(|| PORTS[((h >> 4) % 9) as usize]),
            shipped: (!i.is_multiple_of(13)).then(|| {
                let Scalar::Date(start) = Scalar::date(2024, 1, 1) else {
                    unreachable!("a date")
                };
                start + ((h >> 8) % 366) as i64 * 86_400_000
            }),
        }
    }
}

/// The minimum crew each Ctrl+R filter keeps, in turn; `None` keeps all.
const CREW_FILTERS: [Option<i64>; 3] = [None, Some(6), Some(13)];

const HEADERS: [&str; 6] = ["id", "name", "weight", "crew", "port", "shipped"];

const TICKERS: usize = 200;
/// The live table lists a new ticker every this many ticks, up to [`LISTED`].
const LISTING: u32 = 8;
const LISTED: usize = 240;
const TICKER_HEADERS: [&str; 4] = ["ticker", "price", "trend", "load"];
/// How many prices a ticker's sparkline remembers.
const HISTORY: usize = 24;
const TICK: Duration = Duration::from_millis(250);

/// A row of the live table.
struct Ticker {
    name: String,
    price: f64,
    /// Recent prices, oldest first, ending with `price`.
    history: Vec<f64>,
    /// Between 0 and 1.
    load: f64,
    /// When the price and load last changed, on the demo's clock.
    changed: Duration,
}

/// The live table: tickers whose values change every [`TICK`].
struct Live {
    rows: Vec<Ticker>,
    source: Box<MemorySource<Ticker>>,
    /// A xorshift state, so runs repeat.
    seed: u64,
    next_tick: Instant,
    /// Ticks since the last ticker was listed.
    since_listing: u32,
}

impl Live {
    fn new() -> Live {
        let mut live = Live {
            rows: Vec::new(),
            source: Box::new(
                MemorySource::<Ticker>::new(TICKERS, TICKER_HEADERS.len())
                    .bytes_column(Key(0), |t| Some(Cow::Borrowed(t.name.as_bytes())))
                    .f64_column(Key(1), |t| Some(t.price))
                    .f64_column(Key(3), |t| Some(t.load)),
            ),
            seed: 0x2545_F491_4F6C_DD1D,
            next_tick: Instant::now(),
            since_listing: 0,
        };
        live.rows = (0..TICKERS)
            .map(|i| live.ticker(i, Duration::ZERO))
            .collect();
        live
    }

    /// The `i`th ticker, at a random price and load, changed at `now`.
    fn ticker(&mut self, i: usize, now: Duration) -> Ticker {
        let price = 10.0 + self.random() * 990.0;
        Ticker {
            name: format!("{} {}", NAMES[i % 8].to_uppercase(), i / 8),
            price,
            history: vec![price],
            load: self.random(),
            changed: now,
        }
    }

    /// A number in `0..1`.
    fn random(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Moves about a quarter of the tickers, at `now` on the demo's clock,
    /// now and then lists a new one, and tells the Grid what changed: the
    /// filter in force takes each change and each new ticker as it arrives.
    fn tick(&mut self, grid: &mut Grid, now: Duration) {
        self.next_tick += TICK;
        for i in 0..self.rows.len() {
            if self.random() >= 0.25 {
                continue;
            }
            let (step, load) = (self.random() - 0.5, self.random() - 0.5);
            let t = &mut self.rows[i];
            t.price = (t.price * (1.0 + step * 0.04)).max(0.01);
            t.history.push(t.price);
            if t.history.len() > HISTORY {
                t.history.remove(0);
            }
            t.load = (t.load + load * 0.2).clamp(0.0, 1.0);
            t.changed = now;
            for col in [Key(1), Key(3)] {
                for (axis, notice) in self.source.changed(&self.rows, i, col) {
                    grid.notify(axis, notice);
                }
            }
        }
        self.since_listing += 1;
        if self.since_listing >= LISTING && self.rows.len() < LISTED {
            self.since_listing = 0;
            let ticker = self.ticker(self.rows.len(), now);
            self.rows.push(ticker);
            for (axis, notice) in self.source.appended(&self.rows, &[]) {
                grid.notify(axis, notice);
            }
        }
    }
}

/// Draws another renderer, then flashes the cell if its ticker just changed.
struct Flash<'a, R> {
    inner: R,
    rows: &'a [Ticker],
}

impl<R: CellRenderer> CellRenderer for Flash<'_, R> {
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        self.inner.draw(row, col, canvas);
        canvas.flash(self.rows[row.0 as usize].changed);
    }

    fn preferred_width(&self, row: Key, col: Key) -> u32 {
        self.inner.preferred_width(row, col)
    }
}

/// The one editable column.
const NAME: Key = Key(1);

/// How long a pause starts type-ahead over.
const TYPE_AHEAD: Duration = Duration::from_secs(1);

/// How long the slow source holds back its View Spec and Locate answers.
const SLOW: Duration = Duration::from_millis(1500);

/// How often the demo redraws while something animates.
const FRAME: Duration = Duration::from_millis(33);

/// View Specs and Locates the slow source has not answered yet, each
/// with the time its answer is due.
type Held = Vec<(Instant, Effect)>;

/// The table the demo shows: records, or a huge identity table.
enum Table {
    Records {
        rows: Vec<Record>,
        source: Box<MemorySource<Record>>,
        /// The index into [`CREW_FILTERS`] in force.
        filter: usize,
        /// `Some` for the slow source.
        held: Option<Held>,
        /// The Find Query of the last find step, and where it landed.
        step: Option<(FindQuery, FindStep)>,
    },
    Huge(Huge),
    Live(Live),
    /// The records on their side: each record a column, each field a row.
    Transposed {
        rows: Vec<Record>,
        source: Box<MemorySource<Record>>,
        /// The index into [`CREW_FILTERS`] in force.
        filter: usize,
        /// The Find Query of the last find step, as the source saw it, and
        /// where it landed.
        step: Option<(FindQuery, FindStep)>,
    },
}

/// The huge table: Keys are Positions on both axes.
struct Huge {
    rows: IdentitySource,
    cols: IdentitySource,
}

impl Huge {
    fn axis(&self, axis: Axis) -> &IdentitySource {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }
}

impl PumpSource for Huge {
    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.axis(axis).locate(key)
    }
}

impl Table {
    fn window(&self, axis: Axis, range: std::ops::Range<u64>) -> Vec<Key> {
        match self {
            Table::Records { source, .. } => source.window(axis, range),
            Table::Huge(huge) => huge.axis(axis).window(range),
            Table::Live(live) => live.source.window(axis, range),
            Table::Transposed { source, .. } => source.window(axis.other(), range),
        }
    }

    /// Drains the Grid's Effects, answering View Specs, column moves and `Locate`
    /// from the source and auto-size by measuring the text drawn for `shown`
    /// rows. Returns the Effects the app itself acts on: activation, edits
    /// and typing.
    fn pump(&mut self, grid: &mut Grid, shown: &[Key]) -> Vec<Effect> {
        let mut app = Vec::new();
        let mut seen = |text: &Text, grid: &mut Grid, stamped: Stamped| match stamped.effect {
            Effect::Activated { .. }
            | Effect::EditRequested { .. }
            | Effect::EditCancelled { .. }
            | Effect::TextTyped { .. }
            | Effect::BandSizeChanged { .. }
            | Effect::BandAutoSizeRequested { .. } => app.push(stamped.effect),
            _ => auto_size(text, shown, grid, stamped),
        };
        match self {
            Table::Records {
                rows,
                source,
                held: Some(held),
                step,
                ..
            } => {
                let text = Text::Records(rows);
                let now = Instant::now();
                let mut source = Stepped {
                    inner: source.over(rows),
                    step,
                };
                let (due, later) = held.drain(..).partition(|&(at, _)| at <= now);
                *held = later;
                for (_, effect) in due {
                    answer(&mut source, grid, effect);
                }
                for axis in Axis::ALL {
                    if let Some(movable) = source.movable(axis) {
                        grid.set_movable(axis, movable);
                    }
                }
                while let Some(stamped) = grid.poll_effect() {
                    match &stamped.effect {
                        Effect::ViewRequested { .. } => {
                            // A newer View Spec aborts the one waiting, which
                            // answers with the spec in force.
                            let applied = grid.snapshot().applied_view().clone();
                            held.retain(|(_, e)| {
                                let waiting = matches!(e, Effect::ViewRequested { .. });
                                if waiting {
                                    let snapshot = grid.snapshot();
                                    let totals = Totals {
                                        rows: snapshot.total(Axis::Rows),
                                        cols: snapshot.total(Axis::Cols),
                                    };
                                    grid.view_applied(applied.clone(), totals);
                                }
                                !waiting
                            });
                            held.push((now + SLOW, stamped.effect.clone()));
                        }
                        Effect::Locate { .. }
                        | Effect::MoveRequested { .. }
                        | Effect::FindStepRequested { .. } => {
                            held.push((now + SLOW, stamped.effect.clone()));
                        }
                        _ => {}
                    }
                    seen(&text, grid, stamped);
                }
            }
            Table::Records {
                rows, source, step, ..
            } => {
                let text = Text::Records(rows);
                let mut source = Stepped {
                    inner: source.over(rows),
                    step,
                };
                pump(grid, &mut source, |grid, stamped| {
                    seen(&text, grid, stamped)
                });
            }
            Table::Huge(huge) => {
                pump(grid, huge, |grid, stamped| seen(&Text::Huge, grid, stamped));
            }
            Table::Live(Live { rows, source, .. }) => {
                let text = Text::Live(rows);
                source.pump(rows, grid, |grid, stamped| seen(&text, grid, stamped));
            }
            Table::Transposed {
                rows, source, step, ..
            } => {
                let text = Text::Transposed(rows);
                let mut source = Transposed(Stepped {
                    inner: source.over(rows),
                    step,
                });
                pump(grid, &mut source, |grid, stamped| {
                    seen(&text, grid, stamped)
                });
            }
        }
        app
    }

    /// The source's name, as its command-line flag has it.
    fn source(&self) -> &'static str {
        match self {
            Table::Records { held: None, .. } => "records",
            Table::Records { held: Some(_), .. } => "slow",
            Table::Huge(_) => "huge",
            Table::Live(_) => "live",
            Table::Transposed { .. } => "transposed",
        }
    }

    /// When the slow source's next held answer is due.
    fn next_answer(&self) -> Option<Instant> {
        match self {
            Table::Records {
                held: Some(held), ..
            } => held.iter().map(|&(at, _)| at).min(),
            _ => None,
        }
    }

    /// The row type-ahead lands on for `typed`: in the records, the first
    /// whose name starts with it (ignoring case), searching from Position
    /// `from` and wrapping; in the huge table, the row numbered `typed`.
    fn type_ahead(&self, typed: &str, from: u64) -> Option<Key> {
        match self {
            Table::Records { rows, source, .. } => {
                let count = source.count(Axis::Rows);
                let order = source.window(Axis::Rows, 0..count);
                let typed = typed.to_lowercase();
                let matches = |k: &Key| rows[k.0 as usize].name.to_lowercase().starts_with(&typed);
                let start = from.min(count) as usize;
                let (after, before) = order.split_at(start);
                after.iter().chain(before).copied().find(matches)
            }
            Table::Huge(huge) => {
                let n: u64 = typed.parse().ok()?;
                (1..=huge.rows.count()).contains(&n).then(|| Key(n - 1))
            }
            Table::Live(_) | Table::Transposed { .. } => None,
        }
    }

    /// Stores an edited name and tells the Grid what changed.
    fn commit(&mut self, grid: &mut Grid, row: Key, text: String) {
        if let Table::Records { rows, source, .. } = self {
            rows[row.0 as usize].name = text;
            for (axis, notice) in source.edited(rows, row.0 as usize, NAME) {
                grid.notify(axis, notice);
            }
        }
    }

    fn name(&self, row: Key) -> Option<String> {
        match self {
            Table::Records { rows, .. } => Some(rows[row.0 as usize].name.clone()),
            Table::Huge(_) | Table::Live(_) | Table::Transposed { .. } => None,
        }
    }

    /// Moves to the next crew filter and tells the Grid: the records are
    /// its rows, or on their side its columns.
    fn next_filter(&mut self, grid: &mut Grid) {
        let (rows, source, filter, axis) = match self {
            Table::Records {
                rows,
                source,
                filter,
                ..
            } => (rows, source, filter, Axis::Rows),
            Table::Transposed {
                rows,
                source,
                filter,
                ..
            } => (rows, source, filter, Axis::Cols),
            Table::Huge(_) | Table::Live(_) => return,
        };
        *filter = (*filter + 1) % CREW_FILTERS.len();
        let notice = match CREW_FILTERS[*filter] {
            Some(min) => source.filter(rows, move |r| r.crew >= min),
            None => source.clear_filter(rows),
        };
        grid.notify(axis, notice);
    }

    /// The marked cells among `rows` by `cols` and what the find bar shows,
    /// for the Find Query in force.
    fn find(
        &mut self,
        query: Option<&FindQuery>,
        rows_shown: &[Key],
        cols: &[Key],
    ) -> (HashSet<(Key, Key)>, FindStatus) {
        let Some(asked) = query else {
            return Default::default();
        };
        // On their side, the source is asked about its own rows and columns
        // and the marks are turned back.
        let (rows, source, step, query, turned) = match self {
            Table::Records {
                rows, source, step, ..
            } => (rows, source, step, asked.clone(), false),
            Table::Transposed {
                rows, source, step, ..
            } => match transpose_find(asked) {
                Some(query) => (rows, source, step, query, true),
                None => return Default::default(),
            },
            Table::Huge(_) | Table::Live(_) => return Default::default(),
        };
        let (along, across) = if turned {
            (cols, rows_shown)
        } else {
            (rows_shown, cols)
        };
        let marks = source.find_marks(rows, along, across, &query);
        let marked = (along.iter())
            .flat_map(|&r| across.iter().map(move |&c| (r, c)))
            .zip(marks)
            .filter_map(|(cell, marked)| marked.then_some(cell))
            .map(|(r, c)| if turned { (c, r) } else { (r, c) })
            .collect();
        let query = &query;
        let step = step.as_ref().filter(|(q, _)| q == query).map(|(_, s)| s);
        let status = FindStatus {
            count: Some(source.find_count(rows, query)),
            index: step.and_then(|s| s.index),
            wrapped: step.is_some_and(|s| s.wrapped),
            finding: false,
            error: query_error(&query.query),
        };
        (marked, status)
    }

    /// The rows among `rows_shown` and the columns among `cols_shown` the
    /// source keeps unmatched after an edit.
    fn unmatched(&mut self, rows_shown: &[Key], cols_shown: &[Key]) -> [HashSet<Key>; 2] {
        let Table::Records { source, .. } = self else {
            return Default::default();
        };
        [(Axis::Rows, rows_shown), (Axis::Cols, cols_shown)].map(|(axis, shown)| {
            let flags = source.unmatched(axis, shown);
            (shown.iter().zip(flags))
                .filter_map(|(&k, unmatched)| unmatched.then_some(k))
                .collect()
        })
    }

    /// What the body says when no rows are left, with the quick filter
    /// `quick` on or not.
    fn empty(&self, quick: bool) -> &'static str {
        match self {
            Table::Records { filter, .. } if *filter > 0 || quick => "No rows match the filter",
            Table::Transposed { filter, .. } if *filter > 0 || quick => {
                "No columns match the filter"
            }
            _ => "No rows",
        }
    }

    /// The counts for the status line: the records' (the rows, or on their
    /// side the columns), then the other axis's while it is filtered, such
    /// as "120 of 1,000 rows, 3 of 5 columns".
    fn counts(&self, snapshot: &Snapshot) -> String {
        let (records, other) = match self {
            Table::Transposed { .. } => (
                count(snapshot, Axis::Cols, ("record", "records")),
                (Axis::Rows, ("field", "fields")),
            ),
            _ => (row_count(snapshot), (Axis::Cols, ("column", "columns"))),
        };
        let (axis, noun) = other;
        let filtered = snapshot
            .total(axis)
            .is_some_and(|t| t != snapshot.count(axis));
        match filtered {
            true => format!("{records}, {}", count(snapshot, axis, noun)),
            false => records,
        }
    }

    fn text(&self) -> Text<'_> {
        match self {
            Table::Records { rows, .. } => Text::Records(rows),
            Table::Huge(_) => Text::Huge,
            Table::Live(live) => Text::Live(&live.rows),
            Table::Transposed { rows, .. } => Text::Transposed(rows),
        }
    }

    /// The distinct values of `key`, an item of the other axis, across
    /// `axis` for its checklist, as the source lists them.
    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        match self {
            Table::Records { rows, source, .. } => Some(source.distinct(rows, axis, key, query)),
            Table::Transposed { rows, source, .. } => {
                Transposed(source.over(rows)).distinct(axis, key, query)
            }
            Table::Huge(_) => None,
            Table::Live(live) => Some(live.source.distinct(&live.rows, axis, key, query)),
        }
    }

    /// The Value a OneOf on column `col` keeps row `row` by, as the source
    /// names it.
    fn value(&self, row: Key, col: Key) -> Option<Value> {
        match self {
            Table::Records { rows, source, .. } => {
                Some(source.value(col, rows.get(row.0 as usize)?))
            }
            Table::Huge(_) | Table::Transposed { .. } => None,
            Table::Live(live) => Some(live.source.value(col, live.rows.get(row.0 as usize)?)),
        }
    }

    /// How long until the table next changes by itself, if it ever does.
    fn until_tick(&self) -> Option<Duration> {
        match self {
            Table::Live(live) => Some(live.next_tick.saturating_duration_since(Instant::now())),
            _ => None,
        }
    }
}

/// A Table Source that keeps where its last find step landed, for the find
/// bar's "3 of 120" and wrap note.
struct Stepped<'a, S> {
    inner: S,
    step: &'a mut Option<(FindQuery, FindStep)>,
}

impl<S: PumpSource> PumpSource for Stepped<'_, S> {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.inner.view(spec)
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.inner.support(axis, key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.inner.quick_filter(axis)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        self.inner.distinct(axis, key, query)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.inner.locate(axis, key)
    }

    fn find_support(&mut self) -> Option<TextSupport> {
        self.inner.find_support()
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        let step = self.inner.find_next(query, from, direction);
        *self.step = step.map(|s| (query.clone(), s));
        step
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        self.inner.movable(axis)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        self.inner.move_items(axis, keys, to)
    }
}

/// Answers a View Spec, a Locate, a move or a find step the slow source
/// held, as [`pump`] would have at once.
fn answer(source: &mut impl PumpSource, grid: &mut Grid, effect: Effect) {
    match effect {
        Effect::ViewRequested { spec, .. } => {
            let viewed = source.view(&spec);
            grid.view_applied(viewed.applied, viewed.totals);
            for (axis, notice) in viewed.notices {
                grid.notify(axis, notice);
            }
        }
        Effect::Locate { axis, key } => {
            let located = source.locate(axis, key);
            grid.located(axis, key, located);
        }
        Effect::MoveRequested { axis, keys, to } => answer_move(grid, source, axis, &keys, to),
        Effect::FindStepRequested {
            query,
            from,
            direction,
        } => {
            let step = source.find_next(&query, from, direction);
            grid.find_stepped(step.map(|s| s.at));
        }
        _ => {}
    }
}

/// Answers `AutoSizeRequested` for a column with the widest it draws: the
/// `shown` rows' preferred widths and its header.
fn auto_size(text: &Text, shown: &[Key], grid: &mut Grid, stamped: Stamped) {
    if let Effect::AutoSizeRequested { axis, key } = stamped.effect {
        let width = match axis {
            Axis::Cols => fit_width(text, shown, key).max(1),
            Axis::Rows => 1,
        };
        grid.set_size(axis, key, width as f32);
    }
}

/// The text a [`Table`] draws, borrowed apart from its source.
enum Text<'a> {
    Records(&'a [Record]),
    Huge,
    Live(&'a [Ticker]),
    /// The records on their side.
    Transposed(&'a [Record]),
}

impl Content for Text<'_> {
    fn cell(&self, row: Key, col: Key) -> String {
        match self {
            Text::Records(rows) => {
                let r = &rows[row.0 as usize];
                match col.0 {
                    0 => r.id.to_string(),
                    1 => r.name.clone(),
                    2 => r.weight.map_or_else(|| "-".into(), |w| format!("{w:.1}")),
                    3 => r.crew.to_string(),
                    4 => r.port.unwrap_or_default().to_string(),
                    _ => r
                        .shipped
                        .map(|d| Scalar::Date(d).to_string())
                        .unwrap_or_default(),
                }
            }
            Text::Huge => format!("r{}c{}", row.0, col.0),
            Text::Transposed(rows) => Text::Records(rows).cell(col, row),
            Text::Live(rows) => {
                let t = &rows[row.0 as usize];
                match col.0 {
                    0 => t.name.clone(),
                    1 => format!("{:.2}", t.price),
                    2 => String::new(),
                    _ => format!("{:.0}%", t.load * 100.0),
                }
            }
        }
    }

    /// A record's row is named by its name, a ticker's by its symbol.
    fn row_label(&self, row: Key) -> String {
        match self {
            Text::Records(_) => self.cell(row, Key(1)),
            Text::Live(_) => self.cell(row, Key(0)),
            Text::Huge | Text::Transposed(_) => self.row_header(row, row.0),
        }
    }

    fn row_header(&self, row: Key, position: u64) -> String {
        match self {
            Text::Records(_) | Text::Live(_) => row.0.to_string(),
            Text::Huge => position.saturating_add(1).to_string(),
            Text::Transposed(_) => HEADERS[row.0 as usize].to_string(),
        }
    }

    fn column_header(&self, col: Key) -> String {
        match self {
            Text::Records(_) => HEADERS[col.0 as usize].to_string(),
            Text::Huge => format!("col {}", col.0),
            Text::Live(_) => TICKER_HEADERS[col.0 as usize].to_string(),
            Text::Transposed(_) => col.0.to_string(),
        }
    }

    /// On their side, the records draw by field, which is the row; every
    /// other table draws by column.
    fn renderer(&self, row: Key, col: Key) -> Box<dyn CellRenderer + '_> {
        let Text::Transposed(rows) = *self else {
            return self.column_renderer(col);
        };
        let text = TextRenderer::new(move |row, col| self.cell(row, col));
        match row.0 {
            0 | 2 => Box::new(text.align(Align::Right)),
            3 => Box::new(BarRenderer::new(move |_, col: Key| {
                rows[col.0 as usize].crew as f64 / 12.0
            })),
            _ => Box::new(text),
        }
    }

    fn column_renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        let text = TextRenderer::new(move |row, col| self.cell(row, col));
        let Text::Live(rows) = *self else {
            return Box::new(text);
        };
        match col.0 {
            1 => Box::new(Flash {
                inner: text.align(Align::Right),
                rows,
            }),
            2 => Box::new(SparklineRenderer::new(move |row: Key, _| {
                &rows[row.0 as usize].history[..]
            })),
            3 => Box::new(Flash {
                inner: BarRenderer::new(move |row: Key, _| rows[row.0 as usize].load),
                rows,
            }),
            _ => Box::new(text),
        }
    }
}

/// The text a [`Table`] draws, with the cells that match the Find Query
/// and the unmatched rows and columns.
struct Marked<'a> {
    text: Text<'a>,
    marks: &'a HashSet<(Key, Key)>,
    unmatched: &'a [HashSet<Key>; 2],
}

impl Content for Marked<'_> {
    fn cell(&self, row: Key, col: Key) -> String {
        self.text.cell(row, col)
    }

    fn row_header(&self, row: Key, position: u64) -> String {
        self.text.row_header(row, position)
    }

    fn row_label(&self, row: Key) -> String {
        self.text.row_label(row)
    }

    fn column_header(&self, col: Key) -> String {
        self.text.column_header(col)
    }

    fn marked(&self, row: Key, col: Key) -> bool {
        self.marks.contains(&(row, col))
    }

    fn unmatched(&self, axis: Axis, key: Key) -> bool {
        self.unmatched[axis.index()].contains(&key)
    }

    fn renderer(&self, row: Key, col: Key) -> Box<dyn CellRenderer + '_> {
        self.text.renderer(row, col)
    }
}

/// The host's own editor: the edited cell and the text so far.
struct Editor {
    row: Key,
    col: Key,
    text: String,
}

/// What the demo app keeps beside the Grid: its editor and type-ahead.
#[derive(Default)]
struct App {
    editor: Option<Editor>,
    typed: String,
    typed_at: Option<Instant>,
    /// The focused cell as of the last Snapshot.
    focus: Option<(binnacle_core::FocusEnd, binnacle_core::FocusEnd)>,
    /// Whether the row-number band follows the row numbers shown: from a
    /// double-click on its edge until the user drags it.
    fit_rows: bool,
    /// The Border Style drawn, an index into [`BORDERS`].
    border: usize,
    /// The layout Ctrl+S saved, which Ctrl+O restores.
    saved: Option<SavedLayout>,
    /// What the status line says of the last save or restore, until the
    /// next key.
    layout_said: Option<&'static str>,
}

impl App {
    /// The column header band's fitted thickness: one line for the headers,
    /// and one more for a band rule.
    fn header_lines(&self) -> f32 {
        if BORDERS[self.border].draws_band_rules() {
            2.0
        } else {
            1.0
        }
    }

    /// Moves to the next Border Style: a row Gap of 1 for `Grid` and 0
    /// otherwise, and the column header band refitted when band rules turn
    /// on or off.
    fn next_border(&mut self, grid: &mut Grid) {
        let was = BORDERS[self.border];
        self.border = (self.border + 1) % BORDERS.len();
        let border = BORDERS[self.border];
        let gap = if border.extent == BorderExtent::Grid {
            1.0
        } else {
            0.0
        };
        grid.set_gap(Axis::Rows, gap);
        if border.draws_band_rules() != was.draws_band_rules() {
            grid.set_header(Axis::Rows, self.header_lines());
        }
    }

    /// Pumps until the queue stays empty, acting on what the app owns:
    /// activation and F2 open the editor on a name, `EditCancelled` drops
    /// it, and typing jumps ahead.
    fn pump(&mut self, table: &mut Table, grid: &mut Grid, shown: &[Key]) {
        loop {
            let effects = table.pump(grid, shown);
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                match effect {
                    Effect::Activated { row, col, .. } | Effect::EditRequested { row, col } => {
                        if let (NAME, Some(text)) = (col, table.name(row)) {
                            grid.edit_started(row, col);
                            self.editor = Some(Editor { row, col, text });
                        }
                    }
                    Effect::EditCancelled { .. } => self.editor = None,
                    Effect::BandAutoSizeRequested { axis: Axis::Cols } => self.fit_rows = true,
                    Effect::BandAutoSizeRequested { axis: Axis::Rows } => {
                        grid.set_header(Axis::Rows, self.header_lines())
                    }
                    Effect::BandSizeChanged {
                        axis: Axis::Cols, ..
                    } => self.fit_rows = false,
                    Effect::TextTyped { text } => self.type_ahead(table, grid, &text),
                    _ => {}
                }
            }
        }
    }

    fn type_ahead(&mut self, table: &Table, grid: &mut Grid, text: &str) {
        let now = Instant::now();
        let fresh = self.typed_at.is_none_or(|t| now - t > TYPE_AHEAD);
        if fresh {
            self.typed.clear();
        }
        self.typed.push_str(text);
        self.typed_at = Some(now);
        // A new search starts after the focused row; a longer prefix may
        // still match it.
        let (row, col) = self.focus.map_or((0, NAME), |(r, c)| {
            (r.position + u64::from(fresh), c.key.unwrap_or(NAME))
        });
        if let Some(found) = table.type_ahead(&self.typed, row) {
            grid.set_focus(found, col);
        }
    }

    /// Takes a key press while the editor is open; false if it is closed.
    fn edit_key(&mut self, table: &mut Table, grid: &mut Grid, code: KeyCode) -> bool {
        let Some(editor) = &mut self.editor else {
            return false;
        };
        match code {
            KeyCode::Char(c) => editor.text.push(c),
            KeyCode::Backspace => {
                editor.text.pop();
            }
            KeyCode::Enter => self.commit(table, grid),
            KeyCode::Esc => {
                self.editor = None;
                grid.edit_ended(EditEnd::Cancel);
            }
            _ => {}
        }
        true
    }

    /// Keeps the editor's text, if it is open, and closes it.
    fn commit(&mut self, table: &mut Table, grid: &mut Grid) {
        if let Some(Editor { row, col, text }) = self.editor.take() {
            debug_assert_eq!(col, NAME);
            grid.edit_ended(EditEnd::Commit);
            table.commit(grid, row, text);
        }
    }
}

/// Asks for the Windows the next Snapshot needs and fills them.
fn fill(grid: &mut Grid, table: &Table, viewport: Viewport) {
    for (axis, range) in grid.begin_frame(viewport) {
        let keys = table.window(axis, range.clone());
        grid.set_window(axis, range.start, &keys);
    }
}

/// The row-number band's width for `snapshot`: the widest row header label
/// shown, and a cell after it for the band rule (or a blank when none is
/// drawn).
fn row_band_fit(snapshot: &Snapshot, content: &impl Content) -> Option<f32> {
    let widest = (snapshot.drawn(Axis::Rows))
        .map(|r| {
            let label = r.key.map_or_else(
                || r.position.saturating_add(1).to_string(),
                |k| content.row_header(k, r.position),
            );
            label.chars().count()
        })
        .max()?;
    Some(widest as f32 + 1.0)
}

/// What the status line says of the selection: the picks on each axis and
/// what a click selects, unless it is the cell; empty otherwise, else ending
/// in a separator.
fn selecting(snapshot: &Snapshot, unit: SelectionUnit) -> String {
    let mut parts = Vec::new();
    for (axis, noun) in [(Axis::Rows, "row"), (Axis::Cols, "column")] {
        match snapshot.picked(axis).len() {
            0 => {}
            1 => parts.push(format!("1 {noun} picked")),
            n => parts.push(format!("{n} {noun}s picked")),
        }
    }
    match unit {
        SelectionUnit::Rows => parts.push("clicks select rows".into()),
        SelectionUnit::Cols => parts.push("clicks select columns".into()),
        _ => {}
    }
    match parts.is_empty() {
        true => String::new(),
        false => parts.join(", ") + " | ",
    }
}

/// The keymap preset `--preset` names (`chords`, `modes` or `bare-keys`).
fn preset() -> Result<Preset, String> {
    let args: Vec<String> = std::env::args().collect();
    let named = (args.iter().enumerate()).find_map(|(i, a)| match a.strip_prefix("--preset") {
        Some("") => Some(args.get(i + 1).cloned().unwrap_or_default()),
        Some(rest) => rest.strip_prefix('=').map(str::to_string),
        None => None,
    });
    match named.as_deref() {
        None | Some("chords") => Ok(Preset::Chords),
        Some("modes") => Ok(Preset::Modes),
        Some("bare-keys") => Ok(Preset::BareKeys),
        Some(other) => Err(format!(
            "unknown preset {other:?}: use chords, modes or bare-keys"
        )),
    }
}

/// The records' In-Memory Table Source: every column sorts and filters, and
/// records and fields move.
fn records_source(rows: &[Record]) -> MemorySource<Record> {
    let policy = if std::env::args().any(|a| a == "--refuse-sorted-moves") {
        MovePolicy::Refuse
    } else {
        MovePolicy::Bake
    };
    MemorySource::<Record>::new(rows.len(), HEADERS.len())
        .with_move_policy(policy)
        .i64_column(Key(0), |r| Some(r.id))
        .bytes_column(Key(1), |r| Some(Cow::Borrowed(r.name.as_bytes())))
        .f64_column(Key(2), |r| r.weight)
        .i64_column(Key(3), |r| Some(r.crew))
        .bytes_column(Key(4), |r| r.port.map(|p| Cow::Borrowed(p.as_bytes())))
        .date_column(Key(5), |r| r.shipped)
        // The quick filter searches the weight as drawn.
        .text_column(Key(2), |r| {
            Some(r.weight.map_or("-".into(), |w| format!("{w:.1}").into()))
        })
}

/// The records on their side, each record a column and each field a row.
/// Support is declared from the turned source on both axes, and row headers
/// sort while column headers select, the records' settings swapped.
fn transposed() -> (Table, Grid) {
    let rows: Vec<Record> = (0..RECORDS).map(Record::generate).collect();
    let mut source = records_source(&rows);
    let mut grid = Grid::new(
        AxisConfig::new(HEADERS.len() as u64, 1.0).header(1.0),
        AxisConfig::new(rows.len() as u64, 14.0)
            .header(7.0)
            .gap(1.0),
    );
    grid.set_header_click(Axis::Rows, HeaderClick::Sort);
    grid.set_header_click(Axis::Cols, HeaderClick::Select);
    {
        let mut turned = Transposed(source.over(&rows));
        for axis in Axis::ALL {
            grid.set_quick_filter_support(axis, turned.quick_filter(axis));
            grid.set_hide_support(axis, turned.hide_support(axis));
        }
        // The demo's source takes fewer hidden records than the Grid would,
        // so hiding many of them suggests a filter.
        grid.set_hide_cap(Axis::Cols, RECORD_HIDE_CAP);
        grid.set_find_support(turned.find_support());
        for f in 0..HEADERS.len() as u64 {
            grid.set_support(Axis::Cols, Key(f), turned.support(Axis::Cols, Key(f)));
        }
        for r in 0..rows.len() as u64 {
            grid.set_support(Axis::Rows, Key(r), turned.support(Axis::Rows, Key(r)));
        }
    }
    let table = Table::Transposed {
        rows,
        source: Box::new(source),
        filter: 0,
        step: None,
    };
    (table, grid)
}

fn main() -> io::Result<()> {
    let preset = preset().map_err(io::Error::other)?;
    let live = std::env::args().any(|a| a == "--live");
    let (mut table, mut grid) = if std::env::args().any(|a| a == "--transposed") {
        transposed()
    } else if live {
        let grid = Grid::new(
            AxisConfig::new(TICKERS as u64, 1.0).header(1.0),
            AxisConfig::new(TICKER_HEADERS.len() as u64, 12.0)
                .header(5.0)
                .gap(1.0),
        );
        (Table::Live(Live::new()), grid)
    } else if std::env::args().any(|a| a == "--huge") {
        // The columns fit the widest row number; the row-number band starts
        // there and then fits the rows shown.
        let digits = HUGE_ROWS.to_string().len() as f32;
        let mut grid = Grid::new(
            AxisConfig::new(HUGE_ROWS, 1.0).header(1.0),
            AxisConfig::new(HUGE_COLS, digits + 4.0)
                .header(digits + 1.0)
                .gap(1.0),
        );
        for c in 0..HUGE_COLS {
            let none = Support::default().with_sort(false).with_text(None);
            grid.set_support(Axis::Rows, Key(c), none);
        }
        let table = Table::Huge(Huge {
            rows: IdentitySource::new(HUGE_ROWS),
            cols: IdentitySource::new(HUGE_COLS),
        });
        (table, grid)
    } else {
        let rows: Vec<Record> = (0..RECORDS).map(Record::generate).collect();
        let source = records_source(&rows);
        let mut grid = Grid::new(
            AxisConfig::new(rows.len() as u64, 1.0).header(1.0),
            AxisConfig::new(HEADERS.len() as u64, 14.0)
                .header(5.0)
                .gap(1.0),
        );
        for axis in Axis::ALL {
            grid.set_quick_filter_support(axis, source.quick_filter(axis));
            grid.set_hide_support(axis, source.hide_support(axis));
        }
        grid.set_hide_cap(Axis::Rows, RECORD_HIDE_CAP);
        grid.set_find_support(source.find_support());
        for c in 0..HEADERS.len() as u64 {
            grid.set_support(Axis::Rows, Key(c), source.support(Axis::Rows, Key(c)));
        }
        // A record's values filter the columns too.
        for r in 0..rows.len() as u64 {
            grid.set_support(Axis::Cols, Key(r), source.support(Axis::Cols, Key(r)));
        }
        // The ids take no conditions, as on the web.
        let id = (source.support(Axis::Rows, Key(0)))
            .with_text(None)
            .with_one_of(false)
            .with_compare(None);
        grid.set_support(Axis::Rows, Key(0), id);
        let source = Box::new(source);
        let slow = std::env::args().any(|a| a == "--slow");
        let table = Table::Records {
            rows,
            source,
            filter: 0,
            held: slow.then(Vec::new),
            step: None,
        };
        (table, grid)
    };
    let mut keymap = Keymap::preset(preset);
    keymap.bind(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL,
        Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.5,
        },
    );
    let controls = Controls::preset(preset);
    let hint = |control| controls.hint(control).unwrap_or_default();
    // On their side, the quick filter narrows the records, now the columns.
    let mut quick = QuickFilter::new().placeholder(hint(Control::QuickFilter));
    if let Table::Transposed { .. } = table {
        quick = quick.on(Axis::Cols);
    }
    let mut popup = match preset {
        Preset::BareKeys => ColumnFilter::new().drawer(),
        _ => ColumnFilter::new(),
    };
    let mut menu = HeaderMenu::new();
    let mut chooser = Chooser::new();
    // A hide past the cap's suggestion, until the next key.
    let mut capped: Option<String> = None;
    let mut find = match preset {
        Preset::Chords => FindBar::new(),
        _ => FindBar::new().table_hints(format!(
            "{} next  {} prev  {} edit",
            hint(Control::FindNext),
            hint(Control::FindPrevious),
            hint(Control::Find)
        )),
    };
    let preset_name = match preset {
        Preset::Chords => "chords",
        Preset::Modes => "modes",
        _ => "bare keys",
    };
    // What the status line says while nothing else is to be said.
    let keys = format!(
        "{} find  {} filter  {} column  {} row  {} clear  {} pin  {} hide  {} menu  {} choose",
        hint(Control::Find),
        hint(Control::QuickFilter),
        hint(Control::ColumnFilter),
        hint(Control::RowFilter),
        hint(Control::Clear),
        hint(PIN_LEFT),
        hint(Control::Hide { axis: Axis::Cols }),
        hint(Control::Menu { axis: Axis::Cols }),
        hint(Control::Chooser { axis: Axis::Cols })
    );
    // What a click selects, which Alt+S cycles.
    let mut unit = SelectionUnit::Cells;
    let mut mouse = Mouse::default();
    let mut layout = Layout::default();
    // The row Keys last drawn, which auto-size measures.
    let mut shown_rows: Vec<Key> = Vec::new();
    let mut app = App {
        fit_rows: matches!(table, Table::Huge(_)),
        ..App::default()
    };
    let themes = Theme::presets();
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let mut theme = if no_color { themes.len() - 1 } else { 0 };
    let colors = ColorSupport::from_colorterm(std::env::var("COLORTERM").ok().as_deref());
    if let Table::Live(live) = &table {
        for c in 0..TICKER_HEADERS.len() as u64 {
            let support = live.source.support(Axis::Rows, Key(c));
            grid.set_support(Axis::Rows, Key(c), support);
        }
    }
    // The demo's clock, which the render context and the flashes share.
    let start = Instant::now();
    // Whether the last frame drawn animates.
    let mut moving = false;
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                // The Grid takes all but the bottom line, the status line,
                // the top line while the rows have a quick filter, and the
                // next while they have column conditions.
                let full = frame.area();
                let mut area = full;
                area.height = full.height.saturating_sub(1 + u16::from(find.is_open()));
                // A filter drawer takes the table's right.
                area.width = area.width.saturating_sub(popup.width());
                let now = Instant::now();
                quick.tick(&mut grid, now);
                popup.tick(&mut grid, now);
                find.tick(&mut grid, now);
                app.pump(&mut table, &mut grid, &shown_rows);
                // The source's reasons for what it did not apply.
                let snapshot = grid.snapshot();
                let wanted = snapshot.requested_view().unwrap_or(snapshot.applied_view());
                let asked = wanted.axis(quick.axis()).filter.quick.as_ref();
                quick.set_error(asked.and_then(query_error));
                let item = popup.filtering().zip(popup.item()).and_then(|(axis, key)| {
                    wanted
                        .axis(axis)
                        .filter
                        .conditions
                        .on(key)
                        .find_map(|c| match &c.kind {
                            ConditionKind::Text(q) => query_error(q),
                            _ => None,
                        })
                });
                popup.set_error(item);
                let field = grid.snapshot().quick_filter(quick.axis()).is_some();
                let chips = Axis::ALL
                    .into_iter()
                    .any(|axis| !grid.snapshot().conditions(axis).is_empty());
                for _ in 0..u16::from(field) + u16::from(chips) {
                    area.y += 1;
                    area.height = area.height.saturating_sub(1);
                }
                let border = BORDERS[app.border];
                let ctx = RenderContext::new(&themes[theme])
                    .border(border)
                    .colors(colors)
                    .at(start.elapsed())
                    .empty(table.empty(!quick.text().is_empty()));
                let fill = |grid: &mut Grid, viewport| fill(grid, &table, viewport);
                layout = begin_frame(&mut grid, &ctx, area, fill);
                if app.fit_rows {
                    // Fit to the rows this frame shows, then lay it out again.
                    let snapshot = grid.snapshot();
                    let fit = row_band_fit(&snapshot, &table.text())
                        .filter(|_| !matches!(snapshot.mode(), Mode::ResizingBand { .. }));
                    if let Some(width) = fit.filter(|&w| w != snapshot.header(Axis::Cols)) {
                        grid.set_header(Axis::Cols, width);
                        layout = begin_frame(&mut grid, &ctx, area, fill);
                    }
                }
                let snapshot = grid.snapshot();
                shown_rows.clear();
                shown_rows.extend(snapshot.drawn(Axis::Rows).filter_map(|r| r.key));
                app.focus = snapshot.focus(Axis::Rows).zip(snapshot.focus(Axis::Cols));
                moving = animating(&snapshot);
                let shown_cols: Vec<Key> =
                    snapshot.drawn(Axis::Cols).filter_map(|c| c.key).collect();
                let (marks, found) = table.find(snapshot.find(), &shown_rows, &shown_cols);
                let unmatched = table.unmatched(&shown_rows, &shown_cols);
                let content = Marked {
                    text: table.text(),
                    marks: &marks,
                    unmatched: &unmatched,
                };
                render_with(&snapshot, &content, &ctx, area, frame.buffer_mut());
                if let Some(editor) = &app.editor {
                    render_editor(&snapshot, &editor.text, layout.grid, frame.buffer_mut());
                }
                popup.fetch(&snapshot, |axis, key, query| {
                    table.distinct(axis, key, query)
                });
                let at = match popup.width() {
                    0 => layout.grid,
                    width => ratatui::layout::Rect {
                        x: area.right(),
                        width,
                        ..area
                    },
                };
                popup.render(
                    &snapshot,
                    &table.text(),
                    &themes[theme],
                    at,
                    frame.buffer_mut(),
                );
                let buf = frame.buffer_mut();
                menu.render(&snapshot, &table.text(), &themes[theme], layout.grid, buf);
                chooser.fetch(&snapshot, &table.text(), |axis, range| {
                    table.window(axis, range)
                });
                chooser.render(&snapshot, &themes[theme], layout.grid, buf);
                if field {
                    let top = ratatui::layout::Rect { height: 1, ..full };
                    quick.render(&snapshot, &themes[theme], top, frame.buffer_mut());
                }
                if chips {
                    let line = ratatui::layout::Rect {
                        y: area.y - 1,
                        height: 1,
                        ..full
                    };
                    let buf = frame.buffer_mut();
                    render_conditions(&snapshot, &table.text(), &themes[theme], line, buf);
                }
                if find.is_open() {
                    let bar = ratatui::layout::Rect {
                        y: area.y + area.height,
                        height: 1,
                        ..full
                    };
                    find.render(&snapshot, &found, &themes[theme], bar, frame.buffer_mut());
                }
                let line = ratatui::layout::Rect {
                    y: full.bottom().saturating_sub(1),
                    height: 1,
                    ..full
                };
                let mut said = capped.clone().unwrap_or_else(|| status(&snapshot));
                if let Some(layout_said) = app.layout_said {
                    if !said.is_empty() {
                        said += " | ";
                    }
                    said += layout_said;
                }
                let [rows, cols] = [&unmatched[0], &unmatched[1]].map(|u| u.len());
                if rows + cols > 0 {
                    if !said.is_empty() {
                        said += " | ";
                    }
                    let again = hint(Control::Reapply);
                    let named = |n: usize, noun: &str| match n {
                        0 => None,
                        1 => Some(format!("1 {noun}")),
                        n => Some(format!("{n} {noun}s")),
                    };
                    let edited = [named(rows, "row"), named(cols, "column")];
                    let edited: Vec<String> = edited.into_iter().flatten().collect();
                    let verb = if rows + cols == 1 { "matches" } else { "match" };
                    said += &format!(
                        "{} edited no longer {verb}: {again} re-applies",
                        edited.join(" and ")
                    );
                }
                if said.is_empty() {
                    said = keys.clone();
                }
                frame.render_widget(ratatui::text::Line::from(said), line);
                let names = format!(
                    "{}{} | {} | {preset_name} | {} | {border}",
                    selecting(&snapshot, unit),
                    table.counts(&snapshot),
                    table.source(),
                    themes[theme].name
                );
                let name = ratatui::text::Line::from(names).right_aligned();
                frame.render_widget(name, line);
            })?;
            // Wait for input, but only until the next frame while something
            // animates, until a held answer is due, or until the live table ticks.
            let due = table
                .next_answer()
                .map(|at| at.saturating_duration_since(Instant::now()));
            let typed = [quick.deadline(), popup.deadline(), find.deadline()]
                .into_iter()
                .flatten()
                .min()
                .map(|at| at.saturating_duration_since(Instant::now()));
            let wake = [moving.then_some(FRAME), due, table.until_tick(), typed]
                .into_iter()
                .flatten()
                .min();
            if let Some(timeout) = wake
                && !event::poll(timeout)?
            {
                if let Table::Live(live) = &mut table
                    && live.next_tick <= Instant::now()
                {
                    live.tick(&mut grid, start.elapsed());
                }
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let ctrl = key.modifiers == KeyModifiers::CONTROL;
                    if ctrl && matches!(key.code, KeyCode::Char('c' | 'q')) {
                        return Ok(());
                    }
                    capped = None;
                    app.layout_said = None;
                    if app.edit_key(&mut table, &mut grid, key.code)
                        || chooser.key(&key, &mut grid)
                        || menu.key(&key, &mut grid, |axis, range| table.window(axis, range))
                        || popup.key(&key, &mut grid)
                        || quick.key(&key, &mut grid)
                        || find.key(&key, &mut grid)
                    {
                        if let Some(axis) = menu.take_chooser() {
                            chooser.open(&grid.snapshot(), axis);
                        }
                        continue;
                    }
                    let took = match controls.control(&key) {
                        Some(Control::QuickFilter) => quick.focus(&grid.snapshot()),
                        Some(Control::ColumnFilter) => popup.open(&grid.snapshot()),
                        Some(Control::RowFilter) => popup.open_row(&grid.snapshot()),
                        Some(Control::ColumnText { not }) => popup.open_text(&mut grid, not),
                        Some(Control::KeepValue) => {
                            let snapshot = grid.snapshot();
                            let focus = snapshot.focus(Axis::Rows).zip(snapshot.focus(Axis::Cols));
                            let cell = focus.and_then(|(r, c)| r.key.zip(c.key));
                            let value = cell.and_then(|(r, c)| Some((c, table.value(r, c)?)));
                            value.is_some_and(|(col, value)| keep_value(&mut grid, col, value))
                        }
                        Some(Control::Remove) => {
                            remove_conditions(&mut grid);
                            true
                        }
                        Some(Control::Clear) => {
                            clear_filters(&mut grid);
                            true
                        }
                        Some(Control::Find) => {
                            if find.open(&grid.snapshot()) {
                                find.apply(&mut grid);
                            }
                            true
                        }
                        Some(Control::FindBackward) => {
                            if find.open_backward(&grid.snapshot()) {
                                find.apply(&mut grid);
                            }
                            true
                        }
                        Some(Control::FindNext) => find.repeat(false, &mut grid),
                        Some(Control::FindPrevious) => find.repeat(true, &mut grid),
                        Some(Control::OnlyMatching) => {
                            find.only_matching(&mut grid);
                            true
                        }
                        Some(Control::Case) => {
                            find.toggle_case(&mut grid);
                            true
                        }
                        Some(Control::Reapply) => {
                            grid.reapply_view();
                            true
                        }
                        Some(Control::Pin { axis, edge }) => {
                            toggle_pin(&mut grid, axis, edge);
                            true
                        }
                        Some(Control::Unpin { axis }) => {
                            unpin_focused(&mut grid, axis);
                            true
                        }
                        Some(Control::Hide { axis }) => {
                            let keys_at = |axis, range| table.window(axis, range);
                            if let Hid::Capped(cap) = hide_chosen(&mut grid, axis, keys_at) {
                                capped = Some(cap_hint(axis, cap));
                            }
                            true
                        }
                        Some(Control::Chooser { axis }) => chooser.open(&grid.snapshot(), axis),
                        Some(Control::ShowHidden { axis }) => {
                            menu.open_hidden(&grid.snapshot(), axis)
                        }
                        Some(Control::Menu { axis }) => menu.open(&grid.snapshot(), axis),
                        Some(Control::Freeze { axis }) => {
                            freeze_focused(&mut grid, axis);
                            true
                        }
                        // Esc closes the drawer, else find, else cancels.
                        Some(Control::Close) if popup.close() => true,
                        Some(Control::Close) if find.is_open() => {
                            find.close(&mut grid);
                            true
                        }
                        _ => false,
                    };
                    if took {
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('t') {
                        theme = (theme + 1) % themes.len();
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('b') {
                        app.next_border(&mut grid);
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('r') {
                        table.next_filter(&mut grid);
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('s') {
                        app.saved = Some(save_layout(&mut grid));
                        app.layout_said = Some("layout saved");
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('o') {
                        app.layout_said = Some(match &app.saved {
                            Some(saved) => {
                                restore_layout(&mut grid, saved);
                                // The band keeps the restored thickness.
                                app.fit_rows = false;
                                "layout restored"
                            }
                            None => "no layout saved: Ctrl+S saves one",
                        });
                        continue;
                    }
                    if key.modifiers == KeyModifiers::ALT && key.code == KeyCode::Char('s') {
                        unit = match unit {
                            SelectionUnit::Cells => SelectionUnit::Rows,
                            SelectionUnit::Rows => SelectionUnit::Cols,
                            _ => SelectionUnit::Cells,
                        };
                        grid.set_selection_unit(unit);
                        continue;
                    }
                    if let Some(action) = keymap.action(&key) {
                        grid.action(action);
                    }
                }
                Event::Mouse(m) => {
                    let bar = mouse.scrollbar(&m, &grid.snapshot(), &layout);
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
                    } else if let BarEvent::Took(action) = bar {
                        if let Some(action) = action {
                            grid.action(action);
                        }
                    } else if let Some(pointer) = mouse.event(&m, layout.grid, Instant::now()) {
                        if pointer.kind == binnacle_core::PointerKind::Down {
                            // A press would cancel the edit: keep it first.
                            app.commit(&mut table, &mut grid);
                        }
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
