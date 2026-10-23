//! A grouped source over a hand-made tree, opening and closing its groups
//! as the View Spec's `expand` says.

use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, Depth, Expand, FindDirection, FindMatch, FindQuery, Form, GroupOf,
    Grouping, Key, Level, Located, OpenState, Outline, Position, Support, Value, ViewSpec,
};
use binnacle_memory::{FindCount, FindStep, Moved, PumpSource, Viewed};

use crate::scenario::ScenarioSource;

/// A group of a [`TreeTable`]: its Group Item's Key, its value at its level,
/// and what is under it, groups of the next level or Leaves.
#[derive(Clone, Debug)]
pub struct Group {
    pub key: Key,
    pub value: &'static str,
    pub under: Under,
}

#[derive(Clone, Debug)]
pub enum Under {
    Groups(Vec<Group>),
    Leaves(Vec<Key>),
}

impl Group {
    pub fn leaves(key: u64, value: &'static str, leaves: &[u64]) -> Self {
        Group {
            key: Key(key),
            value,
            under: Under::Leaves(leaves.iter().copied().map(Key).collect()),
        }
    }

    pub fn groups(key: u64, value: &'static str, groups: Vec<Group>) -> Self {
        Group {
            key: Key(key),
            value,
            under: Under::Groups(groups),
        }
    }
}

/// A table whose rows group by `levels` (one field per level, outermost
/// first) into a hand-made tree of [`Group`]s, with no Subtotals unless
/// [`TreeTable::subtotals_after`] places them. Its groups open as the
/// grouping's `expand` says; it answers a change of `expand` that opens or
/// closes one run of items with `Expanded` or `Collapsed`, and any other
/// change with `Refiltered`. In header form a group has no Group Item: a
/// closed one shows its Subtotal alone, and one with nothing under it its
/// Group Item, which can't open. Columns are keyed by Position; the level
/// fields stay among them. [`TreeTable::cols_by`] groups the columns too.
///
/// Find matches, ignoring case, a Leaf's cell text `r{row}c{col}` (Keys)
/// and a Group Item's label, its value, at the column of its level's field;
/// it steps by rows through every item as if all were open, a label before
/// the row's cells. Leaves move within their group while grouped; a move
/// across groups, or of a Group Item, is refused.
pub struct TreeTable {
    plain: Vec<Key>,
    levels: Vec<Key>,
    tree: Vec<Group>,
    cols: u64,
    col_levels: Vec<Level>,
    col_tree: Vec<(Key, Outline)>,
    subtotals: bool,
    grouping: Option<Grouping>,
    shown: Vec<(Key, Outline)>,
}

/// A row of [`TreeTable::every`]: its Key, its ancestors and its label.
type Row = (Key, Vec<Key>, Option<(Key, &'static str)>);

/// What a Group Item's Key gains to make its Subtotal's.
const SUBTOTAL: u64 = 1_000_000;

impl TreeTable {
    pub fn new(levels: Vec<Key>, cols: u64, tree: Vec<Group>) -> Self {
        let mut plain = Vec::new();
        fn leaves(groups: &[Group], out: &mut Vec<Key>) {
            for g in groups {
                match &g.under {
                    Under::Groups(kids) => leaves(kids, out),
                    Under::Leaves(keys) => out.extend(keys),
                }
            }
        }
        leaves(&tree, &mut plain);
        TreeTable {
            plain,
            levels,
            tree,
            cols,
            col_levels: Vec::new(),
            col_tree: Vec::new(),
            subtotals: false,
            grouping: None,
            shown: Vec::new(),
        }
    }

    /// This table with a Subtotal after each open group.
    pub fn subtotals_after(mut self) -> Self {
        self.subtotals = true;
        self
    }

    /// This table with its columns grouped by `levels` into `tree`, each
    /// group open and in header form, while the grouping asks for exactly
    /// those column levels. The values pseudo-level may be among `levels`;
    /// the tree's depth gives the levels its groups head.
    pub fn cols_by(mut self, levels: Vec<Level>, tree: Vec<Group>) -> Self {
        fn walk(groups: &[Group], up: &mut Vec<Key>, out: &mut Vec<(Key, Outline)>) {
            for g in groups {
                up.push(g.key);
                match &g.under {
                    Under::Groups(kids) => walk(kids, up, out),
                    Under::Leaves(keys) => {
                        out.extend(keys.iter().map(|&k| (k, Outline::leaf(up.clone()))));
                    }
                }
                up.pop();
            }
        }
        walk(&tree, &mut Vec::new(), &mut self.col_tree);
        self.col_levels = levels;
        self
    }

    /// The Key of the Subtotal of the group whose Group Item is `group`.
    pub fn subtotal(group: Key) -> Key {
        Key(group.0 + SUBTOTAL)
    }

    /// The rows as the table shows them now, with their outlines.
    pub fn shown(&self) -> &[(Key, Outline)] {
        &self.shown
    }

    fn cols_grouped(&self) -> bool {
        (self.grouping.as_ref()).is_some_and(|g| !g.col_levels.is_empty())
    }

    fn columns(&self) -> Vec<Key> {
        match self.cols_grouped() {
            true => self.col_tree.iter().map(|(k, _)| *k).collect(),
            false => (0..self.cols).map(Key).collect(),
        }
    }

    fn rows(&self) -> Vec<Key> {
        match self.grouping {
            Some(_) => self.shown.iter().map(|(k, _)| *k).collect(),
            None => self.plain.clone(),
        }
    }

    fn lay_out(&self, expand: &Expand, form: Form) -> Vec<(Key, Outline)> {
        let mut out = Vec::new();
        self.walk(
            &self.tree,
            &mut Vec::new(),
            &mut Vec::new(),
            expand,
            form,
            &mut out,
        );
        out
    }

    fn walk(
        &self,
        groups: &[Group],
        up: &mut Vec<Key>,
        path: &mut Vec<GroupOf>,
        expand: &Expand,
        form: Form,
        out: &mut Vec<(Key, Outline)>,
    ) {
        let level = up.len();
        for g in groups {
            let default = match expand.depth {
                Depth::All => true,
                Depth::Levels(n) => (level as u32) < n,
            };
            let open = default != expand.except.contains(&g.key);
            let empty = match &g.under {
                Under::Groups(kids) => kids.is_empty(),
                Under::Leaves(keys) => keys.is_empty(),
            };
            let state = match (empty, open) {
                (true, _) => OpenState::CantOpen,
                (false, true) => OpenState::Open,
                (false, false) => OpenState::Closed,
            };
            let (field, value) = (self.levels[level], Value::Text(g.value.into()));
            let header = form == Form::Header && state != OpenState::CantOpen;
            let at = out.len();
            if !header {
                let outline =
                    Outline::group(up.clone(), state, field, value.clone()).with_path(path.clone());
                out.push((g.key, outline));
            }
            if state == OpenState::CantOpen {
                continue;
            }
            up.push(g.key);
            path.push(GroupOf {
                field,
                value: value.clone(),
            });
            if state == OpenState::Open {
                match &g.under {
                    Under::Groups(kids) => self.walk(kids, up, path, expand, form, out),
                    Under::Leaves(keys) => {
                        let leaf = || Outline::leaf(up.clone()).with_path(path.clone());
                        out.extend(keys.iter().map(|&k| (k, leaf())));
                    }
                }
            }
            if (state == OpenState::Open && self.subtotals)
                || (state == OpenState::Closed && header)
            {
                let outline = Outline::subtotal(up.clone(), field, value).with_path(path.clone());
                out.push((TreeTable::subtotal(g.key), outline));
            }
            up.pop();
            path.pop();
            if !header {
                let span = (out.len() - at - 1) as u64;
                out[at].1 = out[at].1.clone().with_span(span);
            }
        }
    }

    /// Every row as if every group were open, with its ancestors and, for a
    /// Group Item, its label's column and text.
    fn every(&self) -> Vec<Row> {
        fn walk(levels: &[Key], groups: &[Group], up: &mut Vec<Key>, out: &mut Vec<Row>) {
            for g in groups {
                out.push((g.key, up.clone(), Some((levels[up.len()], g.value))));
                up.push(g.key);
                match &g.under {
                    Under::Groups(kids) => walk(levels, kids, up, out),
                    Under::Leaves(keys) => out.extend(keys.iter().map(|&k| (k, up.clone(), None))),
                }
                up.pop();
            }
        }
        let mut out = Vec::new();
        walk(&self.levels, &self.tree, &mut Vec::new(), &mut out);
        out
    }

    /// The cells find visits, in its order, and whether each matches `text`.
    fn find_order(&self, text: &str) -> Vec<(FindMatch, bool, Vec<Key>)> {
        let text = text.to_lowercase();
        let mut cells = Vec::new();
        for (row, up, label) in self.every() {
            match label {
                Some((field, value)) => {
                    let hit = value.to_lowercase().contains(&text);
                    cells.push((FindMatch::new(row, field), hit, up));
                }
                None => {
                    for c in 0..self.cols {
                        let hit = format!("r{}c{c}", row.0).contains(&text);
                        cells.push((FindMatch::new(row, Key(c)), hit, up.clone()));
                    }
                }
            }
        }
        cells
    }

    /// How many cells match `query`, and how many of those lie in groups
    /// the grouping in force closes.
    pub fn find_count(&self, query: &FindQuery) -> FindCount {
        let shown: Vec<Key> = self.rows();
        let hits: Vec<_> = (self.find_order(&query.query.text).into_iter())
            .filter(|(_, hit, _)| *hit)
            .collect();
        FindCount {
            total: hits.len() as u64,
            partial: false,
            hidden: hits
                .iter()
                .filter(|(m, ..)| !shown.contains(&m.row))
                .count() as u64,
        }
    }

    /// The group whose Leaves include `key`, as its run of Leaves.
    fn leaves_of(&mut self, key: Key) -> Option<&mut Vec<Key>> {
        fn find(groups: &mut [Group], key: Key) -> Option<&mut Vec<Key>> {
            groups.iter_mut().find_map(|g| match &mut g.under {
                Under::Groups(kids) => find(kids, key),
                Under::Leaves(keys) => keys.contains(&key).then_some(keys),
            })
        }
        find(&mut self.tree, key)
    }

    fn apply(&mut self, spec: &ViewSpec) -> Viewed {
        let wanted = spec.grouping.clone().filter(|g| {
            (g.col_levels.is_empty() || g.col_levels == self.col_levels)
                && (g.row_levels.iter())
                    .map(|l| l.field_key())
                    .collect::<Vec<_>>()
                    == self.levels.iter().copied().map(Some).collect::<Vec<_>>()
        });
        let before = self.rows();
        let cols_before = self.columns();
        let shape = |g: &Option<Grouping>| g.as_ref().map(|g| (g.row_levels.clone(), g.rows.form));
        let regrouped = shape(&wanted) != shape(&self.grouping);
        self.shown = match &wanted {
            Some(g) => self.lay_out(&g.rows.expand, g.rows.form),
            None => Vec::new(),
        };
        self.grouping = wanted.clone();
        let after = self.rows();
        // A regrouping sends new outlines, so the Window is fetched again
        // even where the Keys stay.
        let mut notices = if regrouped {
            vec![(Axis::Rows, ChangeNotice::Refiltered(after.len() as u64))]
        } else if before == after {
            Vec::new()
        } else {
            vec![(Axis::Rows, change(&before, &after))]
        };
        let cols_after = self.columns();
        if cols_after != cols_before {
            let n = cols_after.len() as u64;
            notices.push((Axis::Cols, ChangeNotice::Refiltered(n)));
        }
        Viewed {
            applied: ViewSpec::default().with_grouping(wanted),
            totals: Default::default(),
            notices,
        }
    }
}

/// The Change Notice taking `before` to `after`: one run inserted or
/// removed, else `Refiltered`.
fn change(before: &[Key], after: &[Key]) -> ChangeNotice {
    let head = (before.iter().zip(after))
        .take_while(|(a, b)| a == b)
        .count();
    let tail = (before[head..].iter().rev())
        .zip(after[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old, new) = (before.len() - head - tail, after.len() - head - tail);
    let at = head as u64;
    match (old, new) {
        (0, n) => ChangeNotice::Expanded { at, n: n as u64 },
        (n, 0) => ChangeNotice::Collapsed { at, n: n as u64 },
        _ => ChangeNotice::Refiltered(after.len() as u64),
    }
}

impl ScenarioSource for TreeTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        let keys = match axis {
            Axis::Rows => self.rows(),
            Axis::Cols => self.columns(),
        };
        let end = (range.end as usize).min(keys.len());
        keys.get(range.start as usize..end)
            .map_or(Vec::new(), <[Key]>::to_vec)
    }

    fn outlines(&mut self, axis: Axis, keys: &[Key]) -> Option<Vec<Outline>> {
        self.grouping.as_ref()?;
        let shown = match axis {
            Axis::Rows => &self.shown,
            Axis::Cols if self.cols_grouped() => &self.col_tree,
            Axis::Cols => return None,
        };
        Some(
            keys.iter()
                .filter_map(|k| shown.iter().find(|(g, _)| g == k))
                .map(|(_, o)| o.clone())
                .collect(),
        )
    }
}

impl PumpSource for TreeTable {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.apply(spec)
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        let cells = self.find_order(&query.query.text);
        let n = cells.len();
        let at = from.and_then(|f| cells.iter().position(|(m, ..)| *m == f));
        let order: Vec<usize> = match (direction, at) {
            (FindDirection::Next, Some(i)) => (i + 1..n).chain(0..=i).collect(),
            (FindDirection::Next, None) => (0..n).collect(),
            (FindDirection::Previous, Some(i)) => (0..i).rev().chain((i..n).rev()).collect(),
            (FindDirection::Previous, None) => (0..n).rev().collect(),
        };
        let i = order.into_iter().find(|&i| cells[i].1)?;
        let index = cells[..i].iter().filter(|(_, hit, _)| *hit).count() as u64;
        let wrapped = at.is_some_and(|a| match direction {
            FindDirection::Next => i <= a,
            FindDirection::Previous => i >= a,
        });
        Some(FindStep {
            at: cells[i].0,
            index: Some(index),
            wrapped,
        })
    }

    fn ancestors(&mut self, axis: Axis, key: Key) -> Vec<Key> {
        if axis == Axis::Cols || self.grouping.is_none() {
            return Vec::new();
        }
        (self.every().into_iter())
            .find(|(k, ..)| *k == key)
            .map_or(Vec::new(), |(_, up, _)| up)
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        Some(axis == Axis::Rows && self.grouping.is_some())
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        let refused = || Err("Can't move between groups".to_string());
        let (Some(&first), Axis::Rows) = (keys.first(), axis) else {
            return refused();
        };
        let shown: Vec<Key> = self.rows();
        let Some(run) = self.leaves_of(first) else {
            return Err("Group rows and totals can't be moved".into());
        };
        if !keys.iter().all(|k| run.contains(k)) {
            return refused();
        }
        // The run's place among the rows shown, and where `to` falls in it.
        let start = shown.iter().position(|k| *k == run[0]).unwrap_or(0) as u64;
        let index = match to.checked_sub(start) {
            Some(i) if i <= run.len() as u64 => i as usize,
            _ => return refused(),
        };
        let before = run.get(index).copied();
        run.retain(|k| !keys.contains(k));
        let at = before.and_then(|b| run.iter().position(|k| *k == b));
        let at = at.unwrap_or(run.len());
        run.splice(at..at, keys.iter().copied());
        let (expand, form) = match &self.grouping {
            Some(g) => (g.rows.expand.clone(), g.rows.form),
            None => (Expand::default(), Form::Outline),
        };
        self.shown = self.lay_out(&expand, form);
        Ok(Moved {
            notices: vec![(Axis::Rows, ChangeNotice::Reordered)],
            view: None,
        })
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        Support::default().with_group(axis == Axis::Rows && self.levels.contains(&key))
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        let keys = match axis {
            Axis::Rows => self.rows(),
            Axis::Cols => self.columns(),
        };
        match keys.iter().position(|&k| k == key) {
            Some(p) => Located::Found(p as u64),
            None => Located::Gone { suggest: None },
        }
    }
}
