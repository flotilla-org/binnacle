//! The binnacle ratatui demo.
//!
//! `binnacle-demo` shows 1000 generated records behind an In-Memory Table
//! Source: click a column header to sort ascending, descending, then
//! unsorted; Shift-click adds a column to the sort. A header ends in `▲` or
//! `▼` (with its place in a multi-column sort) and `~` while a sort is
//! pending. Focus and picks stay on their records. Drag a column header to
//! move the column (a selected column takes the other selected ones with
//! it); a cyan line shows where it will drop, and widths go with their
//! columns. Press Ctrl+R to show only records with a crew of 6 or more, again
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
//! only after 1.5 s: a Shimmer sweeps across a header while its sort is
//! pending, and the focused cell pulses while its Locate is. The demo redraws
//! on a timer only while something animates, an answer is due or the live
//! table ticks, and otherwise waits for input.
//!
//! In the records (and the slow records), Ctrl+L focuses the quick filter
//! field on the top line: typing keeps only the records with that text in
//! any column, ignoring case, as each key arrives. Alt+C matches case, Alt+R
//! reads the text as a regex, Ctrl+U clears it, Enter keeps the filter and
//! leaves the field, and Esc puts back the filter from before. The status
//! line counts the records shown of the total, such as "84 of 1,000 rows".
//! The quick filter and Ctrl+R's crew filter both apply. A name edited so
//! its record no longer matches stays where it is, dimmed, until the filter
//! changes or Alt+L (or Excel's Ctrl+Alt+L) applies it again; the status
//! line counts such records in view.
//!
//! Alt+Down opens the focused column's filter popup under its header (every
//! records column but the id, and the tickers' name, price and load): type to
//! keep the records whose value in that column contains the text, Alt+N for
//! "does not contain", Alt+C to match case, Alt+R for a regex. Below it, the
//! checklist lists the column's values with their counts, narrowed by the
//! other filters: Tab moves to its search and then the list, Space ticks the
//! value under the cursor, and Ctrl+A ticks "(Select all)" or, while
//! searching, "(Select all matching …)". The port column has blanks, every
//! eleventh record. Ctrl+U clears the field, Enter keeps the conditions and
//! Esc puts back the column's conditions from before.
//! Conditions on several columns AND together and with the quick filter; a
//! filtered column's header shows `▿` (with `~` while the change is
//! pending), and a line above the table lists each condition as a chip.
//!
//! Ctrl+F opens the find bar above the status line: typing marks every
//! matching cell in yellow, and the bar counts them. Enter (or Ctrl+N) steps
//! to the next match and Shift+Enter (or Ctrl+P) to the previous, wrapping
//! round: focus moves to the match, which is drawn in its own colour and
//! ends in `◂`, and the selection stays. Alt+C matches case, Alt+R reads a
//! regex, Alt+B steps by columns instead of rows, Alt+F shows only the rows
//! that match (again to show them all). Esc goes back to the table with find
//! still open, where F3 and Shift+F3 step, Ctrl+F returns to the bar, and Esc
//! closes find. The slow records answer each step late.
//!
//! Typing jumps to the next record whose name starts with what was typed
//! (type-ahead through `set_focus`; pause a second to start over); in the
//! huge table, typing digits goes to that row number. Enter, F2 or a
//! double-click on a name opens a minimal inline editor: type, Backspace,
//! Enter to keep the new name, Esc to drop it. A click elsewhere keeps it.
//!
//! The wheel scrolls, and so do the scrollbars: one down the right edge and,
//! when the columns overflow, one along the bottom. Click a track to page;
//! drag a thumb. The adapter's default keymap moves focus (arrows,
//! PageUp and PageDown, Home and End, Ctrl with an arrow; Shift extends the
//! selection) and the view follows it; Ctrl+A selects all, Ctrl+Space the
//! column, Esc cancels a drag or a resize; Ctrl+G scrolls to the middle and
//! Ctrl+R cycles the crew filter. Click to focus, Shift-click or drag to select,
//! Ctrl-click to pick rows, click a row number to select its row. Drag a
//! column header's border (the line in the one-cell Gap) to resize the
//! column; double-click it to fit the column to what it draws. The bottom
//! line shows the gesture in progress and, on the right, the source, the
//! Theme and the Border Style: Ctrl+T cycles the Themes (`classic`,
//! `catppuccin_mocha`, `mono`), and the demo starts in `mono` when `NO_COLOR`
//! is set. Ctrl+B cycles the Border Styles: header handles only (with or
//! without band rules), column lines, a full grid (which turns on a one-cell
//! row Gap through `set_gap`) or none, in light, heavy and double lines.
//! Column lines and the grid close the table and always draw the band rules;
//! with band rules the column header band gets a second line for its rule. Press Ctrl+Q or Ctrl+C to quit.

use std::borrow::Cow;
use std::io;
use std::time::{Duration, Instant};

use std::collections::HashSet;

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Distinct, DistinctQuery, EditEnd, Effect,
    FindDirection, FindMatch, FindQuery, Grid, Key, Located, Mode, Position, Snapshot, Stamped,
    Support, TextSupport, ViewSpec, Viewport,
};
use binnacle_memory::{FindStep, IdentitySource, MemorySource, PumpSource, Viewed, pump};
use binnacle_ratatui::{
    Align, BarEvent, BarRenderer, BorderExtent, BorderStyle, BorderWeight, CellCanvas,
    CellRenderer, ColorSupport, ColumnFilter, Content, Control, Controls, FindBar, FindStatus,
    Keymap, Layout, Mouse, Preset, QuickFilter, RenderContext, SparklineRenderer, TextRenderer,
    Theme, animating, begin_frame, fit_width, render_conditions, render_editor, render_with,
    row_count, status, wheel,
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
        }
    }
}

/// The minimum crew each Ctrl+R filter keeps, in turn; `None` keeps all.
const CREW_FILTERS: [Option<i64>; 3] = [None, Some(6), Some(13)];

const HEADERS: [&str; 5] = ["id", "name", "weight", "crew", "port"];

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
                if let Some(notice) = self.source.changed(&self.rows, i, col) {
                    grid.notify(Axis::Rows, notice);
                }
            }
        }
        self.since_listing += 1;
        if self.since_listing >= LISTING && self.rows.len() < LISTED {
            self.since_listing = 0;
            let ticker = self.ticker(self.rows.len(), now);
            self.rows.push(ticker);
            grid.notify(Axis::Rows, self.source.appended(&self.rows, &[]));
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
                while let Some(stamped) = grid.poll_effect() {
                    match &stamped.effect {
                        Effect::ViewRequested { .. }
                        | Effect::Locate { .. }
                        | Effect::FindStepRequested { .. } => {
                            held.push((now + SLOW, stamped.effect.clone()));
                        }
                        Effect::MoveRequested { axis, keys, to } => {
                            if let Some(notice) = source.move_items(*axis, keys, *to) {
                                grid.notify(*axis, notice);
                            }
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
            Table::Live(_) => None,
        }
    }

    /// Stores an edited name and tells the Grid what changed.
    fn commit(&mut self, grid: &mut Grid, row: Key, text: String) {
        if let Table::Records { rows, source, .. } = self {
            rows[row.0 as usize].name = text;
            if let Some(notice) = source.edited(rows, row.0 as usize, NAME) {
                grid.notify(Axis::Rows, notice);
            }
        }
    }

    fn name(&self, row: Key) -> Option<String> {
        match self {
            Table::Records { rows, .. } => Some(rows[row.0 as usize].name.clone()),
            Table::Huge(_) | Table::Live(_) => None,
        }
    }

    /// Moves to the next crew filter and tells the Grid.
    fn next_filter(&mut self, grid: &mut Grid) {
        if let Table::Records {
            rows,
            source,
            filter,
            ..
        } = self
        {
            *filter = (*filter + 1) % CREW_FILTERS.len();
            let notice = match CREW_FILTERS[*filter] {
                Some(min) => source.filter(rows, move |r| r.crew >= min),
                None => source.clear_filter(rows),
            };
            grid.notify(Axis::Rows, notice);
        }
    }

    /// The marked cells among `rows` by `cols` and what the find bar shows,
    /// for the Find Query in force.
    fn find(
        &mut self,
        query: Option<&FindQuery>,
        rows_shown: &[Key],
        cols: &[Key],
    ) -> (HashSet<(Key, Key)>, FindStatus) {
        let (
            Table::Records {
                rows, source, step, ..
            },
            Some(query),
        ) = (self, query)
        else {
            return Default::default();
        };
        let marks = source.find_marks(rows, rows_shown, cols, query);
        let marked = (rows_shown.iter())
            .flat_map(|&r| cols.iter().map(move |&c| (r, c)))
            .zip(marks)
            .filter_map(|(cell, marked)| marked.then_some(cell))
            .collect();
        let step = step.as_ref().filter(|(q, _)| q == query).map(|(_, s)| s);
        let status = FindStatus {
            count: Some(source.find_count(rows, query)),
            index: step.and_then(|s| s.index),
            wrapped: step.is_some_and(|s| s.wrapped),
        };
        (marked, status)
    }

    /// The rows among `rows_shown` the source keeps unmatched after an edit.
    fn unmatched(&mut self, rows_shown: &[Key]) -> HashSet<Key> {
        let Table::Records { source, .. } = self else {
            return HashSet::new();
        };
        let flags = source.unmatched(Axis::Rows, rows_shown);
        (rows_shown.iter().zip(flags))
            .filter_map(|(&r, unmatched)| unmatched.then_some(r))
            .collect()
    }

    /// What the body says when no rows are left, with the quick filter
    /// `quick` on or not.
    fn empty(&self, quick: bool) -> &'static str {
        match self {
            Table::Records { filter, .. } if *filter > 0 || quick => "No rows match the filter",
            _ => "No rows",
        }
    }

    fn text(&self) -> Text<'_> {
        match self {
            Table::Records { rows, .. } => Text::Records(rows),
            Table::Huge(_) => Text::Huge,
            Table::Live(live) => Text::Live(&live.rows),
        }
    }

    /// Column `key`'s distinct values for its checklist, as the source lists them.
    fn distinct(&self, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        match self {
            Table::Records { rows, source, .. } => Some(source.distinct(rows, key, query)),
            Table::Huge(_) => None,
            Table::Live(live) => Some(live.source.distinct(&live.rows, key, query)),
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

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        self.inner.move_items(axis, keys, to)
    }
}

/// Answers a View Spec, a Locate or a find step the slow source held, as
/// [`pump`] would have at once.
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
                    _ => r.port.unwrap_or_default().to_string(),
                }
            }
            Text::Huge => format!("r{}c{}", row.0, col.0),
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

    fn row_header(&self, row: Key, position: u64) -> String {
        match self {
            Text::Records(_) | Text::Live(_) => row.0.to_string(),
            Text::Huge => position.saturating_add(1).to_string(),
        }
    }

    fn column_header(&self, col: Key) -> String {
        match self {
            Text::Records(_) => HEADERS[col.0 as usize].to_string(),
            Text::Huge => format!("col {}", col.0),
            Text::Live(_) => TICKER_HEADERS[col.0 as usize].to_string(),
        }
    }

    fn renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
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
/// and the unmatched rows.
struct Marked<'a> {
    text: Text<'a>,
    marks: &'a HashSet<(Key, Key)>,
    unmatched: &'a HashSet<Key>,
}

impl Content for Marked<'_> {
    fn cell(&self, row: Key, col: Key) -> String {
        self.text.cell(row, col)
    }

    fn row_header(&self, row: Key, position: u64) -> String {
        self.text.row_header(row, position)
    }

    fn column_header(&self, col: Key) -> String {
        self.text.column_header(col)
    }

    fn marked(&self, row: Key, col: Key) -> bool {
        self.marks.contains(&(row, col))
    }

    fn unmatched(&self, axis: Axis, key: Key) -> bool {
        axis == Axis::Rows && self.unmatched.contains(&key)
    }

    fn renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        self.text.renderer(col)
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
                    Effect::Activated { row, col } | Effect::EditRequested { row, col } => {
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
    let widest = (snapshot.rows().iter())
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

fn main() -> io::Result<()> {
    let live = std::env::args().any(|a| a == "--live");
    let (mut table, mut grid) = if live {
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
        let source = MemorySource::<Record>::new(rows.len(), HEADERS.len())
            .i64_column(Key(0), |r| Some(r.id))
            .bytes_column(Key(1), |r| Some(Cow::Borrowed(r.name.as_bytes())))
            .f64_column(Key(2), |r| r.weight)
            .i64_column(Key(3), |r| Some(r.crew))
            .bytes_column(Key(4), |r| r.port.map(|p| Cow::Borrowed(p.as_bytes())))
            // The quick filter searches the weight as drawn.
            .text_column(Key(2), |r| {
                Some(r.weight.map_or("-".into(), |w| format!("{w:.1}").into()))
            });
        let mut grid = Grid::new(
            AxisConfig::new(rows.len() as u64, 1.0).header(1.0),
            AxisConfig::new(HEADERS.len() as u64, 14.0)
                .header(5.0)
                .gap(1.0),
        );
        grid.set_quick_filter_support(Axis::Rows, source.quick_filter(Axis::Rows));
        grid.set_find_support(source.find_support());
        for c in 0..HEADERS.len() as u64 {
            grid.set_support(Axis::Rows, Key(c), source.support(Axis::Rows, Key(c)));
        }
        // The ids take no conditions, as on the web.
        let id = (source.support(Axis::Rows, Key(0)))
            .with_text(None)
            .with_one_of(false);
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
    let mut keymap = Keymap::default();
    keymap.bind(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL,
        Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.5,
        },
    );
    let controls = Controls::preset(Preset::Chords);
    let mut quick = QuickFilter::new().placeholder("Ctrl+L");
    let mut popup = ColumnFilter::new();
    let mut find = FindBar::new();
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
                app.pump(&mut table, &mut grid, &shown_rows);
                let field = grid.snapshot().quick_filter(Axis::Rows).is_some();
                let chips = !grid.snapshot().conditions(Axis::Rows).is_empty();
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
                shown_rows.extend(snapshot.rows().iter().filter_map(|r| r.key));
                app.focus = snapshot.focus(Axis::Rows).zip(snapshot.focus(Axis::Cols));
                moving = animating(&snapshot);
                let shown_cols: Vec<Key> = snapshot.cols().iter().filter_map(|c| c.key).collect();
                let (marks, found) = table.find(snapshot.find(), &shown_rows, &shown_cols);
                let unmatched = table.unmatched(&shown_rows);
                let content = Marked {
                    text: table.text(),
                    marks: &marks,
                    unmatched: &unmatched,
                };
                render_with(&snapshot, &content, &ctx, area, frame.buffer_mut());
                if let Some(editor) = &app.editor {
                    render_editor(&snapshot, &editor.text, layout.grid, frame.buffer_mut());
                }
                popup.fetch(&snapshot, |key, query| table.distinct(key, query));
                popup.render(
                    &snapshot,
                    &table.text(),
                    &themes[theme],
                    layout.grid,
                    frame.buffer_mut(),
                );
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
                let mut said = status(&snapshot);
                if !unmatched.is_empty() {
                    if !said.is_empty() {
                        said += " | ";
                    }
                    said += &match unmatched.len() {
                        1 => "1 edited row no longer matches: Alt+L re-applies".to_string(),
                        n => format!("{n} edited rows no longer match: Alt+L re-applies"),
                    };
                }
                frame.render_widget(ratatui::text::Line::from(said), line);
                let names = format!(
                    "{} | {} | {} | {border}",
                    row_count(&snapshot),
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
            let wake = [moving.then_some(FRAME), due, table.until_tick()]
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
                    if app.edit_key(&mut table, &mut grid, key.code)
                        || popup.key(&key, &mut grid)
                        || quick.key(&key, &mut grid)
                        || find.key(&key, &mut grid)
                    {
                        continue;
                    }
                    match controls.control(&key) {
                        Some(Control::QuickFilter) => {
                            quick.focus(&grid.snapshot());
                            continue;
                        }
                        Some(Control::ColumnFilter) => {
                            popup.open(&grid.snapshot());
                            continue;
                        }
                        Some(Control::Find) => {
                            if find.open(&grid.snapshot()) {
                                find.apply(&mut grid);
                            }
                            continue;
                        }
                        Some(Control::FindNext) if find.step(FindDirection::Next, &mut grid) => {
                            continue;
                        }
                        Some(Control::FindPrevious)
                            if find.step(FindDirection::Previous, &mut grid) =>
                        {
                            continue;
                        }
                        Some(Control::Reapply) => {
                            grid.reapply_view();
                            continue;
                        }
                        _ => {}
                    }
                    // Esc on the table closes find while it is open.
                    if key.code == KeyCode::Esc && key.modifiers.is_empty() && find.is_open() {
                        find.close(&mut grid);
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
