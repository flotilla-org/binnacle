use std::fmt::Write;

use binnacle_core::{
    Axis, Cursor, FocusEnd, Mode, Snapshot, SortDirection, SortSpec, StripItem, ViewSpec,
};

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
/// its Mode and `capturing`, an edit its Mode and `editor x,y wxh` (the
/// edited cell is marked `=`, before focus), a hovered header `hover axis position`, a resize
/// cursor hint `cursor C`, a resize guide `guide axis offset (key K size
/// S)` and a move `moving keys to axis position at offset` (the drop
/// indicator, also drawn across the header band). A non-empty applied View
/// Spec adds `sort` and an unanswered request `requested`, each as the rows'
/// sort with `key^` or `keyv` per column (then `cols` and the columns' sort,
/// if any).
pub fn render(snap: &Snapshot) -> String {
    Rendered::of(snap).since(None)
}

/// A Snapshot rendered as [`render`] does, in parts, so a sequence of them
/// can leave out what did not change.
pub(crate) struct Rendered {
    summary: String,
    strips: [String; 2],
    picture: String,
}

impl Rendered {
    /// The text of this rendering, with each Strip line and the picture that
    /// match `last` written as `(unchanged)`.
    pub(crate) fn since(&self, last: Option<&Rendered>) -> String {
        let mut out = format!("{}\n", self.summary);
        for (axis, strip) in Axis::ALL.into_iter().zip(&self.strips) {
            if last.is_some_and(|l| l.strips[axis.index()] == *strip) {
                let _ = writeln!(out, "{axis:?}: (unchanged)");
            } else {
                let _ = writeln!(out, "{strip}");
            }
        }
        if last.is_some_and(|l| l.picture == self.picture) {
            out.push_str("(picture unchanged)\n");
        } else {
            out.push_str(&self.picture);
        }
        out
    }

    pub(crate) fn of(snap: &Snapshot) -> Rendered {
        Rendered {
            summary: summary(snap),
            strips: Axis::ALL.map(|axis| strip(snap, axis)),
            picture: picture(snap),
        }
    }
}

/// The summary line, without its newline.
fn summary(snap: &Snapshot) -> String {
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
    if !snap.applied_view().is_empty() {
        let _ = write!(out, " | sort {}", view_text(snap.applied_view()));
    }
    if let Some(spec) = snap.requested_view() {
        let _ = write!(out, " | requested {}", view_text(spec));
    }
    if snap.mode() != Mode::Idle {
        let _ = write!(out, " | mode {:?}", snap.mode());
    }
    if let Some(r) = snap.editor_rect() {
        let _ = write!(out, " | editor {},{} {}x{}", r.x, r.y, r.width, r.height);
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
        let _ = match g.key {
            Some(key) => write!(
                out,
                " | guide {:?} {} (key {} size {})",
                g.axis, g.offset, key.0, g.size
            ),
            None => write!(
                out,
                " | guide {:?} {} (band size {})",
                g.axis, g.offset, g.size
            ),
        };
    }
    if let Some(d) = snap.drop_indicator() {
        let keys: Vec<String> = snap.moving_keys().iter().map(|k| k.0.to_string()).collect();
        let _ = write!(
            out,
            " | moving {} to {:?} {} at {}",
            keys.join(","),
            d.axis,
            d.to,
            d.offset
        );
    }
    out
}

/// The line for the Strip on `axis`, without its newline.
fn strip(snap: &Snapshot, axis: Axis) -> String {
    let mut out = format!(
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
    out
}

/// A View Spec's sorts as [`spec_text`] gives them: the rows', then the
/// columns' after `cols` if they sort.
pub(crate) fn view_text(spec: &ViewSpec) -> String {
    let mut text = spec_text(&spec.rows.sort);
    if !spec.cols.sort.is_empty() {
        text += &format!(" cols {}", spec_text(&spec.cols.sort));
    }
    text
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
            let mark = if state.editing {
                "="
            } else if state.focused {
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
    if let Some(d) = snap.drop_indicator() {
        // A `|` or `-` on the indicator, in the header band only.
        let at = d.offset.floor();
        match d.axis {
            Axis::Cols if at >= 0.0 => {
                for line in canvas.iter_mut().take(top.ceil() as usize) {
                    if let Some(ch) = line.get_mut(at as usize) {
                        *ch = '|';
                    }
                }
            }
            Axis::Rows if at >= 0.0 => {
                if let Some(line) = canvas.get_mut(at as usize) {
                    for ch in line.iter_mut().take(left.ceil() as usize) {
                        *ch = '-';
                    }
                }
            }
            _ => {}
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
