//! Opening and closing groups: the View Edits that change an axis's
//! `expand`, the pending disclosure marks they leave until the Table Source
//! answers, and the tree keys of the outline column.

use crate::effect::FocusStep;
use crate::focus::FocusEnd;
use crate::grid::Grid;
use crate::grouping::{
    Depth, EXCEPT_CAP, ExceptCapped, Expand, Form, Grouping, ItemKind, OpenState, Outline,
};
use crate::types::{Axis, Key, Position};
use crate::view::{ViewEdit, ViewSpec};

/// The grouping of `axis` in `spec`, if it has levels.
fn grouped(spec: &ViewSpec, axis: Axis) -> Option<&Grouping> {
    (spec.grouping.as_ref()).filter(|g| !g.levels(axis).is_empty())
}

impl Grid {
    /// The outline of the item with `key` on `axis`, if the Window holds it.
    fn outline_of(&self, axis: Axis, key: Key) -> Option<&Outline> {
        self.axis(axis).outlines.get(&key)
    }

    /// The level of the Group Item `key` on `axis`, if the Grid holds its
    /// outline and it can open, or (in header form, where a group is a span
    /// rather than an item) an item of the Window lies under it.
    fn openable(&self, axis: Axis, key: Key) -> Option<u32> {
        if let Some(outline) = self.outline_of(axis, key) {
            return (outline.kind == ItemKind::Group && outline.open != OpenState::CantOpen)
                .then_some(outline.depth);
        }
        (self.axis(axis).outlines.values())
            .find_map(|o| o.ancestors.iter().position(|&a| a == key))
            .map(|level| level as u32)
    }

    /// The groups of `axis` the Window shows under the group `key`, at any
    /// depth, with the Position of an item of each and its level: Group
    /// Items, and the groups whose spans hold items in header form.
    fn known_under(&self, axis: Axis, key: Key) -> Vec<(Option<Position>, u32, Key)> {
        let st = self.axis(axis);
        let mut under: Vec<(Option<Position>, u32, Key)> = Vec::new();
        for (&k, o) in &st.outlines {
            if !o.ancestors.contains(&key) {
                continue;
            }
            let at = st.measure.position_of(k);
            if o.kind == ItemKind::Group && o.open != OpenState::CantOpen {
                under.push((at, o.depth, k));
            }
            let level = o.ancestors.iter().position(|&a| a == key).unwrap_or(0);
            for (l, &a) in o.ancestors.iter().enumerate().skip(level + 1) {
                under.push((at, l as u32, a));
            }
        }
        under.sort();
        under.dedup_by_key(|(_, _, k)| *k);
        under
    }

    /// Whether the latest request opens the group `key`, at `level`.
    fn wanted_open(&self, axis: Axis, level: u32, key: Key) -> bool {
        grouped(self.latest_view(), axis).is_some_and(|g| g.axis(axis).expand.opens(level, key))
    }

    /// Requests `axis` opened as `expand` says, by `edit`; returns whether
    /// it asked. Refused with nothing asked if its exceptions would pass
    /// the cap.
    pub(crate) fn request_expand(
        &mut self,
        axis: Axis,
        expand: Expand,
        edit: ViewEdit,
    ) -> Result<bool, ExceptCapped> {
        let mut spec = self.latest_view().clone();
        let Some(grouping) = spec.grouping.as_mut() else {
            return Ok(false);
        };
        let current = &mut grouping.axis_mut(axis).expand;
        if *current == expand {
            return Ok(false);
        }
        if expand.except.len() > EXCEPT_CAP && expand.except.len() > current.except.len() {
            return Err(ExceptCapped { cap: EXCEPT_CAP });
        }
        *current = expand;
        self.request(spec, edit);
        Ok(true)
    }

    /// Opens or closes the group of `axis` headed by the Group Item `key`.
    fn set_open(&mut self, axis: Axis, key: Key, open: bool) -> Result<(), ExceptCapped> {
        self.expanding_all = None;
        let Some(level) = self.openable(axis, key) else {
            return Ok(());
        };
        let Some(g) = grouped(self.latest_view(), axis) else {
            return Ok(());
        };
        let expand = g.axis(axis).expand.clone().with(level, key, open);
        let edit = match open {
            true => ViewEdit::Expand { axis, key },
            false => ViewEdit::Collapse { axis, key },
        };
        self.request_expand(axis, expand, edit).map(|_| ())
    }

    /// Opens the group of `axis` headed by the Group Item `key` and requests
    /// the result ([`ViewEdit::Expand`]): its Key joins or leaves the axis's
    /// `expand.except`. Nothing happens unless the Window holds the Group
    /// Item and it can open, or if the latest request opens it already. The
    /// error carries the cap when the exceptions would pass it.
    pub fn expand(&mut self, axis: Axis, key: Key) -> Result<(), ExceptCapped> {
        self.set_open(axis, key, true)
    }

    /// Closes the group of `axis` headed by the Group Item `key`, as
    /// [`Grid::expand`] opens one ([`ViewEdit::Collapse`]).
    pub fn collapse(&mut self, axis: Axis, key: Key) -> Result<(), ExceptCapped> {
        self.set_open(axis, key, false)
    }

    /// Opens the group headed by `key` if the latest request closes it, and
    /// closes it otherwise: a second toggle before the Table Source answers
    /// takes back the first.
    pub fn toggle(&mut self, axis: Axis, key: Key) -> Result<(), ExceptCapped> {
        let Some(level) = self.openable(axis, key) else {
            return Ok(());
        };
        let open = self.wanted_open(axis, level, key);
        self.set_open(axis, key, !open)
    }

    /// Opens the group headed by `key` and every group under it whose Group
    /// Item the Window holds ([`ViewEdit::ExpandAll`]). Closed groups under
    /// it that later Windows bring are opened the same way, until another
    /// toggle: the Grid knows only the groups in its Windows.
    pub fn expand_all(&mut self, axis: Axis, key: Key) -> Result<(), ExceptCapped> {
        let Some(level) = self.openable(axis, key) else {
            return Ok(());
        };
        self.expanding_all = Some((axis, key));
        let Some(g) = grouped(self.latest_view(), axis) else {
            return Ok(());
        };
        let mut expand = g.axis(axis).expand.clone().with(level, key, true);
        for (_, depth, k) in self.known_under(axis, key) {
            expand = expand.with(depth, k, true);
        }
        (self.request_expand(axis, expand, ViewEdit::ExpandAll { axis, key })).map(|_| ())
    }

    /// Opens every group of `axis` to `depth` levels and closes those below,
    /// clearing its exceptions ([`ViewEdit::ExpandTo`]): `Levels(0)` shows
    /// only the top groups.
    pub fn expand_to(&mut self, axis: Axis, depth: Depth) {
        self.expanding_all = None;
        let _ = self.request_expand(axis, Expand::to(depth), ViewEdit::ExpandTo { axis, depth });
    }

    /// A Window brought outlines to `axis`: closed groups under the one `*`
    /// opens are opened too, once the requests before have their answers.
    pub(crate) fn keep_expanding(&mut self, axis: Axis) {
        let Some((a, key)) = self.expanding_all else {
            return;
        };
        if a != axis || self.view.pending() {
            return;
        }
        let closed = (self.known_under(axis, key).into_iter())
            .any(|(_, level, k)| !self.applied_opens(axis, level, k));
        if closed && self.expand_all(axis, key).is_err() {
            self.expanding_all = None;
        }
    }

    /// Whether the disclosure mark of the Group Item `key` on `axis` is
    /// pending: a request still due opens or closes it, unlike the spec the
    /// Table Source applies.
    pub(crate) fn toggle_pending(&self, axis: Axis, key: Key) -> bool {
        if !self.view.pending() {
            return false;
        }
        let Some(level) = self.openable(axis, key) else {
            return false;
        };
        let applied = grouped(&self.view.applied, axis)
            .is_some_and(|g| g.axis(axis).expand.opens(level, key));
        applied != self.wanted_open(axis, level, key)
    }

    /// How many levels `axis` has in header form; 0 in outline form or
    /// while it is not grouped.
    pub(crate) fn header_levels(&self, axis: Axis) -> u32 {
        (grouped(&self.view.applied, axis))
            .filter(|g| g.axis(axis).form == Form::Header)
            .map_or(0, |g| g.levels(axis).len() as u32)
    }

    /// Whether focus is in the row Header Band, on the focused row's band
    /// cell rather than a body cell: the outline column, or a level column
    /// in header form.
    pub(crate) fn in_outline(&self) -> bool {
        self.outline_focus
            && self.focus.is_some()
            && grouped(&self.view.applied, Axis::Rows).is_some()
    }

    /// The tier focus is in, kept inside the row levels in header form.
    fn tier(&self) -> u32 {
        self.band_tier
            .min(self.header_levels(Axis::Rows).saturating_sub(1))
    }

    /// Focus leaves the outline column for the body, on the column it was
    /// on before.
    pub(crate) fn leave_outline(&mut self) {
        if std::mem::take(&mut self.outline_focus) {
            self.pending_damage.selection = true;
        }
    }

    /// A click on a row header at `x`: on grouped rows focus goes to the
    /// band, in header form to the level column clicked (the last one past
    /// them).
    pub(crate) fn row_header_clicked(&mut self, x: f32) {
        if grouped(&self.view.applied, Axis::Rows).is_none() || self.focus.is_none() {
            return;
        }
        self.outline_focus = true;
        let tiers = &self.tiers[Axis::Rows.index()];
        self.band_tier =
            (tiers.iter().position(|t| x < t.end)).unwrap_or(tiers.len().saturating_sub(1)) as u32;
        self.pending_damage.selection = true;
    }

    /// The focused row's Position and outline, while focus is in the outline
    /// column.
    fn focused_outline(&self) -> Option<(Position, Key, Outline)> {
        let end = self.focus?[Axis::Rows.index()];
        let key = end.key?;
        Some((end.position, key, self.outline_of(Axis::Rows, key)?.clone()))
    }

    /// A focus move along the columns, which in the outline column walks
    /// the tree: Right opens a closed group or steps in to its first child,
    /// else into the body; Left closes an open group or steps out to the
    /// parent. Moving left from the first column enters the outline column.
    /// Returns whether it handled the move.
    pub(crate) fn tree_move(&mut self, by: FocusStep, forward: bool) -> bool {
        if !self.in_outline() {
            return self.enter_outline(by, forward);
        }
        if by != FocusStep::Item {
            if forward {
                self.leave_outline();
            }
            return !forward;
        }
        let Some((position, key, outline)) = self.focused_outline() else {
            return true;
        };
        if self.header_levels(Axis::Rows) > 0 {
            self.header_tree_move(&outline, forward);
            return true;
        }
        let group = outline.kind == ItemKind::Group;
        match (forward, outline.open) {
            (true, OpenState::Closed) if group => {
                let _ = self.expand(Axis::Rows, key);
            }
            (true, OpenState::Open) if group && !self.toggle_pending(Axis::Rows, key) => {
                self.step_rows(position + 1);
            }
            (true, OpenState::Open) if group => {}
            (true, _) => self.leave_outline(),
            (false, OpenState::Open) if group => {
                let _ = self.collapse(Axis::Rows, key);
            }
            (false, _) => {
                if let Some(parent) = outline.parent() {
                    let col = self.focus.and_then(|f| f[Axis::Cols.index()].key);
                    if let Some(col) = col {
                        self.place_focus(parent, col, true);
                    }
                }
            }
        }
        true
    }

    /// The group the focused tier stands for on a row with `outline`, in
    /// header form: its ancestor at that level (an open group's own
    /// Subtotal included), or a Group Item that is its own header there.
    fn tier_group(&self, outline: &Outline, key: Key) -> Option<Key> {
        let tier = self.tier() as usize;
        match outline.ancestors.get(tier) {
            Some(&group) => Some(group),
            None if outline.kind == ItemKind::Group && outline.depth as usize == tier => Some(key),
            None => None,
        }
    }

    /// The tree keys in header form, where each level is a column of the
    /// band: Right opens the focused tier's closed group or steps to the
    /// next level's column (from the last, into the body); Left closes an
    /// open group or steps to the previous level's column.
    fn header_tree_move(&mut self, outline: &Outline, forward: bool) {
        let (tier, levels) = (self.tier(), self.header_levels(Axis::Rows));
        let key = self.focus.and_then(|f| f[Axis::Rows.index()].key);
        let group = key.and_then(|k| self.tier_group(outline, k));
        let open = group
            .and_then(|g| Some((g, self.openable(Axis::Rows, g)?)))
            .map(|(g, level)| (g, self.applied_opens(Axis::Rows, level, g)));
        match (forward, open) {
            (true, Some((g, false))) => {
                let _ = self.expand(Axis::Rows, g);
            }
            (true, Some((g, true))) if self.toggle_pending(Axis::Rows, g) => {}
            (true, _) if tier + 1 < levels => self.step_tier(tier + 1),
            (true, _) => self.leave_outline(),
            (false, Some((g, true))) => {
                let _ = self.collapse(Axis::Rows, g);
            }
            (false, _) if tier > 0 => self.step_tier(tier - 1),
            (false, _) => {}
        }
    }

    /// Focus steps to the band's level column `tier`.
    fn step_tier(&mut self, tier: u32) {
        self.band_tier = tier;
        self.pending_damage.selection = true;
    }

    /// Moving left by one from the first column, on grouped rows, enters
    /// the band: the outline column, or the innermost level's column in
    /// header form. Returns whether it did.
    fn enter_outline(&mut self, by: FocusStep, forward: bool) -> bool {
        if forward || by != FocusStep::Item || grouped(&self.view.applied, Axis::Rows).is_none() {
            return false;
        }
        let Some(f) = self.focus else { return false };
        let st = self.axis(Axis::Cols);
        if st.drawn_index(st.slot_of(f[Axis::Cols.index()])) != 0 {
            return false;
        }
        self.outline_focus = true;
        self.band_tier = self.header_levels(Axis::Rows).saturating_sub(1);
        self.pending_damage.selection = true;
        true
    }

    /// Focus steps to the row at `position`, keeping its column.
    fn step_rows(&mut self, position: Position) {
        let Some(mut cell) = self.focus else { return };
        if position >= self.axis(Axis::Rows).measure.count() {
            return;
        }
        cell[Axis::Rows.index()] = FocusEnd::at(self.axis(Axis::Rows), position);
        self.set_focus_cell(cell);
        self.set_span(self.unit_span(cell));
        self.reveal = true;
        self.reveal_focus();
    }

    /// Enter in the outline column: toggles the focused group. Returns
    /// whether it handled the key.
    pub(crate) fn tree_activate(&mut self) -> bool {
        if !self.in_outline() {
            return false;
        }
        let Some((_, key, outline)) = self.focused_outline() else {
            return false;
        };
        let group = match self.header_levels(Axis::Rows) {
            0 => (outline.kind == ItemKind::Group).then_some(key),
            _ => self.tier_group(&outline, key),
        };
        if let Some(group) = group {
            let _ = self.toggle(Axis::Rows, group);
            return true;
        }
        false
    }

    /// Text typed in the outline column: `*` opens everything under the
    /// focused group (or the focused item's parent), a digit opens the
    /// rows to that many levels. Returns whether it handled the text.
    pub(crate) fn tree_typed(&mut self, text: &str) -> bool {
        if !self.in_outline() {
            return false;
        }
        match text {
            "*" => {
                if let Some((_, key, outline)) = self.focused_outline() {
                    let header = self.header_levels(Axis::Rows) > 0;
                    let group = match outline.kind {
                        _ if header => self.tier_group(&outline, key).or(outline.parent()),
                        ItemKind::Group => Some(key),
                        _ => outline.parent(),
                    };
                    if let Some(group) = group {
                        let _ = self.expand_all(Axis::Rows, group);
                    }
                }
                true
            }
            _ => match text.parse::<u32>() {
                Ok(n) if text.len() == 1 => {
                    self.expand_to(Axis::Rows, Depth::Levels(n));
                    true
                }
                _ => false,
            },
        }
    }
}
