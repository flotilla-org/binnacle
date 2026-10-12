use std::fmt::Write;

use binnacle_core::{Axis, Snapshot, StripItem};

/// Largest canvas drawn, in characters; larger viewports are clipped.
const MAX_CANVAS: (usize, usize) = (160, 60);

/// Renders a Snapshot as text: a summary line, each Strip as
/// `position#key@offset+size` after its Scroll Anchor (`position+offset`) and
/// thumb (`fraction/size`) (key `?` while unfilled), then a character
/// picture treating one unit as one character, each item starting at the first
/// whole character it reaches. Column headers show the column
/// Key, row headers the row Position, cells `rowkey.colkey`.
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
    let _ = writeln!(out, "gen {} | damage {damage}", snap.generation());
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
            put(
                c.offset,
                r.offset,
                left,
                c.end(),
                top,
                &format!("{}.{}", key_text(r), key_text(c)),
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
