//! The `grouping` part of a View Spec and the outline a grouped Table Source
//! sends with each item of a Window.

use crate::types::{Axis, Key};
use crate::view::{Condition, Value};

/// Where a Subtotal or a Grand Total goes: before its group (or the axis),
/// after it, or nowhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    Before,
    After,
    #[default]
    Off,
}

/// One level of an axis's grouping, outermost first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Level {
    /// Groups the axis by the values of `field`, a base field of the Table
    /// Source, each group with a Subtotal placed as `subtotal` says.
    Field { field: Key, subtotal: Placement },
    /// The values pseudo-level: one item per value, where several values
    /// are shown.
    Values,
}

impl Level {
    /// Groups by `field`, with no Subtotal.
    pub fn field(field: Key) -> Self {
        Level::Field {
            field,
            subtotal: Placement::Off,
        }
    }

    /// The field this level groups by; `None` for the values pseudo-level.
    pub fn field_key(&self) -> Option<Key> {
        match self {
            Level::Field { field, .. } => Some(*field),
            Level::Values => None,
        }
    }
}

/// One value a grouped table shows: an aggregate, an open name such as
/// `sum`, `count`, `average`, `minimum`, `maximum`, `distinct count` or one
/// the Table Source declares, over a base field (`None` for a count of
/// items).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct AggregateValue {
    pub field: Option<Key>,
    pub aggregate: String,
}

impl AggregateValue {
    pub fn new(field: Option<Key>, aggregate: impl Into<String>) -> Self {
        AggregateValue {
            field,
            aggregate: aggregate.into(),
        }
    }

    /// The count of items in each group.
    pub fn count() -> Self {
        AggregateValue::new(None, "count")
    }
}

/// How a grouped axis is drawn: each group a Group Item at its own Position,
/// or each level a tier of the cross Header Band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    Outline,
    Header,
}

/// How many levels are open by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Depth {
    /// The first `n` levels are open; 0 shows only the top groups.
    Levels(u32),
    #[default]
    All,
}

/// Which groups of an axis are open: those `depth` opens, except the Group
/// Items keyed in `except`, which are the other way.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Expand {
    pub depth: Depth,
    pub except: Vec<Key>,
}

/// How one grouped axis is laid out.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct GroupedAxis {
    pub grand_total: Placement,
    pub form: Form,
    pub expand: Expand,
}

impl GroupedAxis {
    /// The default for `axis`: outline form for the rows, header form for
    /// the columns, no Grand Total, every group open.
    pub fn new(axis: Axis) -> Self {
        GroupedAxis {
            grand_total: Placement::Off,
            form: match axis {
                Axis::Rows => Form::Outline,
                Axis::Cols => Form::Header,
            },
            expand: Expand::default(),
        }
    }
}

/// The whole-table grouping part of a View Spec: row and column levels,
/// values and slicers, all naming the Table Source's base fields (its items
/// before any pivot; for a plain records table, the column Keys).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Grouping {
    pub row_levels: Vec<Level>,
    /// Empty: no pivot, and the Leaves stay.
    pub col_levels: Vec<Level>,
    pub values: Vec<AggregateValue>,
    /// Conditions on base fields applied before aggregating (`WHERE`).
    pub slicers: Vec<Condition>,
    pub rows: GroupedAxis,
    pub cols: GroupedAxis,
}

impl Default for Grouping {
    fn default() -> Self {
        Grouping {
            row_levels: Vec::new(),
            col_levels: Vec::new(),
            values: Vec::new(),
            slicers: Vec::new(),
            rows: GroupedAxis::new(Axis::Rows),
            cols: GroupedAxis::new(Axis::Cols),
        }
    }
}

impl Grouping {
    /// Groups the rows by `field`, counting each group.
    pub fn rows_by(field: Key) -> Self {
        Grouping {
            row_levels: vec![Level::field(field)],
            values: vec![AggregateValue::count()],
            ..Grouping::default()
        }
    }

    pub fn levels(&self, axis: Axis) -> &[Level] {
        match axis {
            Axis::Rows => &self.row_levels,
            Axis::Cols => &self.col_levels,
        }
    }

    pub fn levels_mut(&mut self, axis: Axis) -> &mut Vec<Level> {
        match axis {
            Axis::Rows => &mut self.row_levels,
            Axis::Cols => &mut self.col_levels,
        }
    }

    pub fn axis(&self, axis: Axis) -> &GroupedAxis {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }

    pub fn axis_mut(&mut self, axis: Axis) -> &mut GroupedAxis {
        match axis {
            Axis::Rows => &mut self.rows,
            Axis::Cols => &mut self.cols,
        }
    }

    /// Whether `axis` is grouped by `field`.
    pub fn groups_by(&self, axis: Axis, field: Key) -> bool {
        self.levels(axis)
            .iter()
            .any(|l| l.field_key() == Some(field))
    }
}

/// What an item of a grouped axis is: a Leaf from the data or one of the
/// Summary Items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Leaf,
    /// Heads a group, with a disclosure mark.
    Group,
    /// Aggregates one group, before or after it.
    Subtotal,
    /// Aggregates the whole axis.
    GrandTotal,
}

impl ItemKind {
    pub fn is_summary(self) -> bool {
        self != ItemKind::Leaf
    }
}

/// Whether a Group Item is open. Items that are not Group Items, and Group
/// Items with nothing under them to show, can't open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpenState {
    Open,
    Closed,
    CantOpen,
}

/// The group a Group Item or Subtotal belongs to: its level's field and the
/// group's value there.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupOf {
    pub field: Key,
    pub value: Value,
}

/// Where an item sits in a grouped axis, as the Table Source sends it with
/// a Window ([`crate::Grid::set_outlines`]) and the Snapshot passes it on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Outline {
    pub kind: ItemKind,
    /// 0 for the top groups and a Grand Total; a Leaf of a group at depth
    /// `d` is at `d + 1`.
    pub depth: u32,
    pub open: OpenState,
    /// The Group Item Keys from the outermost level down to the parent, so
    /// there are `depth` of them.
    pub ancestors: Vec<Key>,
    /// On Group Items and Subtotals: the group's field and value.
    pub group: Option<GroupOf>,
}

impl Outline {
    /// A Leaf under the groups keyed `ancestors`, outermost first.
    pub fn leaf(ancestors: Vec<Key>) -> Self {
        Outline {
            kind: ItemKind::Leaf,
            depth: ancestors.len() as u32,
            open: OpenState::CantOpen,
            ancestors,
            group: None,
        }
    }

    /// A Group Item for `field` = `value`, under the groups keyed
    /// `ancestors`.
    pub fn group(ancestors: Vec<Key>, open: OpenState, field: Key, value: Value) -> Self {
        Outline {
            kind: ItemKind::Group,
            depth: ancestors.len() as u32,
            open,
            ancestors,
            group: Some(GroupOf { field, value }),
        }
    }

    /// The parent Group Item's Key; `None` at the top.
    pub fn parent(&self) -> Option<Key> {
        self.ancestors.last().copied()
    }
}
