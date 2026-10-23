//! Group Items' disclosure marks: when each went pending, so a toggle the
//! Table Source answers fast never flickers, and where a click toggles one.

use std::collections::HashMap;
use std::time::Duration;

use binnacle_core::{Axis, ItemKind, Key, OpenState, Snapshot};

/// How long a toggled group's mark waits before it draws pending.
pub const MARK_DELAY: Duration = Duration::from_millis(150);

/// When each Group Item's disclosure mark went pending, by Key. A host
/// calls [`PendingMarks::seen`] each frame, with the time it passes
/// [`render_with`](crate::render_with) in its
/// [`RenderContext`](crate::RenderContext), and hands it over with
/// [`RenderContext::marks`](crate::RenderContext::marks). Without it no
/// mark draws pending.
#[derive(Clone, Debug, Default)]
pub struct PendingMarks {
    since: HashMap<Key, Duration>,
}

impl PendingMarks {
    /// Brings the marks up to date with `snapshot` at `now`.
    pub fn seen(&mut self, snapshot: &Snapshot, now: Duration) {
        let spans = snapshot.spans(Axis::Rows).iter().map(|s| s.key);
        let mut keys: Vec<Key> = (snapshot.drawn(Axis::Rows))
            .filter_map(|i| i.key)
            .chain(spans)
            .filter(|&k| snapshot.toggle_pending(Axis::Rows, k))
            .collect();
        keys.dedup();
        self.since.retain(|k, _| keys.contains(k));
        for key in keys {
            self.since.entry(key).or_insert(now);
        }
    }

    /// Whether the mark of the Group Item `key` draws pending at `now`: its
    /// toggle has waited at least [`MARK_DELAY`].
    pub fn pending(&self, key: Key, now: Duration) -> bool {
        (self.since.get(&key)).is_some_and(|&t| now.saturating_sub(t) >= MARK_DELAY)
    }
}

/// The Group Item whose disclosure mark is at (`x`, `y`) in the Grid's
/// area, as [`render_with`](crate::render_with) draws it in the row band,
/// or in header form on a span's first line shown: a click there toggles
/// its group with [`Grid::toggle`](binnacle_core::Grid::toggle). `None`
/// elsewhere, and on a Group Item that can't open, which has no mark.
pub fn disclosure_at(snapshot: &Snapshot, x: u16, y: u16) -> Option<Key> {
    let (x, y) = (f32::from(x), f32::from(y));
    if !snapshot.tiers(Axis::Rows).is_empty() {
        let on = |s: &&binnacle_core::HeaderSpan| {
            s.visible.y.ceil() == y && s.rect.x + 1.0 <= x && x < s.rect.x + 2.0
        };
        return snapshot.spans(Axis::Rows).iter().find(on).map(|s| s.key);
    }
    if x >= snapshot.header(Axis::Cols) || y < snapshot.header(Axis::Rows) {
        return None;
    }
    let row = snapshot
        .drawn(Axis::Rows)
        .find(|r| r.offset <= y && y < r.end())?;
    let outline = snapshot.outline(Axis::Rows, row.position)?;
    let openable = outline.kind == ItemKind::Group && outline.open != OpenState::CantOpen;
    let at = 2.0 * outline.depth as f32;
    (openable && at <= x && x < at + 2.0).then_some(row.key?)
}
