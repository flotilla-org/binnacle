//! The grouping layer: a sans-IO state machine that is a Table Source for
//! the grouped axes, built from the answers to its aggregate queries.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, ConditionTree, Expand, Form, GRAND_TOTAL, GroupOf, Grouping, ItemKind, Key,
    Level, Located, OpenState, Outline, Placement, Position, SetPosition, SortSpec, Totals,
    VALUES_FIELD, Value, ViewSpec,
};
use binnacle_memory::{KeyInterner, Viewed};

use crate::query::{
    AggregateBackend, Answer, Batch, Body, GroupRow, Path, PathKey, Request, RequestId, Scope,
    Stamp, Want,
};

/// How many groups or Leaves one request asks for.
const PAGE: u64 = 64;

/// A path, or a part of one, borrowed.
type Steps = [(Key, Value)];

/// The bit set in every Summary Item Key the layer issues, so they never
/// meet a Leaf Key: a backend's Leaf Keys must keep it clear. The Grand
/// Total is the first, [`GRAND_TOTAL`].
pub const SUMMARY_KEY_BIT: u64 = 1 << 63;

/// What a Summary Item Key is interned from: its kind and its group's path.
/// Kind keeps a real blank group apart from a Subtotal's empty slot. A path
/// through the values pseudo-level holds the value's identity under
/// [`VALUES_FIELD`]; a pivot column is the `Group` of its column path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SummaryId {
    Group(Path),
    Subtotal(Path),
    /// The Subtotal of a group above the values pseudo-level for one
    /// value: the group's path, then the value's identity under
    /// [`VALUES_FIELD`], as a pivot column's path holds it.
    ValueSubtotal(Path),
    GrandTotal,
    /// The Grand Total of one value, by its identity, where the values
    /// pseudo-level is on the axis.
    ValueTotal(String),
}

/// A Summary Item as the layer holds it, for drawing: its kind, its group's
/// path (empty for the Grand Total; for a value's Subtotal, its group's
/// then the value's step), how many children the group has (the top
/// groups, for the Grand Total) and its aggregates, one per value of the
/// grouping, as the backend gave them. A Subtotal's are its group's, and a
/// group of the values pseudo-level's those of the group it is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary<'a, A> {
    pub kind: ItemKind,
    pub path: &'a Path,
    pub children: u64,
    pub aggregates: &'a [A],
}

/// Something to ask the backend: `path` is a data path, with no values
/// pseudo-level in it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ask {
    path: Path,
    want: Want,
    range: Range<u64>,
}

/// The path `path` leaves the backend: its steps through the values
/// pseudo-level left out.
fn data(path: &Path) -> Path {
    (path.iter())
        .filter(|(f, _)| *f != VALUES_FIELD)
        .cloned()
        .collect()
}

/// How one grouped axis is laid out: its levels, where the Subtotals of
/// each and the Grand Total go, whether it is in header form, where a
/// group has no Group Item (a closed one shows its Subtotal alone), and
/// whether it is a pivot's, where the innermost groups are the items.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Shape {
    /// One per level, outermost first: its field, or `None` for the values
    /// pseudo-level.
    steps: Vec<Option<Key>>,
    /// One per level.
    subtotals: Vec<Placement>,
    pub(crate) grand_total: Placement,
    header: bool,
    pivot: bool,
    /// The identities of the values, which the values pseudo-level groups.
    values: Vec<String>,
    /// Whether this is the columns' shape, whose groups are asked as
    /// [`Want::Columns`].
    columns: bool,
}

impl Shape {
    fn of(spec: &ViewSpec, axis: Axis) -> Shape {
        let Some(grouping) = &spec.grouping else {
            return Shape::default();
        };
        let levels = grouping.levels(axis);
        let steps: Vec<Option<Key>> = levels.iter().map(Level::field_key).collect();
        let pivot = grouping.pivot();
        let grand_total = match steps.as_slice() {
            // A pivot's axis with no levels shows its Grand Total alone,
            // and one with the values alone repeats nothing in one.
            [] if pivot => Placement::Before,
            [None] => Placement::Off,
            _ => grouping.axis(axis).grand_total,
        };
        Shape {
            subtotals: (levels.iter())
                .map(|l| match l {
                    Level::Field { subtotal, .. } => *subtotal,
                    _ => Placement::Off,
                })
                .collect(),
            steps,
            grand_total,
            header: grouping.axis(axis).form == Form::Header,
            pivot,
            values: grouping.values.iter().map(|v| v.identity()).collect(),
            columns: axis == Axis::Cols,
        }
    }

    /// What asks for this axis's groups.
    fn groups(&self) -> Want {
        match self.columns {
            true => Want::Columns,
            false => Want::Groups,
        }
    }

    pub(crate) fn subtotal(&self, level: usize) -> Placement {
        self.subtotals.get(level).copied().unwrap_or_default()
    }

    /// Whether a group of `level` is above the values pseudo-level, so it
    /// has one Subtotal per value.
    fn splits(&self, level: usize) -> bool {
        !self.values.is_empty() && (self.steps.get(level + 1..)).is_some_and(|s| s.contains(&None))
    }

    /// How many Subtotals a group of `level` has, where they are placed.
    fn per(&self, level: usize) -> u64 {
        match self.splits(level) {
            true => self.values.len() as u64,
            false => 1,
        }
    }

    /// How many items an open group of `level` shows besides those under
    /// it: its Group Item (in outline form) and any Subtotals.
    fn own(&self, level: usize) -> u64 {
        u64::from(!self.header)
            + self.per(level) * u64::from(self.subtotal(level) != Placement::Off)
    }

    /// Where the items under a group of `level` start, past its Group Item
    /// (in outline form) and Subtotals placed before.
    fn lead(&self, level: usize) -> u64 {
        u64::from(!self.header)
            + self.per(level) * u64::from(self.subtotal(level) == Placement::Before)
    }

    /// Whether a closed group of `level` shows its Subtotals, placed or
    /// not: in header form, where it has no Group Item, and above the
    /// values pseudo-level, where its Group Item has no one figure.
    fn stands_in(&self, level: usize) -> bool {
        self.header || self.splits(level)
    }

    /// How many items a closed group of `level` with something under it
    /// shows: its Group Item in outline form, and its Subtotals where they
    /// stand in.
    fn closed(&self, level: usize) -> u64 {
        u64::from(!self.header) + self.per(level) * u64::from(self.stands_in(level))
    }

    /// Where the items a group of `level` hides when it closes start, past
    /// what stays: its Group Item in outline form, and Subtotals placed
    /// before where they stand in.
    fn kept(&self, level: usize) -> u64 {
        match self.stands_in(level) {
            true => self.lead(level),
            false => 1,
        }
    }

    /// Whether the Grand Total is one item per value.
    fn split(&self) -> bool {
        self.steps.contains(&None)
    }

    /// How many Grand Total items the axis shows.
    fn totals(&self) -> u64 {
        match (self.grand_total, self.split()) {
            (Placement::Off, _) => 0,
            (_, true) => self.values.len() as u64,
            (_, false) => 1,
        }
    }

    /// Where the top groups start: past a Grand Total placed before.
    pub(crate) fn top(&self) -> u64 {
        match self.grand_total {
            Placement::Before => self.totals(),
            _ => 0,
        }
    }
}

/// Which groups of an axis are open, as its `expand` says.
#[derive(Clone, Debug, Default)]
pub(crate) struct Open {
    expand: Expand,
    except: HashSet<Key>,
    /// The values of the groups `except` names, by their parent's path.
    named: HashMap<Path, Vec<Value>>,
}

impl Open {
    fn of(spec: &ViewSpec, axis: Axis, keys: &KeyInterner<SummaryId>) -> Open {
        let expand =
            (spec.grouping.as_ref()).map_or_else(Expand::default, |g| g.axis(axis).expand.clone());
        let except: HashSet<Key> = expand.except.iter().copied().collect();
        let mut named: HashMap<Path, Vec<Value>> = HashMap::new();
        for key in except.iter().filter(|k| k.0 & SUMMARY_KEY_BIT != 0) {
            if let Some(SummaryId::Group(path)) = keys.identity(Key(key.0 & !SUMMARY_KEY_BIT))
                && let Some(((_, value), parent)) = path.split_last()
            {
                named
                    .entry(parent.to_vec())
                    .or_default()
                    .push(value.clone());
            }
        }
        Open {
            expand,
            except,
            named,
        }
    }

    fn same(&self, other: &Open) -> bool {
        self.expand.depth == other.expand.depth && self.except == other.except
    }

    /// Whether the group whose Group Item is `key`, at `level`, is open,
    /// whatever is under it.
    pub(crate) fn opens(&self, level: usize, key: Key) -> bool {
        self.expand.opens_level(level as u32) != self.except.contains(&key)
    }

    /// Whether the group at `node` shows what is under it: it is open and
    /// has something to show.
    fn shows<A>(&self, node: &Node<A>) -> bool {
        let Some(level) = node.path.len().checked_sub(1) else {
            return true;
        };
        node.children() > 0 && self.opens(level, node.key)
    }

    /// Whether a group under `node` not yet fetched may be open, so that
    /// laying the node out needs every group under it: their level opens
    /// by default, or `except` opens one the node has not been answered.
    fn needs_all<A>(&self, node: &Node<A>) -> bool {
        let Kids::Groups(g) = &node.kids else {
            return false;
        };
        self.expand.opens_level(node.path.len() as u32)
            || (self.named.get(&node.path))
                .is_some_and(|values| values.iter().any(|v| !g.by_value.contains_key(v)))
    }
}

/// Whether a group's `path` fits an axis whose levels are `steps`: each of
/// its fields is the field of the level at its depth.
fn fits_steps(path: &Path, steps: &[Option<Key>]) -> bool {
    !path.is_empty()
        && path.len() <= steps.len()
        && (path.iter().zip(steps)).all(|((f, _), s)| *f == s.unwrap_or(VALUES_FIELD))
}

/// Leaves out of `expand.except` the Group Item Keys whose paths do not fit
/// `steps`. Keys the layer never issued, and Leaf Keys, stay.
fn prune(keys: &KeyInterner<SummaryId>, steps: &[Option<Key>], expand: &mut Expand) {
    expand.except.retain(|k| {
        if k.0 & SUMMARY_KEY_BIT == 0 {
            return true;
        }
        match keys.identity(Key(k.0 & !SUMMARY_KEY_BIT)) {
            None => true,
            Some(SummaryId::Group(path)) => fits_steps(path, steps),
            Some(_) => false,
        }
    });
}

/// The path Key `key` names on a pivot grouped as `grouping`, if the layer
/// issued it: the data path of its group and the index of its value.
fn path_key(keys: &KeyInterner<SummaryId>, grouping: &Grouping, key: Key) -> Option<PathKey> {
    if key.0 & SUMMARY_KEY_BIT == 0 {
        return None;
    }
    let (path, value) = match keys.identity(Key(key.0 & !SUMMARY_KEY_BIT))? {
        SummaryId::Group(p) | SummaryId::Subtotal(p) => (data(p), value_on(p)),
        SummaryId::GrandTotal => (Vec::new(), None),
        SummaryId::ValueTotal(v) => (Vec::new(), Some(v.as_str())),
    };
    let value = match value {
        Some(v) => grouping.value_of(v)?.0,
        None => 0,
    };
    Some(PathKey { key, path, value })
}

/// The Keys `spec`'s sorts and filters name on a pivot, each with the axis
/// whose item it names: the rows' name columns, the columns' rows.
fn named(spec: &ViewSpec) -> Vec<(Axis, Key)> {
    let pivot = (spec.grouping.as_ref()).is_some_and(Grouping::pivot);
    if !pivot {
        return Vec::new();
    }
    let mut named = Vec::new();
    for axis in Axis::ALL {
        let view = spec.axis(axis);
        let sorted = view.sort.columns().iter().map(|c| c.key);
        let filtered = view.filter.conditions.conditions().iter().map(|c| c.key);
        named.extend((sorted.chain(filtered)).map(|k| (axis.other(), k)));
    }
    named.retain(|(_, k)| k.0 & SUMMARY_KEY_BIT != 0);
    named
}

/// A group (or the whole axis, at the root) and what is known under it.
struct Node<A> {
    path: Path,
    /// Its Group Item's Key.
    key: Key,
    row: Option<GroupRow<A>>,
    /// On a pivot's row group, its aggregates under each column group, by
    /// the column group's data path.
    cells: HashMap<Path, Vec<A>>,
    kids: Kids<A>,
    /// The Window that last read its Group Item or asked for its page, 0
    /// if none did: the cache lets go of the oldest.
    used: Cell<u64>,
}

enum Kids<A> {
    /// The groups of the next level.
    Groups(Groups<A>),
    /// The Leaves fetched, in runs of at most a page, by the index of
    /// their first; runs never overlap.
    Leaves(BTreeMap<u64, Run>),
    /// Nothing: a pivot's innermost group is an item of its own.
    Nothing,
}

struct Run {
    keys: Vec<Key>,
    used: Cell<u64>,
}

struct Groups<A> {
    total: Option<u64>,
    /// The groups fetched, by index.
    nodes: BTreeMap<u64, Node<A>>,
    by_value: HashMap<Value, u64>,
    layout: Option<Layout>,
    /// The groups of the values pseudo-level, known from the start.
    values: bool,
}

impl<A> Groups<A> {
    fn unknown() -> Self {
        Groups {
            total: None,
            nodes: BTreeMap::new(),
            by_value: HashMap::new(),
            layout: None,
            values: false,
        }
    }
}

/// Where the items under a node sit, worked out once every group under it
/// that may show what is under it is known. A closed group shows the same
/// items as any other of its level ([`Shape::closed`]), so the groups not
/// fetched take that many Positions each.
#[derive(Clone, Debug, Default)]
struct Layout {
    /// The groups shown open, and those with nothing under them where a
    /// closed group shows more than its Group Item, by index.
    open: Vec<Span>,
    /// How many items a closed group shows.
    closed: u64,
    /// How many items are shown under the node.
    shown: u64,
}

/// One group's place among the items under its parent: its index, the
/// Position of its first item, how many items it shows, its Group Item and
/// Subtotals included, and whether it is open.
#[derive(Clone, Copy, Debug)]
struct Span {
    index: u64,
    at: u64,
    len: u64,
    open: bool,
}

impl Layout {
    /// Where the first item of the group at `index` sits.
    fn position(&self, index: u64) -> u64 {
        let i = self.open.partition_point(|s| s.index < index);
        match i.checked_sub(1).map(|j| self.open[j]) {
            Some(s) => s.at + s.len + (index - s.index - 1) * self.closed,
            None => index * self.closed,
        }
    }

    /// The span of the group at `index`, if it is shown open.
    fn span(&self, index: u64) -> Option<Span> {
        let i = self.open.partition_point(|s| s.index < index);
        self.open
            .get(i)
            .filter(|s| s.index == index && s.open)
            .copied()
    }

    /// The group whose items take `position`, and how far into them it is.
    fn at(&self, position: u64) -> (u64, u64) {
        let i = self.open.partition_point(|s| s.at <= position);
        let (index, past) = match i.checked_sub(1).map(|j| self.open[j]) {
            Some(s) if position < s.at + s.len => return (s.index, position - s.at),
            Some(s) => (s.index + 1, position - s.at - s.len),
            None => (0, position),
        };
        (index + past / self.closed, past % self.closed)
    }
}

/// What is under a group at `path` before anything is known: groups to ask
/// for, the values pseudo-level's groups, Leaves or nothing.
fn kids<A>(path: &Path, shape: &Shape, keys: &mut KeyInterner<SummaryId>) -> Kids<A> {
    match shape.steps.get(path.len()) {
        Some(Some(_)) => Kids::Groups(Groups::unknown()),
        Some(None) => {
            let mut groups = Groups::unknown();
            groups.values = true;
            groups.total = Some(shape.values.len() as u64);
            for (i, id) in (0..).zip(&shape.values) {
                let value = Value::Text(id.clone());
                let mut path = path.clone();
                path.push((VALUES_FIELD, value.clone()));
                let key = summary_key(keys, SummaryId::Group(path.clone()));
                let kids = kids(&path, shape, keys);
                groups.by_value.insert(value.clone(), i);
                let row = GroupRow::new(value, 0, Vec::new());
                groups.nodes.insert(
                    i,
                    Node {
                        path,
                        key,
                        row: Some(row),
                        cells: HashMap::new(),
                        kids,
                        used: Cell::new(0),
                    },
                );
            }
            Kids::Groups(groups)
        }
        None if shape.pivot => Kids::Nothing,
        None => Kids::Leaves(BTreeMap::new()),
    }
}

/// What of `range` (clipped to `total`) `has` lacks, a page at a time:
/// from the first index lacking in each page to its end, so that a backend
/// that answers a request in part makes progress.
fn missing(range: Range<u64>, total: u64, has: impl Fn(u64) -> bool) -> Vec<Range<u64>> {
    let end = range.end.min(total);
    let first = range.start / PAGE * PAGE;
    (first..end)
        .step_by(PAGE as usize)
        .filter_map(|page| {
            let stop = (page + PAGE).min(end);
            let lacking = (page.max(range.start)..stop).find(|&i| !has(i))?;
            Some(lacking..stop)
        })
        .collect()
}

impl<A> Node<A> {
    /// A node at `path`, nothing under it known yet.
    fn new(
        path: Path,
        key: Key,
        mut row: Option<GroupRow<A>>,
        shape: &Shape,
        keys: &mut KeyInterner<SummaryId>,
        used: u64,
    ) -> Self {
        let kids = kids(&path, shape, keys);
        let cells = (row.as_mut())
            .map(|r| std::mem::take(&mut r.cells).into_iter().collect())
            .unwrap_or_default();
        Node {
            path,
            key,
            row,
            cells,
            kids,
            used: Cell::new(used),
        }
    }

    /// How many children the group has: groups of the next level (the
    /// values, at the values pseudo-level), or Leaves.
    fn children(&self) -> u64 {
        match &self.kids {
            Kids::Nothing => 0,
            Kids::Groups(g) if g.values => g.nodes.len() as u64,
            // A group of the values pseudo-level holds what the group it
            // is in holds: until asked, something.
            Kids::Groups(g) if self.virtual_() => g.total.unwrap_or(1),
            _ => self.row.as_ref().map_or(0, |r| r.children),
        }
    }

    /// Whether this is a group of the values pseudo-level.
    fn virtual_(&self) -> bool {
        self.path.last().is_some_and(|(f, _)| *f == VALUES_FIELD)
    }

    /// How many children this node has, the root's top groups included.
    fn kids(&self) -> u64 {
        match (&self.row, &self.kids) {
            (None, Kids::Groups(g)) => g.total.unwrap_or(0),
            _ => self.children(),
        }
    }

    /// How many items are shown under this node, once laid out.
    fn shown(&self) -> Option<u64> {
        match &self.kids {
            Kids::Groups(g) => g.layout.as_ref().map(|l| l.shown),
            Kids::Leaves(_) => Some(self.children()),
            Kids::Nothing => Some(0),
        }
    }

    /// Lays out the items under this node as `open` opens them, as far as
    /// the groups known allow; returns how many are shown, if known. A
    /// group with nothing under it shows its Group Item alone.
    fn finish(&mut self, shape: &Shape, open: &Open) -> Option<u64> {
        let own = shape.own(self.path.len());
        let closed = shape.closed(self.path.len());
        let all = open.needs_all(self);
        let Kids::Groups(g) = &mut self.kids else {
            return self.shown();
        };
        if let Some(layout) = &g.layout {
            return Some(layout.shown);
        }
        let total = g.total?;
        if all && (g.nodes.len() as u64) < total {
            return None;
        }
        let mut layout = Layout {
            open: Vec::new(),
            closed,
            shown: 0,
        };
        let mut done = true;
        for (&index, node) in &mut g.nodes {
            let (len, open) = match node.children() {
                0 if closed != 1 => (1, false),
                _ if !open.shows(node) => continue,
                _ => match node.finish(shape, open) {
                    Some(n) => (own + n, true),
                    None => {
                        done = false;
                        continue;
                    }
                },
            };
            let at = layout.position(index);
            layout.open.push(Span {
                index,
                at,
                len,
                open,
            });
        }
        if !done {
            return None;
        }
        layout.shown = layout.position(total);
        let shown = layout.shown;
        g.layout = Some(layout);
        Some(shown)
    }

    /// Forgets where the items under this node sit, for a new shape.
    fn unfinish(&mut self) {
        if let Kids::Groups(g) = &mut self.kids {
            g.layout = None;
            g.nodes.values_mut().for_each(Node::unfinish);
        }
    }

    /// The Leaf at `index` among this group's Leaves, if fetched, marked
    /// read in `frame`.
    fn leaf(&self, index: u64, frame: Option<u64>) -> Option<Key> {
        let Kids::Leaves(runs) = &self.kids else {
            return None;
        };
        let (start, run) = runs.range(..=index).next_back()?;
        let key = run.keys.get((index - start) as usize).copied()?;
        if let Some(frame) = frame {
            run.used.set(frame);
        }
        Some(key)
    }

    fn child(&self, value: &Value) -> Option<&Node<A>> {
        let Kids::Groups(g) = &self.kids else {
            return None;
        };
        g.nodes.get(g.by_value.get(value)?)
    }

    fn child_mut(&mut self, value: &Value) -> Option<&mut Node<A>> {
        let Kids::Groups(g) = &mut self.kids else {
            return None;
        };
        g.nodes.get_mut(g.by_value.get(value)?)
    }

    /// What is still to ask for (`want`, groups or columns) so that the
    /// items under this node can be laid out as `open` opens them.
    fn needs(&self, open: &Open, want: Want, asks: &mut Vec<Ask>) {
        let Kids::Groups(g) = &self.kids else {
            return;
        };
        if !g.values {
            let Some(total) = g.total else {
                asks.push(Ask {
                    path: data(&self.path),
                    want,
                    range: 0..PAGE,
                });
                return;
            };
            // A pivot's columns are laid out whole: every group under an
            // open one.
            if want == Want::Columns || open.needs_all(self) {
                let known = |i| g.nodes.contains_key(&i);
                for range in missing(0..total, total, known) {
                    asks.push(Ask {
                        path: data(&self.path),
                        want,
                        range,
                    });
                }
            }
        }
        for node in g.nodes.values() {
            match open.shows(node) {
                true => node.needs(open, want, asks),
                // Whether a closed group of the values pseudo-level can
                // open: how many groups it holds.
                false => match &node.kids {
                    Kids::Groups(k) if !k.values && k.total.is_none() && node.virtual_() => asks
                        .push(Ask {
                            path: data(&node.path),
                            want,
                            range: 0..0,
                        }),
                    _ => {}
                },
            }
        }
    }

    /// What is still to ask for (`want`, groups or columns) to know whether
    /// a group at `rest` past this node is there, open or not.
    fn reach(&self, rest: &[(Key, Value)], want: Want, asks: &mut Vec<Ask>) {
        let (Some(((field, value), rest)), Kids::Groups(g)) = (rest.split_first(), &self.kids)
        else {
            return;
        };
        match self.child(value) {
            Some(child) if child.path.last().is_some_and(|(f, _)| f == field) => {
                return child.reach(rest, want, asks);
            }
            Some(_) => return,
            None if g.values => return,
            None => {}
        }
        let path = data(&self.path);
        match g.total {
            None => asks.push(Ask {
                path,
                want,
                range: 0..PAGE,
            }),
            Some(total) => {
                let known = |i| g.nodes.contains_key(&i);
                asks.extend(missing(0..total, total, known).into_iter().map(|range| Ask {
                    path: path.clone(),
                    want,
                    range,
                }));
            }
        }
    }

    /// Forgets what is under the groups `open` closes, so memory grows only
    /// with the open groups.
    fn fold(&mut self, open: &Open, shape: &Shape, keys: &mut KeyInterner<SummaryId>) {
        let Kids::Groups(g) = &mut self.kids else {
            return;
        };
        for node in g.nodes.values_mut() {
            if open.shows(node) {
                node.fold(open, shape, keys);
                continue;
            }
            let total = match &node.kids {
                Kids::Groups(g) if !g.values && node.virtual_() => g.total,
                _ => None,
            };
            node.kids = kids(&node.path, shape, keys);
            // A closed group of the values pseudo-level keeps how many
            // groups it holds, which its own row does not say.
            if let (Some(total), Kids::Groups(g)) = (total, &mut node.kids) {
                g.total = Some(total);
            }
        }
    }

    /// The paths of the groups held under this node, each with its row.
    fn held_groups(&self, out: &mut Vec<Path>) {
        let Kids::Groups(g) = &self.kids else {
            return;
        };
        for node in g.nodes.values() {
            out.push(node.path.clone());
            node.held_groups(out);
        }
    }

    /// The group under this node whose fetched Leaves hold `key`.
    fn holding(&self, key: Key) -> Option<&Node<A>> {
        match &self.kids {
            Kids::Groups(g) => g.nodes.values().find_map(|n| n.holding(key)),
            Kids::Leaves(runs) => runs
                .values()
                .any(|run| run.keys.contains(&key))
                .then_some(self),
            Kids::Nothing => None,
        }
    }

    /// How many groups and Leaves are held under this node.
    fn held(&self) -> u64 {
        match &self.kids {
            Kids::Groups(g) => g.nodes.values().map(|n| 1 + n.held()).sum(),
            Kids::Leaves(runs) => runs.values().map(|r| r.keys.len() as u64).sum(),
            Kids::Nothing => 0,
        }
    }

    /// Lists what under this node the cache may let go of: the Leaves of
    /// open groups, and the groups no `open` opens under a node laid out
    /// without them. Returns how many items those hold.
    fn cached(&self, opens: [&Open; 2], out: &mut Vec<(u64, Path, Spot)>) -> u64 {
        match &self.kids {
            Kids::Leaves(runs) => {
                for (&start, run) in runs {
                    out.push((run.used.get(), self.path.clone(), Spot::Leaves(start)));
                }
                runs.values().map(|r| r.keys.len() as u64).sum()
            }
            Kids::Nothing => 0,
            Kids::Groups(g) => {
                // The values pseudo-level's groups are never fetched, so
                // never let go.
                let paged = !g.values && opens.iter().all(|o| !o.needs_all(self));
                let mut n = 0;
                for (&index, node) in &g.nodes {
                    if opens.iter().any(|o| o.shows(node)) || node.virtual_() {
                        n += node.cached(opens, out);
                    } else if paged {
                        out.push((node.used.get(), self.path.clone(), Spot::Group(index)));
                        n += 1;
                    }
                }
                n
            }
        }
    }
}

/// Something the cache holds under a node: the run of Leaves from an
/// index, or the closed group at an index.
#[derive(Clone, Copy, Debug)]
enum Spot {
    Leaves(u64),
    Group(u64),
}

/// Puts `keys`, the Leaves from `start`, in `runs` in runs of at most a
/// page, over any held there; a run's `used` is `fresh` where it says, or
/// that of what it replaces if later.
fn insert_leaves(
    runs: &mut BTreeMap<u64, Run>,
    start: u64,
    keys: Vec<Key>,
    fresh: impl Fn(Range<u64>) -> u64,
) -> u64 {
    let end = start + keys.len() as u64;
    let over: Vec<u64> = (runs.range(..end))
        .filter(|(s, r)| *s + r.keys.len() as u64 > start)
        .map(|(s, _)| *s)
        .collect();
    // What it replaces stays as fresh as it was.
    let mut was: Vec<(Range<u64>, u64)> = Vec::new();
    for s in over {
        let run = runs.remove(&s).expect("listed");
        let e = s + run.keys.len() as u64;
        was.push((s.max(start)..e.min(end), run.used.get()));
        if s < start {
            let keys = run.keys[..(start - s) as usize].to_vec();
            let used = run.used.clone();
            runs.insert(s, Run { keys, used });
        }
        if e > end {
            let keys = run.keys[(end - s) as usize..].to_vec();
            runs.insert(
                end,
                Run {
                    keys,
                    used: run.used,
                },
            );
        }
    }
    let mut at = start;
    let mut rest = keys.as_slice();
    while !rest.is_empty() {
        let len = ((at / PAGE + 1) * PAGE - at).min(rest.len() as u64);
        let (run, next) = rest.split_at(len as usize);
        let replaced = (was.iter())
            .filter(|(r, _)| r.start < at + len && at < r.end)
            .map(|(_, used)| *used);
        let used = Cell::new(replaced.fold(fresh(at..at + len), u64::max));
        runs.insert(
            at,
            Run {
                keys: run.to_vec(),
                used,
            },
        );
        at += len;
        rest = next;
    }
    keys.len() as u64
}

/// One grouped axis: its shape, which groups are open in the layout given
/// and which the latest spec opens (they differ while what they show is
/// asked for), and its tree.
struct Tree<A> {
    shape: Shape,
    open: Open,
    want: Open,
    root: Node<A>,
    /// Whether the layout under `open` is worked out and given.
    laid_out: bool,
}

impl<A> Tree<A> {
    fn new(shape: Shape, keys: &mut KeyInterner<SummaryId>) -> Self {
        let root = Node::new(Vec::new(), GRAND_TOTAL, None, &shape, keys, 0);
        Tree {
            shape,
            open: Open::default(),
            want: Open::default(),
            root,
            laid_out: false,
        }
    }

    /// Lays the groups out as `want` opens them.
    fn lay_out(&mut self) {
        self.open = self.want.clone();
        self.root.unfinish();
        self.root.finish(&self.shape, &self.open);
    }

    /// How many items the axis shows, once laid out.
    fn count(&self) -> Option<u64> {
        if !self.laid_out {
            return None;
        }
        Some(self.root.shown()? + self.shape.totals())
    }
}

/// The grouping layer for one table: a Table Source for each grouped axis,
/// asking an aggregate backend for what it needs, in batches, and holding
/// only what it has been answered.
///
/// A host feeds it each requested View Spec ([`GroupingLayer::request_view`]),
/// sends each [`Batch`] it gives to the backend and hands back the answers
/// as they come ([`GroupingLayer::answer`]), and takes the applied spec with
/// its Change Notices from [`GroupingLayer::poll_viewed`] once the layer
/// can lay the axis out. Windows ([`GroupingLayer::window`]) answer with
/// what is known and ask for the rest in the next batch: a group not yet
/// answered takes its Position all the same, so the axis is laid out once
/// the open groups are known, and the Grid draws the rest pending until a
/// Window finds them. Axes it does not group are not the layer's: the
/// host's own source answers them.
///
/// On a pivot both axes are the layer's: the columns come from the data,
/// each innermost column group one column, and [`GroupingLayer::cell`]
/// gives a cell's figure. Any change to them but one group opening or
/// closing comes as `Refiltered`.
///
/// It holds the open groups and, as a cache, pages of closed groups and of
/// Leaves: always those the latest two Windows read, and others up to its
/// size, letting go of those read longest ago. Memory grows with the open
/// groups, never with the rows.
pub struct GroupingLayer<A> {
    stamp: Stamp,
    spec: ViewSpec,
    /// The scope while the rows are grouped.
    scope: Option<Scope>,
    /// The rows, then the columns.
    axes: [Tree<A>; 2],
    /// The Grand Total's aggregates, once answered.
    total: Option<Vec<A>>,
    keys: KeyInterner<SummaryId>,
    next_id: u64,
    in_flight: Vec<(RequestId, Ask)>,
    /// What Windows and Locates found missing since the last batch.
    misses: Vec<Ask>,
    /// What Windows and Locates found missing and the answers have not
    /// covered yet: answered, it stays cached through the next Window.
    read: Vec<Ask>,
    /// The group paths the sorts and filters name, by the axis they are
    /// on: whether each is there decides whether its term applies.
    named: [Vec<Path>; 2],
    /// Whether the applied spec has been given for the latest request.
    settled: bool,
    news: VecDeque<Viewed>,
    /// Answers to Locates: the path and index of each Leaf asked about, or
    /// `None` for one gone from the view.
    located: HashMap<Key, Option<(Path, u64)>>,
    /// Change Notices for figures answered anew since [`GroupingLayer::refigure`].
    changes: VecDeque<(Axis, ChangeNotice)>,
    /// Counts the Windows: the pages the latest two read, or asked for,
    /// stay cached.
    frame: u64,
    /// How many closed groups and Leaves the cache keeps besides those,
    /// and about how many it holds now (never fewer).
    cache: u64,
    cached: u64,
}

/// How many closed groups and Leaves a layer caches by default.
pub const CACHE: u64 = 64 * PAGE;

impl<A: Clone> Default for GroupingLayer<A> {
    fn default() -> Self {
        GroupingLayer::new()
    }
}

/// The scope a View Spec gives, if its rows are grouped: by row levels, or
/// as a pivot's, its sorts and filters naming the path Keys in `keys`.
fn scope_of(spec: &ViewSpec, keys: &KeyInterner<SummaryId>) -> Option<Scope> {
    let grouping = spec.grouping.as_ref()?;
    let fields = |axis| -> Vec<Key> {
        (grouping.levels(axis).iter())
            .filter_map(Level::field_key)
            .collect()
    };
    let levels = fields(Axis::Rows);
    let pivot = grouping.pivot();
    if levels.is_empty() && !pivot {
        return None;
    }
    let rows = &spec.rows;
    Some(Scope {
        levels,
        values: grouping.values.clone(),
        slicers: grouping.slicers.clone(),
        filter: rows.filter.clone(),
        hidden: (rows.hidden.iter())
            .filter(|k| k.0 & SUMMARY_KEY_BIT == 0)
            .copied()
            .collect(),
        sort: rows.sort.clone(),
        columns: fields(Axis::Cols),
        col_sort: match pivot {
            true => spec.cols.sort.clone(),
            false => Default::default(),
        },
        col_filter: match pivot {
            true => spec.cols.filter.clone(),
            false => Default::default(),
        },
        path_keys: (named(spec).into_iter())
            .filter_map(|(_, k)| path_key(keys, grouping, k))
            .collect(),
    })
}

/// `grouping` as the layer applies it: on a pivot with several values and
/// no values pseudo-level, the values go innermost on the columns.
fn normalize(grouping: &mut Grouping) {
    let placed = grouping.splits_values(Axis::Rows) || grouping.splits_values(Axis::Cols);
    if grouping.pivot() && grouping.values.len() > 1 && !placed {
        grouping.col_levels.push(Level::Values);
    }
}

impl<A: Clone> GroupingLayer<A> {
    /// A layer for a plain table, caching [`CACHE`] items.
    pub fn new() -> Self {
        GroupingLayer::with_cache(CACHE)
    }

    /// A layer for a plain table that caches up to `items` closed groups
    /// and Leaves besides those the latest Windows read.
    pub fn with_cache(items: u64) -> Self {
        let mut keys = KeyInterner::new();
        keys.intern(SummaryId::GrandTotal);
        let axes = [
            Tree::new(Shape::default(), &mut keys),
            Tree::new(Shape::default(), &mut keys),
        ];
        GroupingLayer {
            stamp: Stamp(0),
            spec: ViewSpec::default(),
            scope: None,
            axes,
            total: None,
            keys,
            next_id: 0,
            in_flight: Vec::new(),
            misses: Vec::new(),
            read: Vec::new(),
            named: [Vec::new(), Vec::new()],
            settled: true,
            news: VecDeque::new(),
            located: HashMap::new(),
            changes: VecDeque::new(),
            frame: 1,
            cache: items,
            cached: 0,
        }
    }

    fn tree(&self, axis: Axis) -> &Tree<A> {
        &self.axes[axis.index()]
    }

    /// Takes the View Spec the Grid asked for. A spec that groups the rows
    /// differently (other levels, values, slicers, filter, hidden Leaves or
    /// sort, or on a pivot other column levels or column sort) starts the
    /// axes afresh, and the old batches' answers are dropped; one that only
    /// places Subtotals or the Grand Total elsewhere lays out what is known
    /// again; one that opens or closes groups is applied once what they
    /// show is known, with `Expanded` or `Collapsed` for one group the
    /// user sees (on either axis of a pivot) and `Refiltered` for more;
    /// otherwise the spec is applied as it stands. Each axis's `expand.except` loses the Group Item Keys whose
    /// paths no longer fit its levels.
    pub fn request_view(&mut self, mut spec: ViewSpec) {
        if let Some(grouping) = spec.grouping.as_mut() {
            normalize(grouping);
        }
        let scope = scope_of(&spec, &self.keys);
        self.named = [Vec::new(), Vec::new()];
        for (axis, key) in named(&spec) {
            if let Some(SummaryId::Group(path) | SummaryId::Subtotal(path)) = self.identity(key) {
                let path = path.clone();
                self.named[axis.index()].push(path);
            }
        }
        let shapes = Axis::ALL.map(|axis| match scope {
            Some(_) => Shape::of(&spec, axis),
            None => Shape::default(),
        });
        if let Some(grouping) = spec.grouping.as_mut() {
            for (axis, shape) in Axis::ALL.into_iter().zip(&shapes) {
                prune(
                    &self.keys,
                    &shape.steps,
                    &mut grouping.axis_mut(axis).expand,
                );
            }
        }
        for axis in Axis::ALL {
            self.axes[axis.index()].want = Open::of(&spec, axis, &self.keys);
        }
        self.spec = spec;
        let restructured = (self.axes.iter().zip(&shapes)).any(|(t, s)| {
            (&t.shape.steps, t.shape.pivot, &t.shape.values) != (&s.steps, s.pivot, &s.values)
        });
        if scope != self.scope || restructured {
            for (tree, shape) in self.axes.iter_mut().zip(shapes) {
                tree.shape = shape;
            }
            self.reset(scope);
            return;
        }
        for (tree, shape) in self.axes.iter_mut().zip(shapes) {
            if shape != tree.shape {
                tree.shape = shape;
                tree.root.unfinish();
                tree.laid_out = false;
            }
        }
        self.settled = false;
        self.settle();
    }

    /// Starts the grouped axes afresh under the same View Spec, say because
    /// the data changed.
    pub fn refresh(&mut self) {
        self.reset(self.scope.clone());
    }

    fn reset(&mut self, scope: Option<Scope>) {
        self.stamp.0 += 1;
        for tree in &mut self.axes {
            tree.root = Node::new(
                Vec::new(),
                GRAND_TOTAL,
                None,
                &tree.shape,
                &mut self.keys,
                0,
            );
            tree.laid_out = false;
        }
        self.total = None;
        self.scope = scope;
        self.in_flight.clear();
        self.misses.clear();
        self.read.clear();
        self.located.clear();
        self.changes.clear();
        self.cached = 0;
        self.settled = false;
        self.settle();
    }

    /// Whether `axis` is grouped, and so the layer's to answer: the rows
    /// while they have levels or the table is a pivot, the columns only on
    /// a pivot.
    pub fn grouped(&self, axis: Axis) -> bool {
        self.scope.is_some() && (axis == Axis::Rows || self.tree(Axis::Cols).shape.pivot)
    }

    /// Whether the table is a pivot.
    pub fn pivot(&self) -> bool {
        self.grouped(Axis::Cols)
    }

    /// What each grouped axis still needs asked for, the groups the sorts
    /// and filters name included.
    fn needs(&self, asks: &mut Vec<Ask>) {
        let rows = self.tree(Axis::Rows);
        rows.root.needs(&rows.want, Want::Groups, asks);
        if self.pivot() {
            let cols = self.tree(Axis::Cols);
            cols.root.needs(&cols.want, Want::Columns, asks);
            for axis in Axis::ALL {
                let (tree, want) = (self.tree(axis), self.tree(axis).shape.groups());
                for path in &self.named[axis.index()] {
                    tree.root.reach(path, want, asks);
                }
            }
        }
    }

    /// Whether the item `key` names on `axis` is there now: a Grand Total
    /// always, a group once found where its path leads.
    fn present(&self, axis: Axis, key: Key) -> bool {
        match self.identity(key) {
            None => false,
            Some(SummaryId::GrandTotal) => true,
            Some(SummaryId::ValueTotal(id)) => {
                (self.spec.grouping.as_ref()).is_some_and(|g| g.value_of(id).is_some())
            }
            Some(SummaryId::Group(path) | SummaryId::Subtotal(path)) => {
                node_at(&self.tree(axis).root, path).is_some()
            }
        }
    }

    /// The spec as applied: its sort terms and conditions on a pivot that
    /// name an item the data lacks for now left out.
    fn applied(&self) -> ViewSpec {
        let mut applied = self.spec.clone();
        let named = named(&applied);
        if named.is_empty() {
            return applied;
        }
        let absent: Vec<Key> = (named.into_iter())
            .filter(|&(axis, k)| !self.present(axis, k))
            .map(|(_, k)| k)
            .collect();
        for axis in Axis::ALL {
            let view = applied.axis_mut(axis);
            let sort = (view.sort.columns().iter())
                .filter(|c| !absent.contains(&c.key))
                .copied()
                .collect();
            view.sort = SortSpec::new(sort);
            let conditions = (view.filter.conditions.conditions().iter())
                .filter(|c| !absent.contains(&c.key))
                .cloned()
                .collect::<Vec<_>>();
            view.filter.conditions = ConditionTree::All(conditions);
        }
        applied
    }

    /// Whether the Key `key` may still name an item of `axis`: a Leaf Key,
    /// a Key the layer never issued and the Grand Total always; a group's
    /// (a Group Item's, a Subtotal's, a pivot column's) while its path fits
    /// the axis's levels as last asked; a value's Grand Total while the
    /// axis splits the values. Hosts have the Grid forget those that don't.
    pub fn fits(&self, axis: Axis, key: Key) -> bool {
        let shape = &self.tree(axis).shape;
        match self.identity(key) {
            None | Some(SummaryId::GrandTotal) => true,
            Some(SummaryId::ValueTotal(_)) => shape.split(),
            Some(SummaryId::Group(path) | SummaryId::Subtotal(path)) => {
                fits_steps(path, &shape.steps)
            }
        }
    }

    /// The next batch of aggregate queries, if there is anything to ask that
    /// is not asked already.
    pub fn poll_batch(&mut self) -> Option<Batch> {
        let scope = self.scope.clone()?;
        let mut asks = Vec::new();
        // A pivot's corner, and groups of the values pseudo-level at the
        // top of an axis, show the whole table's figures.
        if self.total.is_none() && (self.tree(Axis::Rows).shape.totals() > 0 || self.pivot()) {
            asks.push(Ask {
                path: Vec::new(),
                want: Want::Total,
                range: 0..0,
            });
        }
        self.needs(&mut asks);
        asks.append(&mut self.misses);
        let mut requests = Vec::new();
        for ask in asks {
            let asked = self.in_flight.iter().any(|(_, a)| *a == ask);
            if asked || requests.iter().any(|(_, a)| *a == ask) {
                continue;
            }
            self.next_id += 1;
            requests.push((RequestId(self.next_id), ask));
        }
        if requests.is_empty() {
            return None;
        }
        self.in_flight.extend(requests.iter().cloned());
        Some(Batch {
            stamp: self.stamp,
            scope,
            requests: requests
                .into_iter()
                .map(|(id, a)| Request {
                    id,
                    path: a.path,
                    want: a.want,
                    range: a.range,
                })
                .collect(),
        })
    }

    /// Takes one answer from the backend. Answers to a superseded batch,
    /// and parts naming groups the layer does not know or does not show,
    /// are dropped. A request is done once an answer names it or what the
    /// layer holds covers it, whichever comes first.
    pub fn answer(&mut self, answer: Answer<A>) {
        if answer.stamp != self.stamp || self.scope.is_none() {
            return;
        }
        let placed = match answer.body {
            Body::Located { key, found } => {
                self.located
                    .insert(key, found.then_some((answer.path.clone(), answer.start)));
                None
            }
            Body::Total { aggregates } => {
                self.figured(&answer.path, aggregates);
                None
            }
            Body::Columns { total, groups } => {
                Some((Axis::Cols, Want::Columns, Body::Groups { total, groups }))
            }
            body @ Body::Groups { .. } => Some((Axis::Rows, Want::Groups, body)),
            body => Some((Axis::Rows, Want::Leaves, body)),
        };
        if let Some((axis, want, body)) = placed {
            // What a Window asked for stays cached through the next one;
            // what none did goes first.
            let frame = self.frame;
            let asked = (self.read.iter())
                .filter(|a| a.path == answer.path && a.want == want)
                .map(|a| a.range.clone())
                .collect::<Vec<_>>();
            let fresh =
                |r: Range<u64>| match asked.iter().any(|a| a.start < r.end && r.start < a.end) {
                    true => frame,
                    false => 0,
                };
            let tree = &mut self.axes[axis.index()];
            let (open, want) = (&tree.open, &tree.want);
            let shown = |n: &Node<A>| open.shows(n) || want.shows(n);
            let mut targets = Vec::new();
            at_data_path(&mut tree.root, &answer.path, &shown, &mut targets);
            for node in targets {
                let (start, body) = (answer.start, body.clone());
                let added = place(node, start, body, &tree.shape, &mut self.keys, fresh);
                self.cached = self.cached.saturating_add(added);
            }
        }
        if let Some(id) = answer.id {
            self.in_flight.retain(|(i, _)| *i != id);
        }
        let in_flight = std::mem::take(&mut self.in_flight);
        self.in_flight = (in_flight.into_iter())
            .filter(|(_, ask)| !self.covers(ask))
            .collect();
        let read = std::mem::take(&mut self.read);
        self.read = read.into_iter().filter(|ask| !self.covers(ask)).collect();
        self.settle();
        self.trim();
    }

    /// Takes the figures answered for the group at `path` (the whole axis
    /// at the empty path). Figures answered anew after
    /// [`GroupingLayer::refigure`] say where they are drawn with
    /// `ContentChanged`.
    fn figured(&mut self, path: &Path, aggregates: Vec<A>) {
        let rows = &self.axes[Axis::Rows.index()];
        let (root, shape) = (&rows.root, &rows.shape);
        let top = shape.top();
        let at: Vec<Position> = match path.is_empty() {
            true if self.total.is_some() => match shape.grand_total {
                Placement::Before => (0..shape.totals()).collect(),
                Placement::After => (rows.count())
                    .map_or(0..0, |n| n - shape.totals()..n)
                    .collect(),
                Placement::Off => Vec::new(),
            },
            true => Vec::new(),
            false => [group_position(root, shape, path).filter(|_| {
                !shape.header || node_at(root, path).is_some_and(|n| n.children() == 0)
            })]
            .into_iter()
            .chain((0..shape.per(path.len() - 1)).map(|i| subtotal_position(root, shape, path, i)))
            .flatten()
            .map(|p| p + top)
            .collect(),
        };
        match path.is_empty() {
            true => self.total = Some(aggregates),
            false => {
                let mut nodes = Vec::new();
                let rows = &mut self.axes[Axis::Rows.index()].root;
                at_data_path(rows, path, &|_| true, &mut nodes);
                let rows = nodes.into_iter().filter_map(|n| n.row.as_mut());
                let mut any = false;
                for row in rows {
                    row.aggregates = aggregates.clone();
                    any = true;
                }
                if !any {
                    return;
                }
            }
        }
        (self.changes).extend(at.into_iter().map(|p| {
            let notice = ChangeNotice::ContentChanged(p..p + 1);
            (Axis::Rows, notice)
        }));
    }

    /// The data under the groups changed, by the user's edit: asks the
    /// figures of every group held, and of the whole axis, again, keeping
    /// every item where it is until the next View Spec, as for an
    /// Unmatched Row. Each figure answered says where it is drawn through
    /// [`GroupingLayer::poll_change`]. Groups keep their child counts.
    pub fn refigure(&mut self) {
        if self.scope.is_none() {
            return;
        }
        let mut held = Vec::new();
        self.tree(Axis::Rows).root.held_groups(&mut held);
        // Each data path once, groups of the values pseudo-level left out.
        let mut paths = vec![Vec::new()];
        for path in held.iter().map(data) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        self.misses.extend(paths.into_iter().map(|path| Ask {
            path,
            want: Want::Total,
            range: 0..0,
        }));
    }

    /// The next Change Notice for figures answered anew since
    /// [`GroupingLayer::refigure`].
    pub fn poll_change(&mut self) -> Option<(Axis, ChangeNotice)> {
        self.changes.pop_front()
    }

    /// Where the rows' Subtotals and Grand Total go, as last asked.
    pub(crate) fn shape(&self) -> &Shape {
        &self.tree(Axis::Rows).shape
    }

    /// Which row groups are open in the layout given.
    pub(crate) fn layout(&self) -> &Open {
        &self.tree(Axis::Rows).open
    }

    /// The Key the layer gives the Summary Item `id`, interning it if new.
    pub fn key_of(&mut self, id: SummaryId) -> Key {
        summary_key(&mut self.keys, id)
    }

    /// Which Summary Item `key` is, if the layer gave it out.
    pub fn identity(&self, key: Key) -> Option<&SummaryId> {
        match key.0 & SUMMARY_KEY_BIT {
            0 => None,
            _ => self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)),
        }
    }

    /// The path of the group `key` heads or totals (empty for the Grand
    /// Total), or of the group holding the Leaf `key` among those fetched.
    pub fn path(&self, key: Key) -> Option<&Path> {
        if key.0 & SUMMARY_KEY_BIT != 0 {
            return match self.identity(key)? {
                SummaryId::Group(path) | SummaryId::Subtotal(path) => Some(path),
                SummaryId::ValueSubtotal(path) => {
                    let group = split_value(path)?.0;
                    (self.axes.iter())
                        .find_map(|t| node_at(&t.root, group))
                        .map(|n| &n.path)
                }
                SummaryId::GrandTotal | SummaryId::ValueTotal(_) => {
                    Some(&self.tree(Axis::Rows).root.path)
                }
            };
        }
        self.tree(Axis::Rows).root.holding(key).map(|n| &n.path)
    }

    /// Where a move of Leaves of the group at `path` on `axis` to before
    /// Position `to` lands: the index among the group's Leaves, which the
    /// backend maps to its own order. `None` for a Position outside the
    /// group's Leaves, or a group not shown.
    pub fn place_move(&self, axis: Axis, path: &Path, to: Position) -> Option<u64> {
        if !self.grouped(axis) {
            return None;
        }
        let tree = self.tree(axis);
        let (root, shape, open) = (&tree.root, &tree.shape, &tree.open);
        let node = node_at(root, path).filter(|n| open.shows(n))?;
        let level = path.len().saturating_sub(1);
        let start = group_position(root, shape, path)? + shape.top() + shape.lead(level);
        let index = to.checked_sub(start)?;
        (index <= node.children()).then_some(index)
    }

    /// Whether what the layer holds answers `ask`.
    fn covers(&self, ask: &Ask) -> bool {
        let axis = match ask.want {
            Want::Columns => Axis::Cols,
            _ => Axis::Rows,
        };
        let mut nodes = Vec::new();
        at_data(&self.tree(axis).root, &ask.path, &mut nodes);
        let all = |covered: &dyn Fn(&Node<A>) -> bool| {
            !nodes.is_empty() && nodes.iter().all(|n| covered(n))
        };
        match ask.want {
            Want::Total => self.total.is_some(),
            Want::Locate(key) => self.located.contains_key(&key),
            Want::Groups | Want::Columns => all(&|n| match &n.kids {
                Kids::Groups(g) if !g.values => g.total.is_some_and(|total| {
                    let known = |i| g.nodes.contains_key(&i);
                    missing(ask.range.clone(), total, known).is_empty()
                }),
                _ => false,
            }),
            Want::Leaves => all(&|n| {
                let end = ask.range.end.min(n.children());
                (ask.range.start..end).all(|i| n.leaf(i, None).is_some())
            }),
        }
    }

    /// Lets the cache go of the closed groups and Leaves read longest ago,
    /// sparing the pages the latest two Windows read or asked for and what
    /// a request still due covers, until it holds no more than its size.
    fn trim(&mut self) {
        if self.cached <= self.cache {
            return;
        }
        let mut spots = Vec::new();
        let mut cached = 0;
        for axis in Axis::ALL {
            let tree = self.tree(axis);
            let mut found = Vec::new();
            cached += tree.root.cached([&tree.open, &tree.want], &mut found);
            spots.extend(
                found
                    .into_iter()
                    .map(|(used, path, spot)| (used, axis, path, spot)),
            );
        }
        spots.sort_by_key(|(used, ..)| *used);
        for (used, axis, path, spot) in spots {
            if cached <= self.cache || (used > 0 && used + 1 >= self.frame) {
                break;
            }
            // What a request still due covers stays until it is done.
            let in_flight = &self.in_flight;
            let asked = data(&path);
            let due = |want: Want, range: Range<u64>| {
                in_flight.iter().any(|(_, a)| {
                    a.path == asked
                        && a.want == want
                        && a.range.start < range.end
                        && range.start < a.range.end
                })
            };
            let tree = &mut self.axes[axis.index()];
            let groups = tree.shape.groups();
            let Some(node) = node_mut(&mut tree.root, &path, |_| true) else {
                continue;
            };
            match (spot, &mut node.kids) {
                (Spot::Leaves(start), Kids::Leaves(runs)) => {
                    let len = runs.get(&start).map_or(0, |r| r.keys.len() as u64);
                    if due(Want::Leaves, start..start + len) {
                        continue;
                    }
                    runs.remove(&start);
                    cached -= len;
                }
                (Spot::Group(index), Kids::Groups(g)) if !due(groups, index..index + 1) => {
                    if let Some(gone) = g.nodes.remove(&index) {
                        let value = &gone.path.last().expect("a group has a path").1;
                        g.by_value.remove(value);
                        cached -= 1;
                    }
                }
                _ => {}
            }
        }
        self.cached = cached;
    }

    /// Gives the applied spec once every group the axis shows open under it
    /// is known.
    fn settle(&mut self) {
        if self.settled {
            return;
        }
        let mut notices = Vec::new();
        if self.scope.is_some() {
            let mut asks = Vec::new();
            self.needs(&mut asks);
            if !asks.is_empty() {
                return;
            }
            // A pivot's columns are the layer's too. Any other change to
            // them than one group opening or closing comes as Refiltered,
            // so their state stays by Key.
            let axes = match self.axes[1].shape.pivot {
                true => &Axis::ALL[..],
                false => &[Axis::Rows],
            };
            for &axis in axes {
                let notice = match self.axes[axis.index()].laid_out {
                    true => self.unfold(axis),
                    false => {
                        let tree = &mut self.axes[axis.index()];
                        tree.lay_out();
                        tree.laid_out = true;
                        tree.count().map(ChangeNotice::Refiltered)
                    }
                };
                notices.extend(notice.map(|n| (axis, n)));
            }
        }
        let applied = self.applied();
        if self.scope.is_some() {
            for tree in &mut self.axes {
                tree.laid_out = true;
                let (open, shape) = (&tree.open, &tree.shape);
                tree.root.fold(open, shape, &mut self.keys);
            }
        }
        self.settled = true;
        // What the cache may let go of changes with the groups open.
        self.cached = u64::MAX;
        self.news.push_back(Viewed {
            applied,
            totals: Totals::unknown(),
            notices,
        });
    }

    /// Lays the groups of `axis` out as the latest spec opens them, all
    /// known, and says how: `Collapsed` or `Expanded` when one group the
    /// user sees opened or closed, nothing when no group the user sees did,
    /// and `Refiltered` for more.
    fn unfold(&mut self, axis: Axis) -> Option<ChangeNotice> {
        let keys = &self.keys;
        let tree = &mut self.axes[axis.index()];
        if tree.want.same(&tree.open) {
            return None;
        }
        let single = match tree.want.expand.depth == tree.open.expand.depth {
            true => {
                let mut changed = tree.want.except.symmetric_difference(&tree.open.except);
                match (changed.next(), changed.next()) {
                    (Some(&key), None) => Some(key),
                    _ => None,
                }
            }
            false => None,
        };
        let path = single.and_then(|key| match keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) {
            Some(SummaryId::Group(path)) if key.0 & SUMMARY_KEY_BIT != 0 => Some(path.clone()),
            _ => None,
        });
        // What the group shows past what stays when it closes.
        let under = |root: &Node<A>, shape: &Shape, path: &Path| {
            let (at, span) = find_group(root, shape, path).ok()??;
            let closed = shape.closed(path.len() - 1);
            Some((at, span.map_or(0, |s| s.len.saturating_sub(closed))))
        };
        // A closed group's Subtotals that stand in take the place of what
        // it hid, unless they stood there already.
        let level = path.as_ref().map_or(0, |p| p.len().saturating_sub(1));
        let stands_in = tree.shape.header || path.is_some() && tree.shape.splits(level);
        if stands_in && tree.shape.subtotal(level) == Placement::Off {
            tree.lay_out();
            return tree.count().map(ChangeNotice::Refiltered);
        }
        let before = path
            .as_ref()
            .and_then(|p| under(&tree.root, &tree.shape, p));
        tree.lay_out();
        if single.is_some() && before.is_none() {
            return None;
        }
        let (Some((at, was)), Some(path)) = (before, &path) else {
            return tree.count().map(ChangeNotice::Refiltered);
        };
        let (_, now) = under(&tree.root, &tree.shape, path)?;
        let at = at + tree.shape.top() + tree.shape.kept(path.len() - 1);
        match now.cmp(&was) {
            std::cmp::Ordering::Greater => Some(ChangeNotice::Expanded { at, n: now - was }),
            std::cmp::Ordering::Less => Some(ChangeNotice::Collapsed { at, n: was - now }),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// How many groups and Leaves the layer holds: the open groups, and the
    /// closed groups and Leaves it caches.
    pub fn held(&self) -> u64 {
        self.axes.iter().map(|t| t.root.held()).sum()
    }

    /// The View Spec applied since the last call, with the Change Notices
    /// of the grouped axes, once the layer knows them. The totals are
    /// unknown: the host adds its own for the axes the layer leaves it.
    pub fn poll_viewed(&mut self) -> Option<Viewed> {
        self.news.pop_front()
    }

    /// How many items `axis` shows, once the layer knows; `None` for an
    /// axis it does not group, or while the first answers are due.
    pub fn count(&self, axis: Axis) -> Option<u64> {
        if !self.grouped(axis) {
            return None;
        }
        self.tree(axis).count()
    }

    /// The Keys and outlines of the items at `range` on `axis`, from its
    /// start up to the first one not fetched yet: the Grid draws the rest
    /// pending. The next batch asks for everything in `range` not fetched.
    pub fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<(Key, Outline)> {
        let Some(count) = self.count(axis) else {
            return Vec::new();
        };
        self.frame += 1;
        let mut items = Vec::new();
        let mut missed = false;
        let tree = &self.axes[axis.index()];
        let (top, totals) = (tree.shape.top(), tree.shape.totals());
        for position in range.start..range.end.min(count) {
            let total = match tree.shape.grand_total {
                Placement::Before => position < totals,
                Placement::After => position + totals >= count,
                Placement::Off => false,
            };
            if total {
                if !missed {
                    let i = match tree.shape.grand_total {
                        Placement::Before => position,
                        _ => position + totals - count,
                    };
                    // The top's set: its groups, then the totals placed after.
                    let kids = tree.root.kids();
                    let index = match tree.shape.grand_total {
                        Placement::After => kids + i,
                        _ => i,
                    };
                    let (key, outline) = match tree.shape.split() {
                        true => {
                            let id = tree.shape.values[i as usize].clone();
                            let key =
                                summary_key(&mut self.keys, SummaryId::ValueTotal(id.clone()));
                            (key, Outline::grand_total_of(id))
                        }
                        false => (GRAND_TOTAL, Outline::grand_total()),
                    };
                    items.push((key, outline.with_set(index, kids + totals)));
                }
                continue;
            }
            let (root, shape, frame) = (&tree.root, &tree.shape, self.frame);
            match item_at(root, shape, &mut self.keys, position - top, frame) {
                Ok(item) if !missed => items.push(item),
                Ok(_) => {}
                // Past the first item missing, the rest are only asked for,
                // so that one batch asks for all the Window lacks.
                Err(ask) => {
                    missed = true;
                    let within = |a: &Ask| {
                        a.path == ask.path
                            && a.want == ask.want
                            && a.range.start <= ask.range.start
                            && ask.range.end <= a.range.end
                    };
                    if !self.misses.iter().any(within) {
                        self.read.push(ask.clone());
                        self.misses.push(ask);
                    }
                }
            }
        }
        self.trim();
        items
    }

    /// Where `key` sits on `axis`; `None` while the layer has asked the
    /// backend and waits for its answer.
    pub fn locate(&mut self, axis: Axis, key: Key) -> Option<Located> {
        let gone = Located::Gone { suggest: None };
        let Some(count) = self.count(axis) else {
            return Some(gone);
        };
        let tree = &self.axes[axis.index()];
        let (root, shape) = (&tree.root, &tree.shape);
        let top = shape.top();
        let total_at = |i: u64| match shape.grand_total {
            Placement::Before => Some(i),
            Placement::After => Some(count - shape.totals() + i),
            Placement::Off => None,
        };
        let found = if key.0 & SUMMARY_KEY_BIT != 0 {
            match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) {
                // In header form only a group with nothing under it has a
                // Group Item.
                Some(SummaryId::Group(path))
                    if !shape.header || node_at(root, path).is_some_and(|n| n.children() == 0) =>
                {
                    find_group(root, shape, path).map(|f| f.map(|f| f.0))
                }
                Some(SummaryId::Group(_)) => Ok(None),
                Some(SummaryId::Subtotal(path)) if !shape.splits(path.len().wrapping_sub(1)) => {
                    find_group(root, shape, path).map(|f| subtotal_at(root, shape, path, f?, 0))
                }
                Some(SummaryId::ValueSubtotal(path)) => match split_value(path) {
                    Some((group, id)) if shape.splits(group.len().wrapping_sub(1)) => {
                        match shape.values.iter().position(|v| v == id) {
                            Some(i) => find_group(root, shape, group)
                                .map(|f| subtotal_at(root, shape, group, f?, i as u64)),
                            None => Ok(None),
                        }
                    }
                    _ => Ok(None),
                },
                Some(SummaryId::GrandTotal) if !shape.split() => {
                    return Some(total_at(0).map_or(gone, Located::Found));
                }
                Some(SummaryId::ValueTotal(id)) if shape.split() => {
                    let i = shape.values.iter().position(|v| v == id);
                    let at = i.and_then(|i| total_at(i as u64));
                    return Some(at.map_or(gone, Located::Found));
                }
                _ => Ok(None),
            }
        } else if axis == Axis::Cols {
            Ok(None)
        } else if let Some(found) = leaf_position(root, shape, key, 0) {
            Ok(Some(found))
        } else {
            match self.located.get(&key) {
                Some(Some((path, index))) => find_group(root, shape, path).map(|f| {
                    let (at, span) = f?;
                    span?;
                    Some(at + shape.lead(path.len().saturating_sub(1)) + index)
                }),
                Some(None) => Ok(None),
                None => Err(vec![Ask {
                    path: Vec::new(),
                    want: Want::Locate(key),
                    range: 0..0,
                }]),
            }
        };
        match found {
            Ok(found) => Some(found.map_or(gone, |p| Located::Found(p + top))),
            Err(asks) => {
                for ask in asks {
                    if !self.read.contains(&ask) {
                        self.read.push(ask.clone());
                    }
                    self.misses.push(ask);
                }
                None
            }
        }
    }

    /// The Summary Item with `key`, on either axis, if the layer holds it.
    pub fn summary(&self, key: Key) -> Option<Summary<'_, A>> {
        if key.0 & SUMMARY_KEY_BIT == 0 {
            return None;
        }
        let rows = &self.tree(Axis::Rows).root;
        let (kind, path, group) = match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT))? {
            SummaryId::Group(path) => (ItemKind::Group, path, &path[..]),
            SummaryId::Subtotal(path) => (ItemKind::Subtotal, path, &path[..]),
            SummaryId::ValueSubtotal(path) => (ItemKind::Subtotal, path, split_value(path)?.0),
            SummaryId::GrandTotal | SummaryId::ValueTotal(_) => {
                return Some(Summary {
                    kind: ItemKind::GrandTotal,
                    path: &rows.path,
                    children: match &rows.kids {
                        Kids::Groups(g) => g.total?,
                        _ => 0,
                    },
                    aggregates: self.total.as_deref()?,
                });
            }
        };
        let tree = self
            .axes
            .iter()
            .find(|t| node_at(&t.root, group).is_some())?;
        let node = node_at(&tree.root, group)?;
        // A group of the values pseudo-level at the top holds every row.
        let aggregates = match figures(&tree.root, group) {
            Some(data) => &data.row.as_ref()?.aggregates[..],
            None => self.total.as_deref().unwrap_or_default(),
        };
        Some(Summary {
            kind,
            path,
            children: node.children(),
            aggregates,
        })
    }

    /// The figure of a pivot's cell: the row `row` and the column `col` (or,
    /// off a pivot, the Grand Total column, `GRAND_TOTAL`), one of their
    /// value's aggregates as the backend gave it. `None` where the layer
    /// does not know it yet, a Key names no item it issued, or neither
    /// names a value while an axis splits the values (a group above them).
    pub fn cell(&self, row: Key, col: Key) -> Option<&A> {
        let id = |key: Key| match key.0 & SUMMARY_KEY_BIT {
            0 => None,
            _ => self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)),
        };
        // An item's group path (empty for a Grand Total), and the value it
        // shows alone, if any.
        let item = |key: Key| -> Option<(&Steps, Option<&str>)> {
            Some(match id(key)? {
                SummaryId::Group(p) | SummaryId::Subtotal(p) => (p, value_on(p)),
                SummaryId::ValueSubtotal(p) => split_value(p).map(|(g, v)| (g, Some(v)))?,
                SummaryId::GrandTotal => (&[], None),
                SummaryId::ValueTotal(v) => (&[], Some(v.as_str())),
            })
        };
        let (row_path, row_value) = item(row)?;
        let (col_path, col_value) = item(col)?;
        let grouping = self.spec.grouping.as_ref()?;
        // Where an axis splits the values, a group above them has no one
        // figure: its Subtotals carry one each.
        let index = match row_value.or(col_value) {
            Some(v) => grouping.value_of(v)?.0,
            None if self.axes.iter().any(|t| t.shape.split()) => return None,
            None => 0,
        };
        // The node holding each item's figures, none where it holds every
        // row.
        let aggregates = match (
            figures(&self.axes[0].root, row_path),
            figures(&self.axes[1].root, col_path),
        ) {
            (None, None) => self.total.as_ref()?,
            (None, Some(c)) => &c.row.as_ref()?.aggregates,
            (Some(r), None) => &r.row.as_ref()?.aggregates,
            (Some(r), Some(c)) => r.cells.get(&data(&c.path))?,
        };
        aggregates.get(index)
    }
}

/// A value's Subtotal's path as its group's path and the value's identity.
fn split_value(path: &Path) -> Option<(&Steps, &str)> {
    match path.split_last()? {
        ((VALUES_FIELD, Value::Text(id)), group) => Some((group, id.as_str())),
        _ => None,
    }
}

/// The identity of the value a path goes through, if it does.
fn value_on(path: &Steps) -> Option<&str> {
    path.iter().find_map(|(f, v)| match (f, v) {
        (&VALUES_FIELD, Value::Text(t)) => Some(t.as_str()),
        _ => None,
    })
}

/// The node holding the figures of the group at `path`: its own, or for a
/// group of the values pseudo-level the group it is in.
fn figures<'n, A>(root: &'n Node<A>, path: &Steps) -> Option<&'n Node<A>> {
    let mut node = root;
    let mut found = None;
    for (field, value) in path {
        node = node.child(value)?;
        if node.path.last().is_none_or(|(f, _)| f != field) {
            return None;
        }
        if !node.virtual_() {
            found = Some(node);
        }
    }
    found
}

/// Puts `body`, which starts at `start` under `node`, in place; returns how
/// many groups or Leaves it added. `fresh` gives what the layer asked for
/// the frame, 0 otherwise.
fn place<A>(
    node: &mut Node<A>,
    start: u64,
    body: Body<A>,
    shape: &Shape,
    keys: &mut KeyInterner<SummaryId>,
    fresh: impl Fn(Range<u64>) -> u64,
) -> u64 {
    match (body, &mut node.kids) {
        (Body::Groups { total, groups }, Kids::Groups(g)) if !g.values => {
            // One stamp, one answer: a different count is not this view's.
            if g.total.is_some_and(|t| t != total) {
                return 0;
            }
            g.total = Some(total);
            let Some(&Some(field)) = shape.steps.get(node.path.len()) else {
                return 0;
            };
            let mut added = 0;
            for (i, row) in (start..total).zip(groups) {
                if g.nodes.contains_key(&i) || g.by_value.contains_key(&row.value) {
                    continue;
                }
                let mut path = node.path.clone();
                path.push((field, row.value.clone()));
                g.by_value.insert(row.value.clone(), i);
                let key = summary_key(keys, SummaryId::Group(path.clone()));
                let used = fresh(i..i + 1);
                let node = Node::new(path, key, Some(row), shape, keys, used);
                g.nodes.insert(i, node);
                added += 1;
            }
            added
        }
        (Body::Leaves { keys }, Kids::Leaves(runs)) => insert_leaves(runs, start, keys, fresh),
        _ => 0,
    }
}

fn node_at<'n, A>(root: &'n Node<A>, path: &Steps) -> Option<&'n Node<A>> {
    path.iter().try_fold(root, |node, (field, value)| {
        node.child(value)
            .filter(|c| c.path.last().is_some_and(|(f, _)| f == field))
    })
}

/// The nodes whose data path is `rest` past `node`'s (every copy under the
/// values pseudo-level's groups), if every group on the way is `shown`.
fn at_data_path<'n, A>(
    node: &'n mut Node<A>,
    rest: &Steps,
    shown: &impl Fn(&Node<A>) -> bool,
    out: &mut Vec<&'n mut Node<A>>,
) {
    let values = match &node.kids {
        Kids::Groups(g) => g.values,
        _ => false,
    };
    let (step, rest) = match (values, rest.split_first()) {
        (false, None) => return out.push(node),
        (false, Some(((_, value), rest))) => (Some(value), rest),
        (true, _) => (None, rest),
    };
    let Kids::Groups(g) = &mut node.kids else {
        return;
    };
    let wanted = step.and_then(|v| g.by_value.get(v).copied());
    for (&i, kid) in g.nodes.iter_mut() {
        if step.is_some() && wanted != Some(i) {
            continue;
        }
        // A closed group of the values pseudo-level learns how many groups
        // it holds.
        if shown(kid) || kid.virtual_() && rest.is_empty() {
            at_data_path(kid, rest, shown, out);
        }
    }
}

/// The nodes whose data path is `rest` past `node`'s, shown or not.
fn at_data<'n, A>(node: &'n Node<A>, rest: &[(Key, Value)], out: &mut Vec<&'n Node<A>>) {
    let values = match &node.kids {
        Kids::Groups(g) => g.values,
        _ => false,
    };
    match (values, rest.split_first()) {
        (false, None) => out.push(node),
        (false, Some(((_, value), rest))) => {
            if let Some(kid) = node.child(value) {
                at_data(kid, rest, out);
            }
        }
        (true, _) => {
            if let Kids::Groups(g) = &node.kids {
                for kid in g.nodes.values() {
                    at_data(kid, rest, out);
                }
            }
        }
    }
}

/// The node at `path`, if every group on the way is `shown`.
fn node_mut<'n, A>(
    root: &'n mut Node<A>,
    path: &Path,
    shown: impl Fn(&Node<A>) -> bool,
) -> Option<&'n mut Node<A>> {
    let mut node = root;
    for (_, value) in path {
        node = node.child_mut(value)?;
        if !shown(node) {
            return None;
        }
    }
    Some(node)
}

fn summary_key(keys: &mut KeyInterner<SummaryId>, id: SummaryId) -> Key {
    Key(keys.intern(id).0 | SUMMARY_KEY_BIT)
}

/// The Group Item Keys of the groups on `path`, outermost first.
fn ancestors(keys: &mut KeyInterner<SummaryId>, path: &Path) -> Vec<Key> {
    (1..=path.len())
        .map(|d| summary_key(keys, SummaryId::Group(path[..d].to_vec())))
        .collect()
}

/// The groups on `path`, outermost first.
fn groups(path: &Path) -> Vec<GroupOf> {
    (path.iter())
        .map(|(field, value)| GroupOf {
            field: *field,
            value: value.clone(),
        })
        .collect()
}

/// Where the children of `node` start among its set, past a Subtotal (or at
/// the top a Grand Total) placed before, and how many the set holds.
fn set<A>(node: &Node<A>, shape: &Shape) -> (u64, u64) {
    let placed = match node.path.len() {
        0 => shape.grand_total,
        n => shape.subtotal(n - 1),
    };
    let per = node.path.len().checked_sub(1).map_or(1, |l| shape.per(l));
    let first = per * u64::from(placed == Placement::Before);
    (
        first,
        node.kids() + per * u64::from(placed != Placement::Off),
    )
}

/// The place of `group`'s Subtotal among its set, where its level places one.
fn subtotal_set<A>(group: &Node<A>, shape: &Shape, i: u64) -> Option<SetPosition> {
    let (_, size) = set(group, shape);
    let level = group.path.len() - 1;
    match shape.subtotal(level) {
        Placement::Before => Some(SetPosition { index: i, size }),
        Placement::After => Some(SetPosition {
            index: size - shape.per(level) + i,
            size,
        }),
        Placement::Off => None,
    }
}

/// The item at `position` among those shown under `node`, marked read in
/// `frame`, or what to ask for to know it. `node` is laid out.
fn item_at<A>(
    node: &Node<A>,
    shape: &Shape,
    keys: &mut KeyInterner<SummaryId>,
    position: Position,
    frame: u64,
) -> Result<(Key, Outline), Ask> {
    match &node.kids {
        Kids::Groups(g) => {
            let layout = g.layout.as_ref().expect("a node shown is laid out");
            let (index, at) = layout.at(position);
            let level = node.path.len();
            let Some(child) = g.nodes.get(&index) else {
                let total = g.total.unwrap_or(0);
                let page = index / PAGE * PAGE;
                return Err(Ask {
                    path: data(&node.path),
                    want: shape.groups(),
                    range: index..(page + PAGE).min(total),
                });
            };
            child.used.set(frame);
            let span = layout.span(index);
            let (field, value) = child.path.last().cloned().expect("a group has a path");
            let lead = shape.lead(level);
            let item = u64::from(!shape.header);
            // Which of its Subtotals `at` is, if one: placed around what is
            // under it, or standing in where it is closed (in header form,
            // where an open group has no Group Item, and above the values
            // pseudo-level).
            let subtotal = match (span, shape.subtotal(level)) {
                _ if child.children() == 0 => None,
                (None, _) => (at >= item && shape.stands_in(level)).then(|| at - item),
                (Some(_), Placement::Before) => (at >= item && at < lead).then(|| at - item),
                (Some(s), Placement::After) => {
                    let end = s.len - shape.per(level);
                    (at >= end).then(|| at - end)
                }
                (Some(_), Placement::Off) => None,
            };
            if let Some(i) = subtotal {
                let up = ancestors(keys, &child.path);
                let (key, outline) = match shape.splits(level) {
                    true => {
                        let step = (VALUES_FIELD, Value::Text(shape.values[i as usize].clone()));
                        let mut path = child.path.clone();
                        path.push(step.clone());
                        let key = summary_key(keys, SummaryId::ValueSubtotal(path));
                        (key, Outline::subtotal(up, step.0, step.1))
                    }
                    false => {
                        let key = summary_key(keys, SummaryId::Subtotal(child.path.clone()));
                        (key, Outline::subtotal(up, field, value))
                    }
                };
                let mut outline = outline.with_path(groups(&child.path));
                outline.set = subtotal_set(child, shape, i);
                return Ok((key, outline));
            }
            if at == 0 && !(shape.header && span.is_some()) {
                let state = match (child.children(), span) {
                    (0, _) => OpenState::CantOpen,
                    (_, Some(_)) => OpenState::Open,
                    (_, None) => OpenState::Closed,
                };
                let span = match span {
                    Some(span) => span.len - 1,
                    None => 0,
                };
                let (first, size) = set(node, shape);
                let outline = Outline::group(ancestors(keys, &node.path), state, field, value)
                    .with_span(span)
                    .with_path(groups(&node.path))
                    .with_set(first + index, size);
                return Ok((child.key, outline));
            }
            item_at(child, shape, keys, at - lead, frame)
        }
        Kids::Leaves(_) => match node.leaf(position, Some(frame)) {
            Some(key) => {
                let (first, size) = set(node, shape);
                let outline = Outline::leaf(ancestors(keys, &node.path));
                Ok((
                    key,
                    outline
                        .with_path(groups(&node.path))
                        .with_set(first + position, size),
                ))
            }
            None => {
                let page = position / PAGE * PAGE;
                Err(Ask {
                    path: data(&node.path),
                    want: Want::Leaves,
                    range: position..(page + PAGE).min(node.children()),
                })
            }
        },
        Kids::Nothing => unreachable!("a pivot's innermost group shows nothing under it"),
    }
}

/// Where the Group Item at `path` sits among the items under the root, and
/// its span if it is open: `Ok(None)` if it is not shown (gone, or under a
/// closed group), or the pages to ask for to find it.
#[allow(clippy::type_complexity)]
fn find_group<A>(
    root: &Node<A>,
    shape: &Shape,
    path: &Steps,
) -> Result<Option<(Position, Option<Span>)>, Vec<Ask>> {
    let mut node = root;
    let mut at = 0;
    for (depth, (_, value)) in path.iter().enumerate() {
        let Kids::Groups(g) = &node.kids else {
            return Ok(None);
        };
        let Some(layout) = &g.layout else {
            return Ok(None);
        };
        let Some(&index) = g.by_value.get(value) else {
            let total = g.total.unwrap_or(0);
            let known = |i| g.nodes.contains_key(&i);
            let pages = missing(0..total, total, known);
            let asks = pages.into_iter().map(|range| Ask {
                path: data(&node.path),
                want: shape.groups(),
                range,
            });
            return match asks.collect::<Vec<_>>() {
                asks if asks.is_empty() => Ok(None),
                asks => Err(asks),
            };
        };
        let span = layout.span(index);
        at += layout.position(index);
        if depth + 1 == path.len() {
            return Ok(Some((at, span)));
        }
        if span.is_none() {
            return Ok(None);
        }
        at += shape.lead(depth);
        node = &g.nodes[&index];
    }
    Ok(None)
}

/// Where the Group Item at `path` sits among the items under the root, if
/// known and shown.
fn group_position<A>(root: &Node<A>, shape: &Shape, path: &Path) -> Option<Position> {
    find_group(root, shape, path)
        .ok()
        .flatten()
        .map(|(at, _)| at)
}

/// Where the `i`th Subtotal of the group at `path` sits among the items
/// under the root, if known and placed or standing in.
fn subtotal_position<A>(root: &Node<A>, shape: &Shape, path: &Path, i: u64) -> Option<Position> {
    let found = find_group(root, shape, path).ok().flatten()?;
    subtotal_at(root, shape, path, found, i)
}

/// Where the `i`th Subtotal of the group at `path` sits, given where the
/// group is found and its span if open: a closed group's stand in for it
/// in header form and above the values pseudo-level.
fn subtotal_at<A>(
    root: &Node<A>,
    shape: &Shape,
    path: &Steps,
    (at, span): (Position, Option<Span>),
    i: u64,
) -> Option<Position> {
    let level = path.len().checked_sub(1)?;
    let item = u64::from(!shape.header);
    let Some(span) = span else {
        let filled = node_at(root, path).is_some_and(|n| n.children() > 0);
        return (shape.stands_in(level) && filled).then_some(at + item + i);
    };
    match shape.subtotal(level) {
        Placement::Before => Some(at + item + i),
        Placement::After => Some(at + span.len - shape.per(level) + i),
        Placement::Off => None,
    }
}

/// Where the Leaf with `key` sits among the items shown under `node` (from
/// `base`), if it is among the Leaves fetched.
fn leaf_position<A>(node: &Node<A>, shape: &Shape, key: Key, base: Position) -> Option<Position> {
    match &node.kids {
        Kids::Groups(g) => {
            let layout = g.layout.as_ref()?;
            let lead = shape.lead(node.path.len());
            layout.open.iter().filter(|s| s.open).find_map(|span| {
                let child = g.nodes.get(&span.index)?;
                leaf_position(child, shape, key, base + span.at + lead)
            })
        }
        Kids::Leaves(runs) => runs.iter().find_map(|(start, run)| {
            let i = run.keys.iter().position(|&k| k == key)?;
            Some(base + start + i as u64)
        }),
        Kids::Nothing => None,
    }
}

/// Pumps `layer` against a backend that answers at once, until it asks for
/// nothing more.
pub fn settle<A: Clone>(layer: &mut GroupingLayer<A>, backend: &mut impl AggregateBackend<A>) {
    while let Some(batch) = layer.poll_batch() {
        for answer in backend.answer(&batch) {
            layer.answer(answer);
        }
    }
}
