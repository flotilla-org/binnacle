//! The Snapshot copied out as two flat arrays: numbers (`Float64Array`) and
//! Keys (`BigUint64Array`). Both are written in one fixed order and read back
//! in the same order by `web/src/snapshot.ts`, the only TypeScript module that
//! knows it. An optional value is a flag (0 or 1) followed by its fields when
//! present; a list is its length followed by its items. A Key takes no number:
//! it is the next entry in the Key array. The agreement tests in `web/` check
//! the decoder against [`crate::testing`]'s dump of the same frame.

use binnacle_core::{
    Axis, CellState, Cursor, HeaderPlacement, Key, Mode, Rect, Searched, Snapshot, SortDirection,
    SortSpec, TextQuery, ViewSpec,
};

/// The flat form of one Snapshot. Buffers are reused frame to frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Encoded {
    pub numbers: Vec<f64>,
    pub keys: Vec<u64>,
}

pub const CELL_FOCUSED: u8 = 1;
pub const CELL_SELECTED: u8 = 2;
pub const CELL_EDITING: u8 = 4;

/// A cell's state as the bit flags the Snapshot carries.
pub fn cell_bits(c: CellState) -> u8 {
    let mut bits = 0;
    if c.focused {
        bits |= CELL_FOCUSED;
    }
    if c.selected {
        bits |= CELL_SELECTED;
    }
    if c.editing {
        bits |= CELL_EDITING;
    }
    bits
}

pub fn axis_code(axis: Axis) -> f64 {
    match axis {
        Axis::Rows => 0.0,
        Axis::Cols => 1.0,
    }
}

fn direction_code(direction: SortDirection) -> f64 {
    match direction {
        SortDirection::Ascending => 0.0,
        SortDirection::Descending => 1.0,
    }
}

fn cursor_code(cursor: Cursor) -> f64 {
    match cursor {
        Cursor::Default => 0.0,
        Cursor::ResizeCol => 1.0,
        Cursor::ResizeRow => 2.0,
        Cursor::Move => 3.0,
    }
}

struct Writer<'a> {
    out: &'a mut Encoded,
}

impl Writer<'_> {
    fn num(&mut self, n: impl Into<f64>) {
        self.out.numbers.push(n.into());
    }

    fn position(&mut self, p: u64) {
        self.out.numbers.push(p as f64);
    }

    fn flag(&mut self, b: bool) {
        self.num(u8::from(b));
    }

    fn len(&mut self, n: usize) {
        self.out.numbers.push(n as f64);
    }

    fn key(&mut self, key: Key) {
        self.out.keys.push(key.0);
    }

    fn opt_key(&mut self, key: Option<Key>) {
        self.flag(key.is_some());
        if let Some(k) = key {
            self.key(k);
        }
    }

    fn rect(&mut self, r: Rect) {
        for n in [r.x, r.y, r.width, r.height] {
            self.num(n);
        }
    }

    fn opt_rect(&mut self, r: Option<Rect>) {
        self.flag(r.is_some());
        if let Some(r) = r {
            self.rect(r);
        }
    }

    fn header(&mut self, h: HeaderPlacement) {
        self.position(h.position);
        self.opt_key(h.key);
        self.rect(h.rect);
        self.flag(h.sort.applied.is_some());
        if let Some((direction, index)) = h.sort.applied {
            self.num(direction_code(direction));
            self.len(index);
        }
        self.flag(h.sort.pending);
        self.flag(h.selected);
        self.flag(h.hovered);
    }

    fn sort(&mut self, spec: &SortSpec) {
        self.len(spec.columns().len());
        for c in spec.columns() {
            self.key(c.key);
            self.num(direction_code(c.direction));
        }
    }

    /// Text as its UTF-16 code units, after their count.
    fn text(&mut self, text: &str) {
        let units = self.out.numbers.len();
        self.num(0);
        let mut n = 0;
        for unit in text.encode_utf16() {
            self.num(unit);
            n += 1;
        }
        self.out.numbers[units] = n as f64;
    }

    fn text_query(&mut self, q: &TextQuery) {
        self.text(&q.text);
        self.flag(q.case_sensitive);
        self.flag(q.regex);
        let keys = match &q.searches {
            Searched::Shown => None,
            Searched::Keys(keys) => Some(keys),
        };
        self.flag(keys.is_some());
        if let Some(keys) = keys {
            self.len(keys.len());
            for &k in keys {
                self.key(k);
            }
        }
    }

    /// Each axis's part, rows first: its filter, then its sort.
    fn view(&mut self, spec: &ViewSpec) {
        for axis in Axis::ALL {
            let part = spec.axis(axis);
            self.flag(part.filter.quick.is_some());
            if let Some(q) = &part.filter.quick {
                self.text_query(q);
            }
            self.sort(&part.sort);
        }
    }

    fn mode(&mut self, mode: Mode) {
        match mode {
            Mode::Idle => self.num(0),
            Mode::DraggingSelection => self.num(1),
            Mode::Resizing {
                axis,
                key,
                start_size,
            } => {
                self.num(2);
                self.num(axis_code(axis));
                self.key(key);
                self.num(start_size);
            }
            Mode::ResizingBand {
                axis,
                start_thickness,
            } => {
                self.num(3);
                self.num(axis_code(axis));
                self.num(start_thickness);
            }
            Mode::MovingHeaders { axis, key, to } => {
                self.num(4);
                self.num(axis_code(axis));
                self.key(key);
                self.position(to);
            }
            Mode::Editing { row, col } => {
                self.num(5);
                self.key(row);
                self.key(col);
            }
        }
    }
}

/// Writes `s` into `out`, replacing what it held.
pub fn encode(s: &Snapshot<'_>, out: &mut Encoded) {
    out.numbers.clear();
    out.keys.clear();
    let mut w = Writer { out };
    w.position(s.generation());
    let d = s.damage();
    w.flag(d.layout);
    w.flag(d.selection);
    w.flag(d.positions);
    for axis in Axis::ALL {
        w.position(s.count(axis));
        w.num(s.extent(axis));
        w.num(s.header(axis));
        w.num(s.gap(axis));
        let anchor = s.scroll_anchor(axis);
        w.position(anchor.position);
        w.num(anchor.offset);
        let thumb = s.thumb(axis);
        w.num(thumb.fraction);
        w.num(thumb.size);
        let focus = s.focus(axis);
        w.flag(focus.is_some());
        if let Some(f) = focus {
            w.position(f.position);
            w.opt_key(f.key);
        }
        w.flag(s.locating(axis));
        let strip = s.strip(axis);
        w.len(strip.len());
        for item in strip {
            w.position(item.position);
            w.opt_key(item.key);
            w.num(item.offset);
            w.num(item.size);
        }
        w.len(strip.len());
        for h in s.headers(axis) {
            w.header(h);
        }
        let total = s.total(axis);
        w.flag(total.is_some());
        if let Some(t) = total {
            w.position(t);
        }
        let quick = s.quick_filter(axis);
        w.flag(quick.is_some());
        if let Some(q) = quick {
            w.flag(q.regex);
        }
    }
    w.opt_rect(s.focus_rect());
    w.opt_rect(s.editor_rect());
    w.mode(s.mode());
    w.flag(s.capturing());
    w.num(cursor_code(s.cursor()));
    let guide = s.resize_guide();
    w.flag(guide.is_some());
    if let Some(g) = guide {
        w.num(axis_code(g.axis));
        w.opt_key(g.key);
        w.num(g.offset);
        w.num(g.size);
    }
    let drop = s.drop_indicator();
    w.flag(drop.is_some());
    if let Some(d) = drop {
        w.num(axis_code(d.axis));
        w.position(d.to);
        w.num(d.offset);
    }
    let hovered = s.hovered_header();
    w.flag(hovered.is_some());
    if let Some((axis, item)) = hovered {
        w.num(axis_code(axis));
        w.position(item.position);
    }
    let range = s.selection_range();
    w.flag(range.is_some());
    if let Some(r) = range {
        for p in [r.rows.start, r.rows.end, r.cols.start, r.cols.end] {
            w.position(p);
        }
    }
    let rects = s.selection_rects();
    w.len(rects.len());
    for r in rects {
        w.rect(*r);
    }
    for keys in [s.picked(), s.moving_keys()] {
        w.len(keys.len());
        for k in keys {
            w.key(*k);
        }
    }
    w.view(s.applied_view());
    let requested = s.requested_view();
    w.flag(requested.is_some());
    if let Some(spec) = requested {
        w.view(spec);
    }
    // Cell states, row-major over the Strips, as bit flags.
    for row in s.rows() {
        for col in s.cols() {
            w.num(cell_bits(s.cell_state(row, col)));
        }
    }
}

#[cfg(test)]
mod tests {
    use binnacle_core::{AxisConfig, Grid, Viewport};

    use super::*;

    fn grid_with_keys(rows: &[u64], cols: &[u64]) -> Grid {
        let mut grid = Grid::new(
            AxisConfig::new(rows.len() as u64, 20.0).header(24.0),
            AxisConfig::new(cols.len() as u64, 80.0).header(40.0),
        );
        grid.begin_frame(Viewport::new(400.0, 200.0));
        let keys = |ks: &[u64]| ks.iter().map(|&k| Key(k)).collect::<Vec<_>>();
        grid.set_window(Axis::Rows, 0, &keys(rows));
        grid.set_window(Axis::Cols, 0, &keys(cols));
        grid
    }

    fn encoded(grid: &mut Grid) -> Encoded {
        let mut out = Encoded::default();
        encode(&grid.snapshot(), &mut out);
        out
    }

    #[test]
    fn keys_cross_exactly_in_strip_order() {
        let big = u64::MAX - 1;
        let mut grid = grid_with_keys(&[big, 1 << 60, 3], &[9, 8]);
        let out = encoded(&mut grid);
        // Rows' Strip, row headers, columns' Strip, column headers.
        assert_eq!(out.keys, vec![big, 1 << 60, 3, big, 1 << 60, 3, 9, 8, 9, 8]);
    }

    #[test]
    fn an_unfilled_position_takes_no_key() {
        let mut grid = Grid::new(AxisConfig::new(3, 20.0), AxisConfig::new(1, 80.0));
        grid.begin_frame(Viewport::new(400.0, 200.0));
        grid.set_window(Axis::Cols, 0, &[Key(5)]);
        let out = encoded(&mut grid);
        assert_eq!(out.keys, vec![5, 5]);
    }

    #[test]
    fn every_visible_cell_has_a_state() {
        let mut grid = grid_with_keys(&[1, 2, 3], &[4, 5]);
        let out = encoded(&mut grid);
        let cells = &out.numbers[out.numbers.len() - 6..];
        assert!(cells.iter().all(|&b| b == 0.0), "{cells:?}");
    }

    #[test]
    fn starts_with_the_generation_and_damage() {
        let mut grid = grid_with_keys(&[1], &[2]);
        let out = encoded(&mut grid);
        assert_eq!(out.numbers[..4], [1.0, 1.0, 1.0, 1.0]);
        grid.begin_frame(Viewport::new(400.0, 200.0));
        let out = encoded(&mut grid);
        assert_eq!(out.numbers[..4], [2.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn reuses_its_buffers() {
        let mut grid = grid_with_keys(&[1, 2], &[3]);
        let mut out = Encoded::default();
        encode(&grid.snapshot(), &mut out);
        let first = out.clone();
        encode(&grid.snapshot(), &mut out);
        assert_eq!(out.keys, first.keys);
        assert_eq!(out.numbers.len(), first.numbers.len());
    }
}
