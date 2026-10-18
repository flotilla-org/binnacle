//! Pulse: a Shimmer over pending states, drawn at fixed times.

use std::time::Duration;

use binnacle_core::{
    Action, Axis, AxisConfig, Effect, FocusStep, Grid, Key, Modifiers, PointerEvent, Viewport,
};
use binnacle_memory::{MemorySource, PumpSource};
use binnacle_ratatui::{ColorSupport, Content, RenderContext, Theme, animating, render_with};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

const AREA: Rect = Rect::new(0, 0, 24, 6);
const ROWS: usize = 12;
const COLS: usize = 3;

/// Row `r` holds a scrambled value in each column, so sorting reorders it.
fn rows() -> Vec<[i64; COLS]> {
    (0..ROWS as i64)
        .map(|r| [r, (r * 7) % 12, (r * 5) % 12])
        .collect()
}

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }
}

/// A host whose Table Source answers only when told to: sort requests and
/// Locates wait in `held`, as from a slow source.
struct Host {
    grid: Grid,
    rows: Vec<[i64; COLS]>,
    source: MemorySource<[i64; COLS]>,
    held: Vec<Effect>,
}

impl Host {
    fn new() -> Host {
        let rows = rows();
        let mut source = MemorySource::new(ROWS, COLS);
        for c in 0..COLS {
            source = source.i64_column(Key(c as u64), move |r: &[i64; COLS]| Some(r[c]));
        }
        let grid = Grid::new(
            AxisConfig::new(ROWS as u64, 1.0).header(1.0),
            AxisConfig::new(COLS as u64, 6.0).header(3.0).gap(1.0),
        );
        let mut host = Host {
            grid,
            rows,
            source,
            held: Vec::new(),
        };
        host.frame();
        host
    }

    /// Runs a host frame: holds what a slow source holds, then fills the
    /// Windows.
    fn frame(&mut self) {
        while let Some(stamped) = self.grid.poll_effect() {
            if matches!(
                stamped.effect,
                Effect::ViewRequested { .. } | Effect::Locate { .. }
            ) {
                self.held.push(stamped.effect);
            }
        }
        let viewport = Viewport::new(AREA.width.into(), AREA.height.into());
        for (axis, range) in self.grid.begin_frame(viewport) {
            let keys = self.source.window(axis, range.clone());
            self.grid.set_window(axis, range.start, &keys);
        }
    }

    /// Answers the oldest held Effect.
    fn answer(&mut self) {
        if self.held.is_empty() {
            return;
        }
        let effect = self.held.remove(0);
        let mut source = self.source.over(&self.rows);
        match effect {
            Effect::ViewRequested { spec, .. } => {
                let viewed = source.view(&spec);
                self.grid.view_applied(viewed.applied);
                for (axis, notice) in viewed.notices {
                    self.grid.notify(axis, notice);
                }
            }
            Effect::Locate { axis, key } => {
                let answer = source.locate(axis, key);
                self.grid.located(axis, key, answer);
            }
            _ => unreachable!("only sorts and Locates are held"),
        }
        drop(source);
        self.frame();
    }

    /// Clicks column `col`'s header (it starts at x 3 + 7 * col).
    fn click_header(&mut self, col: u16, modifiers: Modifiers) {
        let x = f32::from(3 + 7 * col + 1);
        self.grid
            .pointer(PointerEvent::down(x, 0.0).modifiers(modifiers));
        self.grid
            .pointer(PointerEvent::up(x, 0.0).modifiers(modifiers));
        // Off the headers, so none is drawn hovered.
        self.grid.pointer(PointerEvent::moved(23.0, 5.0));
        self.frame();
    }

    fn action(&mut self, action: Action) {
        self.grid.action(action);
        self.frame();
    }

    fn draw(&mut self, theme: &Theme, colors: ColorSupport, now: Duration) -> Buffer {
        let mut buf = Buffer::empty(AREA);
        let ctx = RenderContext::new(theme).colors(colors).at(now);
        render_with(&self.grid.snapshot(), &Labels, &ctx, AREA, &mut buf);
        buf
    }
}

fn step(axis: Axis) -> Action {
    Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward: true,
        extend: false,
    }
}

/// Focus on Key 2, column 1, then a click on column 1's header: a sort
/// requested and held. Sorting by column 1 leaves even Keys in place, so the
/// focused row stays visible where it was while its Locate is pending.
fn sorting() -> Host {
    let mut host = Host::new();
    // The first step places focus on Key 0.
    host.action(step(Axis::Rows));
    host.action(step(Axis::Rows));
    host.action(step(Axis::Rows));
    host.action(step(Axis::Cols));
    host.click_header(1, Modifiers::NONE);
    host
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

fn describe(style: Style) -> String {
    let colour = |c: Option<Color>| c.map_or("-".to_string(), |c| c.to_string());
    let mut out = format!("fg {} bg {}", colour(style.fg), colour(style.bg));
    if !style.add_modifier.is_empty() {
        out += &format!(" {:?}", style.add_modifier);
    }
    out
}

/// The text, then each cell's style as a letter (`.` for unstyled), then
/// what each letter stands for.
fn styled(buf: &Buffer) -> String {
    let a = buf.area;
    let mut legend: Vec<Style> = Vec::new();
    let mut map = String::new();
    for y in a.top()..a.bottom() {
        for x in a.left()..a.right() {
            let style = buf[(x, y)].style();
            if style == Style::default().fg(Color::Reset).bg(Color::Reset) {
                map.push('.');
                continue;
            }
            let i = legend.iter().position(|&s| s == style).unwrap_or_else(|| {
                legend.push(style);
                legend.len() - 1
            });
            map.push(char::from(b'a' + i as u8));
        }
        map.push('\n');
    }
    let legend: String = legend
        .iter()
        .enumerate()
        .map(|(i, &s)| format!("{}: {}\n", char::from(b'a' + i as u8), describe(s)))
        .collect();
    format!("{}\n\n{map}\n{legend}", text(buf))
}

/// `host` drawn at each of `times` (in milliseconds).
fn over_time(host: &mut Host, theme: &Theme, colors: ColorSupport, times: &[u64]) -> String {
    times
        .iter()
        .map(|&ms| {
            let buf = host.draw(theme, colors, Duration::from_millis(ms));
            format!("--- at {ms} ms\n{}", styled(&buf))
        })
        .collect()
}

#[test]
fn a_pending_sort_s_header_shimmers_in_theme_colours() {
    let mut host = sorting();
    let theme = Theme::classic();
    let out = over_time(&mut host, &theme, ColorSupport::TrueColor, &[0, 1000, 1200]);
    insta::assert_snapshot!(out);
}

#[test]
fn without_full_rgb_the_shimmer_steps_through_dim_normal_and_bold() {
    let mut host = sorting();
    let theme = Theme::classic();
    let out = over_time(&mut host, &theme, ColorSupport::Ansi16, &[0, 1000, 1200]);
    insta::assert_snapshot!(out);
}

#[test]
fn the_focused_cell_pulses_while_its_locate_is_pending() {
    let mut host = sorting();
    // The sort is answered; the focused row's Locate is now held.
    host.answer();
    assert!(host.grid.snapshot().locating(Axis::Rows));
    let theme = Theme::classic();
    let out = over_time(&mut host, &theme, ColorSupport::TrueColor, &[0, 200, 400]);
    insta::assert_snapshot!(out);
}

#[test]
fn a_frame_drawn_at_the_same_time_is_the_same() {
    let mut host = sorting();
    let theme = Theme::catppuccin_mocha();
    let now = Duration::from_millis(1234);
    for colors in [ColorSupport::TrueColor, ColorSupport::Ansi16] {
        assert_eq!(
            host.draw(&theme, colors, now),
            host.draw(&theme, colors, now)
        );
    }
}

#[test]
fn nothing_animates_once_the_sort_and_locate_are_answered() {
    let mut host = sorting();
    assert!(animating(&host.grid.snapshot()));
    host.answer();
    assert!(animating(&host.grid.snapshot()));
    host.answer();
    assert!(!animating(&host.grid.snapshot()));
}

/// The times a frame is sampled at across one Shimmer period: close enough
/// that the band crosses every cell it sweeps at least once.
const SAMPLES: [u64; 8] = [0, 250, 500, 750, 1000, 1250, 1500, 1750];

/// Whatever the user and the slow source do, `animating` says exactly
/// whether the frame changes with time: drawn at the sampled times, the
/// frames differ while it is true and are all the same while it is false.
#[hegel::test(test_cases = 100)]
fn animating_is_exactly_when_the_frame_moves_with_time(tc: TestCase) {
    let mut host = Host::new();
    let theme = tc.draw(gs::sampled_from(Theme::presets().to_vec()));
    let colors = tc.draw(gs::sampled_from(vec![
        ColorSupport::TrueColor,
        ColorSupport::Ansi16,
    ]));
    let steps = tc.draw(gs::integers::<usize>().max_value(12));
    for _ in 0..steps {
        match tc.draw(gs::integers::<u8>().max_value(4)) {
            0 => {
                let col = tc.draw(gs::integers::<u16>().max_value(2));
                let shift = tc.draw(gs::booleans());
                let modifiers = if shift {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                };
                host.click_header(col, modifiers);
            }
            1 => host.answer(),
            2 => {
                let axis = if tc.draw(gs::booleans()) {
                    Axis::Rows
                } else {
                    Axis::Cols
                };
                let forward = tc.draw(gs::booleans());
                host.action(Action::MoveFocus {
                    axis,
                    by: FocusStep::Item,
                    forward,
                    extend: false,
                });
            }
            3 => {
                let dy = f32::from(tc.draw(gs::integers::<i8>().min_value(-4).max_value(4)));
                host.action(Action::Scroll { dx: 0.0, dy });
            }
            _ => {
                let row = tc.draw(gs::integers::<u64>().max_value(ROWS as u64 - 1));
                let col = tc.draw(gs::integers::<u64>().max_value(COLS as u64 - 1));
                host.grid.set_focus(Key(row), Key(col));
                host.frame();
            }
        }
        let first = host.draw(&theme, colors, Duration::ZERO);
        let moves = SAMPLES[1..]
            .iter()
            .any(|&ms| host.draw(&theme, colors, Duration::from_millis(ms)) != first);
        assert_eq!(animating(&host.grid.snapshot()), moves);
    }
}
