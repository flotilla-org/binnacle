//! Summary Items as the user meets them: selecting a whole group from its
//! Group Item's header, copying what is shown, and the drill-down an
//! activated Summary Item's cell carries.

use crate::effect::Effect;
use crate::focus::FocusEnd;
use crate::grid::Grid;
use crate::grouping::{DrillDown, GRAND_TOTAL, ItemKind, Outline};
use crate::pointer::Modifiers;
use crate::selection::{Selection, Span};
use crate::types::{Axis, Key, Position};
use crate::view::Condition;

/// A pinned Grand Total's outline, for when no Window has listed it.
static GRAND: Outline = Outline::grand_total();

impl Grid {
    /// Whether `axis` is grouped in the View Spec the Table Source applies.
    pub(crate) fn grouped(&self, axis: Axis) -> bool {
        (self.view.applied.grouping.as_ref()).is_some_and(|g| !g.levels(axis).is_empty())
    }

    /// The outline of the item with `key` on `axis`, if the Window holds it.
    /// A grouped axis's Grand Total has its outline wherever it is.
    pub(crate) fn outline_by_key(&self, axis: Axis, key: Key) -> Option<&Outline> {
        match self.axis(axis).outlines.get(&key) {
            None if key == GRAND_TOTAL && self.grouped(axis) => Some(&GRAND),
            found => found,
        }
    }

    /// The drill-down for the cell with `row` and `col`, if either is a
    /// Summary Item whose outline the Grid holds.
    pub(crate) fn drill_down(&self, row: Key, col: Key) -> Option<DrillDown> {
        let grouping = self.view.applied.grouping.as_ref()?;
        let outlines = [(Axis::Rows, row), (Axis::Cols, col)]
            .map(|(axis, key)| self.outline_by_key(axis, key));
        if !outlines.iter().flatten().any(|o| o.kind.is_summary()) {
            return None;
        }
        let paths = (outlines.into_iter().flatten()).flat_map(Outline::groups);
        let conditions = (grouping.slicers.iter().cloned())
            .chain(paths.map(|g| Condition::one_of(g.field, vec![g.value.clone()])))
            .collect();
        Some(DrillDown::new(conditions))
    }

    /// Activates the cell with `row` and `col`.
    pub(crate) fn activate(&mut self, row: Key, col: Key) {
        let drill_down = self.drill_down(row, col);
        self.push(Effect::Activated {
            row,
            col,
            drill_down,
        });
    }

    /// Asks the host to copy the range, or the focused cell without one.
    pub(crate) fn copy(&mut self) {
        if Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0) {
            return;
        }
        let focused = self.focus.map(|f| Span {
            anchor: f,
            head: f,
            whole: [false, false],
        });
        let Some(span) = self.selection.span.or(focused) else {
            return;
        };
        let range = span.drawn_order(&self.axes);
        if range.rows.is_empty() || range.cols.is_empty() {
            return;
        }
        self.push(Effect::CopyRequested {
            rows: range.rows,
            cols: range.cols,
            row_labels: self.grouped(Axis::Rows),
            col_labels: self.grouped(Axis::Cols),
        });
    }

    /// The last Position of the group whose Group Item is at `position` on
    /// `axis`, if the Grid holds its outline: the item `span` past it.
    fn group_end(&self, axis: Axis, position: Position) -> Option<Position> {
        let st = self.axis(axis);
        let key = st.key_at(position)?;
        let outline = self.outline_by_key(axis, key)?;
        let last = st.measure.count().checked_sub(1)?;
        (outline.kind == ItemKind::Group).then(|| position.saturating_add(outline.span).min(last))
    }

    /// A click selecting the header of the item at `position` on `axis`:
    /// on a Group Item it selects the whole group, from its Group Item
    /// through its last descendant, and with Shift stretches the range to
    /// take in the whole group clicked. `cell` is the focus moved to the
    /// clicked item. Returns whether it was a Group Item.
    pub(crate) fn group_click(
        &mut self,
        axis: Axis,
        position: Position,
        cell: [FocusEnd; 2],
        modifiers: Modifiers,
    ) -> bool {
        let Some(end) = self.group_end(axis, position) else {
            return false;
        };
        let i = axis.index();
        let mut whole = [false; 2];
        whole[axis.other().index()] = true;
        let (mut anchor, mut head) = (cell, cell);
        head[i] = FocusEnd::at(self.axis(axis), end);
        // Shift grows the range away from its far corner, so the groups it
        // held stay whole.
        if let Some(span) = self.selection.span.filter(|_| modifiers.shift) {
            let (a, h) = (span.anchor[i], span.head[i]);
            let (low, high) = match a.position <= h.position {
                true => (a, h),
                false => (h, a),
            };
            if position >= low.position {
                anchor[i] = low;
            } else {
                anchor[i] = high;
                head[i] = cell[i];
            }
        }
        self.set_focus_cell(cell);
        self.set_selection(Selection {
            span: Some(Span {
                anchor,
                head,
                whole,
            }),
            picks: Default::default(),
        });
        true
    }
}
