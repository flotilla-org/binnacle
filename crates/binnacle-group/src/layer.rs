//! The grouping layer: a sans-IO state machine that is a Table Source for a
//! grouped axis, built from the answers to its aggregate queries.

use std::cell::Cell;
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
pub(crate) struct Shape {
    /// One per level of the scope, outermost first.
    subtotals: Vec<Placement>,
    pub(crate) grand_total: Placement,
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

    pub(crate) fn subtotal(&self, level: usize) -> Placement {
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
    pub(crate) fn top(&self) -> u64 {
        u64::from(self.grand_total == Placement::Before)
    }
}

/// Which groups of the rows are open, as an `expand` says.
#[derive(Clone, Debug, Default)]
pub(crate) struct Open {
    expand: Expand,
    except: HashSet<Key>,
    /// The values of the groups `except` names, by their parent's path.
    named: HashMap<Path, Vec<Value>>,
}

impl Open {
    fn of(spec: &ViewSpec, keys: &KeyInterner<SummaryId>) -> Open {
        let expand =
            (spec.grouping.as_ref()).map_or_else(Expand::default, |g| g.rows.expand.clone());
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
}

/// Where the items under a node sit, worked out once every group under it
/// that may show what is under it is known. A closed group shows its
/// Group Item alone, so the groups not fetched take one Position each.
#[derive(Clone, Debug, Default)]
struct Layout {
    /// The groups shown open, by index.
    open: Vec<Span>,
    /// How many items are shown under the node.
    shown: u64,
}

/// One open group's place among the items under its parent: its index,
/// the Position of its Group Item, and how many items it shows, its Group
/// Item and Subtotal included.
#[derive(Clone, Copy, Debug)]
struct Span {
    index: u64,
    at: u64,
    len: u64,
}

impl Layout {
    /// Where the Group Item of the group at `index` sits.
    fn position(&self, index: u64) -> u64 {
        let i = self.open.partition_point(|s| s.index < index);
        match i.checked_sub(1).map(|j| self.open[j]) {
            Some(s) => s.at + s.len + (index - s.index - 1),
            None => index,
        }
    }

    /// The span of the group at `index`, if it is shown open.
    fn span(&self, index: u64) -> Option<Span> {
        let i = self.open.partition_point(|s| s.index < index);
        self.open.get(i).filter(|s| s.index == index).copied()
    }

    /// The group whose items take `position`, and how far into them it is.
    fn at(&self, position: u64) -> (u64, u64) {
        let i = self.open.partition_point(|s| s.at <= position);
        match i.checked_sub(1).map(|j| self.open[j]) {
            Some(s) if position < s.at + s.len => (s.index, position - s.at),
            Some(s) => (s.index + 1 + (position - s.at - s.len), 0),
            None => (position, 0),
        }
    }
}

impl<A> Kids<A> {
    /// Nothing known under a group at `depth` of `levels` levels.
    fn unknown(depth: usize, levels: usize) -> Self {
        match depth < levels {
            true => Kids::Groups(Groups {
                total: None,
                nodes: BTreeMap::new(),
                by_value: HashMap::new(),
                layout: None,
            }),
            false => Kids::Leaves(BTreeMap::new()),
        }
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
    /// A node at `path` over `levels` levels in all.
    fn new(path: Path, key: Key, row: Option<GroupRow<A>>, levels: usize, used: u64) -> Self {
        let kids = Kids::unknown(path.len(), levels);
        Node {
            path,
            key,
            row,
            kids,
            used: Cell::new(used),
        }
    }

    fn children(&self) -> u64 {
        self.row.as_ref().map_or(0, |r| r.children)
    }

    /// How many items are shown under this node, once laid out.
    fn shown(&self) -> Option<u64> {
        match &self.kids {
            Kids::Groups(g) => g.layout.as_ref().map(|l| l.shown),
            Kids::Leaves(_) => Some(self.children()),
        }
    }

    /// Lays out the items under this node as `open` opens them, as far as
    /// the groups known allow; returns how many are shown, if known.
    fn finish(&mut self, shape: &Shape, open: &Open) -> Option<u64> {
        let own = shape.own(self.path.len());
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
        let mut spans = Vec::new();
        let mut extra = 0;
        let mut done = true;
        for (&index, node) in &mut g.nodes {
            if !open.shows(node) {
                continue;
            }
            match node.finish(shape, open) {
                Some(n) => {
                    let len = own + n;
                    spans.push(Span {
                        index,
                        at: index + extra,
                        len,
                    });
                    extra += len - 1;
                }
                None => done = false,
            }
        }
        if !done {
            return None;
        }
        let shown = total + extra;
        g.layout = Some(Layout { open: spans, shown });
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

    /// What is still to ask for so that the items under this node can be
    /// laid out as `open` opens them.
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
        if open.needs_all(self) {
            let known = |i| g.nodes.contains_key(&i);
            for range in missing(0..total, total, known) {
                asks.push(Ask {
                    path: self.path.clone(),
                    want: Want::Groups,
                    range,
                });
            }
        }
        for node in g.nodes.values().filter(|n| open.shows(n)) {
            node.needs(open, asks);
        }
    }

    /// Forgets what is under the groups `open` closes, so memory grows only
    /// with the open groups.
    fn fold(&mut self, open: &Open, levels: usize) {
        let Kids::Groups(g) = &mut self.kids else {
            return;
        };
        for node in g.nodes.values_mut() {
            match open.shows(node) {
                true => node.fold(open, levels),
                false => node.kids = Kids::unknown(node.path.len(), levels),
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
        }
    }

    /// How many groups and Leaves are held under this node.
    fn held(&self) -> u64 {
        match &self.kids {
            Kids::Groups(g) => g.nodes.values().map(|n| 1 + n.held()).sum(),
            Kids::Leaves(runs) => runs.values().map(|r| r.keys.len() as u64).sum(),
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
            Kids::Groups(g) => {
                let paged = opens.iter().all(|o| !o.needs_all(self));
                let mut n = 0;
                for (&index, node) in &g.nodes {
                    if opens.iter().any(|o| o.shows(node)) {
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
/// Window finds them. Axes with no levels are not the layer's: the host's
/// own source answers them.
///
/// It holds the open groups and, as a cache, pages of closed groups and of
/// Leaves: always those the latest two Windows read, and others up to its
/// size, letting go of those read longest ago. Memory grows with the open
/// groups, never with the rows.
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
    /// What Windows and Locates found missing and the answers have not
    /// covered yet: answered, it stays cached through the next Window.
    read: Vec<Ask>,
    /// Whether the applied spec has been given for the latest request.
    settled: bool,
    /// Whether the layout under `open` is worked out and given.
    laid_out: bool,
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
    /// A layer for a plain table, caching [`CACHE`] items.
    pub fn new() -> Self {
        GroupingLayer::with_cache(CACHE)
    }

    /// A layer for a plain table that caches up to `items` closed groups
    /// and Leaves besides those the latest Windows read.
    pub fn with_cache(items: u64) -> Self {
        let mut keys = KeyInterner::new();
        keys.intern(SummaryId::GrandTotal);
        GroupingLayer {
            stamp: Stamp(0),
            spec: ViewSpec::default(),
            scope: None,
            shape: Shape::default(),
            open: Open::default(),
            want: Open::default(),
            root: Node::new(Vec::new(), GRAND_TOTAL, None, 0, 0),
            total: None,
            keys,
            next_id: 0,
            in_flight: Vec::new(),
            misses: Vec::new(),
            read: Vec::new(),
            settled: true,
            laid_out: false,
            news: VecDeque::new(),
            located: HashMap::new(),
            changes: VecDeque::new(),
            frame: 1,
            cache: items,
            cached: 0,
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
        self.want = Open::of(&spec, &self.keys);
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
        self.root = Node::new(Vec::new(), GRAND_TOTAL, None, levels, 0);
        self.total = None;
        self.scope = scope;
        self.in_flight.clear();
        self.misses.clear();
        self.read.clear();
        self.located.clear();
        self.changes.clear();
        self.cached = 0;
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
    /// and parts naming groups the layer does not know or does not show,
    /// are dropped. A request is done once an answer names it or what the
    /// layer holds covers it, whichever comes first.
    pub fn answer(&mut self, answer: Answer<A>) {
        if answer.stamp != self.stamp || self.scope.is_none() {
            return;
        }
        let levels = self.scope.as_ref().map_or(0, |s| s.levels.len());
        match answer.body {
            Body::Located { key, found } => {
                self.located
                    .insert(key, found.then_some((answer.path, answer.start)));
            }
            Body::Total { aggregates } => self.figured(&answer.path, aggregates),
            body => {
                let want = match body {
                    Body::Groups { .. } => Want::Groups,
                    _ => Want::Leaves,
                };
                // What a Window asked for stays cached through the next one;
                // what none did goes first.
                let frame = self.frame;
                let asked = (self.read.iter())
                    .filter(|a| a.path == answer.path && a.want == want)
                    .map(|a| a.range.clone())
                    .collect::<Vec<_>>();
                let fresh = |r: Range<u64>| match asked
                    .iter()
                    .any(|a| a.start < r.end && r.start < a.end)
                {
                    true => frame,
                    false => 0,
                };
                let (open, want) = (&self.open, &self.want);
                let shown = |n: &Node<A>| open.shows(n) || want.shows(n);
                if let Some(node) = node_mut(&mut self.root, &answer.path, shown) {
                    let added = place(
                        node,
                        answer.start,
                        body,
                        levels,
                        &self.scope,
                        &mut self.keys,
                        fresh,
                    );
                    self.cached = self.cached.saturating_add(added);
                }
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
        let top = self.shape.top();
        let (root, shape) = (&self.root, &self.shape);
        let at: Vec<Position> = match path.is_empty() {
            true if self.total.is_some() => match shape.grand_total {
                Placement::Before => vec![0],
                Placement::After => self.count(Axis::Rows).map(|n| n - 1).into_iter().collect(),
                Placement::Off => Vec::new(),
            },
            true => Vec::new(),
            false => [
                group_position(root, shape, path),
                subtotal_position(root, shape, path),
            ]
            .into_iter()
            .flatten()
            .map(|p| p + top)
            .collect(),
        };
        match path.is_empty() {
            true => self.total = Some(aggregates),
            false => match node_mut(&mut self.root, path, |_| true).and_then(|n| n.row.as_mut()) {
                Some(row) => row.aggregates = aggregates,
                None => return,
            },
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
        let mut paths = vec![Vec::new()];
        self.root.held_groups(&mut paths);
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

    /// Where the Subtotals and the Grand Total go, as last asked.
    pub(crate) fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Which groups are open in the layout given.
    pub(crate) fn layout(&self) -> &Open {
        &self.open
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
                SummaryId::GrandTotal => Some(&self.root.path),
            };
        }
        self.root.holding(key).map(|n| &n.path)
    }

    /// Where a move of Leaves of the group at `path` on `axis` to before
    /// Position `to` lands: the index among the group's Leaves, which the
    /// backend maps to its own order. `None` for a Position outside the
    /// group's Leaves, or a group not shown.
    pub fn place_move(&self, axis: Axis, path: &Path, to: Position) -> Option<u64> {
        if !self.grouped(axis) {
            return None;
        }
        let (root, shape, open) = (&self.root, &self.shape, &self.open);
        let node = node_at(root, path).filter(|n| open.shows(n))?;
        let level = path.len().saturating_sub(1);
        let start = group_position(root, shape, path)? + shape.top() + shape.lead(level);
        let index = to.checked_sub(start)?;
        (index <= node.children()).then_some(index)
    }

    /// Whether what the layer holds answers `ask`.
    fn covers(&self, ask: &Ask) -> bool {
        match ask.want {
            Want::Total => self.total.is_some(),
            Want::Locate(key) => self.located.contains_key(&key),
            Want::Groups => node_at(&self.root, &ask.path).is_some_and(|n| match &n.kids {
                Kids::Groups(g) => g.total.is_some_and(|total| {
                    let known = |i| g.nodes.contains_key(&i);
                    missing(ask.range.clone(), total, known).is_empty()
                }),
                Kids::Leaves(_) => false,
            }),
            Want::Leaves => node_at(&self.root, &ask.path).is_some_and(|n| {
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
        let mut cached = self.root.cached([&self.open, &self.want], &mut spots);
        spots.sort_by_key(|(used, _, _)| *used);
        for (used, path, spot) in spots {
            if cached <= self.cache || (used > 0 && used + 1 >= self.frame) {
                break;
            }
            // What a request still due covers stays until it is done.
            let in_flight = &self.in_flight;
            let due = |want: Want, range: Range<u64>| {
                in_flight.iter().any(|(_, a)| {
                    a.path == path
                        && a.want == want
                        && a.range.start < range.end
                        && range.start < a.range.end
                })
            };
            let Some(node) = node_mut(&mut self.root, &path, |_| true) else {
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
                (Spot::Group(index), Kids::Groups(g)) if !due(Want::Groups, index..index + 1) => {
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
        // What the cache may let go of changes with the groups open.
        self.cached = u64::MAX;
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
        let under = |root: &Node<A>, shape: &Shape, path: &Path| {
            let (at, span) = find_group(root, shape, path).ok()??;
            Some((at, span.map_or(0, |s| s.len - 1)))
        };
        let before = path
            .as_ref()
            .and_then(|p| under(&self.root, &self.shape, p));
        self.open = self.want.clone();
        self.root.unfinish();
        self.root.finish(&self.shape, &self.open);
        if single.is_some() && before.is_none() {
            return None;
        }
        let (Some((at, was)), Some(path)) = (before, &path) else {
            return self.count(Axis::Rows).map(ChangeNotice::Refiltered);
        };
        let (_, now) = under(&self.root, &self.shape, path)?;
        let at = at + self.shape.top() + 1;
        match now.cmp(&was) {
            std::cmp::Ordering::Greater => Some(ChangeNotice::Expanded { at, n: now - was }),
            std::cmp::Ordering::Less => Some(ChangeNotice::Collapsed { at, n: was - now }),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// How many groups and Leaves the layer holds: the open groups, and the
    /// closed groups and Leaves it caches.
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
    /// start up to the first one not fetched yet: the Grid draws the rest
    /// pending. The next batch asks for everything in `range` not fetched.
    pub fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<(Key, Outline)> {
        let Some(count) = self.count(axis) else {
            return Vec::new();
        };
        self.frame += 1;
        let mut items = Vec::new();
        let mut missed = false;
        let top = self.shape.top();
        for position in range.start..range.end.min(count) {
            let total = match self.shape.grand_total {
                Placement::Before => position == 0,
                Placement::After => position + 1 == count,
                Placement::Off => false,
            };
            if total {
                if !missed {
                    items.push((GRAND_TOTAL, Outline::grand_total()));
                }
                continue;
            }
            let (root, shape, frame) = (&self.root, &self.shape, self.frame);
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
        if self.count(axis).is_none() {
            return Some(gone);
        }
        let top = self.shape.top();
        let (root, shape) = (&self.root, &self.shape);
        let found = if key.0 & SUMMARY_KEY_BIT != 0 {
            match self.keys.identity(Key(key.0 & !SUMMARY_KEY_BIT)) {
                Some(SummaryId::Group(path)) => {
                    find_group(root, shape, path).map(|f| f.map(|f| f.0))
                }
                Some(SummaryId::Subtotal(path)) => find_group(root, shape, path).map(|f| {
                    let level = path.len().checked_sub(1)?;
                    let (at, span) = f?;
                    match shape.subtotal(level) {
                        Placement::Before => span.map(|_| at + 1),
                        Placement::After => span.map(|s| at + s.len - 1),
                        Placement::Off => None,
                    }
                }),
                Some(SummaryId::GrandTotal) => {
                    let at = match shape.grand_total {
                        Placement::Before => Some(0),
                        Placement::After => self.count(axis).map(|n| n - 1),
                        Placement::Off => None,
                    };
                    return Some(at.map_or(gone, Located::Found));
                }
                None => Ok(None),
            }
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

    /// The Summary Item with `key`, if the layer holds it.
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

/// Puts `body`, which starts at `start` under `node`, in place; returns how
/// many groups or Leaves it added. `fresh` gives what the layer asked for
/// the frame, 0 otherwise.
fn place<A>(
    node: &mut Node<A>,
    start: u64,
    body: Body<A>,
    levels: usize,
    scope: &Option<Scope>,
    keys: &mut KeyInterner<SummaryId>,
    fresh: impl Fn(Range<u64>) -> u64,
) -> u64 {
    match (body, &mut node.kids) {
        (Body::Groups { total, groups }, Kids::Groups(g)) => {
            // One stamp, one answer: a different count is not this view's.
            if g.total.is_some_and(|t| t != total) {
                return 0;
            }
            g.total = Some(total);
            let Some(field) = scope.as_ref().and_then(|s| s.levels.get(node.path.len())) else {
                return 0;
            };
            let mut added = 0;
            for (i, row) in (start..total).zip(groups) {
                if g.nodes.contains_key(&i) || g.by_value.contains_key(&row.value) {
                    continue;
                }
                let mut path = node.path.clone();
                path.push((*field, row.value.clone()));
                g.by_value.insert(row.value.clone(), i);
                let key = summary_key(keys, SummaryId::Group(path.clone()));
                let used = fresh(i..i + 1);
                g.nodes
                    .insert(i, Node::new(path, key, Some(row), levels, used));
                added += 1;
            }
            added
        }
        (Body::Leaves { keys }, Kids::Leaves(runs)) => insert_leaves(runs, start, keys, fresh),
        _ => 0,
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
                    path: node.path.clone(),
                    want: Want::Groups,
                    range: index..(page + PAGE).min(total),
                });
            };
            child.used.set(frame);
            let span = layout.span(index);
            let (field, value) = child.path.last().cloned().expect("a group has a path");
            if at == 0 {
                let state = match (child.children(), span) {
                    (0, _) => OpenState::CantOpen,
                    (_, Some(_)) => OpenState::Open,
                    (_, None) => OpenState::Closed,
                };
                let span = match span {
                    Some(span) => span.len - 1,
                    None => 0,
                };
                let outline = Outline::group(ancestors(keys, &node.path), state, field, value)
                    .with_span(span)
                    .with_path(groups(&node.path));
                return Ok((child.key, outline));
            }
            let span = span.expect("a group shows items past its own only when open");
            let subtotal = match shape.subtotal(level) {
                Placement::Before => at == 1,
                Placement::After => at + 1 == span.len,
                Placement::Off => false,
            };
            if subtotal {
                let outline = Outline::subtotal(ancestors(keys, &child.path), field, value)
                    .with_path(groups(&child.path));
                let key = summary_key(keys, SummaryId::Subtotal(child.path.clone()));
                return Ok((key, outline));
            }
            item_at(child, shape, keys, at - shape.lead(level), frame)
        }
        Kids::Leaves(_) => match node.leaf(position, Some(frame)) {
            Some(key) => {
                let outline = Outline::leaf(ancestors(keys, &node.path));
                Ok((key, outline.with_path(groups(&node.path))))
            }
            None => {
                let page = position / PAGE * PAGE;
                Err(Ask {
                    path: node.path.clone(),
                    want: Want::Leaves,
                    range: position..(page + PAGE).min(node.children()),
                })
            }
        },
    }
}

/// Where the Group Item at `path` sits among the items under the root, and
/// its span if it is open: `Ok(None)` if it is not shown (gone, or under a
/// closed group), or the pages to ask for to find it.
#[allow(clippy::type_complexity)]
fn find_group<A>(
    root: &Node<A>,
    shape: &Shape,
    path: &Path,
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
                path: node.path.clone(),
                want: Want::Groups,
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

/// Where the Subtotal of the group at `path` sits among the items under
/// the root, if known and placed.
fn subtotal_position<A>(root: &Node<A>, shape: &Shape, path: &Path) -> Option<Position> {
    let level = path.len().checked_sub(1)?;
    let (at, span) = find_group(root, shape, path).ok().flatten()?;
    let span = span?;
    match shape.subtotal(level) {
        Placement::Before => Some(at + 1),
        Placement::After => Some(at + span.len - 1),
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
            layout.open.iter().find_map(|span| {
                let child = g.nodes.get(&span.index)?;
                leaf_position(child, shape, key, base + span.at + lead)
            })
        }
        Kids::Leaves(runs) => runs.iter().find_map(|(start, run)| {
            let i = run.keys.iter().position(|&k| k == key)?;
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
