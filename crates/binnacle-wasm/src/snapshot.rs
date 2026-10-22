//! The Snapshot copied out as two flat arrays: numbers (`Float64Array`) and
//! Keys (`BigUint64Array`). Both are written in one fixed order and read back
//! in the same order by `web/src/snapshot.ts`, the only TypeScript module that
//! knows it. An optional value is a flag (0 or 1) followed by its fields when
//! present; a list is its length followed by its items. A Key takes no number:
//! it is the next entry in the Key array. The agreement tests in `web/` check
//! the decoder against [`crate::testing`]'s dump of the same frame.

use binnacle_core::{
    AggregateValue, Axis, CellState, Comparison, Condition, ConditionKind, ConditionState, Cursor,
    Depth, DropTarget, Edge, Form, GroupedAxis, Grouping, HeaderPlacement, ItemKind, Key, Level,
    Mode, OpenState, Outline, Placement, Rect, Scalar, Searched, Snapshot, SortDirection, SortSpec,
    TextQuery, Value, ViewSpec,
};

use crate::codes::{find_bits, support_bits};

/// The flat form of one Snapshot. Buffers are reused frame to frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Encoded {
    pub numbers: Vec<f64>,
    pub keys: Vec<u64>,
}

pub const CELL_FOCUSED: u8 = 1;
pub const CELL_SELECTED: u8 = 2;
pub const CELL_EDITING: u8 = 4;
pub const CELL_CURRENT_MATCH: u8 = 8;

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
    if c.current_match {
        bits |= CELL_CURRENT_MATCH;
    }
    bits
}

pub fn axis_code(axis: Axis) -> f64 {
    match axis {
        Axis::Rows => 0.0,
        Axis::Cols => 1.0,
    }
}

/// The pinned edge codes `codes::edge` reads: 0 leading, 1 trailing.
fn edge_code(edge: Edge) -> f64 {
    match edge {
        Edge::Leading => 0.0,
        Edge::Trailing => 1.0,
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

    /// A drop target: a code (0 move, 1 unpin, 2 pin, 3 stay) and its
    /// fields.
    fn target(&mut self, target: DropTarget) {
        match target {
            DropTarget::Move { to, unpin } => {
                self.num(0);
                self.position(to);
                self.flag(unpin);
            }
            DropTarget::Unpin { at } => {
                self.num(1);
                self.position(at);
            }
            DropTarget::Pin {
                edge,
                before,
                reorder,
            } => {
                self.num(2);
                self.num(edge_code(edge));
                self.opt_key(before);
                self.flag(reorder);
            }
            DropTarget::Stay => self.num(3),
        }
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
        self.flag(h.filter.filtered);
        self.flag(h.filter.pending);
        self.flag(h.filter.not_applied);
        self.flag(h.move_pending);
    }

    /// A condition: its Key, `not`, then its kind's code (Text 0, a kind
    /// this bridge does not carry 1, OneOf 2, Compare 3, Native 4) and a
    /// Text condition's query, a OneOf's values, each a blank flag then text
    /// if not blank, a comparison's code (equal 0, not equal 1, less 2, at
    /// most 3, greater 4, at least 5, between 6) and its scalars, each a
    /// date flag then the number or milliseconds, or a Native condition's
    /// text.
    fn condition(&mut self, c: &Condition) {
        self.key(c.key);
        self.flag(c.not);
        match &c.kind {
            ConditionKind::Text(q) => {
                self.num(0);
                self.text_query(q);
            }
            ConditionKind::OneOf(values) => {
                self.num(2);
                self.len(values.len());
                for v in values {
                    self.flag(*v == Value::Blank);
                    if let Value::Text(t) = v {
                        self.text(t);
                    }
                }
            }
            ConditionKind::Compare(comparison) => {
                self.num(3);
                let (code, scalars) = match *comparison {
                    Comparison::Equal(s) => (0, [Some(s), None]),
                    Comparison::NotEqual(s) => (1, [Some(s), None]),
                    Comparison::Less(s) => (2, [Some(s), None]),
                    Comparison::AtMost(s) => (3, [Some(s), None]),
                    Comparison::Greater(s) => (4, [Some(s), None]),
                    Comparison::AtLeast(s) => (5, [Some(s), None]),
                    Comparison::Between(low, high) => (6, [Some(low), Some(high)]),
                };
                self.num(code);
                for s in scalars.into_iter().flatten() {
                    match s {
                        Scalar::Number(n) => {
                            self.flag(false);
                            self.num(n);
                        }
                        Scalar::Date(ms) => {
                            self.flag(true);
                            self.num(ms as f64);
                        }
                    }
                }
            }
            ConditionKind::Native(text) => {
                self.num(4);
                self.text(text);
            }
            _ => self.num(1),
        }
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

    fn placement(&mut self, p: Placement) {
        self.num(match p {
            Placement::Before => 0,
            Placement::After => 1,
            Placement::Off => 2,
        });
    }

    /// A level: 0 and its field and Subtotal placement, 1 for the values
    /// pseudo-level, 2 for one this bridge does not carry.
    fn level(&mut self, level: &Level) {
        match level {
            Level::Field { field, subtotal } => {
                self.num(0);
                self.key(*field);
                self.placement(*subtotal);
            }
            Level::Values => self.num(1),
            _ => self.num(2),
        }
    }

    /// A value: its flagged field, its aggregate's name and its parameters.
    fn value(&mut self, v: &AggregateValue) {
        self.flag(v.field.is_some());
        if let Some(field) = v.field {
            self.key(field);
        }
        self.text(&v.aggregate);
        self.len(v.params.len());
        for p in &v.params {
            self.text(p);
        }
    }

    /// Where a listed part stands: applied 0, adding 1, removing 2, not
    /// applied 3.
    fn state(&mut self, state: ConditionState) {
        self.num(match state {
            ConditionState::Applied => 0,
            ConditionState::Adding => 1,
            ConditionState::Removing => 2,
            ConditionState::NotApplied => 3,
        });
    }

    /// A grouped axis: its Grand Total's placement, its form (0 outline,
    /// 1 header), its default depth (flagged unless every level is open),
    /// then its exceptions.
    fn grouped_axis(&mut self, a: &GroupedAxis) {
        self.placement(a.grand_total);
        self.num(match a.form {
            Form::Outline => 0,
            Form::Header => 1,
        });
        match a.expand.depth {
            Depth::Levels(n) => {
                self.flag(true);
                self.num(n);
            }
            Depth::All => self.flag(false),
        }
        self.len(a.expand.except.len());
        for &k in &a.expand.except {
            self.key(k);
        }
    }

    /// The row levels, the column levels, the values (each a flagged field,
    /// its aggregate's name and its parameters), the slicers, then each
    /// axis's layout.
    fn grouping(&mut self, g: &Grouping) {
        for levels in [&g.row_levels, &g.col_levels] {
            self.len(levels.len());
            for level in levels {
                self.level(level);
            }
        }
        self.len(g.values.len());
        for v in &g.values {
            self.value(v);
        }
        self.len(g.slicers.len());
        for c in &g.slicers {
            self.condition(c);
        }
        self.grouped_axis(&g.rows);
        self.grouped_axis(&g.cols);
    }

    /// An item's outline, flagged: its kind (0 Leaf, 1 Group Item, 2
    /// Subtotal, 3 Grand Total), depth, open state (0 open, 1 closed, 2
    /// can't open), ancestor Keys, then its group, flagged: the field's Key
    /// and the value, flagged unless blank.
    fn outline(&mut self, outline: Option<&Outline>) {
        self.flag(outline.is_some());
        let Some(o) = outline else {
            return;
        };
        self.num(match o.kind {
            ItemKind::Leaf => 0,
            ItemKind::Group => 1,
            ItemKind::Subtotal => 2,
            ItemKind::GrandTotal => 3,
        });
        self.num(o.depth);
        self.num(match o.open {
            OpenState::Open => 0,
            OpenState::Closed => 1,
            OpenState::CantOpen => 2,
        });
        self.len(o.ancestors.len());
        for &k in &o.ancestors {
            self.key(k);
        }
        self.flag(o.group.is_some());
        if let Some(g) = &o.group {
            self.key(g.field);
            match &g.value {
                Value::Text(t) => {
                    self.flag(true);
                    self.text(t);
                }
                Value::Blank => self.flag(false),
            }
        }
    }

    /// Each axis's part, rows first: its filter (the quick filter, then the
    /// conditions), its sort, then its hidden Keys; then the grouping,
    /// flagged.
    fn view(&mut self, spec: &ViewSpec) {
        for axis in Axis::ALL {
            let part = spec.axis(axis);
            self.flag(part.filter.quick.is_some());
            if let Some(q) = &part.filter.quick {
                self.text_query(q);
            }
            let conditions = part.filter.conditions.conditions();
            self.len(conditions.len());
            for c in conditions {
                self.condition(c);
            }
            self.sort(&part.sort);
            self.len(part.hidden.len());
            for &k in &part.hidden {
                self.key(k);
            }
        }
        self.flag(spec.grouping.is_some());
        if let Some(g) = &spec.grouping {
            self.grouping(g);
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
            Mode::MovingHeaders { axis, key, target } => {
                self.num(4);
                self.num(axis_code(axis));
                self.key(key);
                self.target(target);
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
        let drawn: Vec<_> = s.drawn(axis).collect();
        w.len(drawn.len());
        for item in &drawn {
            w.position(item.position);
            w.opt_key(item.key);
            w.num(item.offset);
            w.num(item.size);
            w.outline(s.outline(axis, item.position));
            w.flag(item.key.is_some_and(|k| s.toggle_pending(axis, k)));
        }
        w.len(s.pinned(axis, Edge::Leading).len());
        w.len(s.pinned(axis, Edge::Trailing).len());
        let scrolling = s.scrolling(axis);
        w.num(scrolling.start);
        w.num(scrolling.end);
        for edge in Edge::ALL {
            let region = s.pinned_region(axis, edge);
            w.num(region.start);
            w.num(region.end);
        }
        w.len(drawn.len());
        for h in s.headers(axis) {
            let support = h
                .key
                .map_or(0, |k| support_bits(s.support(axis.other(), k)));
            w.header(h);
            w.num(support);
            // The aggregates offered over the item: each its name and its
            // parameters' names.
            let offered = h.key.map_or(&[][..], |k| s.aggregates(axis.other(), k));
            w.len(offered.len());
            for a in offered {
                w.text(&a.name);
                w.len(a.params.len());
                for p in &a.params {
                    w.text(p);
                }
            }
        }
        let conditions = s.conditions(axis);
        w.len(conditions.len());
        for (c, state) in &conditions {
            w.state(*state);
            w.condition(c);
        }
        let levels = s.levels(axis);
        w.len(levels.len());
        for (level, state) in &levels {
            w.state(*state);
            w.level(level);
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
        w.flag(s.movable(axis));
        w.flag(s.hide_support(axis));
        w.len(s.hide_cap(axis));
        for edge in Edge::ALL {
            let pins: Vec<Key> = s.pins(axis, edge).collect();
            w.len(pins.len());
            for k in pins {
                w.key(k);
            }
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
        w.target(d.target);
        w.num(d.offset);
    }
    let zones = s.pin_zones();
    w.len(zones.len());
    for z in zones {
        w.num(axis_code(z.axis));
        w.num(edge_code(z.edge));
        w.num(z.offset);
        w.num(z.size);
        w.flag(z.hot);
    }
    let hovered = s.hovered_header();
    w.flag(hovered.is_some());
    if let Some((axis, item)) = hovered {
        w.num(axis_code(axis));
        w.position(item.position);
    }
    // The range as each axis's runs of Positions.
    let range = s.selection_range();
    w.flag(range.is_some());
    if let Some(r) = range {
        for runs in [&r.rows, &r.cols] {
            w.len(runs.len());
            for run in runs {
                w.position(run.start);
                w.position(run.end);
            }
        }
    }
    let rects = s.selection_rects();
    w.len(rects.len());
    for r in rects {
        w.rect(*r);
    }
    for keys in [s.picked(Axis::Rows), s.picked(Axis::Cols), s.moving_keys()] {
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
    w.flag(s.view_pending());
    w.flag(s.outline_focused());
    // The move waiting for its answer, then the last refusal.
    let pending = s.pending_move();
    w.flag(pending.is_some());
    if let Some((axis, keys)) = pending {
        w.num(axis_code(axis));
        w.len(keys.len());
        for &k in keys {
            w.key(k);
        }
    }
    let refusal = s.move_refusal();
    w.flag(refusal.is_some());
    if let Some((axis, reason)) = refusal {
        w.num(axis_code(axis));
        w.text(reason);
    }
    // Find: the query and the axis it steps along, the current match, then
    // the support bits.
    let find = s.find();
    w.flag(find.is_some());
    if let Some(f) = find {
        w.text_query(&f.query);
        w.num(axis_code(f.by));
    }
    let found = s.find_match();
    w.flag(found.is_some());
    if let Some(m) = found {
        w.key(m.row);
        w.key(m.col);
    }
    w.num(find_bits(s.find_support()));
    // The grouping's slicers and values as listed, each its state first.
    let slicers = s.slicers();
    w.len(slicers.len());
    for (c, state) in &slicers {
        w.state(*state);
        w.condition(c);
    }
    let values = s.values();
    w.len(values.len());
    for (v, state) in &values {
        w.state(*state);
        w.value(v);
    }
    // Cell states, row-major over the drawn items, as bit flags.
    for row in s.drawn(Axis::Rows) {
        for col in s.drawn(Axis::Cols) {
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
