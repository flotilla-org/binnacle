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

/// The Key of an axis's Grand Total, whoever groups it: the grouping layer
/// names it so, and a grouped Table Source that skips the layer must too,
/// so the Grid can pin a Grand Total placed after before it is in a Window.
pub const GRAND_TOTAL: Key = Key(1 << 63);

/// The field a group of the values pseudo-level names in its path and its
/// [`GroupOf`], beside the value's [`AggregateValue::identity`] as text. Its
/// top bit is set, so it never meets a base field's Key.
pub const VALUES_FIELD: Key = Key(u64::MAX);

/// One value a grouped table shows: an aggregate, an open name such as
/// `sum`, `count`, `average`, `minimum`, `maximum`, `distinct count` or one
/// the Table Source declares, over a base field (`None` for a count of
/// items), with the parameters a declared aggregate takes, passed to the
/// backend as they are.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct AggregateValue {
    pub field: Option<Key>,
    pub aggregate: String,
    pub params: Vec<String>,
}

impl AggregateValue {
    pub fn new(field: Option<Key>, aggregate: impl Into<String>) -> Self {
        AggregateValue {
            field,
            aggregate: aggregate.into(),
            params: Vec::new(),
        }
    }

    /// The count of items in each group.
    pub fn count() -> Self {
        AggregateValue::new(None, "count")
    }

    /// `aggregate` of `field`, such as `sum` of the weights.
    pub fn of(field: Key, aggregate: impl Into<String>) -> Self {
        AggregateValue::new(Some(field), aggregate)
    }

    /// This value with the parameters `params`.
    pub fn with_params(mut self, params: Vec<String>) -> Self {
        self.params = params;
        self
    }

    /// What names this value wherever it heads a group of the values
    /// pseudo-level, in Keys interned from paths (a pivot column's, say):
    /// its aggregate, its field's Key and its parameters, as in
    /// `sum(3)`, `count()` or `percentile(3;0.9)`. Not its index, so state
    /// on its items survives the values being reordered.
    pub fn identity(&self) -> String {
        let field = self.field.map(|f| f.0.to_string()).unwrap_or_default();
        let params: String = self.params.iter().map(|p| format!(";{p}")).collect();
        format!("{}({field}{params})", self.aggregate)
    }
}

/// An aggregate a Table Source offers over a base field, as it declares with
/// [`crate::Grid::set_aggregates`]: its name, and the names of the
/// parameters a value with it takes, in order. A value with no field is a
/// count, which every grouped source offers.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Aggregate {
    pub name: String,
    pub params: Vec<String>,
}

impl Aggregate {
    /// The aggregate `name`, which takes no parameters.
    pub fn new(name: impl Into<String>) -> Self {
        Aggregate {
            name: name.into(),
            params: Vec::new(),
        }
    }

    /// This aggregate taking the parameters named `params`.
    pub fn with_params(mut self, params: Vec<String>) -> Self {
        self.params = params;
        self
    }

    /// The standard aggregates over a field of numbers: count, distinct
    /// count, sum, average, minimum and maximum.
    pub fn numbers() -> Vec<Aggregate> {
        [
            "count",
            "distinct count",
            "sum",
            "average",
            "minimum",
            "maximum",
        ]
        .map(Aggregate::new)
        .to_vec()
    }

    /// The standard aggregates over any field: count and distinct count.
    pub fn counts() -> Vec<Aggregate> {
        ["count", "distinct count"].map(Aggregate::new).to_vec()
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

/// The most Group Item Keys an axis's `expand.except` takes: past it, a
/// toggle is refused and setting a depth is the tool that scales.
pub const EXCEPT_CAP: usize = 1000;

/// A toggle refused because the axis's `expand.except` would hold more
/// than [`EXCEPT_CAP`] Keys: nothing was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExceptCapped {
    pub cap: usize,
}

impl Expand {
    /// Everything open to `depth`, with no exceptions.
    pub fn to(depth: Depth) -> Self {
        Expand {
            depth,
            except: Vec::new(),
        }
    }

    /// Whether `depth` opens the groups at `level` (0 for the top groups)
    /// by default.
    pub fn opens_level(&self, level: u32) -> bool {
        match self.depth {
            Depth::All => true,
            Depth::Levels(n) => level < n,
        }
    }

    /// Whether the group whose Group Item is `key`, at `level`, is open.
    pub fn opens(&self, level: u32, key: Key) -> bool {
        self.opens_level(level) != self.except.contains(&key)
    }

    /// This state with the group `key`, at `level`, open or closed.
    pub fn with(mut self, level: u32, key: Key, open: bool) -> Self {
        self.except.retain(|&k| k != key);
        if self.opens_level(level) != open {
            self.except.push(key);
        }
        self
    }
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

    /// The value whose name is the table's caption: the only one, where
    /// there is one. Several values form the values pseudo-level instead.
    pub fn caption(&self) -> Option<&AggregateValue> {
        match self.values.as_slice() {
            [value] => Some(value),
            _ => None,
        }
    }

    /// Whether `axis` is grouped and its Grand Total placed after, which
    /// the Grid pins at the trailing edge by default.
    pub fn grand_total_after(&self, axis: Axis) -> bool {
        !self.levels(axis).is_empty() && self.axis(axis).grand_total == Placement::After
    }

    /// Whether this grouping is a pivot: the column axis comes from the
    /// data, as it does when there are column levels or the values
    /// pseudo-level is among the row levels. A pivot shows no Leaves: each
    /// group of an axis's innermost level is one item, which can't open.
    pub fn pivot(&self) -> bool {
        !self.col_levels.is_empty() || self.row_levels.contains(&Level::Values)
    }

    /// Whether the values pseudo-level is among `axis`'s levels, so that
    /// its Grand Total is one item per value, each its own Key, rather than
    /// the one [`GRAND_TOTAL`].
    pub fn splits_values(&self, axis: Axis) -> bool {
        self.levels(axis).contains(&Level::Values)
    }

    /// The value a group of the values pseudo-level names by its
    /// [`AggregateValue::identity`], and its index among the values.
    pub fn value_of(&self, identity: &str) -> Option<(usize, &AggregateValue)> {
        (self.values.iter().enumerate()).find(|(_, v)| v.identity() == identity)
    }

    /// Whether the item `outline` describes on `axis` draws blank apart
    /// from its label and count: an open Group Item whose level places a
    /// Subtotal, which carries the group's figures instead (as in Excel). A
    /// closed Group Item carries them itself.
    pub fn blank(&self, axis: Axis, outline: &Outline) -> bool {
        let subtotal = match self.levels(axis).get(outline.depth as usize) {
            Some(Level::Field { subtotal, .. }) => *subtotal,
            _ => Placement::Off,
        };
        outline.kind == ItemKind::Group
            && outline.open == OpenState::Open
            && subtotal != Placement::Off
    }

    /// Adds `level` to `axis` as its innermost level, but inside a values
    /// pseudo-level that is innermost already.
    pub(crate) fn add_level(&mut self, axis: Axis, level: Level) {
        let levels = self.levels_mut(axis);
        let at = match levels.last() {
            Some(Level::Values) if level != Level::Values => levels.len() - 1,
            _ => levels.len(),
        };
        levels.insert(at, level);
    }

    /// Takes the level of `axis` grouping by `field` (`None`: the values
    /// pseudo-level) out of it.
    pub(crate) fn take_level(&mut self, axis: Axis, field: Option<Key>) -> Option<Level> {
        let levels = self.levels_mut(axis);
        let i = levels.iter().position(|l| l.field_key() == field)?;
        Some(levels.remove(i))
    }

    /// Places the values pseudo-level as the values and column levels ask,
    /// after a level or value changed: several values on a table whose
    /// columns group by a field show it, innermost on the columns unless it
    /// is placed already; one value is a caption, with no level; and with
    /// no field left on the columns, a values level there leaves, so the
    /// table is no pivot unless its rows hold the values level.
    pub(crate) fn settle_values(&mut self) {
        let fields = |levels: &[Level]| levels.iter().any(|l| l.field_key().is_some());
        if self.values.len() < 2 || !fields(&self.col_levels) {
            self.col_levels.retain(|l| *l != Level::Values);
        }
        if self.values.len() < 2 {
            self.row_levels.retain(|l| *l != Level::Values);
        } else if fields(&self.col_levels)
            && !self.row_levels.contains(&Level::Values)
            && !self.col_levels.contains(&Level::Values)
        {
            self.col_levels.push(Level::Values);
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

/// The base rows under an activated cell of a Summary Item, as conditions
/// on the base fields: the grouping's slicers, then a OneOf per group on the
/// row's path and the column's, outermost first. Applied as slicers they
/// pick out the rows the cell aggregates.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DrillDown {
    pub conditions: Vec<Condition>,
}

impl DrillDown {
    pub fn new(conditions: Vec<Condition>) -> Self {
        DrillDown { conditions }
    }
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
    /// On a Group Item, how many items are shown under it: everything from
    /// the item after it to its last descendant, its Subtotal included. 0
    /// while it is closed, and on every other kind.
    pub span: u64,
    /// The group of each of `ancestors`, outermost first: with `group`, the
    /// item's whole path, from which the Grid builds a drill-down. A source
    /// that sends no path gets drill-downs naming its own group alone.
    pub path: Vec<GroupOf>,
    /// Where the item sits among its siblings, if the source says.
    pub set: Option<SetPosition>,
}

/// An item's place among its parent's children, as a tree reads it: a
/// group's children are its child groups or Leaves, with its Subtotal first
/// or last where its level places one; the top's are the top groups, with a
/// placed Grand Total first or last. It holds whether or not the parent is
/// open, and in either form. A closed group's Subtotal standing in for it in
/// header form, with its level placing none, has no place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SetPosition {
    /// 0 for the first child.
    pub index: u64,
    /// How many children the parent has.
    pub size: u64,
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
            span: 0,
            path: Vec::new(),
            set: None,
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
            span: 0,
            path: Vec::new(),
            set: None,
        }
    }

    /// The Subtotal of the group `field` = `value`, whose Group Item is the
    /// last of `ancestors`: it sits inside its group, a level below the
    /// Group Item.
    pub fn subtotal(ancestors: Vec<Key>, field: Key, value: Value) -> Self {
        Outline {
            kind: ItemKind::Subtotal,
            depth: ancestors.len() as u32,
            open: OpenState::CantOpen,
            ancestors,
            group: Some(GroupOf { field, value }),
            span: 0,
            path: Vec::new(),
            set: None,
        }
    }

    /// The Grand Total of an axis, at the top level.
    pub const fn grand_total() -> Self {
        Outline {
            kind: ItemKind::GrandTotal,
            depth: 0,
            open: OpenState::CantOpen,
            ancestors: Vec::new(),
            group: None,
            span: 0,
            path: Vec::new(),
            set: None,
        }
    }

    /// The same outline with `span` items shown under it (a Group Item).
    pub fn with_span(mut self, span: u64) -> Self {
        self.span = span;
        self
    }

    /// The same outline with the groups of its ancestors, outermost first.
    pub fn with_path(mut self, path: Vec<GroupOf>) -> Self {
        self.path = path;
        self
    }

    /// The groups the item belongs to, outermost first: its ancestors' and
    /// its own (a Subtotal's own is its last ancestor's).
    pub fn groups(&self) -> impl Iterator<Item = &GroupOf> {
        let own = (self.group.as_ref()).filter(|g| self.path.last() != Some(*g));
        self.path.iter().chain(own)
    }

    /// The same outline at `index` of `size` siblings.
    pub fn with_set(mut self, index: u64, size: u64) -> Self {
        self.set = Some(SetPosition { index, size });
        self
    }

    /// The Grand Total of one value, where the values pseudo-level is on
    /// the axis: at the top level, its group the value's identity under
    /// [`VALUES_FIELD`].
    pub fn grand_total_of(identity: String) -> Self {
        Outline {
            group: Some(GroupOf {
                field: VALUES_FIELD,
                value: Value::Text(identity),
            }),
            ..Outline::grand_total()
        }
    }

    /// The parent Group Item's Key; `None` at the top.
    pub fn parent(&self) -> Option<Key> {
        self.ancestors.last().copied()
    }
}
