//! The grouping layer: a sans-IO state machine that is a Table Source for a
//! grouped axis, built from the answers to its aggregate queries.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, Expand, GRAND_TOTAL, GroupOf, ItemKind, Key, Level, Located, OpenState,
    Outline, Placement, Position, Totals, Value, ViewSpec,
};
use binnacle_memory::{KeyInterner, Viewed};

use crate::query::{
    AggregateBackend, Answer, Batch, Body, GroupRow, Path, Request, RequestId, Scope, Stamp, Want,
};

/// How many groups or Leaves one request asks for.
const PAGE: u64 = 64;

/// The bit set in every Summary Item Key the layer issues, so they never
/// meet a Leaf Key: a backend's Leaf Keys must keep it clear. The Grand
/// Total is the first, [`GRAND_TOTAL`].
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
/// path (empty for the Grand Total), how many children the group has (the
/// top groups, for the Grand Total) and its aggregates, one per value of
/// the grouping, as the backend gave them. A Subtotal's are its group's.
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

/// Where the Subtotals of each level and the Grand Total go.
#[derive(Clone, Debug, Default, PartialEq)]
struct Shape {
    /// One per level of the scope, outermost first.
    subtotals: Vec<Placement>,
    grand_total: Placement,
}

impl Shape {
    fn of(spec: &ViewSpec) -> Shape {
        let Some(grouping) = &spec.grouping else {
            return Shape::default();
        };
        Shape {
            subtotals: (grouping.row_levels.iter())
                .filter_map(|l| match l {
                    Level::Field { subtotal, .. } => Some(*subtotal),
                    _ => None,
                })
                .collect(),
            grand_total: grouping.rows.grand_total,
        }
    }

    fn subtotal(&self, level: usize) -> Placement {
        self.subtotals.get(level).copied().unwrap_or_default()
    }

    /// How many items a group of `level` shows besides those under it:
    /// its Group Item and any Subtotal.
    fn own(&self, level: usize) -> u64 {
        1 + u64::from(self.subtotal(level) != Placement::Off)
    }

    /// Where the items under a group of `level` start, past its Group Item
    /// and a Subtotal placed before.
    fn lead(&self, level: usize) -> u64 {
        1 + u64::from(self.subtotal(level) == Placement::Before)
    }

    /// Where the top groups start: past a Grand Total placed before.
    fn top(&self) -> u64 {
        u64::from(self.grand_total == Placement::Before)
    }
}

/// Which groups of the rows are open, as an `expand` says.
#[derive(Clone, Debug, Default)]
struct Open {
    expand: Expand,
    except: HashSet<Key>,
}

impl Open {
    fn of(spec: &ViewSpec) -> Open {
        let expand =
            (spec.grouping.as_ref()).map_or_else(Expand::default, |g| g.rows.expand.clone());
        let except = expand.except.iter().copied().collect();
        Open { expand, except }
    }

    fn same(&self, other: &Open) -> bool {
        self.expand.depth == other.expand.depth && self.except == other.except
    }

    /// Whether the group at `node` shows what is under it: it is open and
    /// has something to show.
    fn shows<A>(&self, node: &Node<A>) -> bool {
        let Some(level) = node.path.len().checked_sub(1) else {
            return true;
        };
        node.children() > 0
            && self.expand.opens_level(level as u32) != self.except.contains(&node.key)
    }
}

/// Leaves out of `expand.except` the Group Item Keys whose paths do not fit
/// `levels`. Keys the layer never issued, and Leaf Keys, stay.
fn prune(keys: &KeyInterner<SummaryId>, levels: &[Key], expand: &mut Expand) {
    expand.except.retain(|k| {
        if k.0 & SUMMARY_KEY_BIT == 0 {
            return true;
        }
        match keys.identity(Key(k.0 & !SUMMARY_KEY_BIT)) {
            None => true,
            Some(SummaryId::Group(path)) => {
                !path.is_empty()
                    && path.len() <= levels.len()
                    && path.iter().zip(levels).all(|((f, _), l)| f == l)
            }
            Some(_) => false,
        }
    });
}

/// A group (or the whole axis, at the root) and what is known under it.
struct Node<A> {
    path: Path,
    /// Its Group Item's Key.
    key: Key,
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

impl<A> Kids<A> {
    /// Nothing known under a group at `depth` of `levels` levels.
    fn unknown(depth: usize, levels: usize) -> Self {
        match depth < levels {
            true => Kids::Groups(Groups {
                total: None,
                nodes: Vec::new(),
                by_value: HashMap::new(),
                starts: None,
            }),
            false => Kids::Leaves(BTreeMap::new()),
        }
    }
}

impl<A> Node<A> {
    /// A node at `path` over `levels` levels in all.
    fn new(path: Path, key: Key, row: Option<GroupRow<A>>, levels: usize) -> Self {
        let kids = Kids::unknown(path.len(), levels);
        Node {
            path,
            key,
            row,
            kids,
        }
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
    /// known allow; returns how many are shown, if known. A closed group
    /// shows its Group Item alone.
    fn finish(&mut self, shape: &Shape, open: &Open) -> Option<u64> {
        let own = shape.own(self.path.len());
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
            match node {
                Some(n) if !open.shows(n) => at += 1,
                Some(n) => match n.finish(shape, open) {
                    Some(n) => at += own + n,
                    None => done = false,
                },
                None => done = false,
            }
        }
        if !done {
            return None;
        }
        g.starts = Some((starts, at));
        Some(at)
    }

    /// Forgets where the items under this node sit, for a new shape.
    fn unfinish(&mut self) {
        if let Kids::Groups(g) = &mut self.kids {
            g.starts = None;
            g.nodes.iter_mut().flatten().for_each(Node::unfinish);
        }
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

    /// What is still to ask for so that every group under this node that
    /// `open` shows is known.
    fn needs(&self, open: &Open, asks: &mut Vec<Ask>) {
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
        for node in g.nodes.iter().flatten().filter(|n| open.shows(n)) {
            node.needs(open, asks);
        }
    }

    /// Forgets what is under the groups `open` closes, so memory grows only
    /// with the open groups.
    fn fold(&mut self, open: &Open, levels: usize) {
        let Kids::Groups(g) = &mut self.kids else {
            return;
        };
        for node in g.nodes.iter_mut().flatten() {
            match open.shows(node) {
                true => node.fold(open, levels),
                false => node.kids = Kids::unknown(node.path.len(), levels),
            }
        }
    }

    /// How many groups and Leaves are held under this node.
    fn held(&self) -> u64 {
        match &self.kids {
            Kids::Groups(g) => g.nodes.iter().flatten().map(|n| 1 + n.held()).sum(),
            Kids::Leaves(runs) => {
                let (mut held, mut end) = (0, 0);
                for (&start, keys) in runs {
                    let stop = start + keys.len() as u64;
                    held += stop.saturating_sub(start.max(end));
                    end = end.max(stop);
                }
                held
            }
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
    shape: Shape,
    /// The groups open in the layout given, and those the latest spec
    /// opens, which differ while what they show is asked for.
    open: Open,
    want: Open,
    root: Node<A>,
    /// The Grand Total's aggregates, once answered.
    total: Option<Vec<A>>,
    keys: KeyInterner<SummaryId>,
    next_id: u64,
    in_flight: Vec<(RequestId, Ask)>,
    /// What Windows and Locates found missing since the last batch.
    misses: Vec<Ask>,
    /// Whether the applied spec has been given for the latest request.
    settled: bool,
    /// Whether the layout under `open` is worked out and given.
    laid_out: bool,
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
        let mut keys = KeyInterner::new();
        keys.intern(SummaryId::GrandTotal);
        GroupingLayer {
            stamp: Stamp(0),
            spec: ViewSpec::default(),
            scope: None,
            shape: Shape::default(),
            open: Open::default(),
            want: Open::default(),
            root: Node::new(Vec::new(), GRAND_TOTAL, None, 0),
            total: None,
            keys,
            next_id: 0,
            in_flight: Vec::new(),
            misses: Vec::new(),
            settled: true,
            laid_out: false,
            news: VecDeque::new(),
            located: HashMap::new(),
        }
    }

    /// Takes the View Spec the Grid asked for. A spec that groups the rows
    /// differently (other levels, values, slicers, filter, hidden Leaves or
    /// sort) starts the rows afresh, and the old batches' answers are
    /// dropped; one that only places Subtotals or the Grand Total
    /// elsewhere lays out what is known again; one that opens or closes
    /// groups is applied once what they show is known, with `Expanded` or
    /// `Collapsed` for one group the user sees and `Refiltered` for more;
    /// otherwise the spec is applied as it stands. The rows' `expand.except`
    /// loses the Group Item Keys whose paths no longer fit the levels.
    pub fn request_view(&mut self, mut spec: ViewSpec) {
        let scope = scope_of(&spec);
        let shape = Shape::of(&spec);
        if let Some(grouping) = spec.grouping.as_mut() {
            let levels = scope.as_ref().map_or(&[][..], |s| &s.levels);
            prune(&self.keys, levels, &mut grouping.rows.expand);
        }
        self.want = Open::of(&spec);
        self.spec = spec;
        if scope != self.scope {
            self.shape = shape;
            self.reset(scope);
            return;
        }
        if shape != self.shape {
            self.shape = shape;
            self.root.unfinish();
            self.laid_out = false;
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
        let levels = scope.as_ref().map_or(0, |s| s.levels.len());
        self.root = Node::new(Vec::new(), GRAND_TOTAL, None, levels);
        self.total = None;
        self.scope = scope;
        self.in_flight.clear();
        self.misses.clear();
        self.located.clear();
        self.settled = false;
        self.laid_out = false;
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
        if self.total.is_none() && self.shape.grand_total != Placement::Off {
            asks.push(Ask {
                path: Vec::new(),
                want: Want::Total,
                range: 0..0,
            });
        }
        self.root.needs(&self.want, &mut asks);
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
            Body::Total { aggregates } => {
                if answer.path.is_empty() {
                    self.total = Some(aggregates);
                }
            }
            body => {
                let (open, want) = (&self.open, &self.want);
                let shown = |n: &Node<A>| open.shows(n) || want.shows(n);
                if let Some(node) = node_mut(&mut self.root, &answer.path, shown) {
                    place(
                        node,
                        answer.start,
                        body,
                        levels,
                        &self.scope,
                        &mut self.keys,
                    );
                }
            }
        }
        self.settle();
    }

    /// Gives the applied spec once every group the axis shows under it is
    /// known.
    fn settle(&mut self) {
        if self.settled {
            return;
        }
        let notices = match &self.scope {
            Some(_) => {
                let mut asks = Vec::new();
                self.root.needs(&self.want, &mut asks);
                if !asks.is_empty() {
                    return;
                }
                let notice = match self.laid_out {
                    true => self.unfold(),
                    false => {
                        self.open = self.want.clone();
                        self.root.finish(&self.shape, &self.open);
                        self.laid_out = true;
                        self.count(Axis::Rows).map(ChangeNotice::Refiltered)
                    }
                };
                let levels = self.scope.as_ref().map_or(0, |s| s.levels.len());
                self.root.fold(&self.open, levels);
                notice.map(|n| (Axis::Rows, n)).into_iter().collect()
            }
            None => Vec::new(),
        };
        self.settled = true;
        self.laid_out = true;
        self.news.push_back(Viewed {
            applied: self.spec.clone(),
            totals: Totals::unknown(),
            notices,
        });
    }

    /// Lays the groups out as the latest spec opens them, all known, and
    /// says how: `Collapsed` or `Expanded` when one group the user sees
    /// opened or closed, nothing when no group the user sees did, and
    /// `Refiltered` for more.
    fn unfold(&mut self) -> Option<ChangeNotice> {
        if self.want.same(&self.open) {
            return None;
        }
        let single = match self.want.expand.depth == self.open.expand.depth {
            true => {
                let mut changed = self.want.except.symmetric_difference(&self.open.except);
                match (changed.next(), changed.next()) {
                    (Some(&key), None) => Some(key),
                    _ => None,
                }
            }
            false => None,
        };
        let path = single.and_then(
            |key| match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) {
                Some(SummaryId::Group(path)) if key.0 & SUMMARY_KEY_BIT != 0 => Some(path.clone()),
                _ => None,
            },
        );
        let under = |root: &Node<A>, shape: &Shape, open: &Open, path: &Path| {
            let node = node_at(root, path)?;
            let level = path.len() - 1;
            Some(match open.shows(node) {
                true => shape.own(level) - 1 + node.shown()?,
                false => 0,
            })
        };
        let before = path.as_ref().and_then(|path| {
            let at = group_position(&self.root, &self.shape, &self.open, path)?;
            Some((at, under(&self.root, &self.shape, &self.open, path)?))
        });
        self.open = self.want.clone();
        self.root.unfinish();
        self.root.finish(&self.shape, &self.open);
        if single.is_some() && before.is_none() {
            return None;
        }
        let (Some((at, was)), Some(path)) = (before, &path) else {
            return self.count(Axis::Rows).map(ChangeNotice::Refiltered);
        };
        let now = under(&self.root, &self.shape, &self.open, path)?;
        let at = at + self.shape.top() + 1;
        match now.cmp(&was) {
            std::cmp::Ordering::Greater => Some(ChangeNotice::Expanded { at, n: now - was }),
            std::cmp::Ordering::Less => Some(ChangeNotice::Collapsed { at, n: was - now }),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// How many groups and Leaves the layer holds: those under open groups,
    /// never more than it shows once the latest spec is applied.
    pub fn held(&self) -> u64 {
        self.root.held()
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
        if !self.grouped(axis) || !self.laid_out {
            return None;
        }
        let total = u64::from(self.shape.grand_total != Placement::Off);
        Some(self.root.shown()? + total)
    }

    /// The Keys and outlines of the items at `range` on `axis`, from its
    /// start up to the first one not fetched yet, which the next batch asks
    /// for.
    pub fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<(Key, Outline)> {
        let Some(count) = self.count(axis) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        let top = self.shape.top();
        for position in range.start..range.end.min(count) {
            let total = match self.shape.grand_total {
                Placement::Before => position == 0,
                Placement::After => position + 1 == count,
                Placement::Off => false,
            };
            if total {
                items.push((GRAND_TOTAL, Outline::grand_total()));
                continue;
            }
            let (root, shape, open) = (&self.root, &self.shape, &self.open);
            match item_at(root, shape, open, &mut self.keys, position - top) {
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
        let top = self.shape.top();
        let (root, shape, open) = (&self.root, &self.shape, &self.open);
        if key.0 & SUMMARY_KEY_BIT != 0 {
            let found = match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) {
                Some(SummaryId::Group(path)) => {
                    group_position(root, shape, open, path).map(|p| p + top)
                }
                Some(SummaryId::Subtotal(path)) => {
                    subtotal_position(root, shape, open, path).map(|p| p + top)
                }
                Some(SummaryId::GrandTotal) => match shape.grand_total {
                    Placement::Before => Some(0),
                    Placement::After => self.count(axis).map(|n| n - 1),
                    Placement::Off => None,
                },
                None => None,
            };
            return Some(found.map_or(gone, Located::Found));
        }
        if let Some(found) = leaf_position(root, shape, open, key, top) {
            return Some(Located::Found(found));
        }
        match self.located.get(&key) {
            Some(Some((path, index))) => {
                let lead = shape.lead(path.len().saturating_sub(1));
                let shown = node_at(root, path).is_some_and(|n| open.shows(n));
                let at = (group_position(root, shape, open, path))
                    .filter(|_| shown)
                    .map(|g| g + top + lead + index);
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
        let (kind, path) = match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT))? {
            SummaryId::Group(path) => (ItemKind::Group, path),
            SummaryId::Subtotal(path) => (ItemKind::Subtotal, path),
            SummaryId::GrandTotal => {
                let Kids::Groups(g) = &self.root.kids else {
                    return None;
                };
                return Some(Summary {
                    kind: ItemKind::GrandTotal,
                    path: &self.root.path,
                    children: g.total?,
                    aggregates: self.total.as_deref()?,
                });
            }
        };
        let node = node_at(&self.root, path)?;
        let row = node.row.as_ref()?;
        Some(Summary {
            kind,
            path: &node.path,
            children: row.children,
            aggregates: &row.aggregates,
        })
    }
}

/// Puts `body`, which starts at `start` under `node`, in place.
fn place<A>(
    node: &mut Node<A>,
    start: u64,
    body: Body<A>,
    levels: usize,
    scope: &Option<Scope>,
    keys: &mut KeyInterner<SummaryId>,
) {
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
                let key = summary_key(keys, SummaryId::Group(path.clone()));
                *slot = Some(Node::new(path, key, Some(row), levels));
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

/// The item at `position` among those shown under `node`, or what to ask
/// for to know it. `node`'s groups are all known.
fn item_at<A>(
    node: &Node<A>,
    shape: &Shape,
    open: &Open,
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
            let level = node.path.len();
            let at = position - starts[i];
            let (field, value) = child.path.last().cloned().expect("a group has a path");
            let subtotal = match shape.subtotal(level) {
                Placement::Before => at == 1,
                Placement::After => Some(at) == child.shown().map(|n| n + 1),
                Placement::Off => false,
            };
            if at == 0 {
                let state = match (child.children(), open.shows(child)) {
                    (0, _) => OpenState::CantOpen,
                    (_, true) => OpenState::Open,
                    (_, false) => OpenState::Closed,
                };
                let span = match state {
                    OpenState::Open => shape.own(level) - 1 + child.shown().unwrap_or(0),
                    _ => 0,
                };
                let outline = Outline::group(ancestors(keys, &node.path), state, field, value)
                    .with_span(span)
                    .with_path(groups(&node.path));
                Ok((child.key, outline))
            } else if subtotal {
                let outline = Outline::subtotal(ancestors(keys, &child.path), field, value)
                    .with_path(groups(&child.path));
                Ok((
                    summary_key(keys, SummaryId::Subtotal(child.path.clone())),
                    outline,
                ))
            } else {
                item_at(child, shape, open, keys, at - shape.lead(level))
            }
        }
        Kids::Leaves(_) => match node.leaf(position) {
            Some(key) => {
                let outline = Outline::leaf(ancestors(keys, &node.path));
                Ok((key, outline.with_path(groups(&node.path))))
            }
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

/// Where the Group Item at `path` sits among the items under the root, if
/// known and shown.
fn group_position<A>(root: &Node<A>, shape: &Shape, open: &Open, path: &Path) -> Option<Position> {
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
        at += shape.lead(depth);
        node = g.nodes[i].as_ref().filter(|n| open.shows(n))?;
    }
    None
}

/// Where the Subtotal of the group at `path` sits among the items under
/// the root, if known and placed.
fn subtotal_position<A>(
    root: &Node<A>,
    shape: &Shape,
    open: &Open,
    path: &Path,
) -> Option<Position> {
    let level = path.len().checked_sub(1)?;
    let at = group_position(root, shape, open, path)?;
    let node = node_at(root, path).filter(|n| open.shows(n))?;
    match shape.subtotal(level) {
        Placement::Before => Some(at + 1),
        Placement::After => Some(at + 1 + node.shown()?),
        Placement::Off => None,
    }
}

/// Where the Leaf with `key` sits among the items shown under `node` (from
/// `base`), if it is among the Leaves fetched.
fn leaf_position<A>(
    node: &Node<A>,
    shape: &Shape,
    open: &Open,
    key: Key,
    base: Position,
) -> Option<Position> {
    match &node.kids {
        Kids::Groups(g) => {
            let (starts, _) = g.starts.as_ref()?;
            let lead = shape.lead(node.path.len());
            g.nodes.iter().zip(starts).find_map(|(child, &start)| {
                let child = child.as_ref().filter(|n| open.shows(n))?;
                leaf_position(child, shape, open, key, base + start + lead)
            })
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
