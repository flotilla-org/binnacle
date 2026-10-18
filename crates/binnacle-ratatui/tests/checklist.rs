//! The list-of-values checklist in the column filter popup: the values the
//! host hands it through `fetch`, asked again only when the search or the
//! narrowing changes; the keys that tick them, each writing the exact
//! condition (#107); and how it draws. A property mirrors the web's
//! (web/test/checklist.test.ts), so the two ports of the rules can't drift.

use std::time::Duration;

use std::cell::RefCell;

use binnacle_core::{
    Axis, AxisConfig, Blanks, Condition, ConditionKind, ConditionState, Distinct, DistinctQuery,
    DistinctValue, Effect, FilterSpec, Grid, Key, Snapshot, Support, TextQuery, TextSupport,
    Totals, Value, ViewEdit, ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{ColumnFilter, Content, RenderContext, Theme, render_with};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Modifier;

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        ["name", "port", "crew"][col.0 as usize].to_string()
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn char(c: char) -> KeyEvent {
    press(KeyCode::Char(c))
}

const W: u16 = 64;
const H: u16 = 20;

/// Five rows over name, port and crew. Name takes Text conditions; port
/// Text conditions with regexes and OneOf; crew only OneOf.
fn grid() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 12.0).header(4.0).gap(1.0),
    );
    let port = Support::default()
        .with_text(Some(TextSupport::new(true)))
        .with_one_of(true);
    grid.set_support(Axis::Rows, Key(1), port);
    let crew = Support::default().with_text(None).with_one_of(true);
    grid.set_support(Axis::Rows, Key(2), crew);
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(W.into(), H.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

/// Opens the popup on column `col`, with the edits so far drained.
fn open(grid: &mut Grid, col: u64) -> ColumnFilter {
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    assert!(popup.open_column(&snapshot(grid), Key(col)));
    edits(grid);
    popup
}

fn edits(grid: &mut Grid) -> Vec<ViewEdit> {
    let mut out = vec![];
    while let Some(stamped) = grid.poll_effect() {
        if let Effect::ViewRequested { edit, .. } = stamped.effect {
            out.push(edit);
        }
    }
    out
}

/// The ports, two blank, as a source counts them: those containing the
/// search (ignoring case), blanks only unsearched.
fn ports(query: &DistinctQuery) -> Distinct {
    let search = query.search.to_lowercase();
    let values = [("Aberdeen", 1), ("Bergen", 2), ("Cádiz", 1)]
        .into_iter()
        .filter(|(p, _)| p.to_lowercase().contains(&search))
        .map(|(p, n)| DistinctValue {
            text: p.to_string(),
            count: Some(n),
        })
        .collect();
    Distinct {
        values,
        blanks: search.is_empty().then_some(Blanks::Counted(2)),
        partial: false,
        narrowed: true,
    }
}

/// Fetches the ports into the popup, as a host does before each render.
fn fetch(popup: &mut ColumnFilter, grid: &mut Grid) {
    popup.fetch(&snapshot(grid), |_, q| Some(ports(q)));
}

fn text(buf: &Buffer) -> String {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            let line: String = (a.left()..a.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn screen(popup: &ColumnFilter, grid: &mut Grid) -> Buffer {
    let snapshot = snapshot(grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    render_with(&snapshot, &Labels, &ctx, buf.area, &mut buf);
    popup.render(&snapshot, &Labels, &theme, buf.area, &mut buf);
    buf
}

fn one_of(key: u64, values: &[Option<&str>]) -> Condition {
    Condition::one_of(Key(key), values.iter().map(|v| Value::of(*v)).collect())
}

fn contains(key: u64, text: &str) -> Condition {
    Condition::text(Key(key), TextQuery::new(text))
}

fn conditions(list: Vec<Condition>) -> ViewSpec {
    ViewSpec::default().with_filter(
        Axis::Rows,
        FilterSpec::default().with_conditions(list.into()),
    )
}

fn add(c: Condition) -> ViewEdit {
    ViewEdit::AddCondition {
        axis: Axis::Rows,
        condition: c,
    }
}

fn replace(from: &Condition, to: &Condition) -> ViewEdit {
    ViewEdit::ReplaceCondition {
        axis: Axis::Rows,
        from: from.clone(),
        to: to.clone(),
    }
}

#[test]
fn the_popup_opens_on_a_column_that_takes_only_one_of_and_asks_for_its_values_narrowed_by_the_other_columns()
 {
    let mut grid = grid();
    let name = contains(0, "a");
    let own = one_of(2, &[Some("12")]);
    grid.view_applied(
        conditions(vec![name.clone(), own.clone()]),
        Totals::unknown(),
    );
    let mut popup = open(&mut grid, 2);
    let asked = RefCell::new(vec![]);
    let ask = |key: Key, q: &DistinctQuery| {
        asked.borrow_mut().push((key, q.clone()));
        Some(ports(q))
    };
    popup.fetch(&snapshot(&mut grid), ask);
    popup.fetch(&snapshot(&mut grid), ask);
    popup.key(&char('b'), &mut grid);
    popup.fetch(&snapshot(&mut grid), ask);
    let narrow = FilterSpec::default().with_conditions(vec![name].into());
    let query = |search: &str| {
        DistinctQuery::new(1000)
            .search(search)
            .narrow(narrow.clone())
    };
    assert_eq!(
        asked.into_inner(),
        vec![(Key(2), query("")), (Key(2), query("b"))],
        "asked again only once the search changed, and never narrowed by crew's own condition"
    );
    assert_eq!(
        edits(&mut grid),
        vec![],
        "crew's keys search, taking no text"
    );
}

#[test]
fn unticking_from_all_writes_not_one_of_and_ticking_back_removes_it() {
    let mut grid = grid();
    let mut popup = open(&mut grid, 1);
    fetch(&mut popup, &mut grid);
    // Tab past the search to the list: the head, (Blanks), Aberdeen, Bergen.
    for key in [press(KeyCode::Tab), press(KeyCode::Tab)] {
        popup.key(&key, &mut grid);
    }
    for key in [
        press(KeyCode::Down),
        char(' '),
        press(KeyCode::Down),
        press(KeyCode::Down),
        char(' '),
    ] {
        popup.key(&key, &mut grid);
    }
    let blank = one_of(1, &[None]).not(true);
    let both = one_of(1, &[None, Some("Bergen")]).not(true);
    assert_eq!(
        edits(&mut grid),
        vec![add(blank.clone()), replace(&blank, &both)]
    );
    popup.key(&char(' '), &mut grid);
    popup.key(&press(KeyCode::Home), &mut grid);
    popup.key(&char(' '), &mut grid);
    assert_eq!(
        edits(&mut grid),
        vec![
            replace(&both, &blank),
            ViewEdit::RemoveCondition {
                axis: Axis::Rows,
                condition: blank,
            },
        ],
        "ticking the head ticks all, which needs no condition"
    );
}

#[test]
fn ticking_from_none_writes_one_of_and_esc_puts_back_the_conditions_from_before() {
    let mut grid = grid();
    let before = contains(1, "e");
    grid.view_applied(conditions(vec![before.clone()]), Totals::unknown());
    let mut popup = open(&mut grid, 1);
    fetch(&mut popup, &mut grid);
    popup.key(&ctrl('a'), &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::End), &mut grid);
    popup.key(&char(' '), &mut grid);
    let none = one_of(1, &[]);
    assert_eq!(
        edits(&mut grid),
        vec![add(none.clone())],
        "Cádiz has no e, so can't be ticked"
    );
    popup.key(&press(KeyCode::Up), &mut grid);
    popup.key(&char(' '), &mut grid);
    let bergen = one_of(1, &[Some("Bergen")]);
    assert_eq!(edits(&mut grid), vec![replace(&none, &bergen)]);
    popup.key(&press(KeyCode::Esc), &mut grid);
    assert_eq!(popup.column(), None);
    assert_eq!(
        edits(&mut grid),
        vec![ViewEdit::RemoveCondition {
            axis: Axis::Rows,
            condition: bergen,
        }]
    );
}

#[test]
fn select_all_matching_a_search_writes_a_text_condition_and_later_unticks_and_a_not_one_of() {
    let mut grid = grid();
    let mut popup = open(&mut grid, 1);
    popup.key(&press(KeyCode::Tab), &mut grid);
    for c in "EN".chars() {
        popup.key(&char(c), &mut grid);
    }
    fetch(&mut popup, &mut grid);
    popup.key(&ctrl('a'), &mut grid);
    popup.key(&press(KeyCode::Down), &mut grid);
    popup.key(&press(KeyCode::Down), &mut grid);
    popup.key(&char(' '), &mut grid);
    let en = contains(1, "EN");
    assert_eq!(
        edits(&mut grid),
        vec![add(en), add(one_of(1, &[Some("Aberdeen")]).not(true))]
    );
    let buf = screen(&popup, &mut grid);
    insta::assert_snapshot!(text(&buf));
}

#[test]
fn the_checklist_draws_ticks_counts_and_dims_the_values_the_text_condition_rejects_and_says_when_there_are_none()
 {
    let mut grid = grid();
    let en = contains(1, "en");
    let not = one_of(1, &[Some("Bergen")]).not(true);
    grid.view_applied(conditions(vec![en, not]), Totals::unknown());
    let mut popup = open(&mut grid, 1);
    fetch(&mut popup, &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::Down), &mut grid);
    let buf = screen(&popup, &mut grid);
    let row = |label: &str| {
        let lines = text(&buf);
        let y = lines.lines().position(|l| l.contains(label)).expect(label);
        let line = lines.lines().nth(y).unwrap();
        let x = line[..line.find(label).unwrap()].chars().count();
        (x as u16, y as u16)
    };
    let (x, y) = row("Cádiz");
    assert!(
        buf[(x, y)].modifier.contains(Modifier::DIM),
        "rejected by en"
    );
    let (x, y) = row("(Blanks)");
    assert!(
        buf[(x, y)].modifier.contains(Modifier::REVERSED),
        "the cursor"
    );
    let mut out = vec![text(&buf)];
    let mut crew = open(&mut grid, 2);
    crew.fetch(&snapshot(&mut grid), |_, _| None);
    out.push(text(&screen(&crew, &mut grid)));
    let mut empty = open(&mut grid, 2);
    empty.fetch(&snapshot(&mut grid), |_, _| Some(Distinct::default()));
    out.push(text(&screen(&empty, &mut grid)));
    insta::assert_snapshot!(out.join("\n\n"));
}

#[test]
fn a_text_condition_with_an_invalid_pattern_leaves_every_value_tickable() {
    let mut grid = grid();
    let invalid = Condition::text(Key(1), TextQuery::new("(").regex(true));
    grid.view_applied(conditions(vec![invalid]), Totals::unknown());
    let mut popup = open(&mut grid, 1);
    fetch(&mut popup, &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::Down), &mut grid);
    popup.key(&char(' '), &mut grid);
    assert_eq!(edits(&mut grid), vec![add(one_of(1, &[None]).not(true))]);
}

/// The values a checklist may list, as on the web: a blank, empty text, two
/// spellings of one name and a number.
const CANDIDATES: [Option<&str>; 7] = [
    None,
    Some(""),
    Some("Bergen"),
    Some("bergen"),
    Some("Oslo"),
    Some("Cádiz"),
    Some("7"),
];

/// Whether `text` contains `search`, ignoring case, as "select all matching" asks.
fn has(text: &str, search: &str) -> bool {
    text.to_lowercase().contains(&search.to_lowercase())
}

/// Whether `condition` keeps an item whose value is `value`: Text as a
/// contains ignoring case, which never keeps a blank; OneOf exactly.
fn model_keeps(condition: &Condition, value: &Value) -> bool {
    let matched = match (&condition.kind, value) {
        (ConditionKind::OneOf(values), _) => values.contains(value),
        (ConditionKind::Text(q), Value::Text(t)) => has(t, &q.text),
        _ => false,
    };
    matched != condition.not
}

/// Whatever the user ticks, unticks, searches and selects all matching, over
/// a whole or a capped list (which leaves a value out): the conditions written keep exactly the values
/// the user left ticked; values the search rejects can't be ticked; the head
/// is ticked when every value that can be is; ticking every value of a
/// whole list leaves no OneOf; and the other column's condition stays.
#[hegel::test(test_cases = 1000)]
fn whatever_the_user_ticks_the_conditions_keep_exactly_the_values_left_ticked(tc: TestCase) {
    let mut universe: Vec<Value> = (CANDIDATES.iter())
        .filter(|_| tc.draw(gs::booleans()))
        .map(|v| Value::of(*v))
        .collect();
    if universe.is_empty() {
        universe.push(Value::of(tc.draw(gs::sampled_from(CANDIDATES.to_vec()))));
    }
    // A capped list leaves out a value the rows hold: the last text.
    let partial = tc.draw(gs::booleans());
    let unlisted = (universe.iter().rev())
        .find(|v| partial && **v != Value::Blank)
        .cloned();
    let listed = |v: &&Value| Some(*v) != unlisted.as_ref();
    let answer = |q: &DistinctQuery| {
        let search = q.search.as_str();
        let values = (universe.iter().filter(listed))
            .filter_map(|v| match v {
                Value::Text(t) if search.is_empty() || has(t, search) => Some(DistinctValue {
                    text: t.clone(),
                    count: None,
                }),
                _ => None,
            })
            .collect();
        let blank = universe.contains(&Value::Blank) && search.is_empty();
        Distinct {
            values,
            blanks: blank.then_some(Blanks::Uncounted),
            partial,
            narrowed: true,
        }
    };
    let mut grid = grid();
    let other = one_of(0, &[Some("x")]);
    grid.view_applied(conditions(vec![other.clone()]), Totals::unknown());
    let mut popup = open(&mut grid, 1);
    popup.key(&press(KeyCode::Tab), &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    // The model: the values ticked, and the search "select all matching" kept.
    let mut ticked: Vec<Value> = universe.clone();
    let mut text: Option<String> = None;
    let mut search = String::new();
    popup.fetch(&snapshot(&mut grid), |_, q| Some(answer(q)));
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(20));
    for _ in 0..steps {
        if tc.draw(gs::integers::<u8>().max_value(3)) == 0 {
            search = tc
                .draw(gs::sampled_from(vec!["", "berg", "o", "e", "7", "z"]))
                .to_string();
            tc.note(&format!("search {search:?}"));
            popup.key(&ctrl('u'), &mut grid);
            for c in search.chars() {
                popup.key(&char(c), &mut grid);
            }
            popup.fetch(&snapshot(&mut grid), |_, q| Some(answer(q)));
        }
        // The values listed, the blank first, as the popup shows them.
        let blank = (universe.iter()).filter(|v| **v == Value::Blank && search.is_empty());
        let texts = (universe.iter().filter(listed)).filter(|v| match v {
            Value::Text(t) => search.is_empty() || has(t, &search),
            Value::Blank => false,
        });
        let shown: Vec<Value> = blank.chain(texts).cloned().collect();
        let excluded = |v: &Value, text: &Option<String>| match (text, v) {
            (Some(s), Value::Text(t)) => !has(t, s),
            (Some(_), Value::Blank) => true,
            (None, _) => false,
        };
        let head = head_mark(&popup, &mut grid);
        let mut changed = true;
        if tc.draw(gs::integers::<u8>().max_value(4)) == 0 {
            let on = head != "[x]";
            tc.note(&format!("head {on}"));
            popup.key(&ctrl('a'), &mut grid);
            match (search.is_empty(), on) {
                (true, true) => {
                    ticked = universe
                        .iter()
                        .filter(|v| !excluded(v, &text))
                        .cloned()
                        .collect()
                }
                (true, false) => ticked.clear(),
                (false, true) => {
                    text = Some(search.clone());
                    ticked = universe
                        .iter()
                        .filter(|v| !excluded(v, &text))
                        .cloned()
                        .collect();
                }
                (false, false) => ticked.retain(|v| !shown.contains(v)),
            }
        } else if !shown.is_empty() {
            let i = tc.draw(gs::integers::<usize>().max_value(shown.len() - 1));
            let v = &shown[i];
            tc.note(&format!("tick or untick {v:?}"));
            popup.key(&press(KeyCode::Home), &mut grid);
            for _ in 0..=i {
                popup.key(&press(KeyCode::Down), &mut grid);
            }
            popup.key(&char(' '), &mut grid);
            if let Some(at) = ticked.iter().position(|t| t == v) {
                ticked.remove(at);
            } else if excluded(v, &text) {
                changed = false;
            } else {
                ticked.push(v.clone());
            }
        } else {
            continue;
        }
        let asked: Vec<Condition> = (snapshot(&mut grid).conditions(Axis::Rows).into_iter())
            .filter(|(_, state)| *state != ConditionState::Removing)
            .map(|(c, _)| c)
            .collect();
        assert!(asked.contains(&other), "the other column's condition stays");
        let column: Vec<&Condition> = asked.iter().filter(|c| c.key == Key(1)).collect();
        for v in &universe {
            let kept = column.iter().all(|c| model_keeps(c, v));
            assert_eq!(kept, ticked.contains(v), "{v:?} kept: {column:?}");
        }
        let one_ofs: Vec<&&Condition> = (column.iter())
            .filter(|c| matches!(c.kind, ConditionKind::OneOf(_)))
            .collect();
        assert!(one_ofs.len() <= 1, "{column:?}");
        let empty_not = |c: &&Condition| c.not && c.kind == ConditionKind::OneOf(vec![]);
        assert!(!column.iter().any(empty_not), "{column:?}");
        if search.is_empty() {
            let open: Vec<&Value> = shown.iter().filter(|v| !excluded(v, &text)).collect();
            let all = !open.is_empty() && open.iter().all(|v| ticked.contains(v));
            assert_eq!(head_mark(&popup, &mut grid) == "[x]", all, "the head");
            if all && !partial && changed {
                assert!(one_ofs.is_empty(), "all ticked of a whole list: {column:?}");
            }
        }
    }
}

/// The tick box drawn at the head of the list.
fn head_mark(popup: &ColumnFilter, grid: &mut Grid) -> String {
    let screen = text(&screen(popup, grid));
    let line = (screen.lines())
        .find(|l| l.contains("(Select all"))
        .expect("the head");
    let at = line.find('[').expect("a tick box");
    line[at..at + 3].to_string()
}
