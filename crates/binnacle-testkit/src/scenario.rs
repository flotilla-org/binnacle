use std::cell::RefCell;
use std::fmt::Write;
use std::ops::Range;

use binnacle_core::{
    Axis, AxisConfig, ChangeNotice, Distinct, DistinctQuery, Effect, FindDirection, FindMatch,
    FindQuery, Grid, Key, Located, Modifiers, PointerEvent, Position, Snapshot, Stamped, Support,
    ViewSpec, Viewport,
};
use binnacle_memory::{FindStep, IdentitySource, MemorySource, PumpSource, Transposed, Viewed};

use crate::render::{Rendered, view_text};

/// What a scenario's host fetches Keys from, on top of the Effects it answers
/// through the shared pump.
pub trait ScenarioSource: PumpSource {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key>;

    /// Content sizes for the Keys just fetched on `axis`, with the smallest
    /// content size the axis can have. `None` (the default) sends Keys only.
    fn sizes(&mut self, _axis: Axis, _keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        None
    }
}

/// A table whose Keys are their Positions on both axes.
pub struct IdentityTable {
    pub rows: IdentitySource,
    pub cols: IdentitySource,
}

impl IdentityTable {
    fn axis(&self, axis: Axis) -> &IdentitySource {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }
}

impl ScenarioSource for IdentityTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.axis(axis).window(range)
    }
}

impl PumpSource for IdentityTable {
    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.axis(axis).locate(key)
    }
}

/// A table holding the Key at every Position on each axis, so items can be
/// inserted and removed anywhere. New items get Keys never used before.
pub struct KeyedTable {
    pub axes: [Vec<Key>; 2],
    next: u64,
}

impl KeyedTable {
    /// `rows` rows keyed 0.. and `cols` columns keyed 0.. (the axes' Keys
    /// are independent).
    pub fn new(rows: u64, cols: u64) -> Self {
        KeyedTable {
            axes: [(0..rows).map(Key).collect(), (0..cols).map(Key).collect()],
            next: rows.max(cols),
        }
    }

    fn keys(&mut self, axis: Axis) -> &mut Vec<Key> {
        &mut self.axes[axis.index()]
    }

    /// Inserts `n` new items before Position `at` (clamped to the count);
    /// returns the Change Notice the Grid needs.
    pub fn insert(&mut self, axis: Axis, at: Position, n: u64) -> ChangeNotice {
        let at = at.min(self.axes[axis.index()].len() as u64);
        let fresh: Vec<Key> = (self.next..self.next + n).map(Key).collect();
        self.next += n;
        let keys = self.keys(axis);
        keys.splice(at as usize..at as usize, fresh);
        ChangeNotice::Inserted { at, n }
    }

    /// Removes the `n` items from Position `at` (both clipped to the count);
    /// returns the Change Notice the Grid needs.
    pub fn remove(&mut self, axis: Axis, at: Position, n: u64) -> ChangeNotice {
        let keys = self.keys(axis);
        let len = keys.len() as u64;
        let at = at.min(len);
        let n = n.min(len - at);
        keys.drain(at as usize..(at + n) as usize);
        ChangeNotice::Removed { at, n }
    }
}

impl KeyedTable {
    /// Reverses the order of `axis`; returns the Change Notice the Grid
    /// needs.
    pub fn reverse(&mut self, axis: Axis) -> ChangeNotice {
        self.keys(axis).reverse();
        ChangeNotice::Reordered
    }
}

impl ScenarioSource for KeyedTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        let keys = &self.axes[axis.index()];
        let end = (range.end as usize).min(keys.len());
        let start = (range.start as usize).min(end);
        keys[start..end].to_vec()
    }
}

impl PumpSource for KeyedTable {
    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        match self.axes[axis.index()].iter().position(|&k| k == key) {
            Some(p) => Located::Found(p as u64),
            None => Located::Gone { suggest: None },
        }
    }
}

/// A row of a [`RecordTable`]: one integer per column, `None` for null.
pub type Record = Vec<Option<i64>>;

/// A table of integer records behind an In-Memory Table Source, keyed by
/// Logical Index on both axes. Every column sorts unless left out; columns
/// move.
pub struct RecordTable {
    pub rows: Vec<Record>,
    pub source: MemorySource<Record>,
}

impl RecordTable {
    /// A table of `rows`, each `cols` wide, sortable by the columns in
    /// `sortable`.
    pub fn new(rows: Vec<Record>, cols: usize, sortable: impl IntoIterator<Item = u64>) -> Self {
        let mut source = MemorySource::<Record>::new(rows.len(), cols);
        for c in sortable {
            source = source.i64_column(Key(c), move |r: &Record| r[c as usize]);
        }
        RecordTable { rows, source }
    }

    /// Keeps only the records for which `keep` is true; the Grid needs the
    /// returned Change Notice.
    pub fn filter(&mut self, keep: impl Fn(&Record) -> bool + 'static) -> ChangeNotice {
        self.source.filter(&self.rows, keep)
    }

    /// Appends `records` to the base data; the Grid needs the returned
    /// Change Notice.
    pub fn append(&mut self, records: Vec<Record>) -> ChangeNotice {
        self.rows.extend(records);
        self.source.appended(&self.rows, &[])
    }

    /// Sets base row `row`'s value in column `col` as the user's edit (a
    /// row in view stays even if the filter now drops it); the Grid needs
    /// the returned Change Notice, if any.
    pub fn edit(&mut self, row: usize, col: usize, value: Option<i64>) -> Option<ChangeNotice> {
        self.rows[row][col] = value;
        self.source.edited(&self.rows, row, Key(col as u64))
    }

    /// Replaces the base data with `rows`; the Grid needs the returned
    /// Change Notice.
    pub fn replace(&mut self, rows: Vec<Record>) -> ChangeNotice {
        self.rows = rows;
        self.source.replaced(&self.rows, None)
    }
}

impl ScenarioSource for RecordTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.source.window(axis, range)
    }
}

impl PumpSource for RecordTable {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.source.over(&self.rows).view(spec)
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.source.support(axis, key)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        self.source.over(&self.rows).distinct(axis, key, query)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.source.over(&self.rows).locate(axis, key)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        self.source.over(&self.rows).move_items(axis, keys, to)
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        self.source.find_next(&self.rows, query, from, direction)
    }
}

/// Any scenario table on its side: the Grid's rows are the inner table's
/// columns and its columns the inner table's rows.
impl<S: ScenarioSource> ScenarioSource for Transposed<S> {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.0.window(axis.other(), range)
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        self.0.sizes(axis.other(), keys)
    }
}

/// A [`RecordTable`] on its side, as a pivoted or transposed host has it:
/// each record is a column and each field a row, keyed by Logical Index on
/// both axes. The columns sort by the sortable fields, filter and change as
/// the records' rows do; the fields move. Change Notices from its methods
/// are for [`Axis::Cols`].
pub struct ColumnMajorTable {
    table: Transposed<RecordTable>,
}

impl ColumnMajorTable {
    /// A table of `records` as columns, each `fields` long, its columns
    /// sortable by the fields in `sortable`.
    pub fn new(
        records: Vec<Record>,
        fields: usize,
        sortable: impl IntoIterator<Item = u64>,
    ) -> Self {
        ColumnMajorTable {
            table: Transposed(RecordTable::new(records, fields, sortable)),
        }
    }

    /// The records, one per column, in base order.
    pub fn records(&self) -> &[Record] {
        &self.table.0.rows
    }

    /// The records' table, the right way up.
    pub fn row_major(&mut self) -> &mut RecordTable {
        &mut self.table.0
    }

    /// Keeps only the records for which `keep` is true.
    pub fn filter(&mut self, keep: impl Fn(&Record) -> bool + 'static) -> ChangeNotice {
        self.table.0.filter(keep)
    }

    /// Appends `records` as new columns.
    pub fn append(&mut self, records: Vec<Record>) -> ChangeNotice {
        self.table.0.append(records)
    }

    /// Sets record `record`'s value in field `field` as the user's edit (a
    /// column in view stays even if the filter now drops it).
    pub fn edit(
        &mut self,
        record: usize,
        field: usize,
        value: Option<i64>,
    ) -> Option<ChangeNotice> {
        self.table.0.edit(record, field, value)
    }

    /// Replaces the records with `records`.
    pub fn replace(&mut self, records: Vec<Record>) -> ChangeNotice {
        self.table.0.replace(records)
    }
}

impl ScenarioSource for ColumnMajorTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.table.window(axis, range)
    }
}

impl PumpSource for ColumnMajorTable {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.table.view(spec)
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.table.support(axis, key)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        self.table.distinct(axis, key, query)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.table.locate(axis, key)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        self.table.move_items(axis, keys, to)
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        self.table.find_next(query, from, direction)
    }
}

/// A scenario's source seen by the shared pump, logging each answer.
struct Answering<'a, S> {
    source: &'a mut S,
    log: &'a RefCell<String>,
}

impl<S: ScenarioSource> PumpSource for Answering<'_, S> {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        let answer = self.source.view(spec);
        let _ = writeln!(
            self.log.borrow_mut(),
            "  view: applied {}, notices {:?}",
            view_text(&answer.applied),
            answer.notices
        );
        answer
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.source.support(axis, key)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        let answer = self.source.locate(axis, key);
        let _ = writeln!(
            self.log.borrow_mut(),
            "  locate {axis:?} {key:?}: {answer:?}"
        );
        answer
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        let answer = self.source.move_items(axis, keys, to);
        let _ = writeln!(
            self.log.borrow_mut(),
            "  move {axis:?} {keys:?} to {to}: {answer:?}"
        );
        answer
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        let answer = self.source.find_next(query, from, direction);
        let _ = writeln!(self.log.borrow_mut(), "  find: {answer:?}");
        answer
    }
}

/// A host in miniature: a Grid, its Table Source and a log of what passed
/// between them since the last step.
pub struct Scenario<S> {
    pub grid: Grid,
    pub source: S,
    log: String,
    steps: usize,
    /// The last rendering, which the next leaves out what matches.
    shown: Option<Rendered>,
}

impl Scenario<IdentityTable> {
    /// A Grid over an identity Table Source with the configured counts.
    pub fn identity(rows: AxisConfig, cols: AxisConfig) -> Self {
        let source = IdentityTable {
            rows: IdentitySource::new(rows.count),
            cols: IdentitySource::new(cols.count),
        };
        Scenario::new(Grid::new(rows, cols), source)
    }
}

impl<S: ScenarioSource> Scenario<S> {
    pub fn new(grid: Grid, source: S) -> Self {
        Scenario {
            grid,
            source,
            log: String::new(),
            steps: 0,
            shown: None,
        }
    }

    /// Pushes input into the Grid. Input calls return nothing; their Effects are
    /// drained by the next [`Scenario::pump`].
    pub fn input(&mut self, f: impl FnOnce(&mut Grid)) {
        f(&mut self.grid);
    }

    /// Drains the Effect queue through the shared pump, which answers View
    /// Specs and `Locate` from the source; logs each Effect and answer and
    /// returns the Effects in order.
    pub fn pump(&mut self) -> Vec<Stamped> {
        let mut drained = Vec::new();
        let answers = RefCell::new(String::new());
        let mut source = Answering {
            source: &mut self.source,
            log: &answers,
        };
        let log = &mut self.log;
        binnacle_memory::pump(&mut self.grid, &mut source, |_, stamped| {
            let _ = match &stamped.effect {
                Effect::ViewRequested { spec, edit } => writeln!(
                    log,
                    "effect (gen {}): ViewRequested {} by {edit:?}",
                    stamped.generation,
                    view_text(spec)
                ),
                effect => writeln!(log, "effect (gen {}): {effect:?}", stamped.generation),
            };
            // The pump answers an Effect before handing it on.
            log.push_str(&answers.take());
            drained.push(stamped);
        });
        drained
    }

    /// Drains the Effect queue as [`Scenario::pump`] does; returns the
    /// Effects without their stamps.
    pub fn effects(&mut self) -> Vec<Effect> {
        self.pump().into_iter().map(|e| e.effect).collect()
    }

    /// A press and release at `(x, y)` with `modifiers`.
    pub fn click(&mut self, x: f32, y: f32, modifiers: Modifiers) {
        self.input(|g| {
            g.pointer(PointerEvent::down(x, y).modifiers(modifiers));
            g.pointer(PointerEvent::up(x, y).modifiers(modifiers));
        });
    }

    /// Runs one host frame: pump, `begin_frame`, fill each requested Window from
    /// the source, then take the Snapshot.
    pub fn frame(&mut self, viewport: Viewport) -> Snapshot<'_> {
        self.pump();
        for (axis, range) in self.grid.begin_frame(viewport) {
            let _ = writeln!(self.log, "window {axis:?} {range:?}");
            let keys = self.source.window(axis, range.clone());
            match self.source.sizes(axis, &keys) {
                Some((sizes, min_size)) => {
                    self.grid
                        .set_window_sized(axis, range.start, &keys, &sizes, min_size)
                }
                None => self.grid.set_window(axis, range.start, &keys),
            }
        }
        self.grid.snapshot()
    }

    /// Runs a frame and returns a numbered, labelled text rendering of it with
    /// everything logged since the previous step. Strip lines and the picture
    /// unchanged since the last rendering read `(unchanged)`.
    pub fn step(&mut self, label: &str, viewport: Viewport) -> String {
        self.steps += 1;
        let header = format!("--- {}. {label}\n", self.steps);
        let rendered = Rendered::of(&self.frame(viewport));
        let log = std::mem::take(&mut self.log);
        format!("{header}{log}{}", self.show(rendered))
    }

    /// Renders the Grid as it stands, without pumping or a frame, leaving
    /// out what is unchanged as [`Scenario::step`] does.
    pub fn render(&mut self) -> String {
        let rendered = Rendered::of(&self.grid.snapshot());
        self.show(rendered)
    }

    fn show(&mut self, rendered: Rendered) -> String {
        let text = rendered.since(self.shown.as_ref());
        self.shown = Some(rendered);
        text
    }
}
