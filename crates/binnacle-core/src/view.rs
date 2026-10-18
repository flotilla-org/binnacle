use std::collections::BTreeMap;

use crate::effect::Effect;
use crate::grid::Grid;
use crate::pointer::Modifiers;
use crate::sort::{HeaderSort, SortSpec};
use crate::types::{Axis, Key, Position};

/// The filter part of one axis of a View Spec. Empty for now: it keeps every
/// item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FilterSpec {}

impl FilterSpec {
    pub fn is_empty(&self) -> bool {
        true
    }
}

/// One axis's part of a View Spec: which of its items to keep and how to
/// order them, each named by Keys on the other axis.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AxisView {
    pub filter: FilterSpec,
    pub sort: SortSpec,
}

impl AxisView {
    pub fn is_empty(&self) -> bool {
        self.filter.is_empty() && self.sort.is_empty()
    }
}

/// The declared view a Table Source should present for the whole table, one
/// part per axis. The default is the Table Source's base view: nothing
/// filtered, unsorted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ViewSpec {
    pub rows: AxisView,
    pub cols: AxisView,
}

impl ViewSpec {
    pub fn axis(&self, axis: Axis) -> &AxisView {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }

    pub fn axis_mut(&mut self, axis: Axis) -> &mut AxisView {
        match axis {
            Axis::Rows => &mut self.rows,
            Axis::Cols => &mut self.cols,
        }
    }

    /// This spec with `axis` sorted by `sort`.
    pub fn with_sort(mut self, axis: Axis, sort: SortSpec) -> Self {
        self.axis_mut(axis).sort = sort;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.cols.is_empty()
    }
}

/// The named cause of a requested View Spec, carried with it as a hint: a
/// Table Source must be correct from the spec alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ViewEdit {
    /// A header click cycled the sort of `axis` by `key`, an item of the
    /// other axis: ascending, descending, then off. `multi` (Shift) kept the
    /// rest of the sort.
    CycleSort { axis: Axis, key: Key, multi: bool },
}

/// How the items of an axis may be viewed by one item of the other axis,
/// as its Table Source declares with [`Grid::set_support`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Support {
    /// The axis may be sorted by the item.
    pub sort: bool,
}

impl Default for Support {
    fn default() -> Self {
        Support { sort: true }
    }
}

impl Support {
    pub fn with_sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }
}

/// The Grid's view state: the View Spec the Table Source last applied and the
/// one the user last asked for while answers are still due.
#[derive(Clone, Debug, Default)]
pub(crate) struct ViewState {
    pub applied: ViewSpec,
    /// The latest request, kept while any `view_applied` is still due.
    pub requested: Option<ViewSpec>,
    due: u32,
    /// Per axis viewed, the items of the other axis whose support differs
    /// from the default (sparse, set by the host).
    support: [BTreeMap<Key, Support>; 2],
}

impl ViewState {
    /// How the header of `key` shows the sort of the other axis.
    pub fn header(&self, header_axis: Axis, key: Key) -> HeaderSort {
        let sorted = header_axis.other();
        let place = |spec: &ViewSpec| spec.axis(sorted).sort.find(key).map(|(i, d)| (d, i));
        let applied = place(&self.applied);
        let pending = self.requested.as_ref().is_some_and(|r| place(r) != applied);
        HeaderSort { applied, pending }
    }

    fn support(&self, axis: Axis, key: Key) -> Support {
        self.support[axis.index()]
            .get(&key)
            .copied()
            .unwrap_or_default()
    }
}

impl Grid {
    /// What the Table Source supports when viewing `axis` by `key`, an item
    /// of the other axis. Every item has [`Support::default`] until set
    /// otherwise.
    pub fn set_support(&mut self, axis: Axis, key: Key, support: Support) {
        let declared = &mut self.view.support[axis.index()];
        if support == Support::default() {
            declared.remove(&key);
        } else {
            declared.insert(key, support);
        }
    }

    /// The Table Source's answer to a `ViewRequested`, or its own news: the
    /// View Spec it now applies, which may be a part of the request, or the
    /// previous spec if it rejected it. Headers show the applied spec; the
    /// pending flags clear once every request has had its answer.
    pub fn view_applied(&mut self, spec: ViewSpec) {
        self.view.applied = spec;
        self.view.due = self.view.due.saturating_sub(1);
        if self.view.due == 0 {
            self.view.requested = None;
        }
        self.pending_damage.selection = true;
    }

    /// Applies `edit` to the latest requested View Spec (or the applied one)
    /// and requests the result.
    fn edit_view(&mut self, edit: ViewEdit) {
        let mut spec = self
            .view
            .requested
            .clone()
            .unwrap_or_else(|| self.view.applied.clone());
        match edit {
            ViewEdit::CycleSort { axis, key, multi } => {
                let sort = &mut spec.axis_mut(axis).sort;
                *sort = sort.cycled(key, multi);
            }
        }
        self.view.requested = Some(spec.clone());
        self.view.due += 1;
        self.pending_damage.selection = true;
        self.push(Effect::ViewRequested { spec, edit });
    }

    /// A Sort header click on column header `position`: cycles the rows'
    /// sort by that column. Rows never sort columns.
    pub(crate) fn sort_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        if axis != Axis::Cols {
            return;
        }
        let Some(key) = self.axis(axis).measure.key_at(position) else {
            return;
        };
        let sorted = axis.other();
        if !self.view.support(sorted, key).sort {
            return;
        }
        self.edit_view(ViewEdit::CycleSort {
            axis: sorted,
            key,
            multi: modifiers.shift,
        });
    }
}
