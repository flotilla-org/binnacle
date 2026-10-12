use std::fmt::Write;

use binnacle_core::{Axis, Cursor, FocusEnd, Mode, Snapshot, SortDirection, SortSpec, StripItem};

/// Largest canvas drawn, in characters; larger viewports are clipped.
const MAX_CANVAS: (usize, usize) = (160, 60);

/// Renders a Snapshot as text: a summary line, each Strip as
/// `position#key@offset+size` after its Scroll Anchor (`position+offset`) and
/// thumb (`fraction/size`) (key `?` while unfilled), then a character
/// picture treating one unit as one character, each item starting at the first
/// whole character it reaches. Column headers show the column
/// Key, row headers the row Position, cells `rowkey.colkey`. A placed focus
/// adds `focus row,col` (each `position#key`) and its rect to the summary, and
/// marks the focused cell `*rowkey.colkey`; `locating axis` while its new
/// Position is awaited. A range adds `range rows x cols`
/// (half-open Positions), picks add `picks` (row Keys), and either adds the
/// fill rects as `fill x,y wxh`; selected cells are marked `+`. A gesture adds
/// its Mode and `capturing`, a hovered header `hover axis position`, a resize
/// cursor hint `cursor C` and a resize guide `guide axis offset (key K size
/// S)`. A non-empty applied Sort Spec adds `sort` and an unanswered request
/// `requested`, each as `key^` or `keyv` per column.
pub fn render(snap: &Snapshot) -> String {
    let mut out = String::new();
    let d = snap.damage();
    let mut damage: Vec<&str> = Vec::new();
    for (on, name) in [
        (d.layout, "layout"),
        (d.selection, "selection"),
        (d.positions, "positions"),
    ] {
        if on {
            damage.push(name);
        }
    }
    let damage = if damage.is_empty() {
        "none".to_string()
    } else {
        damage.join(" ")
    };
    let _ = write!(out, "gen {} | damage {damage}", snap.generation());
    if let (Some(row), Some(col)) = (snap.focus(Axis::Rows), snap.focus(Axis::Cols)) {
        let end = |e: FocusEnd| {
            format!(
                "{}#{}",
                e.position,
                e.key.map_or("?".into(), |k| k.0.to_string())
            )
        };
        let _ = write!(out, " | focus {},{}", end(row), end(col));
        match snap.focus_rect() {
            Some(r) => {
                let _ = write!(out, " at {},{} {}x{}", r.x, r.y, r.width, r.height);
            }
            None => out.push_str(" not drawn"),
        }
        for axis in Axis::ALL {
            if snap.locating(axis) {
                let _ = write!(out, " | locating {axis:?}");
            }
        }
    }
    if let Some(r) = snap.selection_range() {
        let _ = write!(out, " | range {:?} x {:?}", r.rows, r.cols);
    }
    if !snap.picked().is_empty() {
        let keys: Vec<String> = snap.picked().iter().map(|k| k.0.to_string()).collect();
        let _ = write!(out, " | picks {}", keys.join(","));
    }
    for f in snap.selection_rects() {
        let _ = write!(out, " | fill {},{} {}x{}", f.x, f.y, f.width, f.height);
    }
    if !snap.applied_sort().is_empty() {
        let _ = write!(out, " | sort {}", spec_text(snap.applied_sort()));
    }
    if let Some(spec) = snap.requested_sort() {
        let _ = write!(out, " | requested {}", spec_text(spec));
    }
    if snap.mode() != Mode::Idle {
        let _ = write!(out, " | mode {:?}", snap.mode());
    }
    if snap.capturing() {
        out.push_str(" | capturing");
    }
    if let Some((axis, item)) = snap.hovered_header() {
        let _ = write!(out, " | hover {axis:?} {}", item.position);
    }
    if snap.cursor() != Cursor::Default {
        let _ = write!(out, " | cursor {:?}", snap.cursor());
    }
    if let Some(g) = snap.resize_guide() {
        let _ = write!(
            out,
            " | guide {:?} {} (key {} size {})",
            g.axis, g.offset, g.key.0, g.size
        );
    }
    out.push('\n');
    for axis in Axis::ALL {
        let _ = write!(
            out,
            "{:?}: count {}, extent {}, header {}, anchor {}+{}, thumb {:.4}/{:.4} |",
            axis,
            snap.count(axis),
            snap.extent(axis),
            snap.header(axis),
            snap.scroll_anchor(axis).position,
            snap.scroll_anchor(axis).offset,
            snap.thumb(axis).fraction,
            snap.thumb(axis).size,
        );
        for item in snap.strip(axis) {
            let _ = write!(
                out,
                " {}#{}@{}+{}",
                item.position,
                key_text(item),
                item.offset,
                item.size
            );
        }
        out.push('\n');
    }
    out.push_str(&picture(snap));
    out
}

/// A Sort Spec as `key^` (ascending) or `keyv` (descending) per column, in
/// order, or `unsorted`.
fn spec_text(spec: &SortSpec) -> String {
    if spec.is_empty() {
        return "unsorted".into();
    }
    let columns: Vec<String> = spec
        .columns()
        .iter()
        .map(|c| {
            let arrow = match c.direction {
                SortDirection::Ascending => '^',
                SortDirection::Descending => 'v',
            };
            format!("{}{arrow}", c.key.0)
        })
        .collect();
    columns.join(",")
}

fn key_text(item: &StripItem) -> String {
    item.key
        .map_or_else(|| "?".to_string(), |k| k.0.to_string())
}

fn picture(snap: &Snapshot) -> String {
    let width = (snap.extent(Axis::Cols).ceil() as usize).min(MAX_CANVAS.0);
    let height = (snap.extent(Axis::Rows).ceil() as usize).min(MAX_CANVAS.1);
    let mut canvas = vec![vec![' '; width]; height];
    // Writes `text` from (x, y), keeping only the characters inside the clip
    // `[x_min, x_end) x [y_min, ..)`: cells stay out of the header bands.
    let mut put = |x: f32, y: f32, x_min: f32, x_end: f32, y_min: f32, text: &str| {
        let y = y.max(y_min).ceil();
        let Some(line) = canvas.get_mut(y as usize) else {
            return;
        };
        let (x, x_min, x_end) = (x.ceil() as i64, x_min.ceil() as i64, x_end.ceil() as i64);
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i as i64;
            if cx >= x_end || cx >= line.len() as i64 {
                break;
            }
            if cx >= x_min {
                line[cx as usize] = ch;
            }
        }
    };
    let (top, left) = (snap.header(Axis::Rows), snap.header(Axis::Cols));
    for c in snap.cols() {
        put(c.offset, 0.0, left, c.end(), 0.0, &key_text(c));
    }
    for r in snap.rows() {
        put(0.0, r.offset, 0.0, left, top, &r.position.to_string());
        for c in snap.cols() {
            let state = snap.cell_state(r, c);
            let mark = if state.focused {
                "*"
            } else if state.selected {
                "+"
            } else {
                ""
            };
            put(
                c.offset,
                r.offset,
                left,
                c.end(),
                top,
                &format!("{mark}{}.{}", key_text(r), key_text(c)),
            );
        }
    }
    let mut out = String::new();
    for line in canvas {
        let line: String = line.into_iter().collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
