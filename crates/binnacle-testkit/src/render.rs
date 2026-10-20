use std::fmt::Write;

use binnacle_core::{
    Axis, ConditionKind, Cursor, Edge, FilterSpec, FocusEnd, Key, Mode, Searched, Snapshot,
    SortDirection, SortSpec, StripItem, TextQuery, Value, ViewSpec,
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
/// (runs of half-open Positions, comma-separated), picks add `picks` (row Keys) and `picks cols`
/// (column Keys), and any of these adds the
/// fill rects as `fill x,y wxh`; selected cells are marked `+`. A gesture adds
/// its Mode and `capturing`, an edit its Mode and `editor x,y wxh` (the
/// edited cell is marked `=`, before focus), a hovered header `hover axis position`, a resize
/// cursor hint `cursor C`, a resize guide `guide axis offset (key K size
/// S)` and a move `moving keys to axis position at offset` (the drop
/// indicator, also drawn across the header band); a move waiting for its
/// answer adds `move pending axis keys`, and a refused one `move refused
/// axis: reason`. An applied View Spec that
/// sorts adds `sort`, as the rows' sort with `key^` or `keyv` per column
/// (then `cols` and the columns' sort, if any); one that filters adds
/// `filter` (see [`view_text`]), an unanswered request `requested` with
/// both, and an answered one the source left part of out `wanted, not
/// applied`. Unfiltered counts the Table Source gave add `totals`. A Find Query
/// adds `find` (and `by cols` for that order), the current match `match
/// rowkey,colkey`, and the matched cell is marked `!` after any other mark.
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
        let runs = |runs: &[std::ops::Range<u64>]| {
            let runs: Vec<String> = runs.iter().map(|r| format!("{r:?}")).collect();
            if runs.is_empty() {
                "none".to_string()
            } else {
                runs.join(",")
            }
        };
        let _ = write!(out, " | range {} x {}", runs(&r.rows), runs(&r.cols));
    }
    for (axis, label) in [(Axis::Rows, "picks"), (Axis::Cols, "picks cols")] {
        if !snap.picked(axis).is_empty() {
            let keys: Vec<String> = snap.picked(axis).iter().map(|k| k.0.to_string()).collect();
            let _ = write!(out, " | {label} {}", keys.join(","));
        }
    }
    for f in snap.selection_rects() {
        let _ = write!(out, " | fill {},{} {}x{}", f.x, f.y, f.width, f.height);
    }
    let applied = snap.applied_view();
    if !(applied.rows.sort.is_empty() && applied.cols.sort.is_empty()) {
        let _ = write!(out, " | sort {}", sorts_text(applied));
    }
    if !(applied.rows.filter.is_empty() && applied.cols.filter.is_empty()) {
        let _ = write!(out, " | filter {}", filters_text(applied));
    }
    for axis in Axis::ALL {
        if let Some(total) = snap.total(axis) {
            let _ = write!(out, " | total {axis:?} {total}");
        }
    }
    if let Some(spec) = snap.requested_view() {
        let state = if snap.view_pending() {
            "requested"
        } else {
            "wanted, not applied"
        };
        let _ = write!(out, " | {state} {}", view_text(spec));
    }
    if let Some(find) = snap.find() {
        let _ = write!(out, " | find {}", query_text(&find.query));
        if find.by == Axis::Cols {
            out.push_str(" by cols");
        }
    }
    if let Some(m) = snap.find_match() {
        let _ = write!(out, " | match {},{}", m.row.0, m.col.0);
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
    if let Some((axis, keys)) = snap.pending_move() {
        let keys: Vec<String> = keys.iter().map(|k| k.0.to_string()).collect();
        let _ = write!(out, " | move pending {axis:?} {}", keys.join(","));
    }
    if let Some((axis, reason)) = snap.move_refusal() {
        let _ = write!(out, " | move refused {axis:?}: {reason}");
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
    let pinned = snap.pinned(axis, Edge::Leading);
    if !pinned.is_empty() {
        out.push_str(" pinned");
        for item in pinned {
            let _ = write!(
                out,
                " {}#{}@{}+{}",
                item.position,
                key_text(item),
                item.offset,
                item.size
            );
        }
        let _ = write!(out, " | scrolling from {} |", snap.scrolling(axis).start);
    }
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
    let trailing = snap.pinned(axis, Edge::Trailing);
    if !trailing.is_empty() {
        let _ = write!(
            out,
            " | scrolling to {} | trailing",
            snap.scrolling(axis).end
        );
        for item in trailing {
            let _ = write!(
                out,
                " {}#{}@{}+{}",
                item.position,
                key_text(item),
                item.offset,
                item.size
            );
        }
    }
    out
}

/// A View Spec's sorts, then its filters if any (see [`filters_text`]), then
/// its hidden Keys if any: `hide` and the rows' Keys, then `cols` and the
/// columns'.
pub(crate) fn view_text(spec: &ViewSpec) -> String {
    let mut text = sorts_text(spec);
    if !(spec.rows.filter.is_empty() && spec.cols.filter.is_empty()) {
        text += &format!(" filter {}", filters_text(spec));
    }
    if !(spec.rows.hidden.is_empty() && spec.cols.hidden.is_empty()) {
        let keys = |keys: &[Key]| {
            let keys: Vec<String> = keys.iter().map(|k| k.0.to_string()).collect();
            if keys.is_empty() {
                "none".to_string()
            } else {
                keys.join(",")
            }
        };
        text += &format!(" hide {}", keys(&spec.rows.hidden));
        if !spec.cols.hidden.is_empty() {
            text += &format!(" cols {}", keys(&spec.cols.hidden));
        }
    }
    text
}

/// A View Spec's sorts as [`spec_text`] gives them: the rows', then the
/// columns' after `cols` if they sort.
fn sorts_text(spec: &ViewSpec) -> String {
    let mut text = spec_text(&spec.rows.sort);
    if !spec.cols.sort.is_empty() {
        text += &format!(" cols {}", spec_text(&spec.cols.sort));
    }
    text
}

/// A View Spec's filters: the rows', then the columns' after `cols` if they
/// filter. A quick filter is `quick "text"`, then `case` and `regex` for its
/// options and `in k,k` for the Keys it searches, if not the shown ones. Each
/// condition follows after `&`: a Text condition is `key: "text"` (or `key:
/// not "text"`) with the same options.
fn filters_text(spec: &ViewSpec) -> String {
    let query = |q: &TextQuery, text: &mut String| {
        if q.case_sensitive {
            *text += " case";
        }
        if q.regex {
            *text += " regex";
        }
    };
    let filter = |f: &FilterSpec| {
        let mut parts = Vec::new();
        if let Some(q) = &f.quick {
            parts.push(format!("quick {}", query_text(q)));
        }
        for c in f.conditions.conditions() {
            let not = if c.not { "not " } else { "" };
            let mut text = format!("{}: {not}", c.key.0);
            match &c.kind {
                ConditionKind::Text(q) => {
                    text += &format!("{:?}", q.text);
                    query(q, &mut text);
                }
                ConditionKind::OneOf(values) => {
                    let values: Vec<String> = (values.iter())
                        .map(|v| match v {
                            Value::Text(t) => format!("{t:?}"),
                            Value::Blank => "blank".into(),
                        })
                        .collect();
                    text += &format!("one of [{}]", values.join(", "));
                }
                _ => text += "?",
            }
            parts.push(text);
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(" & ")
        }
    };
    let mut text = filter(&spec.rows.filter);
    if !spec.cols.filter.is_empty() {
        text += &format!(" cols {}", filter(&spec.cols.filter));
    }
    text
}

/// A Text Query as `"text"`, then `case` and `regex` for its options and
/// `in k,k` for the Keys it searches, if not the shown ones.
fn query_text(q: &TextQuery) -> String {
    let mut text = format!("{:?}", q.text);
    if q.case_sensitive {
        text += " case";
    }
    if q.regex {
        text += " regex";
    }
    if let Searched::Keys(keys) = &q.searches {
        let keys: Vec<String> = keys.iter().map(|k| k.0.to_string()).collect();
        text += &format!(" in {}", keys.join(","));
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
    // `[x_min, x_end) x [y_min, y_end)`: cells stay out of the header bands
    // and each in its own Strip.
    let mut put =
        |x: f32, y: f32, (x_min, x_end): (f32, f32), (y_min, y_end): (f32, f32), text: &str| {
            let y = y.max(y_min).ceil();
            if y >= y_end.ceil() {
                return;
            }
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
    // Each drawn item with the span it is clipped to: a pinned one from the
    // Header Band to the viewport's end, a scrolling one to its own region.
    let drawn = |axis: Axis| {
        let pinned = |edge| snap.pinned(axis, edge).iter();
        let whole = (snap.header(axis), snap.extent(axis));
        let scrolling = snap.scrolling(axis);
        (pinned(Edge::Leading).map(move |i| (i, whole)))
            .chain((snap.strip(axis).iter()).map(move |i| (i, (scrolling.start, scrolling.end))))
            .chain(pinned(Edge::Trailing).map(move |i| (i, whole)))
    };
    for (c, (from, to)) in drawn(Axis::Cols) {
        put(
            c.offset,
            0.0,
            (from, c.end().min(to)),
            (0.0, top),
            &key_text(c),
        );
    }
    for (r, rows_clip) in drawn(Axis::Rows) {
        put(
            0.0,
            r.offset,
            (0.0, left),
            rows_clip,
            &r.position.to_string(),
        );
        for (c, (from, to)) in drawn(Axis::Cols) {
            let state = snap.cell_state(r, c);
            let mut mark = if state.editing {
                "="
            } else if state.focused {
                "*"
            } else if state.selected {
                "+"
            } else {
                ""
            }
            .to_string();
            if state.current_match {
                mark.push('!');
            }
            put(
                c.offset,
                r.offset,
                (from, c.end().min(to)),
                rows_clip,
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
