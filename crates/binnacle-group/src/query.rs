//! The aggregate query interface: what the grouping layer asks its backend,
//! a batch per frame, and how the backend answers.

use std::ops::Range;

use binnacle_core::{AggregateValue, Condition, FilterSpec, Key, SortSpec, Value};

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
/// the levels (outermost first), the values to aggregate, the slicers, and
/// the axis's filter, hidden Leaf Keys and Sort Spec. The backend filters
/// the Leaves by the slicers, the filter and the hidden Keys before it
/// groups and aggregates them, and orders siblings by the Sort Spec.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Scope {
    pub levels: Vec<Key>,
    pub values: Vec<AggregateValue>,
    pub slicers: Vec<Condition>,
    pub filter: FilterSpec,
    pub hidden: Vec<Key>,
    pub sort: SortSpec,
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
