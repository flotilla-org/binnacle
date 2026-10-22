//! PROTOTYPE (#186), throwaway: a grouped and a pivoted table in a terminal,
//! for the Map: hierarchical axes (#177). Two presets (F2): a grouped table
//! (rows by region then port, outline form, leaves shown, subtotals after,
//! the grand total pinned at the bottom) and a pivot (region and port down,
//! quarters across, two values so the column header has two tiers). F3 puts
//! the rows in header form, F4 flips the corner between the tiers layout and
//! Excel's compact form. The grid is painted straight into the buffer and the
//! Table Source is a fake that regroups a few hundred generated shipments from
//! the spec on every change: none of this is in the core yet. `--dump` prints
//! scripted scenes from a TestBackend instead. Look and feel are borrowed from
//! the adapter's Themes.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::time::{Duration, Instant};

use binnacle_ratatui::Theme;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Clear, Widget};

/// How long the fake source takes to answer an expand or collapse.
const LATENCY: Duration = Duration::from_millis(400);

// ------------------------------------------------------------------ the fake source

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Field {
    Vessel,
    Region,
    Port,
    Quarter,
    Cargo,
    Weight,
    Crew,
}

const FIELDS: [Field; 7] = [
    Field::Vessel,
    Field::Region,
    Field::Port,
    Field::Quarter,
    Field::Cargo,
    Field::Weight,
    Field::Crew,
];

impl Field {
    fn name(self) -> &'static str {
        match self {
            Field::Vessel => "vessel",
            Field::Region => "region",
            Field::Port => "port",
            Field::Quarter => "quarter",
            Field::Cargo => "cargo",
            Field::Weight => "weight",
            Field::Crew => "crew",
        }
    }
    fn numeric(self) -> bool {
        matches!(self, Field::Weight | Field::Crew)
    }
    fn width(self) -> u16 {
        match self {
            Field::Vessel => 14,
            Field::Region => 8,
            Field::Port => 13,
            Field::Quarter => 8,
            Field::Cargo => 12,
            Field::Weight => 9,
            Field::Crew => 7,
        }
    }
}

const REGIONS: [(&str, &[&str]); 4] = [
    ("North", &["Leith", "Aberdeen", "Whitby", "Lerwick"]),
    ("South", &["Falmouth", "Plymouth", "Southampton"]),
    ("East", &["Hull", "Harwich", "Felixstowe"]),
    ("West", &["Bristol", "Swansea", "Liverpool"]),
];
const QUARTERS: [&str; 4] = ["Q1", "Q2", "Q3", "Q4"];
const CARGOES: [&str; 5] = ["grain", "coal", "timber", "containers", "fish"];
const BIRDS: [&str; 24] = [
    "Cormorant",
    "Petrel",
    "Gannet",
    "Kittiwake",
    "Shearwater",
    "Fulmar",
    "Guillemot",
    "Razorbill",
    "Puffin",
    "Skua",
    "Tern",
    "Merganser",
    "Eider",
    "Dunlin",
    "Curlew",
    "Plover",
    "Osprey",
    "Heron",
    "Lapwing",
    "Sanderling",
    "Turnstone",
    "Whimbrel",
    "Godwit",
    "Oystercatcher",
];

struct Ship {
    id: usize,
    vessel: String,
    region: &'static str,
    port: &'static str,
    quarter: &'static str,
    cargo: &'static str,
    weight: i64,
    crew: i64,
}

impl Ship {
    fn text(&self, f: Field) -> String {
        match f {
            Field::Vessel => self.vessel.clone(),
            Field::Region => self.region.into(),
            Field::Port => self.port.into(),
            Field::Quarter => self.quarter.into(),
            Field::Cargo => self.cargo.into(),
            Field::Weight => self.weight.to_string(),
            Field::Crew => self.crew.to_string(),
        }
    }
    fn num(&self, f: Field) -> i64 {
        match f {
            Field::Weight => self.weight,
            Field::Crew => self.crew,
            _ => 0,
        }
    }
}

fn ships() -> Vec<Ship> {
    let mut s: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move |n: u64| {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s % n) as usize
    };
    (0..420)
        .map(|id| {
            let (region, ports) = REGIONS[next(4)];
            let port = ports[next(ports.len() as u64)];
            Ship {
                id,
                vessel: format!("{} {}", BIRDS[next(24)], ["", "II", "III"][next(3)])
                    .trim_end()
                    .to_string(),
                region,
                port,
                quarter: QUARTERS[next(4)],
                cargo: CARGOES[next(5)],
                weight: 10 + next(51) as i64,
                crew: 4 + next(11) as i64,
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Agg {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Value {
    field: Option<Field>,
    agg: Agg,
}

impl Value {
    fn label(&self) -> String {
        let a = match self.agg {
            Agg::Sum => "sum",
            Agg::Count => "count",
            Agg::Avg => "avg",
            Agg::Min => "min",
            Agg::Max => "max",
        };
        match self.field {
            Some(f) => format!("{a}({})", f.name()),
            None => a.into(),
        }
    }
    fn compute(&self, ships: &[Ship], idx: &[usize]) -> String {
        if idx.is_empty() {
            return String::new();
        }
        let f = self.field.unwrap_or(Field::Weight);
        let vals = idx.iter().map(|&i| ships[i].num(f));
        match self.agg {
            Agg::Count => idx.len().to_string(),
            Agg::Sum => vals.sum::<i64>().to_string(),
            Agg::Min => vals.min().unwrap_or(0).to_string(),
            Agg::Max => vals.max().unwrap_or(0).to_string(),
            Agg::Avg => format!("{:.1}", vals.sum::<i64>() as f64 / idx.len() as f64),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    Outline,
    Header,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Corner {
    Tiers,
    Compact,
}

/// `expand` for one axis (#184): `depth` levels open by default, `except`
/// the Group Keys whose state is the opposite.
#[derive(Clone, Debug, Default)]
struct Expand {
    depth: usize,
    except: BTreeSet<String>,
}

impl Expand {
    fn all() -> Expand {
        Expand {
            depth: 99,
            except: BTreeSet::new(),
        }
    }
    fn is_open(&self, level: usize, key: &str) -> bool {
        (level < self.depth) ^ self.except.contains(key)
    }
    fn set(&mut self, level: usize, key: &str, open: bool) {
        if open == (level < self.depth) {
            self.except.remove(key);
        } else {
            self.except.insert(key.to_string());
        }
    }
}

/// The `grouping` part of the View Spec, plus the per-axis form and corner
/// layout this prototype compares. Subtotals and grand totals are placed
/// after on both axes.
#[derive(Clone, Debug)]
struct Spec {
    row_levels: Vec<Field>,
    col_levels: Vec<Field>,
    values: Vec<Value>,
    row_form: Form,
    corner: Corner,
    rows_expand: Expand,
    cols_expand: Expand,
}

impl Spec {
    fn grouped() -> Spec {
        Spec {
            row_levels: vec![Field::Region, Field::Port],
            col_levels: vec![],
            values: vec![
                Value {
                    field: Some(Field::Weight),
                    agg: Agg::Sum,
                },
                Value {
                    field: Some(Field::Crew),
                    agg: Agg::Sum,
                },
                Value {
                    field: None,
                    agg: Agg::Count,
                },
            ],
            row_form: Form::Outline,
            corner: Corner::Tiers,
            rows_expand: Expand::all(),
            cols_expand: Expand::all(),
        }
    }
    fn pivot() -> Spec {
        Spec {
            row_levels: vec![Field::Region, Field::Port],
            col_levels: vec![Field::Quarter],
            values: vec![
                Value {
                    field: Some(Field::Weight),
                    agg: Agg::Sum,
                },
                Value {
                    field: None,
                    agg: Agg::Count,
                },
            ],
            ..Spec::grouped()
        }
    }
    fn pivot_on(&self) -> bool {
        !self.col_levels.is_empty()
    }
    /// Compact form puts every row level in one indented column.
    fn form(&self) -> Form {
        if self.corner == Corner::Compact {
            Form::Outline
        } else {
            self.row_form
        }
    }
    fn count_caption(&self) -> bool {
        !self.pivot_on() && self.values.iter().any(|v| v.field.is_none())
    }
}

type Path = Vec<(Field, String)>;

fn path_text(p: &[(Field, String)]) -> String {
    p.iter()
        .map(|(f, v)| format!("{}={v}", f.name()))
        .collect::<Vec<_>>()
        .join(", ")
}
fn group_key(p: &[(Field, String)]) -> String {
    format!("(group, {})", path_text(p))
}
fn subtotal_key(p: &[(Field, String)]) -> String {
    format!("(subtotal, {})", path_text(p))
}
fn col_group_key(p: &[(Field, String)]) -> String {
    format!("(colgroup, {})", path_text(p))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Leaf,
    Group,
    Subtotal,
    Grand,
}

/// One row item with its outline (#183): kind, depth, parent Key, open state
/// and the group it stands for.
#[derive(Clone, Debug)]
struct RowItem {
    key: String,
    kind: Kind,
    depth: usize,
    path: Path,
    /// `None` for an item that can't open (a leaf, a pivot's deepest group).
    open: Option<bool>,
    ships: Vec<usize>,
    parent: Option<String>,
}

impl RowItem {
    fn ancestors(&self) -> Vec<String> {
        (1..=self.path.len())
            .map(|n| group_key(&self.path[..n]))
            .collect()
    }
}

#[derive(Clone, Debug)]
enum ColItem {
    Base(Field),
    Pivot {
        path: Path,
        value: usize,
        kind: Kind,
    },
}

impl ColItem {
    fn key(&self, spec: &Spec) -> String {
        match self {
            ColItem::Base(f) => format!("(col, {})", f.name()),
            ColItem::Pivot { path, value, .. } => {
                format!(
                    "(col, {}, {})",
                    path_text(path),
                    spec.values[*value].label()
                )
            }
        }
    }
    fn width(&self) -> u16 {
        match self {
            ColItem::Base(f) => f.width(),
            ColItem::Pivot { .. } => 12,
        }
    }
    fn grand(&self) -> bool {
        matches!(
            self,
            ColItem::Pivot {
                kind: Kind::Grand,
                ..
            }
        )
    }
}

struct Model {
    rows: Vec<RowItem>,
    grand: RowItem,
    cols: Vec<ColItem>,
    /// Column tiers in the column Header Band.
    tiers: usize,
}

fn group_by(ships: &[Ship], idx: &[usize], f: Field) -> BTreeMap<String, Vec<usize>> {
    let mut m: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &i in idx {
        m.entry(ships[i].text(f)).or_default().push(i);
    }
    m
}

fn build(spec: &Spec, ships: &[Ship]) -> Model {
    let all: Vec<usize> = (0..ships.len()).collect();
    let mut rows = Vec::new();
    build_rows(spec, ships, &all, 0, &[], None, &mut rows);
    let grand = RowItem {
        key: "(total)".into(),
        kind: Kind::Grand,
        depth: 0,
        path: vec![],
        open: None,
        ships: all.clone(),
        parent: None,
    };
    let mut cols = Vec::new();
    if spec.pivot_on() {
        build_cols(spec, ships, &all, 0, &[], &mut cols);
        for v in 0..spec.values.len() {
            cols.push(ColItem::Pivot {
                path: vec![],
                value: v,
                kind: Kind::Grand,
            });
        }
    } else {
        cols.extend(
            FIELDS
                .iter()
                .filter(|f| !spec.row_levels.contains(f))
                .map(|&f| ColItem::Base(f)),
        );
    }
    let tiers = if spec.pivot_on() {
        spec.col_levels.len() + usize::from(spec.values.len() > 1)
    } else {
        1
    };
    Model {
        rows,
        grand,
        cols,
        tiers,
    }
}

fn build_rows(
    spec: &Spec,
    ships: &[Ship],
    idx: &[usize],
    level: usize,
    path: &[(Field, String)],
    parent: Option<&String>,
    out: &mut Vec<RowItem>,
) {
    let n = spec.row_levels.len();
    let pivot = spec.pivot_on();
    if level == n {
        if !pivot {
            for &i in idx {
                out.push(RowItem {
                    key: format!("(leaf, {})", ships[i].id),
                    kind: Kind::Leaf,
                    depth: level,
                    path: path.to_vec(),
                    open: None,
                    ships: vec![i],
                    parent: parent.cloned(),
                });
            }
        }
        return;
    }
    for (v, sub) in group_by(ships, idx, spec.row_levels[level]) {
        let mut p = path.to_vec();
        p.push((spec.row_levels[level], v));
        let key = group_key(&p);
        let can_open = level + 1 < n || !pivot;
        let open = can_open.then(|| spec.rows_expand.is_open(level, &key));
        if spec.form() == Form::Outline || !can_open {
            out.push(RowItem {
                key: key.clone(),
                kind: Kind::Group,
                depth: level,
                path: p.clone(),
                open,
                ships: sub.clone(),
                parent: parent.cloned(),
            });
        }
        if open == Some(true) {
            build_rows(spec, ships, &sub, level + 1, &p, Some(&key), out);
        }
        // Outline form: a closed group's Group Item carries its totals, so
        // the Subtotal shows only while open. Header form: the Subtotal stays,
        // standing for the closed group under its span.
        let subtotal = can_open && (open == Some(true) || spec.form() == Form::Header);
        if subtotal {
            out.push(RowItem {
                key: subtotal_key(&p),
                kind: Kind::Subtotal,
                depth: level,
                path: p.clone(),
                open,
                ships: sub,
                parent: Some(key),
            });
        }
    }
}

fn build_cols(
    spec: &Spec,
    ships: &[Ship],
    idx: &[usize],
    level: usize,
    path: &[(Field, String)],
    out: &mut Vec<ColItem>,
) {
    let n = spec.col_levels.len();
    for (v, sub) in group_by(ships, idx, spec.col_levels[level]) {
        let mut p = path.to_vec();
        p.push((spec.col_levels[level], v));
        if level + 1 == n {
            for value in 0..spec.values.len() {
                out.push(ColItem::Pivot {
                    path: p.clone(),
                    value,
                    kind: Kind::Leaf,
                });
            }
            continue;
        }
        if spec.cols_expand.is_open(level, &col_group_key(&p)) {
            build_cols(spec, ships, &sub, level + 1, &p, out);
        }
        for value in 0..spec.values.len() {
            out.push(ColItem::Pivot {
                path: p.clone(),
                value,
                kind: Kind::Subtotal,
            });
        }
    }
}

/// What one body cell shows.
fn cell_text(spec: &Spec, ships: &[Ship], r: &RowItem, c: &ColItem) -> String {
    // Excel's rule: an open Group Item with its Subtotal after shows nothing.
    if r.kind == Kind::Group && r.open == Some(true) {
        return String::new();
    }
    match c {
        ColItem::Base(f) => {
            if r.kind == Kind::Leaf {
                return ships[r.ships[0]].text(*f);
            }
            match spec.values.iter().find(|v| v.field == Some(*f)) {
                Some(v) => v.compute(ships, &r.ships),
                None => String::new(),
            }
        }
        ColItem::Pivot { path, value, .. } => {
            let idx: Vec<usize> = r
                .ships
                .iter()
                .copied()
                .filter(|&i| path.iter().all(|(f, v)| ships[i].text(*f) == *v))
                .collect();
            spec.values[*value].compute(ships, &idx)
        }
    }
}

/// A column's cell in tier `t` of the column Header Band: the merge Key (the
/// Grid merges neighbours by Key, never by label), the label, and the column
/// group it heads when that group can open or close.
type TierCell = (String, String, Option<(usize, String)>);

fn tier_cell(spec: &Spec, c: &ColItem, t: usize) -> Option<TierCell> {
    let ColItem::Pivot { path, value, .. } = c else {
        let ColItem::Base(f) = c else { unreachable!() };
        return Some((f.name().into(), f.name().into(), None));
    };
    let n = spec.col_levels.len();
    if t == n {
        return Some((
            format!("{t}:{}", spec.values[*value].label()),
            spec.values[*value].label(),
            None,
        ));
    }
    let p = path.len();
    if t < p {
        let g = col_group_key(&path[..=t]);
        let toggle = (t + 1 < n).then(|| (t, g.clone()));
        Some((g, path[t].1.clone(), toggle))
    } else if t == p {
        let label = if p == 0 {
            "All".to_string()
        } else {
            format!("{} total", path[p - 1].1)
        };
        Some((format!("total:{}", path_text(path)), label, None))
    } else {
        None
    }
}

// ------------------------------------------------------------------ Theme roles

/// The roles this prototype adds to a Theme: Summary Items and the
/// disclosure mark.
struct Roles {
    group_item: Style,
    subtotal: Style,
    grand_total: Style,
    disclosure: Style,
    pending: Style,
    level_name: Style,
}

fn roles(theme: &Theme) -> Roles {
    let none = Style::default();
    match theme.name {
        "catppuccin_mocha" => Roles {
            group_item: none
                .fg(Color::Rgb(205, 214, 244))
                .bg(Color::Rgb(49, 50, 68))
                .add_modifier(Modifier::BOLD),
            subtotal: none
                .fg(Color::Rgb(116, 199, 236))
                .bg(Color::Rgb(24, 24, 37)),
            grand_total: none
                .fg(Color::Rgb(205, 214, 244))
                .bg(Color::Rgb(69, 71, 90))
                .add_modifier(Modifier::BOLD),
            disclosure: none.fg(Color::Rgb(250, 179, 135)),
            pending: none
                .fg(Color::Rgb(249, 226, 175))
                .add_modifier(Modifier::SLOW_BLINK),
            level_name: none
                .fg(Color::Rgb(203, 166, 247))
                .add_modifier(Modifier::ITALIC),
        },
        "mono" => Roles {
            group_item: none.add_modifier(Modifier::BOLD),
            subtotal: none.add_modifier(Modifier::ITALIC),
            grand_total: none.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            disclosure: none.add_modifier(Modifier::BOLD),
            pending: none.add_modifier(Modifier::DIM | Modifier::SLOW_BLINK),
            level_name: none.add_modifier(Modifier::ITALIC),
        },
        _ => Roles {
            group_item: none
                .fg(Color::White)
                .bg(Color::Rgb(38, 48, 64))
                .add_modifier(Modifier::BOLD),
            subtotal: none.fg(Color::Cyan).bg(Color::Rgb(24, 32, 42)),
            grand_total: none
                .fg(Color::White)
                .bg(Color::Rgb(60, 72, 96))
                .add_modifier(Modifier::BOLD),
            disclosure: none.fg(Color::Yellow),
            pending: none
                .fg(Color::Rgb(255, 240, 120))
                .add_modifier(Modifier::SLOW_BLINK),
            level_name: none.fg(Color::Magenta).add_modifier(Modifier::ITALIC),
        },
    }
}

// ------------------------------------------------------------------ the app

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ax {
    Rows,
    Cols,
}

/// Where focus sits across: a row Header Band column, or a body column by Key.
#[derive(Clone, PartialEq, Eq, Debug)]
enum FCol {
    Band(usize),
    Body(String),
}

#[derive(Clone, Debug)]
enum Op {
    Toggle {
        ax: Ax,
        level: usize,
        key: String,
        open: bool,
    },
    OpenAll {
        keys: Vec<(usize, String)>,
    },
}

struct Pending {
    /// The Group Key whose mark draws pending.
    mark: String,
    op: Op,
    due: Instant,
}

#[derive(Clone, Debug)]
enum MenuAct {
    GroupRows(Field),
    AddColLevel(Field),
    AddValue(Value),
    RemoveRowLevel(Field),
    RemoveColLevel(Field),
    MoveToRows(Field),
    RemoveValue(usize),
}

struct Menu {
    title: String,
    items: Vec<(String, MenuAct)>,
    sel: usize,
    at: (u16, u16),
}

#[derive(Clone, Debug)]
enum Hit {
    Cell(usize, FCol),
    Header(FCol),
    Toggle(Ax, usize, String),
    MenuItem(usize),
}

struct App {
    ships: Vec<Ship>,
    spec: Spec,
    model: Model,
    preset_pivot: bool,
    pin_left: bool,
    theme: usize,
    state_pane: bool,
    focus_row: usize,
    focus_key: String,
    focus_anc: Vec<String>,
    focus_col: FCol,
    row_scroll: usize,
    col_scroll: usize,
    reveal: bool,
    pending: Vec<Pending>,
    menu: Option<Menu>,
    hits: Vec<(Rect, Hit)>,
    /// Body rows and scrolling columns that fit, from the last frame.
    page: usize,
    last: String,
    quit: bool,
}

impl App {
    fn new(pivot: bool) -> App {
        let ships = ships();
        let spec = if pivot {
            Spec::pivot()
        } else {
            Spec::grouped()
        };
        let model = build(&spec, &ships);
        let mut app = App {
            ships,
            spec,
            model,
            preset_pivot: pivot,
            pin_left: false,
            theme: 0,
            state_pane: true,
            focus_row: 0,
            focus_key: String::new(),
            focus_anc: vec![],
            focus_col: FCol::Band(0),
            row_scroll: 0,
            col_scroll: 0,
            reveal: true,
            pending: vec![],
            menu: None,
            hits: vec![],
            page: 20,
            last: "Loaded".into(),
            quit: false,
        };
        app.set_focus_row(0);
        app
    }

    fn theme(&self) -> Theme {
        Theme::presets()[self.theme].clone()
    }

    fn band_count(&self) -> usize {
        if self.spec.form() == Form::Header {
            self.spec.row_levels.len().max(1)
        } else {
            1
        }
    }

    fn row(&self, i: usize) -> &RowItem {
        self.model.rows.get(i).unwrap_or(&self.model.grand)
    }

    fn set_focus_row(&mut self, i: usize) {
        self.focus_row = i.min(self.model.rows.len());
        let r = self.row(self.focus_row).clone();
        self.focus_key = r.key.clone();
        self.focus_anc = r.ancestors();
        self.reveal = true;
    }

    /// Regroups from the spec, then puts focus back by Key: inside a
    /// collapsed group it falls back to the Group Item, or in header form to
    /// the Subtotal left under the span (#184).
    fn rebuild(&mut self) {
        self.model = build(&self.spec, &self.ships);
        let find = |k: &str| self.model.rows.iter().position(|r| r.key == k);
        let mut at = if self.focus_key == "(total)" {
            Some(self.model.rows.len())
        } else {
            find(&self.focus_key)
        };
        if at.is_none() {
            for a in self.focus_anc.iter().rev() {
                let path_key = a.trim_start_matches("(group, ").trim_end_matches(')');
                at = find(a).or_else(|| find(&format!("(subtotal, {path_key})")));
                if at.is_some() {
                    break;
                }
            }
        }
        let anc = std::mem::take(&mut self.focus_anc);
        self.set_focus_row(at.unwrap_or(self.focus_row));
        if at.is_none() {
            self.focus_anc = anc;
        }
        let nb = self.band_count();
        if let FCol::Band(b) = self.focus_col
            && b >= nb
        {
            self.focus_col = FCol::Band(nb - 1);
        }
        if let FCol::Body(k) = &self.focus_col
            && !self.model.cols.iter().any(|c| c.key(&self.spec) == *k)
        {
            self.focus_col = FCol::Band(0);
        }
    }

    /// Body columns in drawn order: pinned left, scrolling, pinned right.
    fn strips(&self) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
        let (mut lead, mut scroll, mut trail) = (vec![], vec![], vec![]);
        for (i, c) in self.model.cols.iter().enumerate() {
            if c.grand() {
                trail.push(i);
            } else if self.pin_left && self.spec.pivot_on() && lead.len() < 3 {
                lead.push(i);
            } else {
                scroll.push(i);
            }
        }
        (lead, scroll, trail)
    }

    fn col_order(&self) -> Vec<FCol> {
        let (l, s, t) = self.strips();
        let mut v: Vec<FCol> = (0..self.band_count()).map(FCol::Band).collect();
        v.extend(
            l.into_iter()
                .chain(s)
                .chain(t)
                .map(|i| FCol::Body(self.model.cols[i].key(&self.spec))),
        );
        v
    }

    fn move_col(&mut self, d: isize) {
        let order = self.col_order();
        let at = order.iter().position(|c| *c == self.focus_col).unwrap_or(0) as isize;
        let to = (at + d).clamp(0, order.len() as isize - 1) as usize;
        self.focus_col = order[to].clone();
        self.reveal = true;
    }

    /// The group that the focused band cell stands for, with its level and
    /// open state; `None` where Left and Right just move.
    fn focused_group(&self) -> Option<(usize, String, bool)> {
        let FCol::Band(b) = self.focus_col else {
            return None;
        };
        let r = self.row(self.focus_row);
        let n = self.spec.row_levels.len();
        let pivot = self.spec.pivot_on();
        if self.spec.form() == Form::Outline {
            return match (r.kind, r.open) {
                (Kind::Group, Some(o)) => Some((r.depth, r.key.clone(), o)),
                _ => None,
            };
        }
        // Header form: band column `b` is level `b`.
        if r.path.len() <= b || r.kind == Kind::Grand {
            return None;
        }
        let can_open = b + 1 < n || !pivot;
        if !can_open {
            return None;
        }
        let key = group_key(&r.path[..=b]);
        Some((b, key.clone(), self.spec.rows_expand.is_open(b, &key)))
    }

    fn effective_open(&self, key: &str, actual: bool) -> bool {
        match self.pending.iter().rev().find(|p| p.mark == key) {
            Some(Pending {
                op: Op::Toggle { open, .. },
                ..
            }) => *open,
            Some(_) => true,
            None => actual,
        }
    }

    fn toggle(&mut self, ax: Ax, level: usize, key: String, actual: bool) {
        let open = !self.effective_open(&key, actual);
        // A second toggle supersedes the first.
        self.pending.retain(|p| p.mark != key);
        self.last = format!(
            "{} {key} (pending)",
            if open { "Expand" } else { "Collapse" }
        );
        self.pending.push(Pending {
            mark: key.clone(),
            op: Op::Toggle {
                ax,
                level,
                key,
                open,
            },
            due: Instant::now() + LATENCY,
        });
    }

    fn open_all_under(&mut self) {
        let r = self.row(self.focus_row).clone();
        let Some((level, key, _)) = self
            .focused_group()
            .or_else(|| (r.kind == Kind::Group).then(|| (r.depth, r.key.clone(), false)))
        else {
            self.last = "* needs a group".into();
            return;
        };
        let n = self.spec.row_levels.len();
        let pivot = self.spec.pivot_on();
        let prefix = &r.path[..=level];
        let mut keys = BTreeSet::new();
        for s in &self.ships {
            if !prefix.iter().all(|(f, v)| s.text(*f) == *v) {
                continue;
            }
            for l in level..n {
                if l + 1 < n || !pivot {
                    let p: Path = self.spec.row_levels[..=l]
                        .iter()
                        .map(|&f| (f, s.text(f)))
                        .collect();
                    keys.insert((l, group_key(&p)));
                }
            }
        }
        self.pending.retain(|p| p.mark != key);
        self.last = format!("Open all under {key}: {} groups (pending)", keys.len());
        self.pending.push(Pending {
            mark: key,
            op: Op::OpenAll {
                keys: keys.into_iter().collect(),
            },
            due: Instant::now() + LATENCY,
        });
    }

    fn tick(&mut self, now: Instant) {
        let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| p.due <= now);
        self.pending = keep;
        if due.is_empty() {
            return;
        }
        for p in due {
            match p.op {
                Op::Toggle {
                    ax: Ax::Rows,
                    level,
                    key,
                    open,
                } => self.spec.rows_expand.set(level, &key, open),
                Op::Toggle {
                    ax: Ax::Cols,
                    level,
                    key,
                    open,
                } => self.spec.cols_expand.set(level, &key, open),
                Op::OpenAll { keys } => {
                    for (l, k) in keys {
                        self.spec.rows_expand.set(l, &k, true);
                    }
                }
            }
        }
        self.rebuild();
        self.last = "The source answered".into();
    }

    fn right(&mut self) {
        match self.focused_group() {
            Some((level, key, open)) => {
                if self.effective_open(&key, open) != open {
                    return;
                }
                if !open {
                    self.toggle(Ax::Rows, level, key, open);
                } else if self.spec.form() == Form::Outline {
                    // Step in: the first child.
                    self.set_focus_row(self.focus_row + 1);
                } else {
                    self.move_col(1);
                }
            }
            None => self.move_col(1),
        }
    }

    fn left(&mut self) {
        // A group already asked to close counts as closed: Left steps out.
        if let Some((level, key, open)) = self.focused_group()
            && self.effective_open(&key, open)
        {
            self.toggle(Ax::Rows, level, key, open);
            return;
        }
        match self.focus_col {
            FCol::Band(b) if self.spec.form() == Form::Header => {
                if b > 0 {
                    self.move_col(-1);
                }
            }
            FCol::Band(_) => {
                // Step out to the parent Group Item, through its Key.
                let r = self.row(self.focus_row);
                let parent = if r.kind == Kind::Group {
                    r.path
                        .len()
                        .checked_sub(1)
                        .filter(|&n| n > 0)
                        .map(|n| group_key(&r.path[..n]))
                } else {
                    r.parent.clone()
                };
                if let Some(p) = parent
                    && let Some(i) = self.model.rows.iter().position(|r| r.key == p)
                {
                    self.set_focus_row(i);
                    self.last = format!("Stepped out to {p}");
                }
            }
            FCol::Body(_) => self.move_col(-1),
        }
    }

    fn key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if let Some(menu) = &mut self.menu {
            match k.code {
                KeyCode::Esc | KeyCode::Char('m') => self.menu = None,
                KeyCode::Up => menu.sel = menu.sel.saturating_sub(1),
                KeyCode::Down => menu.sel = (menu.sel + 1).min(menu.items.len() - 1),
                KeyCode::Enter => {
                    let act = menu.items[menu.sel].1.clone();
                    self.menu = None;
                    self.run_menu(act);
                }
                _ => {}
            }
            return;
        }
        match (k.code, ctrl) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), true) => self.quit = true,
            (KeyCode::F(2), _) | (KeyCode::Char('g'), true) => self.preset(!self.preset_pivot),
            (KeyCode::F(3), _) | (KeyCode::Char('f'), true) => {
                self.spec.row_form = if self.spec.row_form == Form::Outline {
                    Form::Header
                } else {
                    Form::Outline
                };
                self.last = format!("Rows in {:?} form", self.spec.row_form);
                self.rebuild();
            }
            (KeyCode::F(4), _) | (KeyCode::Char('k'), true) => {
                self.spec.corner = if self.spec.corner == Corner::Tiers {
                    Corner::Compact
                } else {
                    Corner::Tiers
                };
                self.last = format!("Corner: {:?}", self.spec.corner);
                self.rebuild();
            }
            (KeyCode::F(5), _) | (KeyCode::Char('x'), true) => {
                self.pin_left = !self.pin_left;
                self.last = format!("Pin 3 columns left: {}", self.pin_left);
            }
            (KeyCode::F(6), _) | (KeyCode::Char('t'), true) => self.theme = (self.theme + 1) % 3,
            (KeyCode::F(7), _) | (KeyCode::Char('e'), true) => self.state_pane = !self.state_pane,
            (KeyCode::Up, _) => self.set_focus_row(self.focus_row.saturating_sub(1)),
            (KeyCode::Down, _) => self.set_focus_row(self.focus_row + 1),
            (KeyCode::PageUp, _) => self.set_focus_row(self.focus_row.saturating_sub(self.page)),
            (KeyCode::PageDown, _) => self.set_focus_row(self.focus_row + self.page),
            (KeyCode::Home, _) => self.set_focus_row(0),
            (KeyCode::End, _) => self.set_focus_row(self.model.rows.len()),
            (KeyCode::Right, _) => self.right(),
            (KeyCode::Left, _) => self.left(),
            (KeyCode::Enter, _) => {
                if let Some((level, key, open)) = self.focused_group() {
                    self.toggle(Ax::Rows, level, key, open);
                }
            }
            (KeyCode::Char('*'), _) => self.open_all_under(),
            (KeyCode::Char(d @ '0'..='3'), _) => {
                let depth = d as usize - '0' as usize;
                self.spec.rows_expand = Expand {
                    depth,
                    except: BTreeSet::new(),
                };
                self.last = format!("Expand to level {depth} (answered as Refiltered)");
                self.rebuild();
            }
            (KeyCode::Char('m'), _) => self.open_menu(self.focus_col.clone(), None),
            _ => {}
        }
    }

    fn preset(&mut self, pivot: bool) {
        self.preset_pivot = pivot;
        let (form, corner) = (self.spec.row_form, self.spec.corner);
        self.spec = if pivot {
            Spec::pivot()
        } else {
            Spec::grouped()
        };
        self.spec.row_form = form;
        self.spec.corner = corner;
        self.col_scroll = 0;
        self.focus_col = FCol::Band(0);
        self.last = format!("Preset: {}", if pivot { "pivot" } else { "grouped" });
        self.rebuild();
    }

    fn open_menu(&mut self, col: FCol, at: Option<(u16, u16)>) {
        let s = &self.spec;
        let mut items: Vec<(String, MenuAct)> = vec![];
        let unused =
            |f: &Field| !f.numeric() && !s.row_levels.contains(f) && !s.col_levels.contains(f);
        let title;
        match &col {
            FCol::Band(_) => {
                title = "Row levels".to_string();
                for &f in &s.row_levels {
                    items.push((
                        format!("Remove row level {}", f.name()),
                        MenuAct::RemoveRowLevel(f),
                    ));
                }
                for f in FIELDS.iter().filter(|f| unused(f)) {
                    items.push((
                        format!("Group rows by {}", f.name()),
                        MenuAct::GroupRows(*f),
                    ));
                }
            }
            FCol::Body(k) => {
                let Some(c) = self.model.cols.iter().find(|c| c.key(s) == *k) else {
                    return;
                };
                match c {
                    ColItem::Base(f) => {
                        title = f.name().to_string();
                        if unused(f) {
                            items.push((
                                format!("Group rows by {}", f.name()),
                                MenuAct::GroupRows(*f),
                            ));
                            items.push((
                                format!("Add {} as column level", f.name()),
                                MenuAct::AddColLevel(*f),
                            ));
                        }
                        let aggs: &[Agg] = if f.numeric() {
                            &[Agg::Sum, Agg::Avg, Agg::Min, Agg::Max]
                        } else {
                            &[]
                        };
                        for &agg in aggs {
                            let v = Value {
                                field: Some(*f),
                                agg,
                            };
                            if !s.values.contains(&v) {
                                items.push((
                                    format!("Add value {}", v.label()),
                                    MenuAct::AddValue(v),
                                ));
                            }
                        }
                    }
                    ColItem::Pivot { value, .. } => {
                        title = s.values[*value].label();
                        for &f in &s.col_levels {
                            items.push((
                                format!("Remove column level {}", f.name()),
                                MenuAct::RemoveColLevel(f),
                            ));
                            items.push((
                                format!("Move {} to rows", f.name()),
                                MenuAct::MoveToRows(f),
                            ));
                        }
                        if s.values.len() > 1 {
                            items.push((
                                format!("Remove value {}", s.values[*value].label()),
                                MenuAct::RemoveValue(*value),
                            ));
                        }
                        for f in [Field::Weight, Field::Crew] {
                            for agg in [Agg::Sum, Agg::Avg, Agg::Max] {
                                let v = Value {
                                    field: Some(f),
                                    agg,
                                };
                                if !s.values.contains(&v) {
                                    items.push((
                                        format!("Add value {}", v.label()),
                                        MenuAct::AddValue(v),
                                    ));
                                }
                            }
                        }
                        for f in FIELDS.iter().filter(|f| unused(f)) {
                            items.push((
                                format!("Add {} as column level", f.name()),
                                MenuAct::AddColLevel(*f),
                            ));
                        }
                    }
                }
            }
        }
        if !self.spec.values.iter().any(|v| v.field.is_none()) {
            items.push((
                "Add value count".into(),
                MenuAct::AddValue(Value {
                    field: None,
                    agg: Agg::Count,
                }),
            ));
        }
        if items.is_empty() {
            return;
        }
        let at = at.or_else(|| self.hit_at(&col)).unwrap_or((2, 4));
        self.menu = Some(Menu {
            title,
            items,
            sel: 0,
            at,
        });
    }

    fn hit_at(&self, col: &FCol) -> Option<(u16, u16)> {
        self.hits.iter().find_map(|(r, h)| match h {
            Hit::Header(c) if c == col => Some((r.x, r.bottom() + 1)),
            _ => None,
        })
    }

    fn run_menu(&mut self, act: MenuAct) {
        let s = &mut self.spec;
        match &act {
            MenuAct::GroupRows(f) => s.row_levels.push(*f),
            MenuAct::AddColLevel(f) => s.col_levels.push(*f),
            MenuAct::AddValue(v) => s.values.push(*v),
            MenuAct::RemoveRowLevel(f) => s.row_levels.retain(|x| x != f),
            MenuAct::RemoveColLevel(f) => s.col_levels.retain(|x| x != f),
            MenuAct::MoveToRows(f) => {
                s.col_levels.retain(|x| x != f);
                s.row_levels.push(*f);
            }
            MenuAct::RemoveValue(i) => {
                s.values.remove(*i);
            }
        }
        // Levels changed: Keys built from paths no longer fit, so pruned (#185).
        if !matches!(act, MenuAct::AddValue(_) | MenuAct::RemoveValue(_)) {
            s.rows_expand.except.clear();
            s.cols_expand.except.clear();
        }
        self.last = format!("{act:?}");
        self.col_scroll = 0;
        self.rebuild();
    }

    fn mouse(&mut self, m: MouseEvent) {
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains((m.column, m.row).into()))
            .map(|(_, h)| h.clone());
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(Hit::MenuItem(i)) => {
                    let act = self.menu.as_ref().map(|mm| mm.items[i].1.clone());
                    self.menu = None;
                    if let Some(a) = act {
                        self.run_menu(a);
                    }
                }
                _ if self.menu.is_some() => self.menu = None,
                Some(Hit::Toggle(ax, level, key)) => {
                    let actual = match ax {
                        Ax::Rows => self.spec.rows_expand.is_open(level, &key),
                        Ax::Cols => self.spec.cols_expand.is_open(level, &key),
                    };
                    self.toggle(ax, level, key, actual);
                }
                Some(Hit::Cell(r, c)) => {
                    self.set_focus_row(r);
                    self.focus_col = c;
                }
                Some(Hit::Header(c)) => self.focus_col = c,
                None => {}
            },
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some(Hit::Header(c) | Hit::Cell(_, c)) = hit {
                    self.focus_col = c.clone();
                    self.open_menu(c, Some((m.column, m.row + 1)));
                }
            }
            MouseEventKind::ScrollDown if m.modifiers.contains(KeyModifiers::SHIFT) => {
                self.col_scroll += 1
            }
            MouseEventKind::ScrollUp if m.modifiers.contains(KeyModifiers::SHIFT) => {
                self.col_scroll = self.col_scroll.saturating_sub(1)
            }
            MouseEventKind::ScrollDown => self.row_scroll += 3,
            MouseEventKind::ScrollUp => self.row_scroll = self.row_scroll.saturating_sub(3),
            MouseEventKind::ScrollRight => self.col_scroll += 1,
            MouseEventKind::ScrollLeft => self.col_scroll = self.col_scroll.saturating_sub(1),
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ drawing

fn put(buf: &mut Buffer, x: u16, y: u16, w: u16, s: &str, style: Style, right: bool) {
    if w == 0 {
        return;
    }
    for i in 0..w {
        buf[(x + i, y)].set_char(' ').set_style(style);
    }
    let len = s.chars().count() as u16;
    let x0 = if right && len < w { x + w - len } else { x };
    buf.set_stringn(x0, y, s, (x + w - x0) as usize, style);
}

fn patch(buf: &mut Buffer, r: Rect, style: Style) {
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            let c = &mut buf[(x, y)];
            let s = c.style().patch(style);
            c.set_style(s);
        }
    }
}

struct DrawnCol {
    col: usize,
    x: u16,
    w: u16,
    strip: u8,
}

fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme();
    let ro = roles(&theme);
    app.hits.clear();
    buf.set_style(area, theme.body);
    let harness = Style::default().fg(Color::Black).bg(Color::Yellow);
    let h = format!(
        " PROTOTYPE #186  F2 preset: {}  F3 rows: {:?}  F4 corner: {:?}  F5 pin 3 left: {}  F6 theme: {}  F7 state  q quit",
        if app.preset_pivot { "pivot" } else { "grouped" },
        app.spec.form(),
        app.spec.corner,
        if app.pin_left { "on" } else { "off" },
        theme.name
    );
    put(buf, area.x, area.y, area.width, &h, harness, false);

    let pane_w = if app.state_pane && area.width >= 110 {
        46
    } else {
        0
    };
    let grid = Rect::new(
        area.x,
        area.y + 1,
        area.width - pane_w,
        area.height.saturating_sub(2),
    );
    let status_y = area.bottom() - 1;
    put(
        buf,
        area.x,
        status_y,
        area.width,
        &format!(
            " Right open/step in  Left close/step out  * open all under  1-3 to level  0 collapse  Enter toggle  m menu  | {}",
            app.last
        ),
        theme.row_number.add_modifier(Modifier::REVERSED),
        false,
    );
    if pane_w > 0 {
        draw_state(
            app,
            Rect::new(grid.right(), grid.y, pane_w, grid.height),
            buf,
        );
    }

    let spec = app.spec.clone();
    let compact = spec.corner == Corner::Compact;
    let extra = usize::from(compact && spec.pivot_on());
    let head_h = (app.model.tiers + extra) as u16;
    let rule_y = grid.y + head_h;
    let body_y = rule_y + 1;
    let gt_y = grid.bottom() - 1;
    let gt_rule_y = gt_y - 1;
    let body_h = gt_rule_y.saturating_sub(body_y) as usize;
    app.page = body_h.max(1);

    // Band columns.
    let nb = app.band_count();
    let band_w: Vec<u16> = if spec.form() == Form::Outline {
        vec![26]
    } else {
        vec![17; nb]
    };
    let mut band = vec![];
    let mut x = grid.x;
    for w in &band_w {
        band.push((x, *w));
        x += w;
    }
    let band_rule_x = x;
    let body_x = x + 1;

    // Body columns: pinned left, scrolling, pinned right.
    let (lead, scroll, trail) = app.strips();
    let mut drawn: Vec<DrawnCol> = vec![];
    let mut x = body_x;
    for &c in &lead {
        let w = app.model.cols[c].width();
        drawn.push(DrawnCol {
            col: c,
            x,
            w,
            strip: 0,
        });
        x += w;
    }
    let lead_edge = (!lead.is_empty()).then_some(x);
    if lead_edge.is_some() {
        x += 1;
    }
    let trail_w: u16 = trail
        .iter()
        .map(|&c| app.model.cols[c].width())
        .sum::<u16>()
        + u16::from(!trail.is_empty());
    let scroll_end = grid.right().saturating_sub(trail_w);
    // Keep the focused column in view.
    if app.reveal
        && let FCol::Body(k) = &app.focus_col
        && let Some(si) = scroll
            .iter()
            .position(|&c| app.model.cols[c].key(&spec) == *k)
    {
        if si < app.col_scroll {
            app.col_scroll = si;
        }
        loop {
            let used: u16 = scroll[app.col_scroll..=si]
                .iter()
                .map(|&c| app.model.cols[c].width())
                .sum();
            if x + used <= scroll_end || app.col_scroll >= si {
                break;
            }
            app.col_scroll += 1;
        }
    }
    app.col_scroll = app.col_scroll.min(scroll.len().saturating_sub(1));
    for &c in scroll.iter().skip(app.col_scroll) {
        // Only whole columns: a clipped number reads as a smaller one.
        let full = app.model.cols[c].width();
        if x >= scroll_end || (x + full > scroll_end && drawn.iter().any(|d| d.strip == 1)) {
            break;
        }
        let w = full.min(scroll_end - x);
        drawn.push(DrawnCol {
            col: c,
            x,
            w,
            strip: 1,
        });
        x += w;
    }
    let trail_edge = (!trail.is_empty()).then_some(scroll_end);
    let mut x = scroll_end + 1;
    for &c in &trail {
        let w = app.model.cols[c]
            .width()
            .min(grid.right().saturating_sub(x));
        drawn.push(DrawnCol {
            col: c,
            x,
            w,
            strip: 2,
        });
        x += w;
    }

    // Rows: keep focus in view.
    let nrows = app.model.rows.len();
    if app.reveal && app.focus_row < nrows {
        if app.focus_row < app.row_scroll {
            app.row_scroll = app.focus_row;
        } else if app.focus_row >= app.row_scroll + body_h {
            app.row_scroll = app.focus_row + 1 - body_h;
        }
    }
    app.reveal = false;
    app.row_scroll = app.row_scroll.min(nrows.saturating_sub(body_h));
    let visible: Vec<(usize, u16)> = (app.row_scroll..nrows.min(app.row_scroll + body_h))
        .enumerate()
        .map(|(i, r)| (r, body_y + i as u16))
        .chain(std::iter::once((nrows, gt_y)))
        .collect();

    // Body cells.
    for &(ri, y) in &visible {
        let r = app.row(ri).clone();
        let row_style = match r.kind {
            Kind::Leaf => match theme.zebra {
                Some(z) if ri % 2 == 1 => theme.body.patch(z),
                _ => theme.body,
            },
            Kind::Group => theme.body.patch(ro.group_item),
            Kind::Subtotal => theme.body.patch(ro.subtotal),
            Kind::Grand => theme.body.patch(ro.grand_total),
        };
        for d in &drawn {
            let c = &app.model.cols[d.col];
            let t = cell_text(&spec, &app.ships, &r, c);
            let num =
                matches!(c, ColItem::Pivot { .. }) || matches!(c, ColItem::Base(f) if f.numeric());
            let style = if c.grand()
                || matches!(
                    c,
                    ColItem::Pivot {
                        kind: Kind::Subtotal,
                        ..
                    }
                ) {
                row_style.patch(ro.subtotal.add_modifier(Modifier::BOLD))
            } else {
                row_style
            };
            put(buf, d.x, y, d.w, &format!(" {t} "), style, num);
            let fc = FCol::Body(c.key(&spec));
            if ri == app.focus_row && app.focus_col == fc {
                patch(buf, Rect::new(d.x, y, d.w, 1), theme.focus);
            }
            app.hits
                .push((Rect::new(d.x, y, d.w, 1), Hit::Cell(ri, fc)));
        }
        let gap_x = drawn
            .iter()
            .filter(|d| d.strip < 2)
            .map(|d| d.x + d.w)
            .max()
            .unwrap_or(body_x);
        if gap_x < scroll_end {
            put(buf, gap_x, y, scroll_end - gap_x, "", row_style, false);
        }
        draw_band_row(
            app, &spec, &ro, &theme, &band, &visible, ri, y, row_style, buf,
        );
    }

    // Rules and pinned edges.
    for x in grid.x..grid.right() {
        buf[(x, rule_y)].set_char('─').set_style(theme.border);
        buf[(x, gt_rule_y)]
            .set_char('═')
            .set_style(theme.pinned_edge);
    }
    for y in grid.y..gt_y + 1 {
        let ch = if y == rule_y {
            '┼'
        } else if y == gt_rule_y {
            '╪'
        } else {
            '│'
        };
        buf[(band_rule_x, y)].set_char(ch).set_style(theme.border);
        for e in [lead_edge, trail_edge].into_iter().flatten() {
            let ch = if y == rule_y {
                '╫'
            } else if y == gt_rule_y {
                '╬'
            } else {
                '║'
            };
            buf[(e, y)].set_char(ch).set_style(theme.pinned_edge);
        }
    }

    // Column Header Band: tiers, spans merged by Key within each strip, so a
    // span breaks at a pinned edge, its label kept at the strip's start.
    let tier_y0 = grid.y + extra as u16;
    for t in 0..app.model.tiers {
        let y = tier_y0 + t as u16;
        let mut i = 0;
        while i < drawn.len() {
            let c0 = &app.model.cols[drawn[i].col];
            let cell = tier_cell(&spec, c0, t);
            let mut j = i + 1;
            while j < drawn.len()
                && drawn[j].strip == drawn[i].strip
                && tier_cell(&spec, &app.model.cols[drawn[j].col], t).map(|c| c.0)
                    == cell.as_ref().map(|c| c.0.clone())
                && cell.is_some()
            {
                j += 1;
            }
            let x0 = drawn[i].x;
            let w = drawn[j - 1].x + drawn[j - 1].w - x0;
            let is_value_tier = spec.pivot_on() && t == spec.col_levels.len();
            let style = if c0.grand()
                || matches!(
                    c0,
                    ColItem::Pivot {
                        kind: Kind::Subtotal,
                        ..
                    }
                ) {
                theme.header.patch(ro.subtotal)
            } else {
                theme.header
            };
            match &cell {
                Some((_, label, toggle)) => {
                    // A level tier's cell is a span even where only one of
                    // its columns is drawn.
                    let span = j - i > 1 || toggle.is_some() || (spec.pivot_on() && !is_value_tier);
                    let mark = match toggle {
                        Some((level, key)) => {
                            let actual = spec.cols_expand.is_open(*level, key);
                            if app.pending.iter().any(|p| p.mark == *key) {
                                "⋯ "
                            } else if actual {
                                "▾ "
                            } else {
                                "▸ "
                            }
                        }
                        None => "",
                    };
                    let text = format!(" {mark}{label} ");
                    let num =
                        !span && (is_value_tier || matches!(c0, ColItem::Base(f) if f.numeric()));
                    put(buf, x0, y, w, &text, style, num);
                    if span {
                        // A thin rule shows how far the span reaches.
                        let start = x0 + text.chars().count() as u16;
                        for xx in start..x0 + w {
                            buf[(xx, y)].set_char('─').set_style(theme.border);
                        }
                        // The cap marks the span's true end, not a clipped one.
                        let next = app
                            .model
                            .cols
                            .get(drawn[j - 1].col + 1)
                            .and_then(|c| tier_cell(&spec, c, t));
                        let continues = next.map(|n| n.0) == cell.as_ref().map(|c| c.0.clone());
                        if j - i > 1 && !continues {
                            buf[(x0 + w - 1, y)].set_char('┐').set_style(theme.border);
                        }
                    }
                    if let Some((level, key)) = toggle {
                        app.hits.push((
                            Rect::new(x0, y, 3, 1),
                            Hit::Toggle(Ax::Cols, *level, key.clone()),
                        ));
                    }
                }
                None => put(buf, x0, y, w, "", style, false),
            }
            if t + 1 == app.model.tiers {
                for d in &drawn[i..j] {
                    let fc = FCol::Body(app.model.cols[d.col].key(&spec));
                    if app.focus_col == fc {
                        patch(
                            buf,
                            Rect::new(d.x, y, d.w, 1),
                            theme.sort_marker.add_modifier(Modifier::UNDERLINED),
                        );
                    }
                    app.hits.push((Rect::new(d.x, y, d.w, 1), Hit::Header(fc)));
                }
            }
            i = j;
        }
    }
    if extra == 1 {
        let w = grid.right() - body_x;
        let names: Vec<&str> = spec.col_levels.iter().map(|f| f.name()).collect();
        put(
            buf,
            body_x,
            grid.y,
            w,
            &format!(" Column labels ▾  {}", names.join(" › ")),
            theme.header.patch(ro.level_name),
            false,
        );
    }
    draw_corner(app, &spec, &ro, &theme, &band, grid.y, head_h, buf);
    for (bi, (bx, bw)) in band.iter().enumerate() {
        app.hits.push((
            Rect::new(*bx, grid.y, *bw, head_h),
            Hit::Header(FCol::Band(bi)),
        ));
    }

    if let Some(menu) = &app.menu {
        let w = menu
            .items
            .iter()
            .map(|(s, _)| s.chars().count())
            .max()
            .unwrap_or(10) as u16
            + 4;
        let h = menu.items.len() as u16 + 2;
        let x = menu.at.0.min(area.right().saturating_sub(w));
        let y = menu.at.1.min(area.bottom().saturating_sub(h));
        let r = Rect::new(x, y, w, h).intersection(area);
        Clear.render(r, buf);
        Block::bordered()
            .title(format!(" {} ", menu.title))
            .style(theme.header)
            .render(r, buf);
        for (i, (label, _)) in menu
            .items
            .iter()
            .enumerate()
            .take(r.height.saturating_sub(2) as usize)
        {
            let style = if i == menu.sel {
                theme.focus
            } else {
                theme.body
            };
            put(
                buf,
                x + 1,
                y + 1 + i as u16,
                w - 2,
                &format!(" {label}"),
                style,
                false,
            );
            app.hits.push((
                Rect::new(x + 1, y + 1 + i as u16, w - 2, 1),
                Hit::MenuItem(i),
            ));
        }
    }
}

/// The row Header Band for one row: the outline column, or one column per
/// level in header form with each group a span down its rows, its label at
/// the top of the span's visible part.
#[allow(clippy::too_many_arguments)]
fn draw_band_row(
    app: &mut App,
    spec: &Spec,
    ro: &Roles,
    theme: &Theme,
    band: &[(u16, u16)],
    visible: &[(usize, u16)],
    ri: usize,
    y: u16,
    row_style: Style,
    buf: &mut Buffer,
) {
    let r = app.row(ri).clone();
    let n = spec.row_levels.len();
    let pivot = spec.pivot_on();
    let count = |idx: &[usize]| {
        if spec.count_caption() {
            format!(" ({})", idx.len())
        } else {
            String::new()
        }
    };
    let mark_for = |app: &App, key: &str, open: bool| -> (&'static str, Style) {
        if app.pending.iter().any(|p| p.mark == key) {
            ("⋯", ro.pending)
        } else if open {
            ("▾", ro.disclosure)
        } else {
            ("▸", ro.disclosure)
        }
    };
    let band_style = row_style.patch(theme.header).patch(match r.kind {
        Kind::Leaf => theme.row_number,
        Kind::Group => ro.group_item,
        Kind::Subtotal => ro.subtotal,
        Kind::Grand => ro.grand_total,
    });
    if spec.form() == Form::Outline || r.kind == Kind::Grand {
        let (bx, _) = band[0];
        let bw: u16 = band.iter().map(|b| b.1).sum();
        let indent = "  ".repeat(r.depth);
        let last = r.path.last().map(|p| p.1.clone()).unwrap_or_default();
        match r.kind {
            Kind::Group => {
                put(buf, bx, y, bw, "", band_style, false);
                let (mark, ms) = match r.open {
                    Some(o) => mark_for(app, &r.key, o),
                    None => (" ", ro.disclosure),
                };
                let mx = bx + 1 + indent.len() as u16;
                buf.set_string(mx, y, mark, band_style.patch(ms));
                buf.set_stringn(
                    mx + 2,
                    y,
                    format!("{last}{}", count(&r.ships)),
                    (bx + bw).saturating_sub(mx + 2) as usize,
                    band_style,
                );
                if r.open.is_some() {
                    app.hits.push((
                        Rect::new(mx, y, 1, 1),
                        Hit::Toggle(Ax::Rows, r.depth, r.key.clone()),
                    ));
                }
            }
            Kind::Subtotal => put(
                buf,
                bx,
                y,
                bw,
                &format!(" {indent}  {last} total{}", count(&r.ships)),
                band_style,
                false,
            ),
            Kind::Grand => put(
                buf,
                bx,
                y,
                bw,
                &format!(" Grand total{}", count(&r.ships)),
                band_style,
                false,
            ),
            Kind::Leaf => put(
                buf,
                bx,
                y,
                bw,
                &format!(" {indent}  #{}", app.ships[r.ships[0]].id),
                band_style,
                false,
            ),
        }
        if ri == app.focus_row && matches!(app.focus_col, FCol::Band(_)) {
            patch(buf, Rect::new(bx, y, bw, 1), theme.focus);
        }
        app.hits
            .push((Rect::new(bx, y, bw, 1), Hit::Cell(ri, FCol::Band(0))));
        // The toggle hit goes last so it wins over the cell.
        if let Some(pos) = app.hits.iter().position(|(rr, h)| {
            matches!(h, Hit::Toggle(Ax::Rows, _, k) if *k == r.key) && rr.y == y
        }) {
            let t = app.hits.remove(pos);
            app.hits.push(t);
        }
        return;
    }
    // Header form.
    let vis_index = visible.iter().position(|v| v.0 == ri).unwrap_or(0);
    let prev = (vis_index > 0 && visible[vis_index - 1].0 < app.model.rows.len())
        .then(|| app.row(visible[vis_index - 1].0).clone());
    for (l, &(bx, bw)) in band.iter().enumerate() {
        // An open group's Subtotal sits outside its own span, labelled in
        // the group's column; a closed group's Subtotal is all that is left
        // under the span, so it keeps the span and its mark.
        let span_of = |row: &RowItem| -> Option<String> {
            let own =
                row.kind == Kind::Subtotal && row.path.len() == l + 1 && row.open == Some(true);
            (row.path.len() > l && row.kind != Kind::Grand && !own)
                .then(|| group_key(&row.path[..=l]))
        };
        let style = theme.header.patch(theme.row_number);
        let key = span_of(&r);
        match &key {
            Some(k) => {
                let starts = prev.as_ref().and_then(span_of).as_ref() != Some(k);
                let can_open = l + 1 < n || !pivot;
                let open = spec.rows_expand.is_open(l, k);
                put(buf, bx, y, bw, "", style, false);
                if starts {
                    let label = &r.path[l].1;
                    if can_open {
                        let (mark, ms) = mark_for(app, k, open);
                        buf.set_string(bx + 1, y, mark, style.patch(ms));
                        app.hits.push((
                            Rect::new(bx + 1, y, 1, 1),
                            Hit::Toggle(Ax::Rows, l, k.clone()),
                        ));
                    }
                    buf.set_stringn(
                        bx + 3,
                        y,
                        label,
                        bw.saturating_sub(4) as usize,
                        style.add_modifier(Modifier::BOLD),
                    );
                } else {
                    buf.set_string(bx + 1, y, "│", theme.border);
                }
            }
            None if r.kind == Kind::Subtotal && l + 1 == r.path.len() => {
                let rest: u16 = band[l..].iter().map(|b| b.1).sum();
                put(
                    buf,
                    bx,
                    y,
                    rest,
                    &format!(" {} total{}", r.path[l].1, count(&r.ships)),
                    band_style,
                    false,
                );
            }
            None if r.kind == Kind::Subtotal && l + 1 > r.path.len() => {}
            None => put(buf, bx, y, bw, "", style, false),
        }
        if ri == app.focus_row && app.focus_col == FCol::Band(l) {
            patch(buf, Rect::new(bx, y, bw, 1), theme.focus);
        }
        app.hits
            .push((Rect::new(bx, y, bw, 1), Hit::Cell(ri, FCol::Band(l))));
        if let Some(pos) = app.hits.iter().position(|(rr, h)| {
            matches!(h, Hit::Toggle(Ax::Rows, _, _)) && rr.y == y && rr.x == bx + 1
        }) {
            let t = app.hits.remove(pos);
            app.hits.push(t);
        }
    }
}

/// The corner grid where the row band's columns meet the column tiers.
#[allow(clippy::too_many_arguments)]
fn draw_corner(
    app: &App,
    spec: &Spec,
    ro: &Roles,
    theme: &Theme,
    band: &[(u16, u16)],
    y0: u16,
    h: u16,
    buf: &mut Buffer,
) {
    let style = theme.header;
    let names = style.patch(ro.level_name);
    let bw: u16 = band.iter().map(|b| b.1).sum();
    for y in y0..y0 + h {
        put(buf, band[0].0, y, bw, "", style, false);
    }
    let caption = match spec.values.as_slice() {
        [v] if spec.pivot_on() => Some(v.label()),
        _ => None,
    };
    let bottom = y0 + h - 1;
    if spec.corner == Corner::Compact {
        let cap = if spec.pivot_on() {
            spec.values
                .iter()
                .map(|v| v.label())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            String::new()
        };
        if !cap.is_empty() {
            put(buf, band[0].0, y0, bw, &format!(" {cap}"), style, false);
        }
        put(buf, band[0].0, bottom, bw, " Row labels ▾", names, false);
        return;
    }
    // Tiers: column level names right-aligned in their tier rows, the values
    // pseudo-level as Σ, the row level names along the bottom row.
    let (lx, lw) = *band.last().unwrap();
    if spec.pivot_on() {
        for t in 0..app.model.tiers {
            let name = match spec.col_levels.get(t) {
                Some(f) => format!("{} › ", f.name()),
                None => "Σ values › ".into(),
            };
            let y = y0 + t as u16;
            if y == bottom && band.len() > 1 {
                // The bottom row is the row levels'; the last band column
                // shares it with the deepest tier's name.
                continue;
            }
            put(buf, lx, y, lw, &name, names, true);
        }
        if let Some(c) = caption {
            put(
                buf,
                band[0].0,
                y0,
                if band.len() > 1 {
                    band[0].1
                } else {
                    bw.saturating_sub(12)
                },
                &format!(" {c}"),
                style,
                false,
            );
        }
    }
    let row_names: Vec<&str> = spec.row_levels.iter().map(|f| f.name()).collect();
    if spec.form() == Form::Outline {
        let deepest = if spec.pivot_on() && h == 1 {
            format!("{} › ", spec.col_levels.last().unwrap().name())
        } else {
            String::new()
        };
        let left = format!(" {}", row_names.join(" › "));
        put(buf, band[0].0, bottom, bw, &left, names, false);
        if !deepest.is_empty() || (spec.pivot_on() && h > 0) {
            let tail = if h == 1 {
                deepest
            } else {
                match spec.col_levels.get(h as usize - 1) {
                    Some(f) => format!("{} › ", f.name()),
                    None => "Σ values › ".into(),
                }
            };
            let len = tail.chars().count() as u16;
            if left.chars().count() as u16 + len < bw {
                put(buf, band[0].0 + bw - len, bottom, len, &tail, names, false);
            }
        }
    } else {
        for (i, (bx, w)) in band.iter().enumerate() {
            let n = row_names.get(i).copied().unwrap_or("");
            put(buf, *bx, bottom, *w, &format!(" {n}"), names, false);
        }
        if spec.pivot_on() {
            let tail = match spec.col_levels.get(h as usize - 1) {
                Some(f) => format!("{} › ", f.name()),
                None => "Σ values › ".into(),
            };
            let len = tail.chars().count() as u16;
            let used = row_names.last().map(|s| s.len() as u16 + 1).unwrap_or(0);
            if used + len < lw {
                put(buf, lx + lw - len, bottom, len, &tail, names, false);
            }
        }
    }
}

fn draw_state(app: &App, r: Rect, buf: &mut Buffer) {
    let style = Style::default().fg(Color::Gray).bg(Color::Rgb(17, 24, 39));
    let s = &app.spec;
    let names = |fs: &[Field]| {
        fs.iter()
            .map(|f| format!("{} after", f.name()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let expand = |e: &Expand| {
        let d = if e.depth >= 99 {
            "all".to_string()
        } else {
            e.depth.to_string()
        };
        let mut v = vec![format!("  {{ depth: {d}, except: [")];
        for k in &e.except {
            v.push(format!("     {k}"));
        }
        v.push("  ] }".into());
        v
    };
    let mut lines: Vec<String> = vec![
        "VIEW SPEC grouping".into(),
        format!(" rowLevels: [{}]", names(&s.row_levels)),
        format!(" colLevels: [{}]", names(&s.col_levels)),
        format!(
            " values: [{}]",
            s.values
                .iter()
                .map(|v| v.label())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        " grandTotal: rows after (pinned),".into(),
        "             cols after (pinned)".into(),
        format!(" rows form: {:?}  corner: {:?}", s.form(), s.corner),
        " rows.expand:".into(),
    ];
    lines.extend(expand(&s.rows_expand));
    if s.pivot_on() && s.col_levels.len() > 1 {
        lines.push(" cols.expand:".into());
        lines.extend(expand(&s.cols_expand));
    }
    let f = app.row(app.focus_row);
    lines.push(String::new());
    lines.push("FOCUSED ITEM outline".into());
    lines.push(format!(" key: {}", f.key));
    lines.push(format!(" kind: {:?}  depth: {}", f.kind, f.depth));
    lines.push(format!(
        " open: {}",
        match f.open {
            Some(true) => "open",
            Some(false) => "closed",
            None => "can't open",
        }
    ));
    lines.push(format!(
        " parent: {}",
        f.parent.clone().unwrap_or("none".into())
    ));
    lines.push(" ancestors:".into());
    for a in f.ancestors() {
        lines.push(format!("  {a}"));
    }
    lines.push(format!(" column: {:?}", app.focus_col));
    lines.push(String::new());
    lines.push(format!("PENDING ({})", app.pending.len()));
    let now = Instant::now();
    for p in &app.pending {
        let what = match &p.op {
            Op::Toggle { open, .. } => if *open { "Expand" } else { "Collapse" }.to_string(),
            Op::OpenAll { keys } => format!("OpenAll {}", keys.len()),
        };
        lines.push(format!(
            " {what} {} {}ms",
            p.mark,
            p.due.saturating_duration_since(now).as_millis()
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "rows: {} items + grand total",
        app.model.rows.len()
    ));
    lines.push(format!(
        "cols: {} items, {} tiers",
        app.model.cols.len(),
        app.model.tiers
    ));
    for (i, l) in lines.iter().take(r.height as usize).enumerate() {
        put(
            buf,
            r.x,
            r.y + i as u16,
            r.width,
            &format!(" {l}"),
            style,
            false,
        );
    }
    for y in r.y + lines.len().min(r.height as usize) as u16..r.bottom() {
        put(buf, r.x, y, r.width, "", style, false);
    }
}

// ------------------------------------------------------------------ main

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "--dump") {
        dump();
        return Ok(());
    }
    let mut app = App::new(std::env::args().any(|a| a == "--pivot"));
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

fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Lets every pending toggle land.
fn settle(app: &mut App) {
    app.tick(Instant::now() + LATENCY * 2);
}

/// Prints scripted scenes, drawn on a TestBackend.
fn dump() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let arg = |name: &str| {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let only = arg("--scene");
    let width = arg("--width").and_then(|w| w.parse().ok()).unwrap_or(150);
    let height = arg("--height").and_then(|w| w.parse().ok()).unwrap_or(32);
    type Scene = (&'static str, bool, fn(&mut App));
    let scenes: [Scene; 17] = [
        ("grouped", false, |_| {}),
        ("grouped-scrolled", false, |a| a.set_focus_row(40)),
        ("grouped-pending", false, |a| {
            press(a, KeyCode::Down);
            press(a, KeyCode::Left);
        }),
        ("grouped-closed-port", false, |a| {
            press(a, KeyCode::Down);
            press(a, KeyCode::Left);
            settle(a);
            press(a, KeyCode::Left);
        }),
        ("grouped-level-1", false, |a| press(a, KeyCode::Char('1'))),
        ("grouped-star", false, |a| {
            press(a, KeyCode::Char('0'));
            press(a, KeyCode::Down);
            press(a, KeyCode::Char('*'));
            settle(a);
        }),
        ("grouped-header-form", false, |a| {
            press(a, KeyCode::F(3));
            a.set_focus_row(30);
        }),
        ("grouped-header-collapsed", false, |a| {
            press(a, KeyCode::F(3));
            press(a, KeyCode::Char('1'));
            a.set_focus_row(2);
            a.focus_col = FCol::Band(1);
        }),
        ("grouped-menu", false, |a| {
            a.focus_col = FCol::Body("(col, quarter)".into());
            press(a, KeyCode::Char('m'));
        }),
        ("grouped-add-col-level", false, |a| {
            a.run_menu(MenuAct::AddColLevel(Field::Quarter))
        }),
        ("pivot-two-col-levels", true, |a| {
            a.spec.col_levels.insert(0, Field::Cargo);
            a.spec
                .cols_expand
                .set(0, &col_group_key(&[(Field::Cargo, "coal".into())]), false);
            a.rebuild();
        }),
        ("pivot-menu", true, |a| {
            a.focus_col = FCol::Body(a.model.cols[1].key(&a.spec));
            press(a, KeyCode::Char('m'));
        }),
        ("pivot", true, |_| {}),
        ("pivot-header-form", true, |a| press(a, KeyCode::F(3))),
        ("pivot-compact", true, |a| press(a, KeyCode::F(4))),
        ("pivot-pinned-break", true, |a| {
            press(a, KeyCode::F(5));
        }),
        ("pivot-scrolled", true, |a| {
            a.col_scroll = 3;
            press(a, KeyCode::Char('1'));
        }),
    ];
    for (name, pivot, setup) in &scenes {
        if only.as_deref().is_some_and(|o| !name.starts_with(o)) {
            continue;
        }
        let mut app = App::new(*pivot);
        app.state_pane = arg("--state").is_some();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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
        println!("=== {name}");
        let buf = terminal.backend().buffer().clone();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            println!("{}", line.trim_end());
        }
    }
}
