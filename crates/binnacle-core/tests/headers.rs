//! Seam 1 scenario for header placements: each visible header with its Key,
//! rect, sort state, selected and hovered flags, so a host draws headers
//! without tracking any state itself.

use std::fmt::Write;

use binnacle_core::{
    Action, Axis, AxisConfig, Key, Modifiers, PointerEvent, Snapshot, SortColumn, SortDirection,
    SortSpec, Viewport,
};
use binnacle_testkit::Scenario;

const V: Viewport = Viewport {
    width: 26.0,
    height: 5.0,
};

/// One line per header: `axis position#key at x,y wxh`, then its sort
/// (`^`/`v` and place), `pending`, `selected` and `hovered` when set.
fn headers(snap: &Snapshot) -> String {
    let mut out = String::new();
    for axis in Axis::ALL {
        for h in snap.headers(axis) {
            let key = h.key.map_or("?".into(), |k| k.0.to_string());
            let r = h.rect;
            let _ = write!(
                out,
                "{:?} {}#{key} at {},{} {}x{}",
                h.axis, h.position, r.x, r.y, r.width, r.height
            );
            if let Some((direction, place)) = h.sort.applied {
                let arrow = match direction {
                    SortDirection::Ascending => '^',
                    SortDirection::Descending => 'v',
                };
                let _ = write!(out, " sort {arrow}{place}");
            }
            for (on, flag) in [
                (h.sort.pending, "pending"),
                (h.selected, "selected"),
                (h.hovered, "hovered"),
            ] {
                if on {
                    let _ = write!(out, " {flag}");
                }
            }
            out.push('\n');
        }
    }
    out
}

#[test]
fn header_placements_carry_their_rect_sort_selection_and_hover() {
    // 20 rows of 1 under a 1-high header band; 5 columns of 6 with a Gap of
    // 1 after a 4-wide row header band.
    let mut s = Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    );
    s.frame(V);
    s.input(|g| {
        g.sort_applied(SortSpec::new(vec![
            SortColumn::ascending(Key(1)),
            SortColumn::descending(Key(3)),
        ]));
        // Focus row 1, column 2 and select that whole column.
        g.pointer(PointerEvent::down(20.0, 2.5));
        g.pointer(PointerEvent::up(20.0, 2.5));
        g.action(Action::SelectColumn);
        // Ctrl-click row 3's header: it is picked, so wholly selected.
        let ctrl = Modifiers::CTRL;
        g.pointer(PointerEvent::down(1.0, 4.5).modifiers(ctrl));
        g.pointer(PointerEvent::up(1.0, 4.5).modifiers(ctrl));
        g.pointer(PointerEvent::moved(7.0, 0.5));
    });
    insta::assert_snapshot!("headers_applied", headers(&s.frame(V)));

    // A click on column 0's header asks for a sort by it alone; until the
    // Table Source answers, every column whose part would change is pending.
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::up(7.0, 0.5));
    });
    let requested = std::iter::from_fn(|| s.grid.poll_effect()).count();
    assert!(requested > 0);
    s.grid.begin_frame(V);
    insta::assert_snapshot!("headers_pending", headers(&s.grid.snapshot()));
}
