//! Themes: the palette [`render_with`](crate::render_with) draws in, and the
//! colour capability it degrades RGB colours for.

use ratatui::style::{Color, Modifier, Style};

/// The styles the table is drawn in. Most are patched over another: the
/// selected and the hovered header over the header (or row number), the
/// zebra stripe over the body, selection, picks and focus over a cell, the sort marker and the
/// loading marker over where they sit. Build your own by overriding fields
/// of a preset: `Theme { zebra: Some(stripe), ..Theme::classic() }`.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    /// Column headers.
    pub header: Style,
    /// The header (or row number) under the pointer.
    pub header_hovered: Style,
    /// The header (or row number) of a selected item: the range spans every
    /// item across it, or it is picked.
    pub header_selected: Style,
    /// Row numbers, down the left band.
    pub row_number: Style,
    pub body: Style,
    /// Every other row, by Position; `None` leaves rows unstriped.
    pub zebra: Option<Style>,
    pub focus: Style,
    pub selection: Style,
    /// Cells in a picked row or column.
    pub picks: Style,
    pub sort_marker: Style,
    /// The lines a resize grabs, in the header band's Gaps.
    pub handle: Style,
    /// The line a resize in progress draws across the area.
    pub guide: Style,
    pub drop_indicator: Style,
    /// Patched over an empty pinned Strip's pin zone while headers are
    /// dragged, where a drop pins to that edge.
    pub pin_zone: Style,
    /// Patched over the pin zone a drop would pin to.
    pub pin_zone_hot: Style,
    pub border: Style,
    /// The line in the Gap after a pinned Strip, where pinned items end and
    /// the scrolling ones begin.
    pub pinned_edge: Style,
    /// A scrollbar's track, drawn as a line.
    pub scrollbar_track: Style,
    /// A scrollbar's thumb, drawn as full blocks.
    pub scrollbar_thumb: Style,
    /// The `…` shown where a Key is not filled yet.
    pub loading: Style,
    /// Patched over a cell whose value just changed (see
    /// [`CellCanvas::flash`](crate::CellCanvas::flash)).
    pub flash: Style,
    /// Patched over a cell that matches the Find Query.
    pub find_match: Style,
    /// Patched over the current match, which also ends in `◂` so it shows
    /// under the focus.
    pub find_current: Style,
    /// Patched over the cells of an unmatched row (kept after the user's
    /// edit though the filter drops it), dimming it.
    pub unmatched: Style,
    /// Patched over a control whose request the Table Source rejected or
    /// failed: a quick filter, condition or find text it did not apply.
    pub error: Style,
    /// Patched over a value the formatter hints dim: a null.
    pub dim: Style,
    /// Patched over a value the formatter hints a warning: an invalid one,
    /// which also starts with a `!`.
    pub warning: Style,
    /// Patched over a Group Item's row, band and body.
    pub group_item: Style,
    /// Patched over a Subtotal's row.
    pub subtotal: Style,
    /// Patched over a Grand Total's row.
    pub grand_total: Style,
    /// The disclosure mark (`▾` open, `▸` closed) before a Group Item's label.
    pub disclosure: Style,
    pub shimmer_base: Color,
    pub shimmer_highlight: Color,
    /// Draws headers and row numbers reversed, so the bands stand out.
    pub invert_headers: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::classic()
    }
}

impl Theme {
    /// The v1 look: bold headers, a reversed focus, a dark grey selection.
    pub fn classic() -> Theme {
        let none = Style::default();
        Theme {
            name: "classic",
            header: none.add_modifier(Modifier::BOLD),
            header_hovered: none.add_modifier(Modifier::UNDERLINED),
            header_selected: none.bg(Color::DarkGray),
            row_number: none.add_modifier(Modifier::BOLD),
            body: none,
            zebra: None,
            focus: none.add_modifier(Modifier::REVERSED),
            selection: none.bg(Color::DarkGray),
            picks: none.bg(Color::DarkGray),
            sort_marker: none,
            handle: none.fg(Color::DarkGray),
            guide: none.fg(Color::Yellow),
            drop_indicator: none.fg(Color::Cyan),
            pin_zone: none.bg(Color::Rgb(0, 50, 60)),
            pin_zone_hot: none.fg(Color::Black).bg(Color::Cyan),
            border: none.fg(Color::DarkGray),
            pinned_edge: none.fg(Color::DarkGray),
            scrollbar_track: none.fg(Color::DarkGray),
            scrollbar_thumb: none.fg(Color::Gray),
            loading: none,
            flash: none.fg(Color::Black).bg(Color::Yellow),
            find_match: none.fg(Color::Black).bg(Color::Yellow),
            find_current: none
                .fg(Color::Black)
                .bg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
            unmatched: none.fg(Color::DarkGray),
            error: none.fg(Color::LightRed).add_modifier(Modifier::BOLD),
            dim: none.fg(Color::DarkGray),
            warning: none.fg(Color::Yellow),
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
            shimmer_base: Color::Rgb(140, 130, 40),
            shimmer_highlight: Color::Rgb(255, 240, 120),
            invert_headers: false,
        }
    }

    /// Catppuccin's Mocha flavour, with zebra stripes.
    pub fn catppuccin_mocha() -> Theme {
        let none = Style::default();
        Theme {
            name: "catppuccin_mocha",
            header: none.fg(mocha::BLUE).add_modifier(Modifier::BOLD),
            header_hovered: none.fg(mocha::SAPPHIRE).add_modifier(Modifier::UNDERLINED),
            header_selected: none.bg(mocha::SURFACE1),
            row_number: none.fg(mocha::OVERLAY0),
            body: none.fg(mocha::TEXT),
            zebra: Some(none.bg(mocha::MANTLE)),
            focus: none.fg(mocha::CRUST).bg(mocha::BLUE),
            selection: none.bg(mocha::SURFACE1),
            picks: none.fg(mocha::MAUVE).bg(mocha::SURFACE1),
            sort_marker: none.fg(mocha::PEACH),
            handle: none.fg(mocha::SURFACE2),
            guide: none.fg(mocha::YELLOW),
            drop_indicator: none.fg(mocha::SAPPHIRE).add_modifier(Modifier::BOLD),
            pin_zone: none.bg(mocha::SAPPHIRE_WASH),
            pin_zone_hot: none.fg(mocha::CRUST).bg(mocha::SAPPHIRE),
            border: none.fg(mocha::SURFACE1),
            pinned_edge: none.fg(mocha::SURFACE1),
            scrollbar_track: none.fg(mocha::SURFACE1),
            scrollbar_thumb: none.fg(mocha::OVERLAY0),
            loading: none.fg(mocha::OVERLAY0),
            flash: none.fg(mocha::CRUST).bg(mocha::YELLOW),
            find_match: none.fg(mocha::CRUST).bg(mocha::YELLOW),
            find_current: none
                .fg(mocha::CRUST)
                .bg(mocha::PEACH)
                .add_modifier(Modifier::BOLD),
            unmatched: none.fg(mocha::OVERLAY0),
            error: none.fg(mocha::RED).add_modifier(Modifier::BOLD),
            dim: none.fg(mocha::OVERLAY0),
            warning: none.fg(mocha::YELLOW),
            group_item: none
                .fg(mocha::TEXT)
                .bg(mocha::SURFACE0)
                .add_modifier(Modifier::BOLD),
            subtotal: none.fg(mocha::SAPPHIRE).bg(mocha::MANTLE),
            grand_total: none
                .fg(mocha::TEXT)
                .bg(mocha::SURFACE1)
                .add_modifier(Modifier::BOLD),
            disclosure: none.fg(mocha::PEACH),
            shimmer_base: mocha::PEACH,
            shimmer_highlight: mocha::YELLOW,
            invert_headers: false,
        }
    }

    /// Modifiers only, for `NO_COLOR` and monochrome terminals.
    pub fn mono() -> Theme {
        let none = Style::default();
        Theme {
            name: "mono",
            header: none.add_modifier(Modifier::BOLD),
            header_hovered: none.add_modifier(Modifier::UNDERLINED),
            header_selected: none.add_modifier(Modifier::REVERSED),
            row_number: none.add_modifier(Modifier::DIM),
            body: none,
            zebra: None,
            focus: none.add_modifier(Modifier::REVERSED),
            selection: none.add_modifier(Modifier::UNDERLINED),
            picks: none.add_modifier(Modifier::UNDERLINED),
            sort_marker: none.add_modifier(Modifier::BOLD),
            handle: none.add_modifier(Modifier::DIM),
            guide: none.add_modifier(Modifier::BOLD),
            drop_indicator: none.add_modifier(Modifier::BOLD),
            pin_zone: none.add_modifier(Modifier::DIM),
            pin_zone_hot: none.add_modifier(Modifier::REVERSED),
            border: none.add_modifier(Modifier::DIM),
            pinned_edge: none.add_modifier(Modifier::DIM),
            scrollbar_track: none.add_modifier(Modifier::DIM),
            scrollbar_thumb: none,
            loading: none.add_modifier(Modifier::DIM),
            flash: none.add_modifier(Modifier::BOLD),
            find_match: none.add_modifier(Modifier::UNDERLINED),
            find_current: none.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            unmatched: none.add_modifier(Modifier::DIM),
            error: none.add_modifier(Modifier::BOLD | Modifier::ITALIC),
            dim: none.add_modifier(Modifier::DIM),
            warning: none.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            // Undimmed, so a summary's label in the dim row band reads as bold.
            group_item: none
                .add_modifier(Modifier::BOLD)
                .remove_modifier(Modifier::DIM),
            subtotal: none.add_modifier(Modifier::ITALIC),
            grand_total: none
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                .remove_modifier(Modifier::DIM),
            disclosure: none.add_modifier(Modifier::BOLD),
            shimmer_base: Color::Reset,
            shimmer_highlight: Color::Reset,
            invert_headers: false,
        }
    }

    /// Every preset, in the order a host cycles them.
    pub fn presets() -> [Theme; 3] {
        [Theme::classic(), Theme::catppuccin_mocha(), Theme::mono()]
    }

    /// This Theme with its RGB colours replaced by what `colors` can show.
    pub(crate) fn for_colors(&self, colors: ColorSupport) -> Theme {
        if colors == ColorSupport::TrueColor {
            return self.clone();
        }
        let s = |style: Style| Style {
            fg: style.fg.map(nearest_ansi),
            bg: style.bg.map(nearest_ansi),
            underline_color: style.underline_color.map(nearest_ansi),
            ..style
        };
        Theme {
            header: s(self.header),
            header_hovered: s(self.header_hovered),
            header_selected: s(self.header_selected),
            row_number: s(self.row_number),
            body: s(self.body),
            zebra: self.zebra.map(s),
            focus: s(self.focus),
            selection: s(self.selection),
            picks: s(self.picks),
            sort_marker: s(self.sort_marker),
            handle: s(self.handle),
            guide: s(self.guide),
            drop_indicator: s(self.drop_indicator),
            pin_zone: s(self.pin_zone),
            pin_zone_hot: s(self.pin_zone_hot),
            border: s(self.border),
            pinned_edge: s(self.pinned_edge),
            scrollbar_track: s(self.scrollbar_track),
            scrollbar_thumb: s(self.scrollbar_thumb),
            loading: s(self.loading),
            flash: s(self.flash),
            find_match: s(self.find_match),
            find_current: s(self.find_current),
            unmatched: s(self.unmatched),
            error: s(self.error),
            dim: s(self.dim),
            warning: s(self.warning),
            group_item: s(self.group_item),
            subtotal: s(self.subtotal),
            grand_total: s(self.grand_total),
            disclosure: s(self.disclosure),
            shimmer_base: nearest_ansi(self.shimmer_base),
            shimmer_highlight: nearest_ansi(self.shimmer_highlight),
            ..self.clone()
        }
    }
}

/// What colours the terminal can show. The host decides (see
/// [`ColorSupport::from_colorterm`]); the adapter never reads the environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorSupport {
    /// Full 24-bit RGB.
    #[default]
    TrueColor,
    /// The 16 ANSI colours (and the indexed palette, which passes through).
    Ansi16,
}

impl ColorSupport {
    /// The capability a `COLORTERM` value announces: `truecolor` and `24bit`
    /// mean full RGB.
    pub fn from_colorterm(colorterm: Option<&str>) -> ColorSupport {
        match colorterm {
            Some("truecolor" | "24bit") => ColorSupport::TrueColor,
            _ => ColorSupport::Ansi16,
        }
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

/// The ANSI colour nearest an RGB colour; other colours are kept.
pub(crate) fn nearest_ansi(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let distance = |(ar, ag, ab): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(ar, r) + d(ag, g) + d(ab, b)
    };
    ANSI.iter()
        .min_by_key(|(_, rgb)| distance(*rgb))
        .map_or(color, |&(ansi, _)| ansi)
}

/// The Catppuccin Mocha colours the preset uses.
mod mocha {
    use ratatui::style::Color;

    pub const CRUST: Color = Color::Rgb(17, 17, 27);
    pub const MANTLE: Color = Color::Rgb(24, 24, 37);
    pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
    pub const SURFACE1: Color = Color::Rgb(69, 71, 90);
    pub const SURFACE2: Color = Color::Rgb(88, 91, 112);
    pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);
    pub const TEXT: Color = Color::Rgb(205, 214, 244);
    pub const MAUVE: Color = Color::Rgb(203, 166, 247);
    pub const PEACH: Color = Color::Rgb(250, 179, 135);
    pub const RED: Color = Color::Rgb(243, 139, 168);
    pub const YELLOW: Color = Color::Rgb(249, 226, 175);
    pub const SAPPHIRE: Color = Color::Rgb(116, 199, 236);
    pub const BLUE: Color = Color::Rgb(137, 180, 250);
    /// Not Catppuccin's: Sapphire mixed into the base, for lit pin zones.
    pub const SAPPHIRE_WASH: Color = Color::Rgb(34, 58, 74);
}
