//! Themes: `render_with` draws a Snapshot in a Theme's palette.

use binnacle_core::{
    Action, Axis, AxisConfig, FocusStep, Grid, Key, Modifiers, PointerEvent, SortColumn, SortSpec,
    ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{ColorSupport, Content, RenderContext, Theme, render_with};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }
}

const AREA: Rect = Rect::new(0, 0, 30, 8);

/// A small table scrolled part-way into column 0 and down two rows, with
/// focus, a selected range, a picked row, a hovered header, an applied sort
/// with a request pending, and rows from 7 still loading.
fn fixture() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(3.0).gap(1.0),
    );
    grid.view_applied(ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::ascending(Key(1))]),
    ));
    grid.action(Action::Scroll { dx: 2.0, dy: 2.0 });
    window(&mut grid);
    // Column 1 spans x 8..14 after the scroll: Ctrl-click picks row 4 (line 3).
    grid.pointer(PointerEvent::down(9.0, 3.0).modifiers(Modifiers::CTRL));
    grid.pointer(PointerEvent::up(9.0, 3.0).modifiers(Modifiers::CTRL));
    let move_focus = |axis, extend| Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward: true,
        extend,
    };
    grid.action(move_focus(Axis::Rows, false));
    grid.action(move_focus(Axis::Rows, true));
    grid.action(move_focus(Axis::Cols, true));
    // Shift-click column 2's header: a sort request, not yet answered.
    grid.pointer(PointerEvent::down(16.0, 0.0).modifiers(Modifiers::SHIFT));
    grid.pointer(PointerEvent::up(16.0, 0.0).modifiers(Modifiers::SHIFT));
    grid.pointer(PointerEvent::moved(23.0, 0.0));
    grid
}

/// Runs a host frame, leaving rows from 7 on unfilled.
fn window(grid: &mut Grid) {
    for (axis, range) in grid.begin_frame(Viewport::new(AREA.width.into(), AREA.height.into())) {
        let end = match axis {
            Axis::Rows => range.end.min(7),
            Axis::Cols => range.end,
        };
        let keys = IdentitySource::new(end).window(range.start..end);
        grid.set_window(axis, range.start, &keys);
    }
}

fn draw(grid: &mut Grid, ctx: &RenderContext) -> Buffer {
    window(grid);
    let mut buf = Buffer::empty(AREA);
    render_with(&grid.snapshot(), &Labels, ctx, AREA, &mut buf);
    buf
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

/// A preset as drawn, then with inverted headers and zebra stripes.
fn both_ways(theme: Theme, stripe: Style) -> String {
    let plain = draw(&mut fixture(), &RenderContext::new(&theme));
    let striped = Theme {
        invert_headers: true,
        zebra: Some(stripe),
        ..theme
    };
    let striped = draw(&mut fixture(), &RenderContext::new(&striped));
    format!(
        "--- {}\n{}\n--- inverted headers, zebra\n{}",
        theme.name,
        styled(&plain),
        styled(&striped)
    )
}

#[test]
fn classic_draws_today_s_look() {
    let stripe = Style::default().bg(Color::Indexed(235));
    insta::assert_snapshot!(both_ways(Theme::classic(), stripe));
}

#[test]
fn catppuccin_mocha_draws_its_palette() {
    let theme = Theme::catppuccin_mocha();
    let stripe = theme.zebra.unwrap();
    insta::assert_snapshot!(both_ways(theme, stripe));
}

#[test]
fn mono_draws_with_modifiers_only() {
    let stripe = Style::default().add_modifier(ratatui::style::Modifier::DIM);
    let out = both_ways(Theme::mono(), stripe);
    insta::assert_snapshot!(out);
}

#[test]
fn mono_sets_no_colour_on_any_cell() {
    let theme = Theme::mono();
    let theme = Theme {
        invert_headers: true,
        ..theme
    };
    let mut grid = fixture();
    // A resize in progress adds its guide; the hovered border its handle.
    grid.pointer(PointerEvent::down(14.0, 0.0));
    grid.pointer(PointerEvent::moved(17.0, 0.0));
    let buf = draw(&mut grid, &RenderContext::new(&theme));
    for cell in buf.content() {
        assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset));
    }
}

/// The 16 ANSI colours with xterm's default RGB values.
const ANSI: [(Color, (u8, u8, u8)); 16] = [
    (Color::Black, (0, 0, 0)),
    (Color::Red, (205, 0, 0)),
    (Color::Green, (0, 205, 0)),
    (Color::Yellow, (205, 205, 0)),
    (Color::Blue, (0, 0, 238)),
    (Color::Magenta, (205, 0, 205)),
    (Color::Cyan, (0, 205, 205)),
    (Color::Gray, (229, 229, 229)),
    (Color::DarkGray, (127, 127, 127)),
    (Color::LightRed, (255, 0, 0)),
    (Color::LightGreen, (0, 255, 0)),
    (Color::LightYellow, (255, 255, 0)),
    (Color::LightBlue, (92, 92, 255)),
    (Color::LightMagenta, (255, 0, 255)),
    (Color::LightCyan, (0, 255, 255)),
    (Color::White, (255, 255, 255)),
];

fn is_ansi(c: Color) -> bool {
    ANSI.iter().any(|&(a, _)| a == c)
}

#[test]
fn catppuccin_degrades_to_ansi_colours_without_full_rgb() {
    let theme = Theme::catppuccin_mocha();
    let ctx = RenderContext::new(&theme).colors(ColorSupport::Ansi16);
    let buf = draw(&mut fixture(), &ctx);
    let mut seen = 0;
    for cell in buf.content() {
        for c in [cell.fg, cell.bg] {
            assert!(c == Color::Reset || is_ansi(c), "{c:?} is not ANSI");
            seen += usize::from(c != Color::Reset);
        }
    }
    assert!(seen > 0, "the degraded palette still colours cells");
    insta::assert_snapshot!(styled(&buf));
}

/// Draws the fixture with the header's foreground set to `fg`, and returns
/// the colour a header cell ends up with.
fn header_colour(fg: Color, colors: ColorSupport) -> Color {
    let theme = Theme {
        header: Style::default().fg(fg),
        ..Theme::classic()
    };
    let buf = draw(&mut fixture(), &RenderContext::new(&theme).colors(colors));
    // x 4 is inside column 0's header, clear of the sort marker.
    buf[(4, 0)].fg
}

#[test]
fn rgb_colours_degrade_to_the_nearest_ansi_colour() {
    let cases = [
        (Color::Rgb(250, 10, 10), Color::LightRed),
        (Color::Rgb(200, 0, 0), Color::Red),
        (Color::Rgb(10, 10, 10), Color::Black),
        (Color::Rgb(130, 128, 125), Color::DarkGray),
        (Color::Rgb(250, 250, 250), Color::White),
        (Color::Rgb(235, 235, 235), Color::Gray),
        (Color::Rgb(0, 0, 230), Color::Blue),
        (Color::Rgb(100, 100, 250), Color::LightBlue),
    ];
    for (rgb, ansi) in cases {
        assert_eq!(header_colour(rgb, ColorSupport::Ansi16), ansi, "{rgb:?}");
    }
    // Named and indexed colours pass through.
    assert_eq!(
        header_colour(Color::Magenta, ColorSupport::Ansi16),
        Color::Magenta
    );
    assert_eq!(
        header_colour(Color::Indexed(208), ColorSupport::Ansi16),
        Color::Indexed(208)
    );
}

fn rgb(tc: &TestCase) -> Color {
    let channel = || tc.draw(gs::integers::<u8>());
    Color::Rgb(channel(), channel(), channel())
}

/// Degrading lands on an ANSI colour that degrading its own RGB gives back.
#[hegel::test(test_cases = 200)]
fn every_rgb_colour_degrades_to_an_ansi_colour_that_is_its_own_nearest(tc: TestCase) {
    let got = header_colour(rgb(&tc), ColorSupport::Ansi16);
    let &(_, (r, g, b)) = ANSI
        .iter()
        .find(|&&(a, _)| a == got)
        .expect("an ANSI colour");
    assert_eq!(
        header_colour(Color::Rgb(r, g, b), ColorSupport::Ansi16),
        got
    );
}

#[hegel::test(test_cases = 200)]
fn full_rgb_colour_keeps_every_rgb_colour(tc: TestCase) {
    let c = rgb(&tc);
    assert_eq!(header_colour(c, ColorSupport::TrueColor), c);
}

#[test]
fn colour_support_follows_colorterm() {
    assert_eq!(
        ColorSupport::from_colorterm(Some("truecolor")),
        ColorSupport::TrueColor
    );
    assert_eq!(
        ColorSupport::from_colorterm(Some("24bit")),
        ColorSupport::TrueColor
    );
    assert_eq!(ColorSupport::from_colorterm(Some("")), ColorSupport::Ansi16);
    assert_eq!(ColorSupport::from_colorterm(None), ColorSupport::Ansi16);
}

#[test]
fn the_presets_are_classic_catppuccin_mocha_and_mono() {
    let names: Vec<&str> = Theme::presets().iter().map(|t| t.name).collect();
    assert_eq!(names, ["classic", "catppuccin_mocha", "mono"]);
    assert_eq!(Theme::default(), Theme::classic());
}
