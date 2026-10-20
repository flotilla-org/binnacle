//! PROTOTYPE (#140), throwaway: the column presentation controls and the
//! pinned edge in a terminal. Three variants (Ctrl+V or `--variant a|b|c`),
//! three keymap presets (Ctrl+P or `--preset chords|modes|bare-keys`), and
//! the Border Styles (Ctrl+B). The grid is a fake painted straight into the
//! buffer, and so is the Table Source: pinning, hiding and moves are not in
//! the core yet. `--dump` prints scripted scenes from a TestBackend instead.
//! Look and feel are borrowed from the adapter's Themes and Border Styles.

use std::collections::BTreeSet;
use std::io;
use std::time::{Duration, Instant};

use binnacle_ratatui::{BorderExtent, BorderStyle, BorderWeight, Theme};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Clear, Widget};

// ------------------------------------------------------------------ the fake source

const COLS: [&str; 30] = [
    "Vessel",
    "IMO",
    "Flag",
    "Type",
    "Operator",
    "Port",
    "Berth",
    "Arrived",
    "ETD",
    "Cargo",
    "Tonnage",
    "Draught",
    "Length",
    "Beam",
    "Built",
    "Class",
    "Agent",
    "Pilot",
    "Tugs",
    "Crew",
    "Fuel",
    "Speed",
    "Heading",
    "Lat",
    "Lon",
    "Last port",
    "Next port",
    "Inspected",
    "Status",
    "Notes",
];
const VESSELS: [&str; 40] = [
    "Aurora",
    "Bergen Star",
    "Cape Wrath",
    "Dagny",
    "Eider",
    "Fjordline",
    "Gannet",
    "Hafren",
    "Isbjorn",
    "Juno",
    "Kittiwake",
    "Lofoten",
    "Maere",
    "Nordkapp",
    "Orkney Maid",
    "Petrel",
    "Quayside",
    "Ranger",
    "Skarv",
    "Tern",
    "Ulla",
    "Vesteral",
    "Whimbrel",
    "Xanthe",
    "Ymir",
    "Zephyr",
    "Albatross",
    "Brage",
    "Corrie",
    "Dunlin",
    "Ellida",
    "Fulmar",
    "Grimsay",
    "Heimdal",
    "Iona",
    "Jarl",
    "Kestrel",
    "Lyra",
    "Merlin",
    "Njord",
];
const PORTS: [&str; 8] = [
    "Bergen",
    "Alesund",
    "Tromso",
    "Aberdeen",
    "Lerwick",
    "Esbjerg",
    "Rotterdam",
    "Hull",
];
const LATENCY: Duration = Duration::from_millis(1200);
/// The least scrolling region the pinned Strips leave: a few cells each way.
const MIN_SCROLL_COLS: u16 = 8;
const MIN_SCROLL_ROWS: u16 = 3;
/// Past this many rows, hiding suggests a filter instead (#134).
const ROW_HIDE_CAP: usize = 5;
const RN_W: u16 = 5;

fn hash(a: usize, b: usize) -> usize {
    let mut h = (a as u32).wrapping_mul(374_761_393) ^ (b as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) % 100_000) as usize
}

fn value(row: usize, col: usize) -> String {
    let h = hash(row + 1, col + 7);
    let pick = |xs: &[&str]| xs[h % xs.len()].to_string();
    match COLS[col] {
        "Vessel" => VESSELS[row].to_string(),
        "IMO" => (9_100_000 + (row * 7919) % 899_999).to_string(),
        "Flag" => pick(&["NO", "GB", "DK", "NL", "FO", "IS"]),
        "Type" => pick(&["Ferry", "Tanker", "Trawler", "Bulk", "Supply", "Ro-ro"]),
        "Operator" => pick(&["Hurtigruten", "Fjord1", "NorthLink", "DFDS", "Smyril"]),
        "Port" | "Last port" | "Next port" => pick(&PORTS),
        "Berth" => format!("{}{}", (b'A' + (h % 6) as u8) as char, h % 20),
        "Arrived" | "ETD" | "Inspected" => format!("10-{:02}", 1 + h % 28),
        "Cargo" => pick(&["Fish", "Timber", "Boxes", "Gravel", "People", "Oil"]),
        "Status" => pick(&["Alongside", "Anchored", "Departed", "Due"]),
        "Notes" => pick(&["", "Awaiting pilot", "Bunkering", "Crew change"]),
        _ => (h % 5000).to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ax {
    Cols,
    Rows,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Lead,
    Trail,
}

/// How the fake source treats a sort on the axis (#136).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sort {
    None,
    /// Sorted, and a move bakes the sorted order and clears the sort.
    Bake,
    /// Sorted, and the source withdraws "movable".
    Withdraw,
}

struct Pending {
    key: usize,
    before: Option<usize>,
    since: Instant,
}

struct AxisModel {
    ax: Ax,
    labels: Vec<&'static str>,
    /// The source's own order over every Key, hidden ones included.
    order: Vec<usize>,
    hidden: BTreeSet<usize>,
    /// The Grid's pinned Key lists, in pin order (#135).
    lead: Vec<usize>,
    trail: Vec<usize>,
    /// Content widths (columns only).
    sizes: Vec<u16>,
    sort: Sort,
    refuse_next: bool,
    pending: Option<Pending>,
    /// The Scroll Anchor, as an index into the scrolling Strip.
    scroll: usize,
}

impl AxisModel {
    fn new(ax: Ax, labels: &[&'static str]) -> Self {
        AxisModel {
            ax,
            labels: labels.to_vec(),
            order: (0..labels.len()).collect(),
            hidden: BTreeSet::new(),
            lead: Vec::new(),
            trail: Vec::new(),
            sizes: labels.iter().map(|_| 9).collect(),
            sort: Sort::None,
            refuse_next: false,
            pending: None,
            scroll: 0,
        }
    }
    fn shown(&self) -> Vec<usize> {
        (self.order.iter().copied())
            .filter(|k| !self.hidden.contains(k))
            .collect()
    }
    fn position(&self, k: usize) -> Option<usize> {
        self.shown().iter().position(|&x| x == k)
    }
    fn side(&self, k: usize) -> Option<Side> {
        if self.lead.contains(&k) {
            Some(Side::Lead)
        } else if self.trail.contains(&k) {
            Some(Side::Trail)
        } else {
            None
        }
    }
    fn strips(&self) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
        let shown = |ks: &Vec<usize>| -> Vec<usize> {
            ks.iter()
                .copied()
                .filter(|k| !self.hidden.contains(k))
                .collect()
        };
        let scroll = (self.shown().into_iter())
            .filter(|k| !self.lead.contains(k) && !self.trail.contains(k))
            .collect();
        (shown(&self.lead), scroll, shown(&self.trail))
    }
    /// An item's extent along the axis: a column's width and its Gap, or one line.
    fn size(&self, k: usize) -> u16 {
        match self.ax {
            Ax::Cols => self.sizes[k] + 1,
            Ax::Rows => 1,
        }
    }
    fn total(&self, ks: &[usize]) -> u16 {
        ks.iter().map(|&k| self.size(k)).sum()
    }
    fn movable(&self) -> bool {
        self.sort != Sort::Withdraw
    }
    fn noun(&self, n: usize) -> &'static str {
        match (self.ax, n == 1) {
            (Ax::Cols, true) => "column",
            (Ax::Cols, false) => "columns",
            (Ax::Rows, true) => "row",
            (Ax::Rows, false) => "rows",
        }
    }
    fn side_name(&self, s: Side) -> &'static str {
        match (self.ax, s) {
            (Ax::Cols, Side::Lead) => "left",
            (Ax::Cols, Side::Trail) => "right",
            (Ax::Rows, Side::Lead) => "top",
            (Ax::Rows, Side::Trail) => "bottom",
        }
    }
    fn count(&self) -> String {
        format!(
            "{} of {} {}",
            self.shown().len(),
            self.labels.len(),
            self.noun(2)
        )
    }
    fn names(&self, ks: &[usize]) -> String {
        if ks.len() > 3 {
            return format!("{} {}", ks.len(), self.noun(2));
        }
        (ks.iter().map(|&k| format!("\"{}\"", self.labels[k])))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// How much of each pinned Strip is drawn: together at most the body less the
/// least scrolling region, the trailing Strip clipped first, each from its
/// inside edge; a focused item in a clipped Strip slides it (#137). `edge` is
/// what drawing the pinned edge costs inside each nonempty Strip.
struct Allot {
    lead: Vec<usize>,
    scroll: Vec<usize>,
    trail: Vec<usize>,
    lead_w: u16,
    trail_w: u16,
    lead_a: u16,
    trail_a: u16,
    lead_off: u16,
    trail_off: u16,
}

fn allot(m: &AxisModel, body: u16, focused: usize, edge: u16) -> Allot {
    let (lead, scroll, trail) = m.strips();
    let cost = |ks: &Vec<usize>| if ks.is_empty() { 0 } else { edge };
    let lead_w = m.total(&lead) + cost(&lead);
    let trail_w = m.total(&trail) + cost(&trail);
    let min = match m.ax {
        Ax::Cols => MIN_SCROLL_COLS,
        Ax::Rows => MIN_SCROLL_ROWS,
    };
    let room = body.saturating_sub(min);
    let lead_a = lead_w.min(room);
    let trail_a = trail_w.min(room - lead_a);
    let slide = |list: &Vec<usize>, shown: u16, from: u16| -> u16 {
        let Some(at) = list.iter().position(|&k| k == focused) else {
            return from;
        };
        let start = m.total(&list[..at]);
        let end = start + m.size(focused);
        if start < from {
            start
        } else if end > from + shown {
            end - shown
        } else {
            from
        }
    };
    let lead_off = slide(&lead, lead_a.saturating_sub(cost(&lead)), 0);
    let trail_items = trail_w - cost(&trail);
    let trail_shown = trail_a.saturating_sub(cost(&trail));
    let trail_off = slide(&trail, trail_shown, trail_items.saturating_sub(trail_shown));
    Allot {
        lead,
        scroll,
        trail,
        lead_w,
        trail_w,
        lead_a,
        trail_a,
        lead_off,
        trail_off,
    }
}

// ------------------------------------------------------------------ presets and actions

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Preset {
    Chords,
    Modes,
    BareKeys,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Variant {
    A,
    B,
    C,
}

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Variant::A => "A popup menu, checklist, count in the status line, ║ with caps",
            Variant::B => "B which-key line, columns sheet, count in the bar, ║ through the header",
            Variant::C => "C menu with hidden inline, grouped chooser, count in the corner, tint",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    Hide,
    ShowHidden,
    Pin(Side),
    TogglePinLead,
    Unpin,
    Freeze,
    Menu,
    Chooser,
    /// The modes preset: enter the presentation mode for an axis.
    Mode,
}

/// The preset's bindings: key, modifiers, axis, action, and the label the keys line shows.
fn bindings(p: Preset) -> Vec<(KeyCode, KeyModifiers, Ax, Act)> {
    use Act::*;
    use Ax::*;
    use KeyCode::{Char, F};
    const NONE: KeyModifiers = KeyModifiers::NONE;
    const ALT: KeyModifiers = KeyModifiers::ALT;
    const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
    match p {
        // Excel's Ctrl+0 and Ctrl+9 move to Alt: terminals drop Ctrl with a digit.
        Preset::Chords => vec![
            (Char('0'), ALT, Cols, Hide),
            (Char('9'), ALT, Rows, Hide),
            (Char(')'), ALT, Cols, ShowHidden),
            (Char('('), ALT, Rows, ShowHidden),
            (Char(','), ALT, Cols, Pin(Side::Lead)),
            (Char('.'), ALT, Cols, Pin(Side::Trail)),
            (Char('<'), ALT, Rows, Pin(Side::Lead)),
            (Char('>'), ALT, Rows, Pin(Side::Trail)),
            (Char('u'), ALT, Cols, Unpin),
            (Char('U'), ALT, Rows, Unpin),
            (Char('w'), ALT, Cols, Freeze),
            (Char('W'), ALT, Rows, Freeze),
            (F(10), SHIFT, Cols, Menu),
            (Char('m'), ALT, Cols, Menu),
            (Char('M'), ALT, Rows, Menu),
            (Char('o'), ALT, Cols, Chooser),
            (Char('O'), ALT, Rows, Chooser),
        ],
        // vim's z for view commands opens a mode; VisiData's - ! and C work outside it.
        Preset::Modes => vec![
            (Char('z'), NONE, Cols, Mode),
            (Char('Z'), NONE, Rows, Mode),
            (Char('-'), NONE, Cols, Hide),
            (Char('!'), NONE, Cols, TogglePinLead),
            (Char('C'), NONE, Cols, Chooser),
        ],
        Preset::BareKeys => vec![
            (Char('-'), NONE, Cols, Hide),
            (Char('_'), NONE, Rows, Hide),
            (Char('+'), NONE, Cols, ShowHidden),
            (Char('*'), NONE, Rows, ShowHidden),
            (Char('<'), NONE, Cols, Pin(Side::Lead)),
            (Char('>'), NONE, Cols, Pin(Side::Trail)),
            (Char('['), NONE, Rows, Pin(Side::Lead)),
            (Char(']'), NONE, Rows, Pin(Side::Trail)),
            (Char('u'), NONE, Cols, Unpin),
            (Char('U'), NONE, Rows, Unpin),
            (Char('F'), NONE, Cols, Freeze),
            (Char('T'), NONE, Rows, Freeze),
            (Char('.'), NONE, Cols, Menu),
            (Char(','), NONE, Rows, Menu),
            (Char('c'), NONE, Cols, Chooser),
            (Char('C'), NONE, Rows, Chooser),
        ],
    }
}

/// Inside the modes preset's presentation mode.
fn mode_binding(c: char) -> Option<Act> {
    Some(match c {
        'h' => Act::Hide,
        's' => Act::ShowHidden,
        '<' => Act::Pin(Side::Lead),
        '>' => Act::Pin(Side::Trail),
        'u' => Act::Unpin,
        'f' => Act::Freeze,
        'm' => Act::Menu,
        'c' => Act::Chooser,
        _ => return None,
    })
}

/// A binding's key: Shift dropped from a character, whose case carries it.
fn chord(code: KeyCode, mut mods: KeyModifiers) -> (KeyCode, KeyModifiers) {
    if matches!(code, KeyCode::Char(_)) {
        mods.remove(KeyModifiers::SHIFT);
    }
    (code, mods)
}

fn key_name(code: KeyCode, mods: KeyModifiers) -> String {
    let mut s = String::new();
    if mods.contains(KeyModifiers::CONTROL) {
        s += "Ctrl+";
    }
    if mods.contains(KeyModifiers::ALT) {
        s += "Alt+";
    }
    if mods.contains(KeyModifiers::SHIFT) {
        s += "Shift+";
    }
    match code {
        KeyCode::Char(c) if !mods.is_empty() && c.is_ascii_lowercase() => {
            s.push(c.to_ascii_uppercase())
        }
        KeyCode::Char(c) if !mods.is_empty() && c.is_ascii_uppercase() => {
            s += "Shift+";
            s.push(c)
        }
        KeyCode::Char(c) => s.push(c),
        KeyCode::F(n) => s += &format!("F{n}"),
        other => s += &format!("{other:?}"),
    }
    s
}

// ------------------------------------------------------------------ the app

#[derive(Clone, PartialEq, Eq, Debug)]
enum Popup {
    None,
    /// The column (or row) menu; `flyout` holds the selection in "Show hidden ▸".
    Menu {
        ax: Ax,
        key: usize,
        sel: usize,
        flyout: Option<usize>,
    },
    /// Variant B's menu: a which-key line; `hidden` once "s" asked for the list.
    WhichKey {
        ax: Ax,
        key: usize,
        hidden: bool,
    },
    Chooser {
        ax: Ax,
        sel: usize,
        search: String,
        searching: bool,
        /// Variant B's sheet: which of its columns sorts it.
        by: usize,
    },
}

#[derive(Clone, Debug)]
struct Drop {
    kind: DropKind,
    side: Option<Side>,
    index: usize,
    before: Option<usize>,
    /// The cell (x for columns, y for rows) the drop line marks.
    line: Option<u16>,
    label: String,
    refused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DropKind {
    Pin,
    Move,
    UnpinMove,
    UnpinReturn,
    None,
}

struct Drag {
    ax: Ax,
    key: usize,
    from: (u16, u16),
    active: bool,
    drop: Option<Drop>,
}

/// Where the last frame drew things, for the mouse.
#[derive(Default, Clone)]
struct Geometry {
    grid: Rect,
    body_x: u16,
    body_y: u16,
    body_w: u16,
    body_h: u16,
    /// (start, end, key) of each drawn column header and row header.
    cols: Vec<(i32, i32, usize)>,
    rows: Vec<(i32, i32, usize)>,
    head_y: u16,
}

struct App {
    cols: AxisModel,
    rows: AxisModel,
    focus: (usize, usize),
    variant: Variant,
    preset: Preset,
    border: usize,
    theme: usize,
    popup: Popup,
    mode: Option<Ax>,
    message: Option<(String, bool, Instant)>,
    last: String,
    drag: Option<Drag>,
    show_state: bool,
    quit: bool,
    geo: Geometry,
}

const BORDERS: [(&str, BorderStyle); 6] = [
    (
        "columns light",
        BorderStyle::new(BorderExtent::Columns, BorderWeight::Light),
    ),
    (
        "grid light",
        BorderStyle::new(BorderExtent::Grid, BorderWeight::Light),
    ),
    (
        "headers light",
        BorderStyle::new(BorderExtent::Headers, BorderWeight::Light),
    ),
    (
        "headers + band rules",
        BorderStyle::new(BorderExtent::Headers, BorderWeight::Light).band_rules(true),
    ),
    (
        "none",
        BorderStyle::new(BorderExtent::None, BorderWeight::Light),
    ),
    (
        "columns double",
        BorderStyle::new(BorderExtent::Columns, BorderWeight::Double),
    ),
];

impl App {
    fn new(variant: Variant, preset: Preset) -> Self {
        let mut cols = AxisModel::new(Ax::Cols, &COLS);
        let rows = AxisModel::new(Ax::Rows, &VESSELS);
        for (k, w) in [
            (0, 11),
            (1, 7),
            (2, 4),
            (4, 11),
            (7, 5),
            (8, 5),
            (27, 9),
            (29, 14),
        ] {
            cols.sizes[k] = w;
        }
        let mut app = App {
            cols,
            rows,
            focus: (0, 5),
            variant,
            preset,
            border: 0,
            theme: 1,
            popup: Popup::None,
            mode: None,
            message: None,
            last: "Loaded".into(),
            drag: None,
            show_state: true,
            quit: false,
            geo: Geometry::default(),
        };
        // A starting layout with something in every region.
        app.cols.lead = vec![0, 1];
        app.cols.trail = vec![28];
        app.cols.hidden.extend([14, 15, 22]);
        app.rows.lead = vec![3];
        app
    }

    fn m(&self, ax: Ax) -> &AxisModel {
        match ax {
            Ax::Cols => &self.cols,
            Ax::Rows => &self.rows,
        }
    }
    fn m_mut(&mut self, ax: Ax) -> &mut AxisModel {
        match ax {
            Ax::Cols => &mut self.cols,
            Ax::Rows => &mut self.rows,
        }
    }
    fn focused(&self, ax: Ax) -> usize {
        match ax {
            Ax::Cols => self.focus.1,
            Ax::Rows => self.focus.0,
        }
    }
    fn set_focus(&mut self, ax: Ax, k: usize) {
        match ax {
            Ax::Cols => self.focus.1 = k,
            Ax::Rows => self.focus.0 = k,
        }
    }
    fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), error, Instant::now()));
    }
    fn theme(&self) -> Theme {
        Theme::presets()[self.theme].clone()
    }
    fn style(&self) -> BorderStyle {
        BORDERS[self.border].1
    }
    /// Rows pay a line for the pinned edge in variant A.
    fn row_edge(&self) -> u16 {
        u16::from(self.variant == Variant::A)
    }
    fn allot(&self, ax: Ax) -> Allot {
        match ax {
            Ax::Cols => allot(&self.cols, self.geo.body_w, self.focus.1, 0),
            Ax::Rows => allot(&self.rows, self.geo.body_h, self.focus.0, self.row_edge()),
        }
    }

    // -------------------------------------------------------------- actions on the fake

    fn hide(&mut self, ax: Ax, ks: Vec<usize>) {
        if ax == Ax::Rows && ks.len() > ROW_HIDE_CAP {
            self.say(
                format!("Hiding more than {ROW_HIDE_CAP} rows at once: use a filter instead"),
                true,
            );
            return;
        }
        let shown = self.m(ax).shown();
        if shown.iter().all(|k| ks.contains(k)) {
            let noun = self.m(ax).noun(1);
            self.say(format!("At least one {noun} stays shown"), true);
            return;
        }
        let f = self.focused(ax);
        if ks.contains(&f) {
            // The source's suggested neighbour: the next item in the old order, else the previous.
            let at = shown.iter().position(|&k| k == f).unwrap_or(0);
            let next = (shown[at + 1..].iter().find(|k| !ks.contains(k)))
                .or_else(|| shown[..at].iter().rev().find(|k| !ks.contains(k)))
                .copied()
                .unwrap_or(f);
            self.set_focus(ax, next);
        }
        let m = self.m_mut(ax);
        m.hidden.extend(ks.iter().copied());
        let dormant: Vec<usize> = ks
            .iter()
            .copied()
            .filter(|&k| m.side(k).is_some())
            .collect();
        let mut last = format!("Hid {}", m.names(&ks));
        if !dormant.is_empty() {
            last += &format!(" (pins of {} go dormant)", m.names(&dormant));
        }
        self.last = last;
    }

    fn show(&mut self, ax: Ax, ks: Vec<usize>) {
        let m = self.m_mut(ax);
        for k in &ks {
            m.hidden.remove(k);
        }
        self.last = format!("Showed {}", m.names(&ks));
    }

    fn pin(&mut self, ax: Ax, k: usize, side: Side, index: Option<usize>) {
        let m = self.m_mut(ax);
        let was = m.side(k);
        let list = match side {
            Side::Lead => &m.lead,
            Side::Trail => &m.trail,
        };
        let mut at = index.unwrap_or(list.len());
        if was == Some(side) && list.iter().position(|&x| x == k).is_some_and(|i| i < at) {
            at -= 1;
        }
        m.lead.retain(|&x| x != k);
        m.trail.retain(|&x| x != k);
        let list = match side {
            Side::Lead => &mut m.lead,
            Side::Trail => &mut m.trail,
        };
        list.insert(at.min(list.len()), k);
        self.last = if was == Some(side) {
            format!("Reordered the {} pins", m.side_name(side))
        } else {
            format!("Pinned {} {}", m.names(&[k]), m.side_name(side))
        };
    }

    fn unpin(&mut self, ax: Ax, k: usize) {
        let m = self.m_mut(ax);
        m.lead.retain(|&x| x != k);
        m.trail.retain(|&x| x != k);
        self.last = format!("Unpinned {}", m.names(&[k]));
    }

    /// Pins every Key from the start up to `k`, in drawn order, capped at what fits (#137).
    fn freeze(&mut self, ax: Ax, k: usize) {
        let body = match ax {
            Ax::Cols => self.geo.body_w,
            Ax::Rows => self.geo.body_h,
        };
        let edge = if ax == Ax::Rows { self.row_edge() } else { 0 };
        let m = self.m_mut(ax);
        if m.side(k) == Some(Side::Trail) {
            self.say("Freeze up to here works from the leading edge", true);
            return;
        }
        let (lead, scroll, _) = m.strips();
        let drawn: Vec<usize> = lead.iter().chain(scroll.iter()).copied().collect();
        let upto = drawn.iter().position(|&x| x == k).map_or(0, |i| i + 1);
        let wanted = &drawn[..upto];
        let min = if ax == Ax::Cols {
            MIN_SCROLL_COLS
        } else {
            MIN_SCROLL_ROWS
        };
        let room = body.saturating_sub(min).saturating_sub(edge);
        let mut used = 0;
        let mut fits = Vec::new();
        for &x in wanted {
            if used + m.size(x) > room {
                break;
            }
            used += m.size(x);
            fits.push(x);
        }
        let dormant: Vec<usize> = (m.lead.iter().copied())
            .filter(|x| m.hidden.contains(x))
            .collect();
        m.trail.retain(|x| !fits.contains(x));
        let (n, total) = (fits.len(), wanted.len());
        m.lead = fits;
        m.lead.extend(dormant);
        let (name, noun) = (m.names(&[k]), m.noun(n));
        self.last = format!("Froze up to {name}");
        if n < total {
            self.say(
                format!("Froze the first {n} {noun}: the rest would not fit"),
                false,
            );
        }
    }

    /// Asks the fake source to move `k` before `before` (None: to the end); one at a time.
    fn request_move(&mut self, ax: Ax, k: usize, before: Option<usize>) {
        if self.m(ax).pending.is_some() {
            self.say("One move at a time: the last one is still pending", true);
            return;
        }
        let m = self.m_mut(ax);
        m.pending = Some(Pending {
            key: k,
            before,
            since: Instant::now(),
        });
        self.last = format!(
            "Asked the source to move {}{}",
            m.names(&[k]),
            before.map_or(" to the end".into(), |b| format!(
                " before {}",
                m.names(&[b])
            ))
        );
    }

    /// Answers pending moves whose time has come.
    fn tick(&mut self, now: Instant) {
        for ax in [Ax::Cols, Ax::Rows] {
            let m = self.m_mut(ax);
            let Some(p) = m
                .pending
                .take_if(|p| now.duration_since(p.since) >= LATENCY)
            else {
                continue;
            };
            if m.refuse_next {
                m.refuse_next = false;
                let name = m.names(&[p.key]);
                self.last = format!("The source refused the move of {name}");
                self.say(
                    format!("Move refused: the source's order changed under the drop ({name})"),
                    true,
                );
                continue;
            }
            m.order.retain(|&x| x != p.key);
            let at = p
                .before
                .and_then(|b| m.order.iter().position(|&x| x == b))
                .unwrap_or(m.order.len());
            m.order.insert(at, p.key);
            let mut last = format!("Moved {}", m.names(&[p.key]));
            if m.sort == Sort::Bake {
                m.sort = Sort::None;
                last += "; the source baked the sorted order and cleared the sort";
            }
            self.last = last;
        }
        if self
            .message
            .as_ref()
            .is_some_and(|(_, _, at)| now.duration_since(*at) > Duration::from_secs(4))
        {
            self.message = None;
        }
    }

    fn act(&mut self, ax: Ax, act: Act) {
        let k = self.focused(ax);
        match act {
            Act::Hide => self.hide(ax, vec![k]),
            Act::ShowHidden => self.open_menu(ax, k, true),
            Act::Pin(side) => self.pin(ax, k, side, None),
            Act::TogglePinLead => match self.m(ax).side(k) {
                Some(Side::Lead) => self.unpin(ax, k),
                _ => self.pin(ax, k, Side::Lead, None),
            },
            Act::Unpin => {
                if self.m(ax).side(k).is_some() {
                    self.unpin(ax, k)
                } else {
                    let noun = self.m(ax).noun(1);
                    self.say(format!("The focused {noun} isn't pinned"), true);
                }
            }
            Act::Freeze => self.freeze(ax, k),
            Act::Menu => self.open_menu(ax, k, false),
            Act::Chooser => {
                self.popup = Popup::Chooser {
                    ax,
                    sel: 0,
                    search: String::new(),
                    searching: false,
                    by: 0,
                }
            }
            Act::Mode => self.mode = Some(ax),
        }
    }

    fn open_menu(&mut self, ax: Ax, key: usize, hidden: bool) {
        self.popup = match self.variant {
            Variant::B => Popup::WhichKey { ax, key, hidden },
            _ => Popup::Menu {
                ax,
                key,
                sel: 0,
                flyout: (hidden && self.variant == Variant::A).then_some(0),
            },
        };
        if hidden && self.variant == Variant::C {
            // C lists the hidden items inline: start the selection on the first.
            let first = (self.menu_items(ax, key).iter())
                .position(|i| matches!(i.1, MenuAct::Show(_)))
                .unwrap_or(0);
            if let Popup::Menu { sel, .. } = &mut self.popup {
                *sel = first;
            }
        }
    }

    // -------------------------------------------------------------- menus

    /// The menu's items: label, action and mnemonic.
    fn menu_items(&self, ax: Ax, k: usize) -> Vec<(String, MenuAct, char)> {
        let m = self.m(ax);
        let side = m.side(k);
        let mut items = vec![("Hide".to_string(), MenuAct::Hide, 'h')];
        let hidden: Vec<usize> = (m.order.iter().copied())
            .filter(|x| m.hidden.contains(x))
            .collect();
        if self.variant == Variant::A {
            let label = match hidden.len() {
                0 => "Show hidden (none)".to_string(),
                n => format!("Show hidden ▸ {n}"),
            };
            items.push((label, MenuAct::ShowHidden, 's'));
        }
        if side != Some(Side::Lead) {
            let label = format!("Pin {}", m.side_name(Side::Lead));
            items.push((label, MenuAct::Pin(Side::Lead), '<'));
        }
        if side != Some(Side::Trail) {
            let label = format!("Pin {}", m.side_name(Side::Trail));
            items.push((label, MenuAct::Pin(Side::Trail), '>'));
        }
        if side.is_some() {
            let clipped = self.clipped(ax, k);
            let label = format!("Unpin{}", if clipped { "  (clipped)" } else { "" });
            items.push((label, MenuAct::Unpin, 'u'));
        }
        if side != Some(Side::Trail) {
            items.push(("Freeze up to here".into(), MenuAct::Freeze, 'f'));
        }
        items.push((format!("Choose {}…", m.noun(2)), MenuAct::Chooser, 'c'));
        if self.variant == Variant::C {
            // C: the hidden items inline, numbered, after the actions.
            if !hidden.is_empty() {
                items.push((
                    format!("Show all {} hidden", hidden.len()),
                    MenuAct::ShowAll,
                    'a',
                ));
            }
            for (i, &x) in hidden.iter().enumerate() {
                let c = char::from_digit((i + 1) as u32 % 10, 10).unwrap_or(' ');
                items.push((format!("Show {}", m.labels[x]), MenuAct::Show(x), c));
            }
        }
        items
    }

    fn clipped(&self, ax: Ax, k: usize) -> bool {
        let a = self.allot(ax);
        match self.m(ax).side(k) {
            Some(Side::Lead) => a.lead_a < a.lead_w,
            Some(Side::Trail) => a.trail_a < a.trail_w,
            None => false,
        }
    }

    fn run_menu(&mut self, ax: Ax, k: usize, act: MenuAct) {
        self.popup = Popup::None;
        match act {
            MenuAct::Hide => self.hide(ax, vec![k]),
            MenuAct::ShowHidden => {
                self.popup = Popup::Menu {
                    ax,
                    key: k,
                    sel: 1,
                    flyout: Some(0),
                }
            }
            MenuAct::Pin(s) => self.pin(ax, k, s, None),
            MenuAct::Unpin => self.unpin(ax, k),
            MenuAct::Freeze => self.freeze(ax, k),
            MenuAct::Chooser => self.act(ax, Act::Chooser),
            MenuAct::ShowAll => {
                let all: Vec<usize> = self.m(ax).hidden.iter().copied().collect();
                self.show(ax, all)
            }
            MenuAct::Show(x) => self.show(ax, vec![x]),
        }
    }

    /// The hidden items, in the source's order.
    fn hidden(&self, ax: Ax) -> Vec<usize> {
        let m = self.m(ax);
        (m.order.iter().copied())
            .filter(|x| m.hidden.contains(x))
            .collect()
    }

    // -------------------------------------------------------------- choosers

    /// The chooser's rows: the items matching the search, in the variant's order.
    fn chooser_items(&self, ax: Ax, search: &str, by: usize) -> Vec<ChooserRow> {
        let m = self.m(ax);
        let q = search.to_lowercase();
        let matches = |k: &usize| m.labels[*k].to_lowercase().contains(&q);
        match self.variant {
            Variant::C => {
                let (_, scroll, _) = m.strips();
                let lead_name = format!("Pinned {}", m.side_name(Side::Lead));
                let trail_name = format!("Pinned {}", m.side_name(Side::Trail));
                let groups: [(String, Vec<usize>); 4] = [
                    (lead_name, m.lead.clone()),
                    ("Scrolling".into(), scroll),
                    (trail_name, m.trail.clone()),
                    (
                        "Hidden".into(),
                        (m.order.iter().copied())
                            .filter(|k| m.hidden.contains(k) && m.side(*k).is_none())
                            .collect(),
                    ),
                ];
                let mut out = Vec::new();
                for (name, ks) in groups {
                    let ks: Vec<usize> = ks.into_iter().filter(matches).collect();
                    out.push(ChooserRow::Section(format!("{name} ({})", ks.len())));
                    out.extend(ks.into_iter().map(ChooserRow::Item));
                }
                out
            }
            _ => {
                let mut ks: Vec<usize> = m.order.iter().copied().filter(matches).collect();
                if self.variant == Variant::B {
                    let rank = |k: &usize| match m.side(*k) {
                        Some(Side::Lead) => 0,
                        None => 1,
                        Some(Side::Trail) => 2,
                    };
                    match by {
                        1 => ks.sort_by_key(|k| m.labels[*k]),
                        2 => ks.sort_by_key(|k| m.hidden.contains(k)),
                        3 => ks.sort_by_key(rank),
                        4 => ks.sort_by_key(|k| std::cmp::Reverse(m.sizes[*k])),
                        _ => {}
                    }
                }
                let mut out = Vec::new();
                if self.variant == Variant::A {
                    out.push(ChooserRow::All);
                }
                out.extend(ks.into_iter().map(ChooserRow::Item));
                out
            }
        }
    }

    // -------------------------------------------------------------- keys

    fn key(&mut self, e: KeyEvent) {
        if e.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = e.modifiers.contains(KeyModifiers::CONTROL);
        // The prototype's own switches, apart from the presets.
        if ctrl {
            let handled = match e.code {
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    self.quit = true;
                    true
                }
                KeyCode::Char('v') => {
                    self.variant = match self.variant {
                        Variant::A => Variant::B,
                        Variant::B => Variant::C,
                        Variant::C => Variant::A,
                    };
                    self.popup = Popup::None;
                    self.last = format!("Switched to variant {:?}", self.variant);
                    true
                }
                KeyCode::Char('p') => {
                    self.preset = match self.preset {
                        Preset::Chords => Preset::Modes,
                        Preset::Modes => Preset::BareKeys,
                        Preset::BareKeys => Preset::Chords,
                    };
                    self.mode = None;
                    self.last = format!("Switched to the {:?} preset", self.preset);
                    true
                }
                KeyCode::Char('b') => {
                    self.border = (self.border + 1) % BORDERS.len();
                    self.last = format!("Border Style {}", BORDERS[self.border].0);
                    true
                }
                KeyCode::Char('t') => {
                    self.theme = (self.theme + 1) % 3;
                    true
                }
                KeyCode::Char('e') | KeyCode::Char('y') => {
                    let ax = if e.code == KeyCode::Char('e') {
                        Ax::Cols
                    } else {
                        Ax::Rows
                    };
                    let m = self.m_mut(ax);
                    m.sort = match m.sort {
                        Sort::None => Sort::Bake,
                        Sort::Bake => Sort::Withdraw,
                        Sort::Withdraw => Sort::None,
                    };
                    self.last = format!("{ax:?} sort: {:?}", self.m(ax).sort);
                    true
                }
                KeyCode::Char('r') => {
                    self.cols.refuse_next = !self.cols.refuse_next;
                    self.rows.refuse_next = self.cols.refuse_next;
                    true
                }
                KeyCode::Char('s') => {
                    self.show_state = !self.show_state;
                    true
                }
                KeyCode::Char('x') => {
                    let (_, scroll, _) = self.cols.strips();
                    for &k in scroll.iter().take(6) {
                        self.pin(Ax::Cols, k, Side::Lead, None);
                    }
                    for &k in scroll.iter().rev().take(2) {
                        self.pin(Ax::Cols, k, Side::Trail, None);
                    }
                    self.last = "Pinned 6 more left and 2 more right, past what fits".into();
                    true
                }
                _ => false,
            };
            if handled {
                return;
            }
        }
        if e.code == KeyCode::F(12) {
            return self.key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));
        }
        if self.popup != Popup::None {
            self.popup_key(e);
            return;
        }
        if let Some(ax) = self.mode {
            match e.code {
                KeyCode::Esc => {
                    self.mode = None;
                    return;
                }
                KeyCode::Tab => {
                    self.mode = Some(match ax {
                        Ax::Cols => Ax::Rows,
                        Ax::Rows => Ax::Cols,
                    });
                    return;
                }
                KeyCode::Char(c) => {
                    if let Some(act) = mode_binding(c) {
                        self.act(ax, act);
                        return;
                    }
                }
                _ => {}
            }
        }
        let pressed = chord(e.code, e.modifiers);
        let found = (bindings(self.preset).into_iter())
            .find(|&(c, m, _, _)| chord(c, m) == pressed)
            .map(|(_, _, ax, act)| (ax, act));
        if let Some((ax, act)) = found {
            self.act(ax, act);
            return;
        }
        match e.code {
            KeyCode::Left => self.step(Ax::Cols, false),
            KeyCode::Right => self.step(Ax::Cols, true),
            KeyCode::Up => self.step(Ax::Rows, false),
            KeyCode::Down => self.step(Ax::Rows, true),
            KeyCode::Char('q') if self.preset != Preset::Chords => self.quit = true,
            _ => {}
        }
    }

    /// Arrow keys follow drawn order across pinned and scrolling items (#137).
    fn step(&mut self, ax: Ax, forward: bool) {
        let (lead, scroll, trail) = self.m(ax).strips();
        let drawn: Vec<usize> = lead.iter().chain(&scroll).chain(&trail).copied().collect();
        let f = self.focused(ax);
        let at = drawn.iter().position(|&k| k == f).unwrap_or(0);
        let to = if forward {
            (at + 1).min(drawn.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.set_focus(ax, drawn[to]);
        self.reveal(ax);
    }

    /// Scrolls the scrolling Strip so the focused item, if in it, is in view.
    fn reveal(&mut self, ax: Ax) {
        let a = self.allot(ax);
        let f = self.focused(ax);
        let Some(i) = a.scroll.iter().position(|&k| k == f) else {
            return;
        };
        let body = match ax {
            Ax::Cols => self.geo.body_w,
            Ax::Rows => self.geo.body_h,
        };
        let room = body.saturating_sub(a.lead_a + a.trail_a);
        let m = self.m_mut(ax);
        if i < m.scroll {
            m.scroll = i;
        }
        while m.scroll < i && m.total(&a.scroll[m.scroll..=i]) > room {
            m.scroll += 1;
        }
    }

    fn popup_key(&mut self, e: KeyEvent) {
        match self.popup.clone() {
            Popup::None => {}
            Popup::Menu {
                ax,
                key,
                sel,
                flyout,
            } => {
                let items = self.menu_items(ax, key);
                if let Some(f) = flyout {
                    let hidden = self.hidden(ax);
                    let n = hidden.len() + 1;
                    match e.code {
                        KeyCode::Esc | KeyCode::Left => {
                            self.popup = Popup::Menu {
                                ax,
                                key,
                                sel,
                                flyout: None,
                            }
                        }
                        KeyCode::Up | KeyCode::Down => {
                            let f = if e.code == KeyCode::Up {
                                (f + n - 1) % n
                            } else {
                                (f + 1) % n
                            };
                            self.popup = Popup::Menu {
                                ax,
                                key,
                                sel,
                                flyout: Some(f),
                            };
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            self.popup = Popup::None;
                            if f == 0 {
                                self.show(ax, hidden);
                            } else {
                                self.show(ax, vec![hidden[f - 1]]);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                match e.code {
                    KeyCode::Esc => self.popup = Popup::None,
                    KeyCode::Up | KeyCode::Down => {
                        let n = items.len();
                        let sel = if e.code == KeyCode::Up {
                            (sel + n - 1) % n
                        } else {
                            (sel + 1) % n
                        };
                        self.popup = Popup::Menu {
                            ax,
                            key,
                            sel,
                            flyout: None,
                        };
                    }
                    KeyCode::Right if items[sel].1 == MenuAct::ShowHidden => {
                        self.run_menu(ax, key, MenuAct::ShowHidden)
                    }
                    KeyCode::Enter => self.run_menu(ax, key, items[sel].1),
                    KeyCode::Char(c) => {
                        if let Some(item) = items.iter().find(|i| i.2 == c) {
                            self.run_menu(ax, key, item.1);
                        }
                    }
                    _ => {}
                }
            }
            Popup::WhichKey { ax, key, hidden } => {
                let KeyCode::Char(c) = e.code else {
                    self.popup = Popup::None;
                    return;
                };
                if hidden {
                    let list = self.hidden(ax);
                    self.popup = Popup::None;
                    if c == 'a' {
                        self.show(ax, list);
                    } else if let Some(d) = c.to_digit(10) {
                        let i = (d as usize + 9) % 10;
                        if let Some(&x) = list.get(i) {
                            self.show(ax, vec![x]);
                        }
                    }
                    return;
                }
                if c == 's' {
                    self.popup = Popup::WhichKey {
                        ax,
                        key,
                        hidden: true,
                    };
                    return;
                }
                let items = self.menu_items(ax, key);
                match items.iter().find(|i| i.2 == c) {
                    Some(item) => self.run_menu(ax, key, item.1),
                    None => self.popup = Popup::None,
                }
            }
            Popup::Chooser {
                ax,
                sel,
                mut search,
                mut searching,
                mut by,
            } => {
                let rows = self.chooser_items(ax, &search, by);
                let n = rows.len().max(1);
                let mut sel = sel.min(n - 1);
                let item = match rows.get(sel) {
                    Some(ChooserRow::Item(k)) => Some(*k),
                    _ => None,
                };
                // A types into the search at once; B and C take `/` first, as their letters act.
                let typing = self.variant == Variant::A || searching;
                match e.code {
                    KeyCode::Esc if searching && self.variant != Variant::A => searching = false,
                    KeyCode::Esc | KeyCode::Enter if !searching || self.variant == Variant::A => {
                        self.popup = Popup::None;
                        return;
                    }
                    KeyCode::Enter => searching = false,
                    KeyCode::Up => sel = (sel + n - 1) % n,
                    KeyCode::Down => sel = (sel + 1) % n,
                    KeyCode::Backspace if typing => {
                        search.pop();
                    }
                    KeyCode::Char(' ') => match rows.get(sel) {
                        Some(ChooserRow::Item(k)) => {
                            let k = *k;
                            if self.m(ax).hidden.contains(&k) {
                                self.show(ax, vec![k])
                            } else {
                                self.hide(ax, vec![k])
                            }
                        }
                        Some(ChooserRow::All) => {
                            let ks: Vec<usize> = (rows.iter())
                                .filter_map(|r| match r {
                                    ChooserRow::Item(k) => Some(*k),
                                    _ => None,
                                })
                                .collect();
                            if ks.iter().all(|k| !self.m(ax).hidden.contains(k)) {
                                self.hide(ax, ks)
                            } else {
                                self.show(ax, ks)
                            }
                        }
                        _ => {}
                    },
                    KeyCode::Char(c) if typing => search.push(c),
                    KeyCode::Char('/') => searching = true,
                    KeyCode::Char('<') if item.is_some() => {
                        self.pin(ax, item.unwrap(), Side::Lead, None)
                    }
                    KeyCode::Char('>') if item.is_some() => {
                        self.pin(ax, item.unwrap(), Side::Trail, None)
                    }
                    KeyCode::Char('u') if item.is_some() => self.unpin(ax, item.unwrap()),
                    KeyCode::Char('+') | KeyCode::Char('-') if item.is_some() && ax == Ax::Cols => {
                        let k = item.unwrap();
                        let w = &mut self.cols.sizes[k];
                        *w = if e.code == KeyCode::Char('+') {
                            *w + 1
                        } else {
                            w.saturating_sub(1).max(2)
                        };
                        self.last = format!("Set the width of \"{}\"", COLS[k]);
                    }
                    KeyCode::Tab if self.variant == Variant::B => by = (by + 1) % 5,
                    _ => {}
                }
                if typing && matches!(e.code, KeyCode::Char(_) | KeyCode::Backspace) {
                    sel = 0;
                }
                if matches!(self.popup, Popup::Chooser { .. }) {
                    self.popup = Popup::Chooser {
                        ax,
                        sel,
                        search,
                        searching,
                        by,
                    };
                }
            }
        }
    }

    // -------------------------------------------------------------- mouse and dragging

    fn mouse(&mut self, e: MouseEvent) {
        let (x, y) = (e.column, e.row);
        match e.kind {
            MouseEventKind::Down(button) => {
                let hit = self.header_at(x, y);
                if button == MouseButton::Right {
                    if let Some((ax, k)) = hit {
                        self.open_menu(ax, k, false);
                    }
                    return;
                }
                self.popup = Popup::None;
                if let Some((ax, key)) = hit {
                    self.drag = Some(Drag {
                        ax,
                        key,
                        from: (x, y),
                        active: false,
                        drop: None,
                    });
                } else if let Some((r, c)) = self.cell_at(x, y) {
                    self.focus = (r, c);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(d) = &self.drag else { return };
                let (ax, key, from) = (d.ax, d.key, d.from);
                let moved = from.0.abs_diff(x) + from.1.abs_diff(y) > 1;
                let p = if ax == Ax::Cols { x } else { y };
                let drop = self.drop_at(ax, key, p);
                if let Some(d) = &mut self.drag {
                    d.active |= moved;
                    d.drop = Some(drop);
                }
            }
            MouseEventKind::Up(_) => {
                let Some(d) = self.drag.take() else { return };
                if !d.active {
                    self.set_focus(d.ax, d.key);
                    return;
                }
                if let Some(drop) = d.drop {
                    self.apply_drop(d.ax, d.key, drop);
                }
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let ax = if e.modifiers.contains(KeyModifiers::SHIFT) {
                    Ax::Cols
                } else {
                    Ax::Rows
                };
                let len = self.allot(ax).scroll.len();
                let m = self.m_mut(ax);
                m.scroll = if e.kind == MouseEventKind::ScrollDown {
                    (m.scroll + 1).min(len.saturating_sub(1))
                } else {
                    m.scroll.saturating_sub(1)
                };
            }
            _ => {}
        }
    }

    fn header_at(&self, x: u16, y: u16) -> Option<(Ax, usize)> {
        let (x, y) = (x as i32, y as i32);
        if y == self.geo.head_y as i32 && x >= self.geo.body_x as i32 {
            let hit = self.geo.cols.iter().find(|c| c.0 <= x && x < c.1);
            return hit.map(|c| (Ax::Cols, c.2));
        }
        if x < (self.geo.grid.x + RN_W) as i32 {
            let hit = self.geo.rows.iter().find(|r| r.0 <= y && y < r.1);
            return hit.map(|r| (Ax::Rows, r.2));
        }
        None
    }

    fn cell_at(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        let (x, y) = (x as i32, y as i32);
        let c = self.geo.cols.iter().find(|c| c.0 <= x && x < c.1)?;
        let r = self.geo.rows.iter().find(|r| r.0 <= y && y < r.1)?;
        Some((r.2, c.2))
    }

    /// What dropping header `k` at cell `p` along its axis would do (#136).
    fn drop_at(&self, ax: Ax, k: usize, p: u16) -> Drop {
        let g = &self.geo;
        let (start, len) = match ax {
            Ax::Cols => (g.body_x, g.body_w),
            Ax::Rows => (g.body_y, g.body_h),
        };
        let end = start + len;
        let edge = if ax == Ax::Rows { self.row_edge() } else { 0 };
        let a = self.allot(ax);
        let m = self.m(ax);
        let lead_end = start + a.lead_a;
        let trail_start = end - a.trail_a;
        let src = m.side(k);
        // The slot `p` falls in, and the cell its line goes on.
        let slot = |list: &[usize], origin: i32| -> (usize, i32) {
            let mut x = origin;
            for (i, &item) in list.iter().enumerate() {
                let w = m.size(item) as i32;
                if (p as i32) < x + w / 2 + 1 {
                    return (i, x);
                }
                x += w;
            }
            (list.len(), x)
        };
        let near = 2;
        let zone = if p < lead_end || (a.lead_a == 0 && p < start + near) {
            Some(Side::Lead)
        } else if p >= trail_start || (a.trail_a == 0 && p + near >= end) {
            Some(Side::Trail)
        } else {
            None
        };
        let clamp =
            |x: i32, lo: u16, hi: u16| x.clamp(lo as i32, hi.saturating_sub(1) as i32) as u16;
        // A column's line goes in the Gap before it; rows have no Gap, so theirs marks the row itself.
        let g1 = u16::from(ax == Ax::Cols);
        if let Some(side) = zone {
            let (list, origin) = match side {
                Side::Lead => (&a.lead, start as i32 - a.lead_off as i32),
                Side::Trail => (&a.trail, (trail_start + edge) as i32 - a.trail_off as i32),
            };
            let (index, line) = slot(list, origin);
            let line = if list.is_empty() {
                if side == Side::Lead {
                    start - g1
                } else {
                    end - 1
                }
            } else if side == Side::Lead {
                clamp(line - g1 as i32, start - g1, end)
            } else {
                clamp(line, start, end)
            };
            let label = if src == Some(side) {
                "Reorder the pins".to_string()
            } else {
                format!("Pin {}", m.side_name(side))
            };
            return Drop {
                kind: DropKind::Pin,
                side: Some(side),
                index,
                before: None,
                line: Some(line),
                label,
                refused: false,
            };
        }
        let origin = lead_end as i32 - m.total(&a.scroll[..m.scroll.min(a.scroll.len())]) as i32;
        let (index, line) = slot(&a.scroll, origin);
        let line = Some(clamp(line - g1 as i32, lead_end - g1, trail_start));
        let before = a.scroll.get(index).copied();
        let none = |label: String, refused: bool| Drop {
            kind: DropKind::None,
            side: None,
            index: 0,
            before: None,
            line: None,
            label,
            refused,
        };
        match (src, m.movable()) {
            (None, false) => none(
                format!(
                    "{} can't move while sorted: drag to an edge to pin",
                    cap(m.noun(2))
                ),
                true,
            ),
            (None, true) => {
                if before == Some(k) || index > 0 && a.scroll.get(index - 1) == Some(&k) {
                    return none("Its own place".into(), false);
                }
                Drop {
                    kind: DropKind::Move,
                    side: None,
                    index,
                    before,
                    line,
                    label: "Move here".into(),
                    refused: false,
                }
            }
            (Some(_), true) => Drop {
                kind: DropKind::UnpinMove,
                side: None,
                index,
                before,
                line,
                label: "Unpin and move here".into(),
                refused: false,
            },
            (Some(_), false) => {
                // The item returns to its own Position: the line shows where that is.
                let pos = m.position(k).unwrap_or(0);
                let own = (a.scroll.iter())
                    .filter(|&&x| m.position(x).is_some_and(|p| p < pos))
                    .count();
                let at = origin + m.total(&a.scroll[..own]) as i32;
                let off = at < lead_end as i32 || at >= trail_start as i32;
                Drop {
                    kind: DropKind::UnpinReturn,
                    side: None,
                    index: own,
                    before: None,
                    line: Some(clamp(at - g1 as i32, lead_end - g1, trail_start)),
                    label: format!(
                        "Unpin: returns to its place{}",
                        if off { " (off screen)" } else { "" }
                    ),
                    refused: false,
                }
            }
        }
    }

    fn apply_drop(&mut self, ax: Ax, k: usize, d: Drop) {
        match d.kind {
            DropKind::Pin => self.pin(ax, k, d.side.unwrap(), Some(d.index)),
            DropKind::UnpinReturn => self.unpin(ax, k),
            DropKind::UnpinMove => {
                self.unpin(ax, k);
                self.request_move(ax, k, d.before);
            }
            DropKind::Move => self.request_move(ax, k, d.before),
            DropKind::None if d.refused => self.say(d.label, true),
            DropKind::None => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuAct {
    Hide,
    ShowHidden,
    Pin(Side),
    Unpin,
    Freeze,
    Chooser,
    ShowAll,
    Show(usize),
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum ChooserRow {
    All,
    Section(String),
    Item(usize),
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

// ------------------------------------------------------------------ drawing

/// Writes `text` from virtual column `x`, keeping only the cells inside `clip`.
fn put(buf: &mut Buffer, x: i32, y: u16, text: &str, style: Style, clip: (u16, u16)) {
    let area = buf.area;
    if y < area.y || y >= area.bottom() {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        let cx = x + i as i32;
        if cx < clip.0 as i32
            || cx >= clip.1 as i32
            || cx < area.x as i32
            || cx >= area.right() as i32
        {
            continue;
        }
        buf[(cx as u16, y)].set_char(ch).set_style(style);
    }
}

fn fill(buf: &mut Buffer, r: Rect, style: Style) {
    let r = r.intersection(buf.area);
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            buf[(x, y)].set_char(' ').set_style(style);
        }
    }
}

fn fit(text: &str, w: u16) -> String {
    let w = w as usize;
    let n = text.chars().count();
    if n <= w {
        format!("{text:<w$}")
    } else if w == 0 {
        String::new()
    } else {
        text.chars().take(w - 1).collect::<String>() + "…"
    }
}

/// A tint for pinned cells (variant C without vertical rules).
fn tint(theme: &Theme) -> Style {
    match theme.name {
        "catppuccin_mocha" => Style::default().bg(Color::Rgb(17, 17, 27)),
        "mono" => Style::default().add_modifier(Modifier::DIM),
        _ => Style::default().bg(Color::Indexed(236)),
    }
}

fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme();
    fill(buf, area, theme.body);
    let state_w = if app.show_state && area.width > 110 {
        46
    } else {
        0
    };
    let main = Rect::new(area.x, area.y, area.width - state_w, area.height);
    // The harness line: the prototype's own switches.
    let harness = format!(
        " PROTOTYPE #140  Ctrl+V variant {:?}  Ctrl+P preset {:?}  Ctrl+B border {}  Ctrl+T theme  Ctrl+E/Y sort cols/rows  Ctrl+R refuse  Ctrl+X clip  Ctrl+S state  Ctrl+Q quit ",
        app.variant, app.preset, BORDERS[app.border].0
    );
    let harness_style = Style::default().fg(Color::Black).bg(Color::Yellow);
    fill(buf, Rect::new(area.x, area.y, area.width, 1), harness_style);
    put(
        buf,
        area.x as i32,
        area.y,
        &harness,
        harness_style,
        (area.x, area.right()),
    );
    let mut y = area.y + 1;
    if app.variant == Variant::B {
        // B: a bar above the grid, as the filter bar, holding the counts.
        let counts = format!("{}  {} ", app.cols.count(), app.rows.count());
        put(
            buf,
            main.x as i32 + 1,
            y,
            "Filter rows…",
            theme.loading,
            (main.x, main.right()),
        );
        let x = main.right() as i32 - counts.chars().count() as i32;
        put(buf, x, y, &counts, theme.header, (main.x, main.right()));
        y += 1;
    }
    let grid = Rect::new(main.x, y, main.width, main.bottom().saturating_sub(y + 3));
    draw_grid(app, &theme, grid, buf);
    draw_keys_line(
        app,
        &theme,
        Rect::new(main.x, main.bottom() - 3, main.width, 2),
        buf,
    );
    draw_status(
        app,
        &theme,
        Rect::new(main.x, main.bottom() - 1, main.width, 1),
        buf,
    );
    draw_popup(app, &theme, grid, buf);
    if state_w > 0 {
        draw_state(
            app,
            Rect::new(main.right(), area.y + 1, state_w, area.height - 1),
            buf,
        );
    }
}

fn draw_grid(app: &mut App, theme: &Theme, g: Rect, buf: &mut Buffer) {
    let style = app.style();
    let band_rule = style.draws_band_rules();
    let vrules = matches!(style.extent, BorderExtent::Columns | BorderExtent::Grid);
    let closing = style.extent == BorderExtent::Grid;
    let double = style.weight == BorderWeight::Double;
    let (vert, horiz) = if double {
        ('║', '═')
    } else {
        ('│', '─')
    };
    let head_h = 1 + u16::from(band_rule);
    app.geo = Geometry {
        grid: g,
        body_x: g.x + RN_W + 1,
        body_y: g.y + head_h,
        body_w: g.width.saturating_sub(RN_W + 1),
        body_h: g.height.saturating_sub(head_h + u16::from(closing)),
        cols: Vec::new(),
        rows: Vec::new(),
        head_y: g.y,
    };
    let geo = app.geo.clone();
    let ca = app.allot(Ax::Cols);
    let ra = app.allot(Ax::Rows);
    let row_edge = app.row_edge();

    // Each drawn column: (content x, width, gap x, clip, key, zone).
    struct Col {
        x: i32,
        w: u16,
        gap: i32,
        clip: (u16, u16),
        key: usize,
        pinned: bool,
    }
    let mut cols: Vec<Col> = Vec::new();
    let bx = geo.body_x;
    let bend = bx + geo.body_w;
    let lead_clip = (bx, bx + ca.lead_a);
    let mut x = bx as i32 - ca.lead_off as i32;
    for &k in &ca.lead {
        let w = app.cols.sizes[k];
        cols.push(Col {
            x,
            w,
            gap: x + w as i32,
            clip: lead_clip,
            key: k,
            pinned: true,
        });
        x += w as i32 + 1;
    }
    let scroll_clip = (bx + ca.lead_a, bend - ca.trail_a);
    let mut x = scroll_clip.0 as i32;
    for &k in ca.scroll.iter().skip(app.cols.scroll) {
        if x >= scroll_clip.1 as i32 {
            break;
        }
        let w = app.cols.sizes[k];
        cols.push(Col {
            x,
            w,
            gap: x + w as i32,
            clip: scroll_clip,
            key: k,
            pinned: false,
        });
        x += w as i32 + 1;
    }
    let trail_clip = (bend - ca.trail_a, bend);
    let mut x = trail_clip.0 as i32 - ca.trail_off as i32;
    for &k in &ca.trail {
        let w = app.cols.sizes[k];
        cols.push(Col {
            x: x + 1,
            w,
            gap: x,
            clip: trail_clip,
            key: k,
            pinned: true,
        });
        x += w as i32 + 1;
    }

    // Each drawn row: (y, clip, key, pinned).
    let by = geo.body_y;
    let bbot = by + geo.body_h;
    let mut rows: Vec<(i32, (u16, u16), usize, bool)> = Vec::new();
    let lead_edge_y = (row_edge > 0 && !ra.lead.is_empty()).then(|| by + ra.lead_a - 1);
    let trail_edge_y = (row_edge > 0 && !ra.trail.is_empty()).then(|| bbot - ra.trail_a);
    let rlead_clip = (by, by + ra.lead_a - u16::from(lead_edge_y.is_some()));
    let mut y = by as i32 - ra.lead_off as i32;
    for &k in &ra.lead {
        rows.push((y, rlead_clip, k, true));
        y += 1;
    }
    let rscroll_clip = (by + ra.lead_a, bbot - ra.trail_a);
    let mut y = rscroll_clip.0 as i32;
    for &k in ra.scroll.iter().skip(app.rows.scroll) {
        if y >= rscroll_clip.1 as i32 {
            break;
        }
        rows.push((y, rscroll_clip, k, false));
        y += 1;
    }
    let rtrail_clip = (bbot - ra.trail_a + u16::from(trail_edge_y.is_some()), bbot);
    let mut y = rtrail_clip.0 as i32 - ra.trail_off as i32;
    for &k in &ra.trail {
        rows.push((y, rtrail_clip, k, true));
        y += 1;
    }

    let tinted = app.variant == Variant::C && !vrules;
    let pending_col = app.cols.pending.as_ref().map(|p| p.key);
    let pending_row = app.rows.pending.as_ref().map(|p| p.key);
    let shimmer = Style::default().bg(theme.shimmer_base);

    // The header band: corner, column headers, handles.
    let corner = if app.variant == Variant::C {
        format!("{}/{}", app.cols.shown().len(), COLS.len())
    } else {
        String::new()
    };
    put(
        buf,
        g.x as i32,
        g.y,
        &fit(&corner, RN_W),
        theme.row_number,
        (g.x, g.right()),
    );
    if style.extent != BorderExtent::None {
        put(
            buf,
            (g.x + RN_W) as i32,
            g.y,
            &vert.to_string(),
            theme.handle,
            (g.x, g.right()),
        );
    }
    for c in &cols {
        let mut s = theme.header;
        if c.key == app.focus.1 {
            s = s.patch(theme.header_hovered);
        }
        if Some(c.key) == pending_col {
            s = s.patch(shimmer);
        }
        if tinted && c.pinned {
            s = s.patch(tint(theme));
        }
        let mut label = COLS[c.key].to_string();
        if c.key == 0 && app.rows.sort != Sort::None {
            label = format!("{label} ▲");
        }
        put(buf, c.x, g.y, &fit(&label, c.w), s, c.clip);
        geo_push(
            &mut app.geo.cols,
            c.x.max(c.clip.0 as i32),
            (c.x + c.w as i32).min(c.clip.1 as i32),
            c.key,
        );
        if style.extent != BorderExtent::None {
            put(buf, c.gap, g.y, &vert.to_string(), theme.handle, c.clip);
        }
    }
    // The band rule under the header, with its crossings.
    if band_rule {
        let rule: String = std::iter::repeat_n(horiz, g.width as usize).collect();
        put(
            buf,
            g.x as i32,
            g.y + 1,
            &rule,
            theme.border,
            (g.x, g.right()),
        );
        let cross = match (vrules, double) {
            (true, false) => "┼",
            (true, true) => "╬",
            (false, false) => "┴",
            (false, true) => "╩",
        };
        put(
            buf,
            (g.x + RN_W) as i32,
            g.y + 1,
            cross,
            theme.border,
            (g.x, g.right()),
        );
        for c in &cols {
            put(buf, c.gap, g.y + 1, cross, theme.border, c.clip);
        }
    }
    // The row-number band and the body.
    for &(y, clip, k, pinned) in &rows {
        if y < clip.0 as i32 || y >= clip.1 as i32 {
            continue;
        }
        let y = y as u16;
        app.geo.rows.push((y as i32, y as i32 + 1, k));
        let pos = app.rows.position(k).unwrap_or(0);
        let mut rn = theme.row_number;
        if k == app.focus.0 {
            rn = rn.patch(theme.header_hovered);
        }
        if Some(k) == pending_row {
            rn = rn.patch(shimmer);
        }
        let mark = if k == app.rows.order[0] && app.cols.sort != Sort::None {
            "◂"
        } else {
            " "
        };
        let num = format!("{mark}{:>w$}", pos + 1, w = RN_W as usize - 1);
        put(buf, g.x as i32, y, &num, rn, (g.x, g.right()));
        if band_rule {
            put(
                buf,
                (g.x + RN_W) as i32,
                y,
                &vert.to_string(),
                theme.border,
                (g.x, g.right()),
            );
        }
        let zebra = pos % 2 == 1;
        for c in &cols {
            let mut s = theme.body;
            if zebra && let Some(z) = theme.zebra {
                s = s.patch(z);
            }
            if tinted && (c.pinned || pinned) {
                s = s.patch(tint(theme));
            }
            if app.variant == Variant::B {
                // B's mark for a pinned row edge: an underline on the last top row, an overline being out of reach.
                if ra.lead.last() == Some(&k) && pinned {
                    s = s.add_modifier(Modifier::UNDERLINED);
                }
            }
            if (k, c.key) == app.focus {
                s = s.patch(theme.focus);
            }
            let v = value(k, c.key);
            let text = if v.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
                format!("{v:>w$}", w = c.w as usize)
            } else {
                fit(&v, c.w)
            };
            put(buf, c.x, y, &fit(&text, c.w), s, c.clip);
            if vrules {
                put(buf, c.gap, y, &vert.to_string(), theme.border, c.clip);
            }
        }
    }
    // Grid's closing rule.
    if closing {
        let yb = bbot;
        let rule: String = std::iter::repeat_n(horiz, g.width as usize).collect();
        put(buf, g.x as i32, yb, &rule, theme.border, (g.x, g.right()));
        for c in &cols {
            put(
                buf,
                c.gap,
                yb,
                if double { "╩" } else { "┴" },
                theme.border,
                c.clip,
            );
        }
    }

    // The pinned column edges.
    let edges: Vec<u16> = [
        (!ca.lead.is_empty()).then(|| bx + ca.lead_a - 1),
        (!ca.trail.is_empty()).then(|| bend - ca.trail_a),
    ]
    .into_iter()
    .flatten()
    .collect();
    let edge_style = theme
        .border
        .patch(Style::default().add_modifier(Modifier::BOLD));
    // Under a double Border Style every rule is already ║: the edge goes heavy instead.
    let edge = if double { '┃' } else { '║' };
    for &ex in &edges {
        let column = |buf: &mut Buffer, y: u16, ch: char| {
            put(
                buf,
                ex as i32,
                y,
                &ch.to_string(),
                edge_style,
                (g.x, g.right()),
            )
        };
        let body_glyph = match app.variant {
            Variant::A => Some(edge),
            Variant::B if vrules => Some(edge),
            Variant::B => Some('┊'),
            Variant::C if vrules => Some(edge),
            Variant::C => None,
        };
        if let Some(ch) = body_glyph {
            for y in by..bbot {
                column(buf, y, ch);
            }
        }
        match app.variant {
            Variant::A => {
                if band_rule {
                    column(buf, g.y + 1, if double { '┰' } else { '╥' });
                }
            }
            Variant::B => {
                column(buf, g.y, body_glyph.unwrap_or(edge));
                if band_rule {
                    column(buf, g.y + 1, if double { '╋' } else { '╫' });
                }
            }
            Variant::C => {}
        }
        if closing && body_glyph.is_some() {
            column(buf, bbot, if double { '┸' } else { '╨' });
        }
        // Where a row edge line crosses the column edge.
        for ey in [lead_edge_y, trail_edge_y].into_iter().flatten() {
            column(buf, ey, '╬');
        }
    }
    // The pinned row edges (variant A): a double rule, costing a line.
    for ey in [lead_edge_y, trail_edge_y].into_iter().flatten() {
        let rule: String = std::iter::repeat_n('═', g.width as usize).collect();
        put(buf, g.x as i32, ey, &rule, edge_style, (g.x, g.right()));
        for c in &cols {
            if vrules && !edges.contains(&(c.gap.max(0) as u16)) {
                put(buf, c.gap, ey, "╪", edge_style, c.clip);
            }
        }
        if band_rule {
            put(
                buf,
                (g.x + RN_W) as i32,
                ey,
                "╪",
                edge_style,
                (g.x, g.right()),
            );
        }
        for &ex in &edges {
            put(buf, ex as i32, ey, "╬", edge_style, (g.x, g.right()));
        }
    }

    // A clipped pinned Strip: a mark at its inside edge, in the header (columns) or the row-number band (rows).
    let mark = theme.loading.add_modifier(Modifier::BOLD);
    if ca.lead_a < ca.lead_w {
        put(
            buf,
            (bx + ca.lead_a - 2) as i32,
            g.y,
            "›",
            mark,
            (g.x, g.right()),
        );
    }
    if ca.trail_a < ca.trail_w {
        put(
            buf,
            (bend - ca.trail_a + 1) as i32,
            g.y,
            "‹",
            mark,
            (g.x, g.right()),
        );
    }
    if ra.lead_a < ra.lead_w {
        put(
            buf,
            g.x as i32,
            rlead_clip.1.saturating_sub(1),
            "▾",
            mark,
            (g.x, g.right()),
        );
    }
    if ra.trail_a < ra.trail_w {
        put(buf, g.x as i32, rtrail_clip.0, "▴", mark, (g.x, g.right()));
    }

    // Drag feedback: a drop line in the drop indicator's style.
    if let Some(Drag {
        ax,
        active: true,
        drop: Some(d),
        ..
    }) = &app.drag
        && let Some(line) = d.line
    {
        let ch = match (ax, d.kind) {
            (Ax::Cols, DropKind::UnpinReturn) => "┇",
            (Ax::Cols, _) => "┃",
            (Ax::Rows, _) => "━",
        };
        match ax {
            Ax::Cols => {
                for y in g.y..bbot {
                    put(
                        buf,
                        line as i32,
                        y,
                        ch,
                        theme.drop_indicator,
                        (g.x, g.right()),
                    );
                }
            }
            Ax::Rows => {
                // Rows have no Gap to draw in: the line runs through the row-number band, before the row it lands above.
                let arrow = if d.kind == DropKind::UnpinReturn {
                    "┅┅┅┅▶"
                } else {
                    "━━━━▶"
                };
                put(
                    buf,
                    g.x as i32,
                    line,
                    arrow,
                    theme.drop_indicator,
                    (g.x, g.right()),
                );
            }
        }
    }
}

fn geo_push(v: &mut Vec<(i32, i32, usize)>, a: i32, b: i32, k: usize) {
    if b > a {
        v.push((a, b, k));
    }
}

/// The current preset's keys for the presentation controls.
fn keys_text(app: &App) -> String {
    let b = bindings(app.preset);
    let name = |ax: Ax, act: Act| -> String {
        (b.iter())
            .filter(|x| x.2 == ax && x.3 == act)
            .map(|x| key_name(x.0, x.1))
            .collect::<Vec<_>>()
            .join(" or ")
    };
    match app.preset {
        Preset::Chords => format!(
            "hide {}/{}  hidden {}/{}  pin {} {} (rows {} {})  unpin {}/{}  freeze {}/{}  menu {} (rows {})  chooser {}/{}",
            name(Ax::Cols, Act::Hide),
            name(Ax::Rows, Act::Hide),
            name(Ax::Cols, Act::ShowHidden),
            name(Ax::Rows, Act::ShowHidden),
            name(Ax::Cols, Act::Pin(Side::Lead)),
            name(Ax::Cols, Act::Pin(Side::Trail)),
            name(Ax::Rows, Act::Pin(Side::Lead)),
            name(Ax::Rows, Act::Pin(Side::Trail)),
            name(Ax::Cols, Act::Unpin),
            name(Ax::Rows, Act::Unpin),
            name(Ax::Cols, Act::Freeze),
            name(Ax::Rows, Act::Freeze),
            name(Ax::Cols, Act::Menu),
            name(Ax::Rows, Act::Menu),
            name(Ax::Cols, Act::Chooser),
            name(Ax::Rows, Act::Chooser),
        ),
        Preset::Modes => match app.mode {
            Some(ax) => format!(
                "-- {} --  h hide  s hidden  < pin {}  > pin {}  u unpin  f freeze  m menu  c chooser  Tab other axis  Esc leave",
                if ax == Ax::Cols { "COLUMNS" } else { "ROWS" },
                app.m(ax).side_name(Side::Lead),
                app.m(ax).side_name(Side::Trail),
            ),
            None => {
                "z columns mode  Z rows mode  - hide column  ! pin left or unpin  C columns sheet"
                    .into()
            }
        },
        Preset::BareKeys => format!(
            "hide {}/{}  hidden {}/{}  pin {} {} (rows {} {})  unpin {}/{}  freeze {}/{}  menu {}/{}  chooser {}/{}",
            name(Ax::Cols, Act::Hide),
            name(Ax::Rows, Act::Hide),
            name(Ax::Cols, Act::ShowHidden),
            name(Ax::Rows, Act::ShowHidden),
            name(Ax::Cols, Act::Pin(Side::Lead)),
            name(Ax::Cols, Act::Pin(Side::Trail)),
            name(Ax::Rows, Act::Pin(Side::Lead)),
            name(Ax::Rows, Act::Pin(Side::Trail)),
            name(Ax::Cols, Act::Unpin),
            name(Ax::Rows, Act::Unpin),
            name(Ax::Cols, Act::Freeze),
            name(Ax::Rows, Act::Freeze),
            name(Ax::Cols, Act::Menu),
            name(Ax::Rows, Act::Menu),
            name(Ax::Cols, Act::Chooser),
            name(Ax::Rows, Act::Chooser),
        ),
    }
}

fn draw_keys_line(app: &App, theme: &Theme, r: Rect, buf: &mut Buffer) {
    let s = if app.mode.is_some() {
        theme.header.add_modifier(Modifier::REVERSED)
    } else {
        theme.row_number
    };
    fill(buf, r, s);
    // Wrapped between bindings, at the double spaces.
    let width = r.width.saturating_sub(2) as usize;
    let mut lines = vec![String::new()];
    for part in keys_text(app).split("  ") {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && line.chars().count() + 2 + part.chars().count() > width {
            lines.push(part.to_string());
        } else {
            if !line.is_empty() {
                line.push_str("  ");
            }
            line.push_str(part);
        }
    }
    for (i, line) in lines.iter().enumerate().take(r.height as usize) {
        put(
            buf,
            r.x as i32 + 1,
            r.y + i as u16,
            line,
            s,
            (r.x, r.right()),
        );
    }
}

fn draw_status(app: &App, theme: &Theme, r: Rect, buf: &mut Buffer) {
    fill(buf, r, theme.body);
    // B's which-key menu lives in the status line.
    if let Popup::WhichKey { ax, key, hidden } = &app.popup {
        let m = app.m(*ax);
        let text = if *hidden {
            let list = app.hidden(*ax);
            let mut s = format!("Show hidden {}:  a all", m.noun(2));
            for (i, &x) in list.iter().enumerate().take(10) {
                s += &format!("  {} {}", (i + 1) % 10, m.labels[x]);
            }
            s
        } else {
            let mut s = format!("{}:", m.labels[*key]);
            let n = app.hidden(*ax).len();
            for (label, act, c) in app.menu_items(*ax, *key) {
                let label = match act {
                    MenuAct::Chooser => format!("{}…", m.noun(2)),
                    _ => label.to_lowercase(),
                };
                s += &format!("  {c} {label}");
                if act == MenuAct::Hide {
                    s += &format!("  s show hidden ({n})");
                }
            }
            s + "  Esc"
        };
        put(
            buf,
            r.x as i32 + 1,
            r.y,
            &text,
            theme.header.add_modifier(Modifier::REVERSED),
            (r.x, r.right()),
        );
        return;
    }
    let left = match &app.message {
        Some((text, true, _)) => (text.clone(), theme.error),
        Some((text, false, _)) => (text.clone(), theme.body),
        None => match &app.drag {
            Some(Drag {
                active: true,
                drop: Some(d),
                ..
            }) => (
                d.label.clone(),
                if d.refused {
                    theme.error
                } else {
                    theme.drop_indicator
                },
            ),
            _ => (String::new(), theme.body),
        },
    };
    put(buf, r.x as i32 + 1, r.y, &left.0, left.1, (r.x, r.right()));
    if app.variant == Variant::A {
        let count = format!("{} · {} ", app.cols.count(), app.rows.count());
        let x = r.right() as i32 - count.chars().count() as i32;
        put(buf, x, r.y, &count, theme.row_number, (r.x, r.right()));
    }
}

fn draw_popup(app: &App, theme: &Theme, grid: Rect, buf: &mut Buffer) {
    let header_x = |ax: Ax, k: usize| -> (u16, u16) {
        let list = if ax == Ax::Cols {
            &app.geo.cols
        } else {
            &app.geo.rows
        };
        match list.iter().find(|c| c.2 == k) {
            Some(c) if ax == Ax::Cols => (c.0.max(0) as u16, grid.y + 1),
            Some(c) => (grid.x + RN_W, c.0.max(0) as u16),
            None => (grid.x + RN_W, grid.y + 1),
        }
    };
    let border = theme.body.patch(theme.border);
    let boxed = |buf: &mut Buffer, r: Rect, title: &str| {
        let r = r.intersection(grid);
        Clear.render(r, buf);
        fill(buf, r, theme.body);
        Block::bordered()
            .border_style(border)
            .title(format!(" {title} "))
            .render(r, buf);
        r
    };
    let place = |at: (u16, u16), w: u16, h: u16| -> Rect {
        let x = at.0.min(grid.right().saturating_sub(w));
        let y = at.1.min(grid.bottom().saturating_sub(h));
        Rect::new(x, y, w, h)
    };
    match &app.popup {
        Popup::None | Popup::WhichKey { .. } => {}
        Popup::Menu {
            ax,
            key,
            sel,
            flyout,
        } => {
            let m = app.m(*ax);
            let items = app.menu_items(*ax, *key);
            let w = 30;
            let r = place(header_x(*ax, *key), w, items.len() as u16 + 2);
            let r = boxed(buf, r, &format!("{} {}", cap(m.noun(1)), m.labels[*key]));
            for (i, (label, act, c)) in items.iter().enumerate() {
                let y = r.y + 1 + i as u16;
                let mut s = theme.body;
                if i == *sel {
                    s = s.patch(theme.focus);
                }
                let dim = matches!(act, MenuAct::Show(_) | MenuAct::ShowAll);
                if dim && i != *sel {
                    s = s.patch(theme.row_number);
                }
                let line = format!(" {c}  {}", fit(label, w - 6));
                put(buf, r.x as i32 + 1, y, &line, s, (r.x + 1, r.right() - 1));
            }
            if let Some(f) = flyout {
                let hidden = app.hidden(*ax);
                let fr = Rect::new(r.right(), r.y + 1, 24, hidden.len() as u16 + 3);
                let fr = place((fr.x, fr.y), fr.width, fr.height);
                let fr = boxed(buf, fr, &format!("Hidden {}", m.noun(2)));
                let mut lines = vec![format!("Show all {}", hidden.len())];
                lines.extend(hidden.iter().map(|&x| {
                    let pinned = m
                        .side(x)
                        .map_or(String::new(), |s| format!("  pinned {}", m.side_name(s)));
                    format!("{}{pinned}", m.labels[x])
                }));
                for (i, line) in lines.iter().enumerate() {
                    let s = if i == *f {
                        theme.body.patch(theme.focus)
                    } else {
                        theme.body
                    };
                    put(
                        buf,
                        fr.x as i32 + 2,
                        fr.y + 1 + i as u16,
                        &fit(line, fr.width - 4),
                        s,
                        (fr.x + 1, fr.right() - 1),
                    );
                }
            }
        }
        Popup::Chooser {
            ax,
            sel,
            search,
            searching,
            by,
        } => {
            let m = app.m(*ax);
            let rows = app.chooser_items(*ax, search, *by);
            let r = match app.variant {
                // B: a drawer down the right side, a small grid of its own.
                Variant::B => Rect::new(grid.right().saturating_sub(48), grid.y, 48, grid.height),
                _ => {
                    let h = (rows.len() as u16 + 4).min(grid.height);
                    place(header_x(Ax::Cols, app.focus.1), 34, h)
                }
            };
            let title = match app.variant {
                Variant::B => format!("{} sheet", cap(m.noun(1))),
                _ => format!("Choose {}", m.noun(2)),
            };
            let r = boxed(buf, r, &title);
            let inner = (r.x + 1, r.right() - 1);
            let search_line = if search.is_empty() && !*searching && app.variant != Variant::A {
                "/ search".to_string()
            } else {
                format!(
                    "Search: {search}{}",
                    if *searching || app.variant == Variant::A {
                        "▏"
                    } else {
                        ""
                    }
                )
            };
            put(
                buf,
                r.x as i32 + 2,
                r.y + 1,
                &search_line,
                theme.loading,
                inner,
            );
            let mut y = r.y + 2;
            if app.variant == Variant::B {
                let heads = ["Name", "Shown", "Pin", "Width"];
                let mut text = String::new();
                for (i, (h, w)) in heads.iter().zip([14usize, 6, 6, 6]).enumerate() {
                    let h = if *by == i + 1 {
                        format!("{h}▲")
                    } else {
                        h.to_string()
                    };
                    text += &format!("{h:<w$} ");
                }
                put(buf, r.x as i32 + 2, y, &text, theme.header, inner);
                y += 1;
            }
            let room = r.bottom().saturating_sub(y + 2) as usize;
            let first = sel.saturating_sub(room.saturating_sub(1));
            for (i, row) in rows.iter().enumerate().skip(first).take(room) {
                let mut s = theme.body;
                if i == *sel {
                    s = s.patch(theme.focus);
                }
                let line = match row {
                    ChooserRow::All => {
                        let all = rows
                            .iter()
                            .all(|r| !matches!(r, ChooserRow::Item(k) if m.hidden.contains(k)));
                        format!("[{}] (Show all)", if all { "x" } else { " " })
                    }
                    ChooserRow::Section(name) => {
                        s = if i == *sel {
                            s
                        } else {
                            theme.row_number.add_modifier(Modifier::BOLD)
                        };
                        name.to_uppercase()
                    }
                    ChooserRow::Item(k) => {
                        let shown = !m.hidden.contains(k);
                        let pin = match m.side(*k) {
                            Some(Side::Lead) => {
                                if *ax == Ax::Cols {
                                    "L"
                                } else {
                                    "T"
                                }
                            }
                            Some(Side::Trail) => {
                                if *ax == Ax::Cols {
                                    "R"
                                } else {
                                    "B"
                                }
                            }
                            None => "-",
                        };
                        let dormant = if !shown && m.side(*k).is_some() {
                            " dormant"
                        } else {
                            ""
                        };
                        match app.variant {
                            Variant::B => format!(
                                "{:<14} {:<6} {:<6} {:<6}{dormant}",
                                fit(m.labels[*k], 14),
                                if shown { "[x]" } else { "[ ]" },
                                pin,
                                if *ax == Ax::Cols {
                                    m.sizes[*k].to_string()
                                } else {
                                    "1".into()
                                },
                            ),
                            Variant::C => format!(
                                "  {}{}",
                                m.labels[*k],
                                if dormant.is_empty() {
                                    ""
                                } else {
                                    "  (dormant)"
                                }
                            ),
                            Variant::A => {
                                let tag = match m.side(*k) {
                                    Some(sd) => format!("  pinned {}{dormant}", m.side_name(sd)),
                                    None => String::new(),
                                };
                                format!("[{}] {}{tag}", if shown { "x" } else { " " }, m.labels[*k])
                            }
                        }
                    }
                };
                if matches!(row, ChooserRow::Item(k) if m.hidden.contains(k)) && i != *sel {
                    s = s.patch(theme.loading);
                }
                put(buf, r.x as i32 + 2, y, &fit(&line, r.width - 4), s, inner);
                y += 1;
            }
            let foot = match app.variant {
                Variant::A => format!("{} shown  Space tick  Enter", m.count()),
                Variant::B => "Space shown  < > u pin  + - width  Tab sort".to_string(),
                Variant::C => "Space show/hide  < > pin  u unpin".to_string(),
            };
            put(
                buf,
                r.x as i32 + 2,
                r.bottom() - 2,
                &fit(&foot, r.width - 4),
                theme.row_number,
                inner,
            );
        }
    }
}

fn draw_state(app: &App, r: Rect, buf: &mut Buffer) {
    let s = Style::default().fg(Color::Gray).bg(Color::Rgb(17, 24, 39));
    fill(buf, r, s);
    let describe = |m: &AxisModel, a: &Allot| -> Vec<String> {
        let list = |ks: &[usize]| -> String {
            let names: Vec<String> = ks
                .iter()
                .map(|&k| {
                    let d = if m.hidden.contains(&k) && m.side(k).is_some() {
                        " (dormant)"
                    } else {
                        ""
                    };
                    format!("{}{d}", m.labels[k])
                })
                .collect();
            format!("[{}]", names.join(", "))
        };
        let hidden: Vec<usize> = m.hidden.iter().copied().collect();
        let movable = match m.sort {
            Sort::None => "yes",
            Sort::Bake => "yes (sorted, a move bakes)",
            Sort::Withdraw => "no (sorted, withdrawn)",
        };
        let clipped = if a.lead_a < a.lead_w || a.trail_a < a.trail_w {
            "  CLIPPED"
        } else {
            ""
        };
        vec![
            format!("{:?}: {}", m.ax, m.count()),
            format!("  movable {movable}"),
            format!(
                "  pending {}{}",
                m.pending
                    .as_ref()
                    .map_or("none".to_string(), |p| m.labels[p.key].to_string()),
                if m.refuse_next { " (next refused)" } else { "" }
            ),
            format!("  lead    {}", list(&m.lead)),
            format!("  trail   {}", list(&m.trail)),
            format!("  hidden  {}", list(&hidden)),
            format!(
                "  drawn   lead {}/{} trail {}/{}{clipped}",
                a.lead_a, a.lead_w, a.trail_a, a.trail_w
            ),
            format!("  anchor  {}", m.scroll),
        ]
    };
    let mut lines = vec![
        format!("variant {}", app.variant.name()),
        format!(
            "preset  {:?}{}",
            app.preset,
            app.mode
                .map_or(String::new(), |ax| format!(" (mode {ax:?})"))
        ),
        format!(
            "border  {}   theme {}",
            BORDERS[app.border].0,
            app.theme().name
        ),
        format!("focus   {} / {}", VESSELS[app.focus.0], COLS[app.focus.1]),
        String::new(),
    ];
    lines.extend(describe(&app.cols, &app.allot(Ax::Cols)));
    lines.push(String::new());
    lines.extend(describe(&app.rows, &app.allot(Ax::Rows)));
    lines.push(String::new());
    lines.push(format!("last    {}", app.last));
    let mut y = r.y;
    let w = r.width as usize - 2;
    for line in lines {
        // Wrap long lines.
        let chars: Vec<char> = line.chars().collect();
        for chunk in chars.chunks(w.max(1)) {
            if y >= r.bottom() {
                return;
            }
            let text: String = chunk.iter().collect();
            put(buf, r.x as i32 + 1, y, &text, s, (r.x, r.right()));
            y += 1;
        }
        if chars.is_empty() {
            y += 1;
        }
    }
}

// ------------------------------------------------------------------ main

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    (args.iter().enumerate()).find_map(|(i, a)| match a.strip_prefix(name) {
        Some("") => args.get(i + 1).cloned(),
        Some(rest) => rest.strip_prefix('=').map(str::to_string),
        None => None,
    })
}

fn options() -> (Variant, Preset) {
    let variant = match arg("--variant").as_deref() {
        Some("b" | "B") => Variant::B,
        Some("c" | "C") => Variant::C,
        _ => Variant::A,
    };
    let preset = match arg("--preset").as_deref() {
        Some("modes") => Preset::Modes,
        Some("bare-keys") => Preset::BareKeys,
        _ => Preset::Chords,
    };
    (variant, preset)
}

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "--dump") {
        dump();
        return Ok(());
    }
    let (variant, preset) = options();
    let mut app = App::new(variant, preset);
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    while !app.quit {
        app.tick(Instant::now());
        terminal.draw(|f| {
            let area = f.area();
            draw(&mut app, area, f.buffer_mut());
        })?;
        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(k) => app.key(k),
                Event::Mouse(m) => app.mouse(m),
                _ => {}
            }
        }
    }
    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    Ok(())
}

/// Prints scripted scenes for each variant, drawn on a TestBackend.
fn dump() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let only = arg("--scene");
    let width = arg("--width").and_then(|w| w.parse().ok()).unwrap_or(150);
    type Scene = (&'static str, fn(&mut App));
    let scenes: [Scene; 11] = [
        ("start", |_| {}),
        ("menu", |a| a.act(Ax::Cols, Act::Menu)),
        ("show-hidden", |a| a.act(Ax::Cols, Act::ShowHidden)),
        ("row-menu", |a| a.act(Ax::Rows, Act::Menu)),
        ("chooser", |a| a.act(Ax::Cols, Act::Chooser)),
        ("border-none", |a| a.border = 4),
        ("border-grid", |a| a.border = 1),
        ("border-headers-rules", |a| a.border = 3),
        ("clipped", |a| {
            let mut fake = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
            fake.kind = KeyEventKind::Press;
            a.key(fake);
            a.focus.1 = 29;
        }),
        ("drag-pin", |a| {
            let x = a.geo.body_x + 1;
            a.drag = Some(Drag {
                ax: Ax::Cols,
                key: 5,
                from: (0, 0),
                active: true,
                drop: Some(a.drop_at(Ax::Cols, 5, x)),
            });
        }),
        ("drag-return", |a| {
            a.cols.sort = Sort::Withdraw;
            let x = a.geo.body_x + 40;
            a.drag = Some(Drag {
                ax: Ax::Cols,
                key: 1,
                from: (0, 0),
                active: true,
                drop: Some(a.drop_at(Ax::Cols, 1, x)),
            });
        }),
    ];
    for variant in [Variant::A, Variant::B, Variant::C] {
        for (name, setup) in &scenes {
            if only.as_deref().is_some_and(|o| !name.starts_with(o)) {
                continue;
            }
            let mut app = App::new(variant, Preset::Chords);
            let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
            // A first frame lays out the geometry the scene's setup reads.
            terminal
                .draw(|f| {
                    let a = f.area();
                    draw(&mut app, a, f.buffer_mut())
                })
                .unwrap();
            setup(&mut app);
            terminal
                .draw(|f| {
                    let a = f.area();
                    draw(&mut app, a, f.buffer_mut())
                })
                .unwrap();
            println!("=== variant {variant:?}, {name}");
            let buf = terminal.backend().buffer().clone();
            for y in 0..buf.area.height {
                let line: String = (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect();
                println!("{}", line.trim_end());
            }
        }
    }
}
