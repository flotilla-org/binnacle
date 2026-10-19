//! PROTOTYPE shell for issue #137: drives `model` by typed commands and
//! redraws the whole frame after each. Throwaway; `model.rs` is the part
//! worth reading. `--walk` runs the scripted walkthrough from NOTES.md.

mod model;

use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, IsTerminal, Write};

use model::*;

const COLS: [(&str, i32); 12] = [
    ("id", 5),
    ("name", 10),
    ("city", 8),
    ("age", 5),
    ("email", 12),
    ("phone", 8),
    ("score", 7),
    ("team", 8),
    ("joined", 8),
    ("notes", 12),
    ("total", 7),
    ("flag", 6),
];
const ROWS: u64 = 30;

fn setup() -> (Grid, Source) {
    let mut names = HashMap::new();
    let rows: Vec<Key> = (1..=ROWS).map(|i| Key(100 + i)).collect();
    for (i, k) in rows.iter().enumerate() {
        names.insert(*k, format!("r{}", i + 1));
    }
    let cols: Vec<Key> = (1..=COLS.len() as u64).map(Key).collect();
    for (k, (n, _)) in cols.iter().zip(COLS) {
        names.insert(*k, n.to_string());
    }
    let axis = |base: Vec<Key>| SourceAxis {
        order: base.clone(),
        prev_order: base.clone(),
        base,
        sort: SortKind::Natural,
        filter: false,
        hidden: BTreeSet::new(),
    };
    let src = Source {
        axes: [axis(rows), axis(cols.clone())],
        names,
    };
    let mut rows_ax = AxisGrid::new(Axis::Rows, ROWS, 1, 1, 3);
    rows_ax.extent = 12;
    let mut cols_ax = AxisGrid::new(Axis::Cols, COLS.len() as u64, 8, 5, 10);
    cols_ax.extent = 72;
    for (k, (_, w)) in cols.iter().zip(COLS) {
        if w != 8 {
            cols_ax.overrides.insert(*k, w);
        }
    }
    let mut grid = Grid {
        axes: [rows_ax, cols_ax],
        focus: None,
        range: None,
        options: Options {
            focus_order: FocusOrder::Drawn,
            range_order: RangeOrder::Source,
            clip_first: ClipFirst::Leading,
            clipped_reveal: ClippedReveal::Nothing,
        },
        outbox: Vec::new(),
    };
    grid.frame(&src);
    grid.set_focus(Key(101), Key(1), &src);
    grid.frame(&src);
    (grid, src)
}

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, Default, PartialEq)]
struct Attr {
    pinned: bool,
    selected: bool,
    focused: bool,
    header: bool,
}

fn regions(ax: &AxisGrid) -> Vec<(char, &Placed)> {
    let s = &ax.strips;
    s.leading
        .iter()
        .map(|p| ('L', p))
        .chain(s.body.iter().map(|p| ('B', p)))
        .chain(s.trailing.iter().map(|p| ('T', p)))
        .collect()
}

fn picture(g: &Grid, src: &Source, ansi: bool) -> String {
    let (rows, cols) = (g.axis(Axis::Rows), g.axis(Axis::Cols));
    let (w, h) = (cols.extent.max(0) as usize, rows.extent.max(0) as usize);
    let mut buf = vec![vec![(' ', Attr::default()); w]; h];
    let focus = g.focus.unwrap_or([Key(0), Key(0)]);
    let mark = |pending: bool| if pending { "?" } else { "" };
    for (cr, c) in regions(cols) {
        let attr = Attr {
            header: true,
            pinned: cr != 'B',
            focused: c.key == focus[1],
            ..Attr::default()
        };
        let text = format!("{}{}", src.name(c.key), mark(c.pending));
        for y in 0..rows.header {
            put(&mut buf, y, c, &text, attr);
        }
    }
    for (rr, r) in regions(rows) {
        let rname = src.name(r.key);
        let num = &rname[1..];
        let attr = Attr {
            header: true,
            pinned: rr != 'B',
            focused: r.key == focus[0],
            ..Attr::default()
        };
        let band = Placed {
            key: r.key,
            position: None,
            pending: false,
            offset: 0,
            size: cols.header,
            shown: 0..cols.header,
        };
        for y in r.shown.clone() {
            put(
                &mut buf,
                y,
                &band,
                &format!("{rname}{}", mark(r.pending)),
                attr,
            );
        }
        for (cr, c) in regions(cols) {
            let sel = g.in_range(Axis::Rows, r.key, src) && g.in_range(Axis::Cols, c.key, src);
            let foc = g.focus == Some([r.key, c.key]);
            let cname = src.name(c.key);
            let mut text = format!("{}{num}", &cname[..cname.len().min(3)]);
            if !ansi && sel {
                text = text.to_uppercase();
            }
            if !ansi && foc {
                text = format!(">{text}");
            }
            let attr = Attr {
                pinned: cr != 'B' || rr != 'B',
                selected: sel,
                focused: foc,
                header: false,
            };
            for y in r.shown.clone() {
                put(&mut buf, y, c, &text, attr);
            }
        }
    }
    let region_of = |ax: &AxisGrid, v: i32| match ax.hit(v).0 {
        Region::Header => 'h',
        Region::Leading => 'L',
        Region::Body => 'B',
        Region::Trailing => 'T',
        Region::Outside => '.',
    };
    let mut out = String::new();
    let tens: String = (0..w)
        .map(|x| {
            if x % 10 == 0 {
                char::from_digit((x / 10 % 10) as u32, 10).unwrap()
            } else {
                ' '
            }
        })
        .collect();
    let units: String = (0..w)
        .map(|x| char::from_digit((x % 10) as u32, 10).unwrap())
        .collect();
    let ruler: String = (0..w as i32).map(|x| region_of(cols, x)).collect();
    out += &format!("   x {tens}\n     {units}\n     {ruler}\n");
    for (y, line) in buf.iter().enumerate() {
        out += &format!("{y:>2} {} ", region_of(rows, y as i32));
        for &(c, a) in line {
            if ansi {
                let mut codes = Vec::new();
                if a.header {
                    codes.push("1");
                }
                if a.pinned {
                    codes.push("36");
                }
                if a.selected {
                    codes.push("7");
                }
                if a.focused {
                    codes.push("4;33");
                }
                if codes.is_empty() {
                    out.push(c);
                } else {
                    out += &format!("\x1b[{}m{c}{RESET}", codes.join(";"));
                }
            } else {
                out.push(c);
            }
        }
        out.push('\n');
    }
    out
}

/// Writes `text` across `item`'s visible part on line `y`. The last cell of
/// each item is left blank as a divider.
fn put(buf: &mut [Vec<(char, Attr)>], y: i32, item: &Placed, text: &str, attr: Attr) {
    let Some(line) = usize::try_from(y).ok().and_then(|y| buf.get_mut(y)) else {
        return;
    };
    let chars: Vec<char> = text.chars().collect();
    for x in item.shown.clone() {
        let i = (x - item.offset) as usize;
        // An item cut short by its region's end keeps a divider there too.
        let last = i + 1 == item.size as usize || x + 1 == item.shown.end;
        let c = if !last {
            *chars.get(i).unwrap_or(&' ')
        } else {
            ' '
        };
        if let Some(cell) = line.get_mut(x as usize) {
            *cell = (c, attr);
        }
    }
}

fn pin_text(ax: &AxisGrid, side: Side, src: &Source) -> String {
    let placed = match side {
        Side::Leading => &ax.strips.leading,
        Side::Trailing => &ax.strips.trailing,
    };
    let items: Vec<String> = ax.pins[side.index()]
        .iter()
        .map(|p| {
            let name = src.name(p.key);
            let clip = placed.iter().find(|q| q.key == p.key).map_or("", |q| {
                if q.shown.is_empty() {
                    " clipped"
                } else if q.shown.len() < q.size as usize {
                    " part-clipped"
                } else {
                    ""
                }
            });
            match p.state {
                PinState::Found(at) => format!("{name}@{at}{clip}"),
                PinState::Pending(at) => format!(
                    "{name} pending(last @{}){clip}",
                    at.map_or("?".into(), |a| a.to_string())
                ),
                PinState::Dormant => format!("{name} DORMANT"),
            }
        })
        .collect();
    format!("[{}]", items.join(", "))
}

fn span(r: &std::ops::Range<i32>) -> String {
    format!("{}..{}", r.start, r.end)
}

fn axis_state(g: &Grid, axis: Axis, src: &Source, b: &str, d: &str, r: &str) -> String {
    let ax = g.axis(axis);
    let order = &src.axes[axis.index()].order;
    let s = &ax.strips;
    let label = match axis {
        Axis::Rows => "ROWS",
        Axis::Cols => "COLS",
    };
    let body: Vec<String> = s
        .body
        .iter()
        .map(|p| format!("{}@{}", src.name(p.key), p.position.unwrap_or(0)))
        .collect();
    let true_extent: i32 = order
        .iter()
        .filter(|&&k| ax.pin_of(k).is_none())
        .map(|&k| ax.size(k))
        .sum();
    let est = ax.scroll_extent(order);
    let off_by = if est != true_extent {
        format!(" {b}(true {true_extent}: off while a Locate is pending){r}")
    } else {
        String::new()
    };
    let in_range: Vec<(bool, String)> = ax
        .drawn_order()
        .into_iter()
        .filter_map(|slot| ax.slot_key(slot, order))
        .map(|k| (g.in_range(axis, k, src), src.name(k).to_string()))
        .collect();
    let runs = in_range.windows(2).filter(|w| w[1].0 && !w[0].0).count()
        + usize::from(in_range.first().is_some_and(|x| x.0));
    let picked: Vec<&str> = in_range
        .iter()
        .filter(|x| x.0)
        .map(|x| x.1.as_str())
        .collect();
    let range = if g.range.is_some() {
        format!(
            "\n  {b}range{r}    {} {d}(drawn order; {runs} run{} on screen){r}",
            picked.join(" "),
            if runs == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };
    format!(
        "{b}{label}{r} count {} | viewport {} band {} min-scroll {} | anchor @{} +{} | {d}Window {}..{}{r} | scroll extent {est}{off_by}\n  \
         {b}leading{r}  {} {d}want {} region {} slide {}{r}\n  \
         {b}body{r}     {} {d}region {}{r}\n  \
         {b}trailing{r} {} {d}want {} region {} slide {}{r}{range}",
        ax.count,
        ax.extent,
        ax.header,
        ax.min_scroll,
        ax.anchor.position,
        ax.anchor.offset,
        ax.needed.start,
        ax.needed.end,
        pin_text(ax, Side::Leading, src),
        s.wants[0],
        span(&s.spans[0]),
        ax.slide[0],
        body.join(" "),
        span(&s.spans[1]),
        pin_text(ax, Side::Trailing, src),
        s.wants[1],
        span(&s.spans[2]),
        ax.slide[1],
    )
}

fn where_is(g: &Grid, axis: Axis, key: Key, src: &Source) -> String {
    let ax = g.axis(axis);
    let order = &src.axes[axis.index()].order;
    let pos = ax
        .position_of(key, order)
        .map_or("?".into(), |p| p.to_string());
    let region = match ax.pin_of(key) {
        Some((Side::Leading, _)) => "leading",
        Some((Side::Trailing, _)) => "trailing",
        None => "body",
    };
    let visible = regions(ax)
        .iter()
        .find(|(_, p)| p.key == key)
        .is_some_and(|(_, p)| !p.shown.is_empty());
    format!(
        "{} @{pos} {region}{}",
        src.name(key),
        if visible { "" } else { " NOT VISIBLE" }
    )
}

fn frame(g: &Grid, src: &Source, ansi: bool, msg: &str) -> String {
    let (b, d, r) = if ansi {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };
    let mut out = picture(g, src, ansi);
    out += &format!("{}\n", axis_state(g, Axis::Cols, src, b, d, r));
    out += &format!("{}\n", axis_state(g, Axis::Rows, src, b, d, r));
    let focus = g.focus.map_or("none".into(), |f| {
        format!(
            "{} x {}",
            where_is(g, Axis::Rows, f[0], src),
            where_is(g, Axis::Cols, f[1], src)
        )
    });
    out += &format!("{b}focus{r} {focus}");
    if let Some(rg) = g.range {
        out += &format!(
            "   {b}range corners{r} {}x{} .. {}x{}",
            src.name(rg.anchor[0]),
            src.name(rg.anchor[1]),
            src.name(rg.head[0]),
            src.name(rg.head[1])
        );
    }
    out += "\n";
    let asks: Vec<&str> = g
        .outbox
        .iter()
        .map(|Effect::Locate { key, .. }| src.name(*key))
        .collect();
    out += &format!(
        "{b}Locate out{r} [{}]   {b}toggles{r} focus={:?} range={:?} clip-first={:?} clipped-reveal={:?}\n",
        asks.join(", "),
        g.options.focus_order,
        g.options.range_order,
        g.options.clip_first,
        g.options.clipped_reveal
    );
    if !msg.is_empty() {
        out += &format!("{b}>{r} {msg}\n");
    }
    out
}

const HELP: &str = "pin ITEM [l|t]  unpin ITEM  mv ITEM IDX  freeze ITEM  s N (cols) | sr N (rows)  f ROW COL  h/l/k/j move, H/L/K/J extend\n\
hit X Y  vw N | vh N  w ITEM N  sort rows|cols natural|reverse|name  filter rows|cols  hide ITEM | unhide ITEM\n\
a (answer one Locate) aa (all)  t focus|range|clip|reveal  reset  q   (ITEM: a column name or r1..r30)";

fn item(src: &Source, name: Option<&str>) -> Result<(Axis, Key), String> {
    let name = name.ok_or("which item?")?;
    src.key(name).ok_or(format!("no item {name}"))
}

fn num(s: Option<&str>) -> Result<i32, String> {
    s.ok_or("number?")?
        .parse()
        .map_err(|_| "not a number".into())
}

fn axis_arg(s: Option<&str>) -> Result<Axis, String> {
    match s {
        Some("rows") => Ok(Axis::Rows),
        Some("cols") => Ok(Axis::Cols),
        _ => Err("rows or cols?".into()),
    }
}

fn run(g: &mut Grid, src: &mut Source, line: &str) -> Result<String, String> {
    let mut w = line.split_whitespace();
    let cmd = w.next().unwrap_or("");
    let msg = match cmd {
        "pin" => {
            let (axis, key) = item(src, w.next())?;
            let side = if w.next() == Some("t") {
                Side::Trailing
            } else {
                Side::Leading
            };
            g.pin(axis, key, side, src);
            format!("pinned {} {side:?}", src.name(key))
        }
        "unpin" => {
            let (axis, key) = item(src, w.next())?;
            g.unpin(axis, key);
            format!("unpinned {}", src.name(key))
        }
        "mv" => {
            let (axis, key) = item(src, w.next())?;
            g.move_pin(axis, key, num(w.next())? as usize);
            "moved within its pinned Strip".into()
        }
        "freeze" => {
            let (axis, key) = item(src, w.next())?;
            g.freeze_up_to(axis, key, src);
            format!("froze up to {}", src.name(key))
        }
        "s" | "sr" => {
            let axis = if cmd == "s" { Axis::Cols } else { Axis::Rows };
            g.scroll(axis, num(w.next())?, src);
            "scrolled".into()
        }
        "f" => {
            let (_, r) = item(src, w.next())?;
            let (_, c) = item(src, w.next())?;
            g.range = None;
            g.set_focus(r, c, src);
            "focused".into()
        }
        "h" | "l" | "k" | "j" | "H" | "L" | "K" | "J" => {
            let lower = cmd.to_lowercase();
            let axis = if lower == "h" || lower == "l" {
                Axis::Cols
            } else {
                Axis::Rows
            };
            let forward = lower == "l" || lower == "j";
            g.move_focus(axis, forward, cmd != lower, src);
            format!("moved focus ({:?} order)", g.options.focus_order)
        }
        "hit" => {
            let (x, y) = (num(w.next())?, num(w.next())?);
            let (rr, ri) = g.axis(Axis::Rows).hit(y);
            let (cr, ci) = g.axis(Axis::Cols).hit(x);
            let name = |p: Option<&Placed>, v: i32| {
                p.map_or(String::new(), |p| {
                    format!(
                        " {} @{:?} within {}",
                        src.name(p.key),
                        p.position,
                        v - p.offset
                    )
                })
            };
            format!(
                "hit x={x} y={y}: col {cr:?}{} | row {rr:?}{}",
                name(ci, x),
                name(ri, y)
            )
        }
        "vw" | "vh" => {
            let axis = if cmd == "vw" { Axis::Cols } else { Axis::Rows };
            g.axis_mut(axis).extent = num(w.next())?.max(0);
            g.reveal_focus(src);
            "viewport resized".into()
        }
        "w" => {
            let (axis, key) = item(src, w.next())?;
            g.resize(axis, key, num(w.next())?);
            format!("resized {}", src.name(key))
        }
        "sort" | "filter" | "hide" | "unhide" => {
            let axis = match cmd {
                "sort" => {
                    let axis = axis_arg(w.next())?;
                    src.axes[axis.index()].sort = match w.next() {
                        Some("reverse") => SortKind::Reverse,
                        Some("name") => SortKind::ByName,
                        _ => SortKind::Natural,
                    };
                    axis
                }
                "filter" => {
                    let axis = axis_arg(w.next())?;
                    let a = &mut src.axes[axis.index()];
                    a.filter = !a.filter;
                    axis
                }
                _ => {
                    let (axis, key) = item(src, w.next())?;
                    let hidden = &mut src.axes[axis.index()].hidden;
                    if cmd == "hide" {
                        hidden.insert(key);
                    } else {
                        hidden.remove(&key);
                    }
                    axis
                }
            };
            src.apply(axis);
            g.refiltered(axis, src);
            format!(
                "source applied the View Spec: Refiltered({axis:?}); every pin asked with Locate"
            )
        }
        "a" => g.answer_one(src).unwrap_or("no Locate out".into()),
        "aa" => {
            let mut all = Vec::new();
            while let Some(m) = g.answer_one(src) {
                all.push(m);
            }
            all.join("; ")
        }
        "t" => {
            let o = &mut g.options;
            match w.next() {
                Some("focus") => {
                    o.focus_order = match o.focus_order {
                        FocusOrder::Drawn => FocusOrder::Source,
                        FocusOrder::Source => FocusOrder::Drawn,
                    }
                }
                Some("range") => {
                    o.range_order = match o.range_order {
                        RangeOrder::Drawn => RangeOrder::Source,
                        RangeOrder::Source => RangeOrder::Drawn,
                    }
                }
                Some("clip") => {
                    o.clip_first = match o.clip_first {
                        ClipFirst::Leading => ClipFirst::Trailing,
                        ClipFirst::Trailing => ClipFirst::Leading,
                    }
                }
                Some("reveal") => {
                    o.clipped_reveal = match o.clipped_reveal {
                        ClippedReveal::Nothing => ClippedReveal::Slide,
                        ClippedReveal::Slide => ClippedReveal::Nothing,
                    }
                }
                _ => return Err("t focus|range|clip|reveal".into()),
            }
            g.reveal_focus(src);
            "toggled".into()
        }
        "reset" => {
            (*g, *src) = setup();
            "reset".into()
        }
        "" => String::new(),
        _ => return Err(format!("unknown command {cmd}")),
    };
    g.frame(src);
    Ok(msg)
}

/// The walkthrough NOTES.md describes: each command and why.
const WALK: &[(&str, &str)] = &[
    (
        "",
        "Start: 12 columns, 30 rows, nothing pinned. Focus r1 x id.",
    ),
    (
        "pin name l",
        "Pin a column leading. The body skips name by Key.",
    ),
    (
        "pin city l",
        "A second leading pin, after the first (pin order).",
    ),
    ("pin total t", "A trailing pin, flush with the far edge."),
    ("mv city 0", "Reorder within the leading Strip."),
    (
        "s 20",
        "Scroll: only the body's anchor moves; extent excludes pins.",
    ),
    ("hit 8 3", "Hit-test a point in the leading Strip."),
    ("hit 40 3", "...in the body."),
    ("hit 68 3", "...in the trailing Strip."),
    ("f r3 name", "Focus a leading pin."),
    (
        "l",
        "Drawn order: name is last leading, so right goes to the first scrolling item (Position 0, id) and the body scrolls back to it.",
    ),
    ("h", "And left comes back to name."),
    ("t focus", "Switch to Source order."),
    (
        "l",
        "Source order: name is Position 1, so right goes to Position 2, city, which is pinned before it: focus jumps backwards on screen.",
    ),
    ("l", "Then Position 3, age, in the body."),
    ("t focus", "Back to Drawn order."),
    ("f r2 city", "Start a range on the first leading pin."),
    ("L", "Extend right..."),
    ("L", "...into the body..."),
    ("L", "...two body items in. Range corners: city and age."),
    (
        "t range",
        "Drawn range order: the same corners select what lies between them on screen.",
    ),
    ("t range", "Back to Source order for the range."),
    (
        "w city 50",
        "Resize a pinned column so the pinned Strips no longer fit (60 + 7 wanted, 57 allowed): clip-first Leading clips name, the inside end of the leading Strip.",
    ),
    (
        "t clip",
        "Clip-first Trailing: leading keeps 57, so total is clipped whole and name in part.",
    ),
    ("t clip", "Back."),
    (
        "f r2 name",
        "Focus the clipped pin: nothing shows it (clipped-reveal Nothing).",
    ),
    (
        "t reveal",
        "Clipped-reveal Slide: the leading Strip slides to show name, hiding most of city.",
    ),
    ("w city 8", "Back to a sane width."),
    ("t reveal", ""),
    (
        "filter cols",
        "Refiltered: the filter drops columns with an o, total among them. Pins go pending: total is still drawn and still subtracted from the extent.",
    ),
    (
        "aa",
        "Answers: name and city Found, total Gone: dormant, no longer drawn, Key kept.",
    ),
    (
        "filter cols",
        "Filter off: Refiltered again, the dormant pin is asked about too.",
    ),
    ("aa", "total Found: back in the trailing Strip, in place."),
    (
        "hide name",
        "Hide a pinned column (hidden Keys in the View Spec): same path.",
    ),
    ("aa", "name dormant."),
    ("unhide name", ""),
    ("aa", "name is back in its pin-order slot, after city."),
    (
        "unpin name",
        "Unpin: name returns to the body at its Position.",
    ),
    ("unpin city", ""),
    (
        "freeze age",
        "Freeze up to here: pins id, name, city, age in their current order.",
    ),
    (
        "freeze name",
        "On a leading pin: unpins the pins after it (city, age). They return before the anchor, off screen, and focus on city is lost from view.",
    ),
    ("pin r5 l", "Rows: pin r5 to the top."),
    ("pin r30 t", "And r30 to the bottom."),
    (
        "sort rows reverse",
        "A sort is a Refiltered too: pins pending at stale Positions.",
    ),
    (
        "j",
        "Moving focus while pending: inside the Window the body skips by Key, so the stale pin Positions do no harm here.",
    ),
    (
        "aa",
        "Answers give the new Positions; the pinned rows never moved on screen.",
    ),
];

fn main() {
    let (mut g, mut src) = setup();
    if std::env::args().any(|a| a == "--walk") {
        for (cmd, note) in WALK {
            let msg = match run(&mut g, &mut src, cmd) {
                Ok(m) | Err(m) => m,
            };
            println!("==== $ {cmd}\n# {note}");
            print!("{}", frame(&g, &src, false, &msg));
            println!();
        }
        return;
    }
    let ansi = io::stdout().is_terminal();
    let mut msg = "type a command; the frame redraws after each".to_string();
    let stdin = io::stdin();
    loop {
        if ansi {
            print!("\x1b[2J\x1b[H");
        }
        print!("{}", frame(&g, &src, ansi, &msg));
        let (d, r) = if ansi { (DIM, RESET) } else { ("", "") };
        print!("{d}{HELP}{r}\n> ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 || line.trim() == "q" {
            break;
        }
        msg = match run(&mut g, &mut src, line.trim()) {
            Ok(m) => m,
            Err(e) => format!("error: {e}"),
        };
    }
}
