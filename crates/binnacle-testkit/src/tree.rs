//! A grouped source over a hand-made tree, opening and closing its groups
//! as the View Spec's `expand` says.

use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, Depth, Expand, Grouping, Key, Located, OpenState, Outline, Position,
    Support, Value, ViewSpec,
};
use binnacle_memory::{PumpSource, Viewed};

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
/// first) into a hand-made tree of [`Group`]s, with no Subtotals. Its groups
/// open as the grouping's `expand` says; it answers a change of `expand`
/// that opens or closes one run of items with `Expanded` or `Collapsed`,
/// and any other change with `Refiltered`. Columns are keyed by Position;
/// the level fields stay among them.
pub struct TreeTable {
    plain: Vec<Key>,
    levels: Vec<Key>,
    tree: Vec<Group>,
    cols: u64,
    grouping: Option<Grouping>,
    shown: Vec<(Key, Outline)>,
}

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
            grouping: None,
            shown: Vec::new(),
        }
    }

    /// The rows as the table shows them now, with their outlines.
    pub fn shown(&self) -> &[(Key, Outline)] {
        &self.shown
    }

    fn rows(&self) -> Vec<Key> {
        match self.grouping {
            Some(_) => self.shown.iter().map(|(k, _)| *k).collect(),
            None => self.plain.clone(),
        }
    }

    fn lay_out(&self, expand: &Expand) -> Vec<(Key, Outline)> {
        let mut out = Vec::new();
        self.walk(&self.tree, &mut Vec::new(), expand, &mut out);
        out
    }

    fn walk(
        &self,
        groups: &[Group],
        up: &mut Vec<Key>,
        expand: &Expand,
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
            let value = Value::Text(g.value.into());
            let outline = Outline::group(up.clone(), state, self.levels[level], value);
            out.push((g.key, outline));
            if state != OpenState::Open {
                continue;
            }
            up.push(g.key);
            match &g.under {
                Under::Groups(kids) => self.walk(kids, up, expand, out),
                Under::Leaves(keys) => {
                    out.extend(keys.iter().map(|&k| (k, Outline::leaf(up.clone()))));
                }
            }
            up.pop();
        }
    }

    fn apply(&mut self, spec: &ViewSpec) -> Viewed {
        let wanted = spec.grouping.clone().filter(|g| {
            g.col_levels.is_empty()
                && (g.row_levels.iter())
                    .map(|l| l.field_key())
                    .collect::<Vec<_>>()
                    == self.levels.iter().copied().map(Some).collect::<Vec<_>>()
        });
        let before = self.rows();
        let regrouped = (wanted.as_ref().map(|g| &g.row_levels))
            != (self.grouping.as_ref().map(|g| &g.row_levels));
        self.shown = match &wanted {
            Some(g) => self.lay_out(&g.rows.expand),
            None => Vec::new(),
        };
        self.grouping = wanted.clone();
        let after = self.rows();
        let notices = if before == after {
            Vec::new()
        } else if regrouped {
            vec![(Axis::Rows, ChangeNotice::Refiltered(after.len() as u64))]
        } else {
            vec![(Axis::Rows, change(&before, &after))]
        };
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
            Axis::Cols => (0..self.cols).map(Key).collect(),
        };
        let end = (range.end as usize).min(keys.len());
        keys.get(range.start as usize..end)
            .map_or(Vec::new(), <[Key]>::to_vec)
    }

    fn outlines(&mut self, axis: Axis, keys: &[Key]) -> Option<Vec<Outline>> {
        self.grouping.as_ref()?;
        (axis == Axis::Rows).then(|| {
            keys.iter()
                .filter_map(|k| self.shown.iter().find(|(g, _)| g == k))
                .map(|(_, o)| o.clone())
                .collect()
        })
    }
}

impl PumpSource for TreeTable {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.apply(spec)
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        Support::default().with_group(axis == Axis::Rows && self.levels.contains(&key))
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        let keys = match axis {
            Axis::Rows => self.rows(),
            Axis::Cols => (0..self.cols).map(Key).collect(),
        };
        match keys.iter().position(|&k| k == key) {
            Some(p) => Located::Found(p as u64),
            None => Located::Gone { suggest: None },
        }
    }
}
