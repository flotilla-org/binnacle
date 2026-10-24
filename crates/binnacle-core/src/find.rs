//! Find: the Find Query, stepping between its matches, and the current
//! match. The Table Source works out which cells match; the Grid holds only
//! the query and the match it last stepped to.

use crate::effect::Effect;
use crate::grid::Grid;
use crate::notice::placement;
use crate::sort::Located;
use crate::types::{Axis, Key};
use crate::view::{TextQuery, TextSupport, ViewEdit};

/// What find looks for: a Text Query, matched against each cell of the items
/// it searches, and the order it steps through the matches in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FindQuery {
    pub query: TextQuery,
    /// The axis stepped along first: [`Axis::Rows`] (the default) goes
    /// through each row's searched cells in column order, then the next row;
    /// [`Axis::Cols`] through each column's cells in row order.
    pub by: Axis,
}

impl FindQuery {
    /// Finds `query`, by rows.
    pub fn new(query: TextQuery) -> Self {
        FindQuery {
            query,
            by: Axis::Rows,
        }
    }

    pub fn by(mut self, by: Axis) -> Self {
        self.by = by;
        self
    }
}

/// A cell that matches the Find Query, named by its row and column Keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FindMatch {
    pub row: Key,
    pub col: Key,
}

impl FindMatch {
    pub fn new(row: Key, col: Key) -> Self {
        FindMatch { row, col }
    }

    pub fn key(&self, axis: Axis) -> Key {
        match axis {
            Axis::Rows => self.row,
            Axis::Cols => self.col,
        }
    }
}

/// Which way a find step goes, in the Find Query's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FindDirection {
    Next,
    Previous,
}

/// The Grid's find state.
#[derive(Clone, Debug, Default)]
pub(crate) struct FindState {
    pub query: Option<FindQuery>,
    /// The match the last step landed on, until its row or column leaves.
    pub current: Option<FindMatch>,
    pub support: Option<TextSupport>,
    /// Step answers still to come for the query in force.
    due: u32,
    /// Whether a step for the query in force has been answered.
    answered: bool,
    /// The step asked for last while one was unanswered, sent once it is,
    /// from where that one landed. Never more than one.
    held: Option<FindDirection>,
    /// Step answers still to come for an earlier query, ignored.
    stale: u32,
    /// Per axis, `Locate` answers still to come about the current match's Key.
    locating: [u32; 2],
    /// Whether the current match is a Group Item's label, drawn in the row
    /// Header Band rather than in a column.
    pub label: bool,
    /// The level of that label, whose tier holds it in header form.
    pub tier: u32,
    /// The match waiting for the groups it lies in to open before it takes
    /// focus.
    revealing: Option<FindMatch>,
}

impl Grid {
    /// Whether the Table Source finds, and how it matches: adapters offer
    /// find only when this is `Some`. `None` by default.
    pub fn set_find_support(&mut self, support: Option<TextSupport>) {
        self.find.support = support;
    }

    /// Sets the Find Query, or closes find with `None` (or a query with no
    /// text), emitting [`Effect::FindRequested`] so the host fetches the new
    /// marks. The current match stays until the first step for the new
    /// query lands (closing find drops it), and a step still unanswered is
    /// ignored when it lands. Setting the query in force does nothing.
    pub fn set_find(&mut self, query: Option<FindQuery>) {
        let query = query.filter(|q| !q.query.text.is_empty());
        if self.find.query == query {
            return;
        }
        self.find.query = query.clone();
        self.find.stale += std::mem::take(&mut self.find.due);
        self.find.held = None;
        self.find.answered = false;
        if query.is_none() {
            self.drop_match();
        }
        self.push(Effect::FindRequested { query });
    }

    /// A Find Next or Find Previous Action: asks the Table Source for the
    /// next match from the current one, or from the focused cell. While a
    /// step is unanswered one more waits, the latest asked for, so it starts
    /// where that lands; before the query's first answer, none does, so
    /// presses made while a slow source finds the first match move once.
    pub(crate) fn find_step(&mut self, direction: FindDirection) {
        let Some(query) = self.find.query.clone() else {
            return;
        };
        if self.find.due > 0 {
            if self.find.answered {
                self.find.held = Some(direction);
            }
            return;
        }
        let from = self
            .find
            .current
            .or_else(|| self.focused_keys().map(|(row, col)| FindMatch { row, col }));
        self.find.due += 1;
        self.push(Effect::FindStepRequested {
            query,
            from,
            direction,
        });
    }

    /// The Table Source's answer to a `FindStepRequested`: the match it
    /// stepped to, or `None` if nothing matches. Answer every request once,
    /// in order. The match becomes the current match and takes focus (found
    /// with `Locate` if outside the Window) and is scrolled into view; the
    /// selection is left as it is. An answer for a query since replaced, or
    /// with find closed, is ignored. On grouped rows, a match is under no
    /// group, as [`Grid::find_stepped_within`] with no ancestors takes it: a
    /// top level Group Item's label is focused in the outline column.
    pub fn find_stepped(&mut self, found: Option<FindMatch>) {
        match found {
            Some(found) => self.find_stepped_within(found, [&[], &[]]),
            None if self.find_answered() => self.step_held(),
            None => {}
        }
    }

    /// The answer to a `FindStepRequested` on a grouped table, as
    /// [`Grid::find_stepped`] takes it, with the Group Item Keys above the
    /// match's row and column (`ancestors`, rows first), outermost first as
    /// in an Outline, shown or not. A match in collapsed groups is counted
    /// all the same: each group the latest request leaves closed is opened,
    /// one [`ViewEdit::Expand`] each, so they stay open as ordinary
    /// exceptions, and the match takes focus once the Table Source has
    /// answered. A match in the column of the row's own level field is a
    /// Group Item's label, which focus takes in the outline column. In
    /// header form a match in the column of the field of a level the row
    /// lies at or under is the label of that level's group, drawn in its
    /// span on the row carrying it, which focus takes in that level's tier.
    pub fn find_stepped_within(&mut self, found: FindMatch, ancestors: [&[Key]; 2]) {
        if !self.find_answered() {
            return;
        }
        self.find.current = Some(found);
        let depth = ancestors[Axis::Rows.index()].len();
        let levels =
            (self.view.applied.grouping.as_ref()).map_or(&[][..], |g| g.levels(Axis::Rows));
        let level = levels.iter().position(|l| l.field_key() == Some(found.col));
        self.find.label = match (level, self.header_levels(Axis::Rows) > 0) {
            (Some(level), true) => level <= depth,
            (Some(level), false) => level == depth,
            (None, _) => false,
        };
        if let (true, Some(level)) = (self.find.label, level) {
            self.find.tier = level as u32;
        }
        let mut asked = false;
        for axis in Axis::ALL {
            for (level, &key) in ancestors[axis.index()].iter().enumerate() {
                let Some(g) = self.latest_view().grouping.as_ref() else {
                    break;
                };
                let expand = &g.axis(axis).expand;
                if expand.opens(level as u32, key) {
                    continue;
                }
                let expand = expand.clone().with(level as u32, key, true);
                let edit = ViewEdit::Expand { axis, key };
                asked |= self.request_expand(axis, expand, edit) == Ok(true);
            }
        }
        match asked {
            true => self.find.revealing = Some(found),
            false => self.land(found),
        }
        self.step_held();
    }

    /// Counts in a step's answer: whether it is for the query in force,
    /// which then has an answer and drops the current match.
    fn find_answered(&mut self) -> bool {
        if self.find.stale > 0 {
            self.find.stale -= 1;
            return false;
        }
        self.find.due = self.find.due.saturating_sub(1);
        if self.find.query.is_none() {
            return false;
        }
        self.find.answered = true;
        self.drop_match();
        true
    }

    /// The current match takes focus, found with `Locate` where it is
    /// outside the Window: in the outline column for a label.
    fn land(&mut self, found: FindMatch) {
        if !self.find.label {
            self.band_focus = None;
            self.place_focus(found.row, found.col, false);
        } else {
            let col = (self.focus.and_then(|f| f[Axis::Cols.index()].key)).or_else(|| {
                self.axis(Axis::Cols)
                    .key_at(self.axis(Axis::Cols).anchor.position)
            });
            if let Some(col) = col {
                self.place_focus(found.row, col, false);
                self.band_focus = Some(Axis::Rows);
                self.band_tier = self.find.tier;
            }
        }
        for axis in Axis::ALL {
            let key = found.key(axis);
            if self.axis(axis).measure.position_of(key).is_none() {
                self.find_locate(axis);
            }
        }
    }

    /// Sends the step asked for while one was unanswered.
    fn step_held(&mut self) {
        if let Some(direction) = self.find.held.take() {
            self.find_step(direction);
        }
    }

    /// Every View Spec request is answered: a match waiting for its groups
    /// to open takes focus, if it is still the current match.
    pub(crate) fn find_revealed(&mut self) {
        let Some(found) = self.find.revealing.take() else {
            return;
        };
        if self.find.current == Some(found) {
            self.land(found);
        }
    }

    fn drop_match(&mut self) {
        if self.find.current.take().is_some() {
            self.pending_damage.selection = true;
        }
        self.find.locating = [0; 2];
        self.find.label = false;
        self.find.revealing = None;
    }

    /// Asks where the current match's Key on `axis` is, to drop the match if
    /// it has left.
    pub(crate) fn find_locate(&mut self, axis: Axis) {
        let Some(found) = self.find.current else {
            return;
        };
        // A label's column is the level field, which no column shows.
        if axis == Axis::Cols && self.find.label {
            return;
        }
        let due = self.find.locating[axis.index()];
        self.find.locating[axis.index()] = self.request_locate(axis, found.key(axis), due);
    }

    /// A `located` answer: the current match is dropped once the last answer
    /// due about its Key on `axis` says it has left.
    pub(crate) fn find_located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        let Some(found) = self.find.current else {
            return;
        };
        if found.key(axis) != key || self.find.locating[i] == 0 {
            return;
        }
        self.find.locating[i] -= 1;
        let count = self.axis(axis).measure.count();
        if self.find.locating[i] == 0 && placement(answer, count).is_err() {
            self.drop_match();
        }
    }
}
