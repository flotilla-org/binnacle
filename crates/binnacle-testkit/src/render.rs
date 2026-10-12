use std::fmt::Write;

use binnacle_core::{Axis, Snapshot, StripItem};

/// Largest canvas drawn, in characters; larger viewports are clipped.
const MAX_CANVAS: (usize, usize) = (160, 60);

/// Renders a Snapshot as text: a summary line, each Strip as
/// `position#key@offset+size` (key `?` while unfilled), then a character
/// picture treating one unit as one character. Column headers show the column
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
            "{:?}: count {}, extent {}, header {} |",
            axis,
            snap.count(axis),
            snap.extent(axis),
            snap.header(axis)
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
    let mut put = |x: f32, y: f32, limit: f32, text: &str| {
        let (x, y, limit) = (
            x.floor() as usize,
            y.floor() as usize,
            limit.floor() as usize,
        );
        let Some(line) = canvas.get_mut(y) else {
            return;
        };
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i;
            if cx >= limit || cx >= line.len() {
                break;
            }
            line[cx] = ch;
        }
    };
    for c in snap.cols() {
        put(c.offset, 0.0, c.end(), &key_text(c));
    }
    for r in snap.rows() {
        put(
            0.0,
            r.offset,
            snap.header(Axis::Cols),
            &r.position.to_string(),
        );
        for c in snap.cols() {
            put(
                c.offset,
                r.offset,
                c.end(),
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
