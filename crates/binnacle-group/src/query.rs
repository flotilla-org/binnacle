//! The aggregate query interface: what the grouping layer asks its backend,
//! a batch per frame, and how the backend answers.

use std::ops::Range;

use binnacle_core::{AggregateValue, Condition, ConditionKind, FilterSpec, Key, SortSpec, Value};

/// A group's place in the tree: its level's field and its value there, for
/// each level from the outermost. The empty path is the whole axis.
pub type Path = Vec<(Key, Value)>;

/// Which View Spec a batch was asked for. Answers carrying an older stamp
/// are dropped: a newer spec superseded them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Stamp(pub u64);

/// Names one request, so the backend can answer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub u64);

/// What every request of a batch shares, from the View Spec: the fields of
/// the row levels (outermost first), the values to aggregate, the slicers,
/// and the rows' filter, hidden Leaf Keys and Sort Spec. The backend filters
/// the Leaves by the slicers (`WHERE`), the filter and the hidden Keys
/// before it groups and aggregates them, and orders siblings by the Sort
/// Spec. A comparison in the filter on a field a value aggregates acts after
/// aggregating instead (`HAVING`, see [`having`]): on the groups by that
/// value's figure, never on the Leaves.
///
/// On a pivot, `columns` holds the fields of the column levels, which group
/// the same Leaves across, and `col_sort` orders the column groups. The
/// values pseudo-level is in neither list: where it goes is the layer's
/// concern, and the backend aggregates every value for every group.
///
/// On a pivot too, a sort column or a comparison may name an item of the
/// other axis by its Key, a path Key listed in `path_keys`: the rows sort
/// by a pivot column, or the columns filter by a row (`HAVING` on the
/// columns). It reads each group's figure where it crosses that item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Scope {
    pub levels: Vec<Key>,
    pub values: Vec<AggregateValue>,
    pub slicers: Vec<Condition>,
    pub filter: FilterSpec,
    pub hidden: Vec<Key>,
    pub sort: SortSpec,
    pub columns: Vec<Key>,
    /// The column axis's Sort Spec: a sort column on a column level's field
    /// orders its groups by their value, one on a path Key (the rows' Grand
    /// Total, `GRAND_TOTAL`, say) by their figure in that row. Unsorted, or
    /// on a tie, column groups come in ascending value.
    pub col_sort: SortSpec,
    /// The column axis's filter: a comparison on a path Key, a row, leaves
    /// out the column groups whose figure in that row fails it, with
    /// everything under them. Nothing else in it applies.
    pub col_filter: FilterSpec,
    /// The items of either axis that the sorts and filters name, each by
    /// its Key with the path it crosses at.
    pub path_keys: Vec<PathKey>,
}

/// An item of a pivot named by its Key in a sort or a filter of the other
/// axis: the data path of its group on its own axis (the columns' for a
/// pivot column, the rows' for a row; empty for a Grand Total) and the
/// index of the value whose figure it shows (the first where it shows
/// every value). A group's figure there is its cell where its path and
/// `path` cross: the figure of its rows that are also under `path`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathKey {
    pub key: Key,
    pub path: Path,
    pub value: usize,
}

impl Scope {
    /// The path Key `key`, if a sort or a filter names it.
    pub fn path_key(&self, key: Key) -> Option<&PathKey> {
        self.path_keys.iter().find(|p| p.key == key)
    }
}

/// The value whose figure `condition`, in a grouped axis's filter, reads
/// once aggregated (`HAVING`): a comparison on a field one of `values`
/// aggregates reads the first such value's figure, and a group whose figure
/// fails it is left out with everything under it. Its Subtotal goes with
/// it; the figures of the groups above it and the Grand Total stay as they
/// are. Other conditions act on the Leaves, before aggregating.
pub fn having(condition: &Condition, values: &[AggregateValue]) -> Option<usize> {
    match condition.kind {
        ConditionKind::Compare(_) => values.iter().position(|v| v.field == Some(condition.key)),
        _ => None,
    }
}

/// What a request asks for under its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Want {
    /// The groups of the next level, in order, `range` of them, each with
    /// its value, aggregates and child count, and how many there are.
    Groups,
    /// The Leaf Keys of the group, in order, `range` of them.
    Leaves,
    /// Where the Leaf with this Key is: its group's path and its place
    /// among that group's Leaves. The request's path and range are empty.
    Locate(Key),
    /// The aggregates of the whole group at the path: at the empty path,
    /// the Grand Total's. The range is empty.
    Total,
    /// On a pivot, the distinct values of the next column level under the
    /// path, a column path: `range` of the column groups there, in the
    /// column Sort Spec's order, each with its value, its child count and
    /// its aggregates over all the rows, and how many there are.
    Columns,
}

/// One aggregate query.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Request {
    pub id: RequestId,
    pub path: Path,
    pub want: Want,
    pub range: Range<u64>,
}

/// One frame's queries, sent together. A backend answers each by id, in any
/// order, in parts or jointly, or with more than asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub stamp: Stamp,
    pub scope: Scope,
    pub requests: Vec<Request>,
}

/// One group, as a backend gives it: its value at its level, how many
/// children it has (groups of the next level, or Leaves at the innermost),
/// and its aggregates, one per value of the scope, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupRow<A> {
    pub value: Value,
    pub children: u64,
    pub aggregates: Vec<A>,
    /// On a pivot, a row group's aggregates under each column group, by
    /// the column group's path, for every column group at every column
    /// level, each one per value of the scope; empty otherwise.
    pub cells: Vec<(Path, Vec<A>)>,
}

impl<A> GroupRow<A> {
    /// A group with no cells, as off a pivot.
    pub fn new(value: Value, children: u64, aggregates: Vec<A>) -> Self {
        GroupRow {
            value,
            children,
            aggregates,
            cells: Vec::new(),
        }
    }
}

/// What an answer holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Body<A> {
    /// Groups under the answer's path from its `start`, and how many groups
    /// there are under it in all.
    Groups {
        total: u64,
        groups: Vec<GroupRow<A>>,
    },
    /// Leaf Keys of the group at the answer's path, from its `start`.
    Leaves { keys: Vec<Key> },
    /// Where the Leaf `key` is: under the answer's path at `start`, or gone
    /// from the view (`found` false).
    Located { key: Key, found: bool },
    /// The aggregates of the whole group at the answer's path, one per
    /// value of the scope, in order.
    Total { aggregates: Vec<A> },
    /// Column groups under the answer's path, a column path, from its
    /// `start`, and how many there are under it in all.
    Columns {
        total: u64,
        groups: Vec<GroupRow<A>>,
    },
}

/// A backend's answer, self-describing so that it can come in any order,
/// in parts or unasked: the batch's stamp, the request it answers (`None`
/// for data the layer did not ask for), and where its body belongs. A
/// request is done once an answer names its id; whatever it left out is
/// asked again when still needed.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer<A> {
    pub stamp: Stamp,
    pub id: Option<RequestId>,
    pub path: Path,
    pub start: u64,
    pub body: Body<A>,
}

/// A backend that answers a batch at once, for hosts whose data is at hand
/// (see [`crate::settle`]). Others answer [`crate::GroupingLayer::answer`]
/// as their answers arrive.
pub trait AggregateBackend<A> {
    fn answer(&mut self, batch: &Batch) -> Vec<Answer<A>>;
}
