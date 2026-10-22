//! The grouping layer: a sans-IO state machine that is a Table Source for a
//! grouped axis, built from the answers to its aggregate queries.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, ItemKind, Key, Located, OpenState, Outline, Position, Totals, Value,
    ViewSpec,
};
use binnacle_memory::{KeyInterner, Viewed};

use crate::query::{
    AggregateBackend, Answer, Batch, Body, GroupRow, Path, Request, RequestId, Scope, Stamp, Want,
};

/// How many groups or Leaves one request asks for.
const PAGE: u64 = 64;

/// The bit set in every Summary Item Key the layer issues, so they never
/// meet a Leaf Key: a backend's Leaf Keys must keep it clear.
pub const SUMMARY_KEY_BIT: u64 = 1 << 63;

/// What a Summary Item Key is interned from: its kind and its group's path.
/// Kind keeps a real blank group apart from a Subtotal's empty slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SummaryId {
    Group(Path),
    Subtotal(Path),
    GrandTotal,
}

/// A Summary Item as the layer holds it, for drawing: its kind, its group's
/// path, how many children the group has and its aggregates, one per value
/// of the grouping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary<'a, A> {
    pub kind: ItemKind,
    pub path: &'a Path,
    pub children: u64,
    pub aggregates: &'a [A],
}

/// Something to ask the backend.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ask {
    path: Path,
    want: Want,
    range: Range<u64>,
}

/// A group (or the whole axis, at the root) and what is known under it.
struct Node<A> {
    path: Path,
    row: Option<GroupRow<A>>,
    kids: Kids<A>,
}

enum Kids<A> {
    /// The groups of the next level.
    Groups(Groups<A>),
    /// The Leaves, as runs of Keys by the index of their first.
    Leaves(BTreeMap<u64, Vec<Key>>),
}

struct Groups<A> {
    total: Option<u64>,
    nodes: Vec<Option<Node<A>>>,
    by_value: HashMap<Value, usize>,
    /// Once every group under it is known: where each group's item sits
    /// among the items shown under this node, and how many those are.
    starts: Option<(Vec<u64>, u64)>,
}

impl<A> Node<A> {
    /// A node at `path` over `levels` levels in all.
    fn new(path: Path, row: Option<GroupRow<A>>, levels: usize) -> Self {
        let kids = match path.len() < levels {
            true => Kids::Groups(Groups {
                total: None,
                nodes: Vec::new(),
                by_value: HashMap::new(),
                starts: None,
            }),
            false => Kids::Leaves(BTreeMap::new()),
        };
        Node { path, row, kids }
    }

    fn children(&self) -> u64 {
        self.row.as_ref().map_or(0, |r| r.children)
    }

    /// How many items are shown under this node, once known.
    fn shown(&self) -> Option<u64> {
        match &self.kids {
            Kids::Groups(g) => g.starts.as_ref().map(|(_, n)| *n),
            Kids::Leaves(_) => Some(self.children()),
        }
    }

    /// Works out where the items under this node sit, as far as the groups
    /// known allow; returns how many are shown, if known.
    fn finish(&mut self) -> Option<u64> {
        let Kids::Groups(g) = &mut self.kids else {
            return self.shown();
        };
        if let Some((_, n)) = &g.starts {
            return Some(*n);
        }
        g.total?;
        let mut starts = Vec::with_capacity(g.nodes.len());
        let mut at = 0;
        let mut done = true;
        for node in &mut g.nodes {
            starts.push(at);
            match node.as_mut().and_then(Node::finish) {
                Some(n) => at += 1 + n,
                None => done = false,
            }
        }
        if !done {
            return None;
        }
        g.starts = Some((starts, at));
        Some(at)
    }

    /// The Leaf at `index` among this group's Leaves, if fetched.
    fn leaf(&self, index: u64) -> Option<Key> {
        let Kids::Leaves(runs) = &self.kids else {
            return None;
        };
        let (start, keys) = runs.range(..=index).next_back()?;
        keys.get((index - start) as usize).copied()
    }

    fn child(&self, value: &Value) -> Option<&Node<A>> {
        let Kids::Groups(g) = &self.kids else {
            return None;
        };
        g.nodes[*g.by_value.get(value)?].as_ref()
    }

    fn child_mut(&mut self, value: &Value) -> Option<&mut Node<A>> {
        let Kids::Groups(g) = &mut self.kids else {
            return None;
        };
        g.nodes[*g.by_value.get(value)?].as_mut()
    }

    /// What is still to ask for so that every group under this node is
    /// known.
    fn needs(&self, asks: &mut Vec<Ask>) {
        let Kids::Groups(g) = &self.kids else {
            return;
        };
        let Some(total) = g.total else {
            asks.push(Ask {
                path: self.path.clone(),
                want: Want::Groups,
                range: 0..PAGE,
            });
            return;
        };
        for page in (0..total).step_by(PAGE as usize) {
            let end = (page + PAGE).min(total);
            if g.nodes[page as usize..end as usize]
                .iter()
                .any(Option::is_none)
            {
                asks.push(Ask {
                    path: self.path.clone(),
                    want: Want::Groups,
                    range: page..end,
                });
            }
        }
        for node in g.nodes.iter().flatten() {
            node.needs(asks);
        }
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
/// knows the axis. Windows ([`GroupingLayer::window`]) answer with what is
/// known and ask for the rest in the next batch. Axes with no levels are
/// not the layer's: the host's own source answers them.
pub struct GroupingLayer<A> {
    stamp: Stamp,
    spec: ViewSpec,
    /// The rows' scope while they are grouped.
    scope: Option<Scope>,
    root: Node<A>,
    keys: KeyInterner<SummaryId>,
    next_id: u64,
    in_flight: Vec<(RequestId, Ask)>,
    /// What Windows and Locates found missing since the last batch.
    misses: Vec<Ask>,
    /// Whether the applied spec has been given for the current stamp.
    settled: bool,
    news: VecDeque<Viewed>,
    /// Answers to Locates: the path and index of each Leaf asked about, or
    /// `None` for one gone from the view.
    located: HashMap<Key, Option<(Path, u64)>>,
}

impl<A> Default for GroupingLayer<A> {
    fn default() -> Self {
        GroupingLayer::new()
    }
}

/// The scope a View Spec gives the rows, if they are grouped.
fn scope_of(spec: &ViewSpec) -> Option<Scope> {
    let grouping = spec.grouping.as_ref()?;
    let levels: Vec<Key> = (grouping.row_levels.iter())
        .filter_map(|l| l.field_key())
        .collect();
    if levels.is_empty() {
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
    })
}

impl<A> GroupingLayer<A> {
    /// A layer for a plain table.
    pub fn new() -> Self {
        GroupingLayer {
            stamp: Stamp(0),
            spec: ViewSpec::default(),
            scope: None,
            root: Node::new(Vec::new(), None, 0),
            keys: KeyInterner::new(),
            next_id: 0,
            in_flight: Vec::new(),
            misses: Vec::new(),
            settled: true,
            news: VecDeque::new(),
            located: HashMap::new(),
        }
    }

    /// Takes the View Spec the Grid asked for. A spec that groups the rows
    /// differently (other levels, values, slicers, filter, hidden Leaves or
    /// sort) starts the rows afresh, and the old batches' answers are
    /// dropped; otherwise the spec is applied as it stands.
    pub fn request_view(&mut self, spec: ViewSpec) {
        let scope = scope_of(&spec);
        self.spec = spec;
        if scope != self.scope {
            self.reset(scope);
        } else if self.settled {
            self.news.push_back(Viewed {
                applied: self.spec.clone(),
                totals: Totals::unknown(),
                notices: Vec::new(),
            });
        }
    }

    /// Starts the grouped axes afresh under the same View Spec, say because
    /// the data changed.
    pub fn refresh(&mut self) {
        self.reset(self.scope.clone());
    }

    fn reset(&mut self, scope: Option<Scope>) {
        self.stamp.0 += 1;
        let levels = scope.as_ref().map_or(0, |s| s.levels.len());
        self.root = Node::new(Vec::new(), None, levels);
        self.scope = scope;
        self.in_flight.clear();
        self.misses.clear();
        self.located.clear();
        self.settled = false;
        self.settle();
    }

    /// Whether `axis` is grouped, and so the layer's to answer.
    pub fn grouped(&self, axis: Axis) -> bool {
        axis == Axis::Rows && self.scope.is_some()
    }

    /// The next batch of aggregate queries, if there is anything to ask that
    /// is not asked already.
    pub fn poll_batch(&mut self) -> Option<Batch> {
        let scope = self.scope.clone()?;
        let mut asks = Vec::new();
        self.root.needs(&mut asks);
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
    /// and parts naming groups the layer does not know, are dropped.
    pub fn answer(&mut self, answer: Answer<A>) {
        if answer.stamp != self.stamp || self.scope.is_none() {
            return;
        }
        if let Some(id) = answer.id {
            self.in_flight.retain(|(i, _)| *i != id);
        }
        let levels = self.scope.as_ref().map_or(0, |s| s.levels.len());
        match answer.body {
            Body::Located { key, found } => {
                self.located
                    .insert(key, found.then_some((answer.path, answer.start)));
            }
            body => {
                if let Some(node) = node_mut(&mut self.root, &answer.path) {
                    place(node, answer.start, body, levels, &self.scope);
                }
            }
        }
        self.settle();
    }

    /// Gives the applied spec once every group the axis shows is known.
    fn settle(&mut self) {
        if self.settled {
            return;
        }
        let notices = match &self.scope {
            Some(_) => match self.root.finish() {
                Some(count) => vec![(Axis::Rows, ChangeNotice::Refiltered(count))],
                None => return,
            },
            None => Vec::new(),
        };
        self.settled = true;
        self.news.push_back(Viewed {
            applied: self.spec.clone(),
            totals: Totals::unknown(),
            notices,
        });
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
        if !self.grouped(axis) || !self.settled {
            return None;
        }
        self.root.shown()
    }

    /// The Keys and outlines of the items at `range` on `axis`, from its
    /// start up to the first one not fetched yet, which the next batch asks
    /// for.
    pub fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<(Key, Outline)> {
        let Some(count) = self.count(axis) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        for position in range.start..range.end.min(count) {
            match item_at(&self.root, &mut self.keys, position) {
                Ok(item) => items.push(item),
                Err(ask) => {
                    self.misses.push(ask);
                    break;
                }
            }
        }
        items
    }

    /// Where `key` sits on `axis`; `None` while the layer has asked the
    /// backend and waits for its answer.
    pub fn locate(&mut self, axis: Axis, key: Key) -> Option<Located> {
        let gone = Located::Gone { suggest: None };
        if self.count(axis).is_none() {
            return Some(gone);
        }
        if key.0 & SUMMARY_KEY_BIT != 0 {
            let found = match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) {
                Some(SummaryId::Group(path)) => group_position(&self.root, path),
                _ => None,
            };
            return Some(found.map_or(gone, Located::Found));
        }
        if let Some(found) = leaf_position(&self.root, key, 0) {
            return Some(Located::Found(found));
        }
        match self.located.get(&key) {
            Some(Some((path, index))) => {
                let at = group_position(&self.root, path).map(|g| g + 1 + index);
                Some(at.map_or(gone, Located::Found))
            }
            Some(None) => Some(gone),
            None => {
                self.misses.push(Ask {
                    path: Vec::new(),
                    want: Want::Locate(key),
                    range: 0..0,
                });
                None
            }
        }
    }

    /// The Summary Item with `key`, if the layer knows it.
    pub fn summary(&self, key: Key) -> Option<Summary<'_, A>> {
        if key.0 & SUMMARY_KEY_BIT == 0 {
            return None;
        }
        let Some(SummaryId::Group(path)) = self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) else {
            return None;
        };
        let node = node_at(&self.root, path)?;
        let row = node.row.as_ref()?;
        Some(Summary {
            kind: ItemKind::Group,
            path: &node.path,
            children: row.children,
            aggregates: &row.aggregates,
        })
    }
}

/// Puts `body`, which starts at `start` under `node`, in place.
fn place<A>(node: &mut Node<A>, start: u64, body: Body<A>, levels: usize, scope: &Option<Scope>) {
    match (body, &mut node.kids) {
        (Body::Groups { total, groups }, Kids::Groups(g)) => {
            if g.total != Some(total) {
                g.total = Some(total);
                g.nodes.clear();
                g.nodes.resize_with(total as usize, || None);
                g.by_value.clear();
                g.starts = None;
            }
            let Some(field) = scope.as_ref().and_then(|s| s.levels.get(node.path.len())) else {
                return;
            };
            for (i, row) in (start..).zip(groups) {
                let Some(slot) = g.nodes.get_mut(i as usize) else {
                    break;
                };
                if slot.is_some() {
                    continue;
                }
                let mut path = node.path.clone();
                path.push((*field, row.value.clone()));
                g.by_value.insert(row.value.clone(), i as usize);
                *slot = Some(Node::new(path, Some(row), levels));
                g.starts = None;
            }
        }
        (Body::Leaves { keys }, Kids::Leaves(runs)) if !keys.is_empty() => {
            runs.insert(start, keys);
        }
        _ => {}
    }
}

fn node_at<'n, A>(root: &'n Node<A>, path: &Path) -> Option<&'n Node<A>> {
    path.iter()
        .try_fold(root, |node, (_, value)| node.child(value))
}

fn node_mut<'n, A>(root: &'n mut Node<A>, path: &Path) -> Option<&'n mut Node<A>> {
    let mut node = root;
    for (_, value) in path {
        node = node.child_mut(value)?;
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

/// The item at `position` among those shown under `node`, or what to ask
/// for to know it. `node`'s groups are all known.
fn item_at<A>(
    node: &Node<A>,
    keys: &mut KeyInterner<SummaryId>,
    position: Position,
) -> Result<(Key, Outline), Ask> {
    match &node.kids {
        Kids::Groups(g) => {
            let (starts, _) = g.starts.as_ref().expect("a settled node knows its groups");
            let i = starts.partition_point(|&s| s <= position) - 1;
            let child = g.nodes[i]
                .as_ref()
                .expect("a settled node holds its groups");
            if position == starts[i] {
                let (field, value) = child.path.last().cloned().expect("a group has a path");
                let outline =
                    Outline::group(ancestors(keys, &node.path), OpenState::Open, field, value);
                Ok((
                    summary_key(keys, SummaryId::Group(child.path.clone())),
                    outline,
                ))
            } else {
                item_at(child, keys, position - starts[i] - 1)
            }
        }
        Kids::Leaves(_) => match node.leaf(position) {
            Some(key) => Ok((key, Outline::leaf(ancestors(keys, &node.path)))),
            None => {
                let page = position / PAGE * PAGE;
                Err(Ask {
                    path: node.path.clone(),
                    want: Want::Leaves,
                    range: page..(page + PAGE).min(node.children()),
                })
            }
        },
    }
}

/// Where the Group Item at `path` sits among the items shown, if known.
fn group_position<A>(root: &Node<A>, path: &Path) -> Option<Position> {
    let mut node = root;
    let mut at = 0;
    for (depth, (_, value)) in path.iter().enumerate() {
        let Kids::Groups(g) = &node.kids else {
            return None;
        };
        let i = *g.by_value.get(value)?;
        at += g.starts.as_ref()?.0[i];
        if depth + 1 == path.len() {
            return Some(at);
        }
        at += 1;
        node = g.nodes[i].as_ref()?;
    }
    None
}

/// Where the Leaf with `key` sits among the items shown under `node` (from
/// `base`), if it is among the Leaves fetched.
fn leaf_position<A>(node: &Node<A>, key: Key, base: Position) -> Option<Position> {
    match &node.kids {
        Kids::Groups(g) => {
            let (starts, _) = g.starts.as_ref()?;
            g.nodes
                .iter()
                .zip(starts)
                .find_map(|(child, &start)| leaf_position(child.as_ref()?, key, base + start + 1))
        }
        Kids::Leaves(runs) => runs.iter().find_map(|(start, keys)| {
            let i = keys.iter().position(|&k| k == key)?;
            Some(base + start + i as u64)
        }),
    }
}

/// Pumps `layer` against a backend that answers at once, until it asks for
/// nothing more.
pub fn settle<A>(layer: &mut GroupingLayer<A>, backend: &mut impl AggregateBackend<A>) {
    while let Some(batch) = layer.poll_batch() {
        for answer in backend.answer(&batch) {
            layer.answer(answer);
        }
    }
}
