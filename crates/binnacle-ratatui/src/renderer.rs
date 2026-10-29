//! Cell Renderers: per-cell drawing into a canvas clipped to the cell's
//! visible part.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Range;
use std::time::Duration;

use binnacle_core::Key;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::{ColorSupport, Content, NextFrame, Theme};

/// Draws cells. [`Content::renderer`] picks one per cell (by column unless
/// the host says otherwise); the text renderer is the default.
pub trait CellRenderer {
    /// Draws the cell at `row`, `col` into `canvas`, which starts filled with
    /// blanks in the body style. Focus and selection are patched on after.
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas);

    /// The width, in cells, the cell would like: what a host answers an
    /// auto-size Effect with (see [`fit_width`]).
    fn preferred_width(&self, row: Key, col: Key) -> u32;
}

impl<R: CellRenderer + ?Sized> CellRenderer for &R {
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        (**self).draw(row, col, canvas);
    }

    fn preferred_width(&self, row: Key, col: Key) -> u32 {
        (**self).preferred_width(row, col)
    }
}

/// What the Grid says about the cell being drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStatus {
    pub focused: bool,
    /// Inside the selected range.
    pub selected: bool,
    /// In a picked row.
    pub picked: bool,
    /// The pointer is over the cell's row or column header.
    pub hovered: bool,
}

/// One cell to draw into, in cell-relative coordinates: `(0, 0)` is the
/// cell's top-left corner even when that corner is scrolled out of view.
/// Writes outside the cell's visible part are dropped.
pub struct CellCanvas<'a> {
    pub(crate) buf: &'a mut Buffer,
    /// The cell's top-left corner in buffer coordinates.
    pub(crate) origin: (i64, i64),
    pub(crate) size: (u32, u32),
    /// The visible part, in buffer coordinates.
    pub(crate) visible: Rect,
    pub(crate) status: CellStatus,
    pub(crate) theme: &'a Theme,
    pub(crate) now: Duration,
    pub(crate) colors: ColorSupport,
    pub(crate) next_frame: Option<&'a NextFrame>,
}

/// How long [`CellCanvas::flash`] lights a changed cell.
pub const FLASH: Duration = Duration::from_millis(800);

impl<'a> CellCanvas<'a> {
    /// The cell's full width, visible or not.
    pub fn width(&self) -> u32 {
        self.size.0
    }

    /// The cell's full height, visible or not.
    pub fn height(&self) -> u32 {
        self.size.1
    }

    /// The visible columns of the cell, in cell coordinates.
    pub fn visible_x(&self) -> Range<u32> {
        let v = self.visible;
        self.relative(v.left(), self.origin.0)..self.relative(v.right(), self.origin.0)
    }

    /// The visible lines of the cell, in cell coordinates.
    pub fn visible_y(&self) -> Range<u32> {
        let v = self.visible;
        self.relative(v.top(), self.origin.1)..self.relative(v.bottom(), self.origin.1)
    }

    fn relative(&self, at: u16, origin: i64) -> u32 {
        u32::try_from(i64::from(at) - origin).unwrap_or(0)
    }

    pub fn status(&self) -> CellStatus {
        self.status
    }

    /// The Theme in use, already degraded to the terminal's colours.
    pub fn theme(&self) -> &Theme {
        self.theme
    }

    /// The frame's time, from the host's origin.
    pub fn now(&self) -> Duration {
        self.now
    }

    pub fn colors(&self) -> ColorSupport {
        self.colors
    }

    /// Asks for a frame `after` this one, for a cell whose drawing moves on
    /// with time; the host learns of it through its [`NextFrame`].
    pub fn request_frame_after(&self, after: Duration) {
        if let Some(next) = self.next_frame {
            next.request(self.now.saturating_add(after));
        }
    }

    /// The buffer cell at cell-relative `(x, y)`, if it is visible.
    fn at(&mut self, x: u32, y: u32) -> Option<&mut ratatui::buffer::Cell> {
        let bx = self.origin.0.checked_add(i64::from(x))?;
        let by = self.origin.1.checked_add(i64::from(y))?;
        let (bx, by) = (u16::try_from(bx).ok()?, u16::try_from(by).ok()?);
        if !self.visible.contains((bx, by).into()) {
            return None;
        }
        self.buf.cell_mut((bx, by))
    }

    /// Writes `ch` at `(x, y)`, patching `style` over the cell's style.
    pub fn set(&mut self, x: u32, y: u32, ch: char, style: Style) {
        if let Some(c) = self.at(x, y) {
            c.set_char(ch).set_style(style);
        }
    }

    /// Writes `text` from `(x, y)` rightwards, one character a cell.
    pub fn print(&mut self, x: u32, y: u32, text: &str, style: Style) {
        for (i, ch) in text.chars().enumerate() {
            let Some(cx) = u32::try_from(i).ok().and_then(|i| x.checked_add(i)) else {
                return;
            };
            self.set(cx, y, ch, style);
        }
    }

    /// Patches `style` over every visible cell, leaving the characters.
    pub fn patch(&mut self, style: Style) {
        let v = self.visible;
        for (x, y) in (v.left()..v.right()).flat_map(|x| (v.top()..v.bottom()).map(move |y| (x, y)))
        {
            if let Some(c) = self.buf.cell_mut((x, y)) {
                c.set_style(style);
            }
        }
    }

    /// Lights the cell in the Theme's flash style for [`FLASH`] after
    /// `changed`, the time the host last saw its value change (on the same
    /// clock as the render context's `now`). True while it is lit.
    pub fn flash(&mut self, changed: Duration) -> bool {
        let lit = flashing(self.now, changed);
        if lit {
            self.patch(self.theme.flash);
        }
        lit
    }
}

/// Whether a value that changed at `changed` still flashes at `now`.
pub fn flashing(now: Duration, changed: Duration) -> bool {
    now.checked_sub(changed).is_some_and(|age| age < FLASH)
}

/// Where text sits in its cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

/// Text from a closure, aligned in the first visible line of the cell. Text
/// longer than the cell shows its start, whatever the alignment, ending in
/// `…` in the cell's last column.
pub struct TextRenderer<F> {
    text: F,
    align: Align,
    style: Style,
}

impl<F: Fn(Key, Key) -> String> TextRenderer<F> {
    pub fn new(text: F) -> Self {
        TextRenderer {
            text,
            align: Align::Left,
            style: Style::default(),
        }
    }

    pub fn align(self, align: Align) -> Self {
        TextRenderer { align, ..self }
    }

    pub fn style(self, style: Style) -> Self {
        TextRenderer { style, ..self }
    }
}

impl<F: Fn(Key, Key) -> String> CellRenderer for TextRenderer<F> {
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        let text = (self.text)(row, col);
        let text = fit(&text, canvas.width());
        let room = canvas.width().saturating_sub(char_count(&text));
        let x = match self.align {
            Align::Left => 0,
            Align::Right => room,
            Align::Center => room / 2,
        };
        let y = canvas.visible_y().start;
        canvas.print(x, y, &text, self.style);
    }

    fn preferred_width(&self, row: Key, col: Key) -> u32 {
        char_count(&(self.text)(row, col))
    }
}

/// `text` cut to `width` characters, ending in `…` when cut.
pub(crate) fn fit(text: &str, width: u32) -> Cow<'_, str> {
    if char_count(text) <= width {
        return Cow::Borrowed(text);
    }
    let kept = usize::try_from(width.saturating_sub(1)).unwrap_or(usize::MAX);
    let mut cut: String = text.chars().take(kept).collect();
    if width > 0 {
        cut.push('…');
    }
    Cow::Owned(cut)
}

pub(crate) fn char_count(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

/// The eight levels a sparkline draws, lowest first.
pub const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A series from a closure, one value a cell, in [`SPARK`] levels on the
/// first visible line. A series longer than the cell shows its last values.
/// Values scale to a fixed range, or to the series' own minimum and maximum;
/// NaN draws a blank.
pub struct SparklineRenderer<F> {
    series: F,
    range: Option<(f64, f64)>,
    style: Style,
}

impl<F, S> SparklineRenderer<F>
where
    F: Fn(Key, Key) -> S,
    S: AsRef<[f64]>,
{
    pub fn new(series: F) -> Self {
        SparklineRenderer {
            series,
            range: None,
            style: Style::default(),
        }
    }

    /// Scales every cell to `min..=max`, so rows compare.
    pub fn range(self, min: f64, max: f64) -> Self {
        SparklineRenderer {
            range: Some((min, max)),
            ..self
        }
    }

    pub fn style(self, style: Style) -> Self {
        SparklineRenderer { style, ..self }
    }
}

impl<F, S> CellRenderer for SparklineRenderer<F>
where
    F: Fn(Key, Key) -> S,
    S: AsRef<[f64]>,
{
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        let series = (self.series)(row, col);
        let series = series.as_ref();
        let shown = usize::try_from(canvas.width()).unwrap_or(usize::MAX);
        let series = &series[series.len().saturating_sub(shown)..];
        let (min, max) = self.range.unwrap_or_else(|| {
            let finite = series.iter().copied().filter(|v| v.is_finite());
            finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            })
        });
        let y = canvas.visible_y().start;
        for (x, &v) in (0u32..).zip(series) {
            let ch = spark_level(v, min, max).map_or(' ', |level| SPARK[level]);
            canvas.set(x, y, ch, self.style);
        }
    }

    fn preferred_width(&self, row: Key, col: Key) -> u32 {
        u32::try_from((self.series)(row, col).as_ref().len()).unwrap_or(u32::MAX)
    }
}

/// The [`SPARK`] level of `v` in `min..=max`, clamped into the eight levels.
/// A flat range draws the middle level; NaN has none.
fn spark_level(v: f64, min: f64, max: f64) -> Option<usize> {
    if v.is_nan() {
        return None;
    }
    if min.partial_cmp(&max) != Some(Ordering::Less) {
        return Some(3);
    }
    // Halving keeps the differences finite across the whole f64 range.
    let (v, min, max) = (v / 2.0, min / 2.0, max / 2.0);
    let level = ((v - min) / (max - min) * 8.0).floor();
    Some(level.clamp(0.0, 7.0) as usize)
}

/// The partial eighths a bar ends in, one eighth first.
pub const BAR: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// A fraction from a closure as a bar across the first visible line, in
/// eighths of a cell: `floor(fraction * width * 8)` eighths, the fraction
/// clamped into `0..=1` (NaN draws nothing).
pub struct BarRenderer<F> {
    fraction: F,
    width: u32,
    style: Style,
}

impl<F: Fn(Key, Key) -> f64> BarRenderer<F> {
    pub fn new(fraction: F) -> Self {
        BarRenderer {
            fraction,
            width: 10,
            style: Style::default(),
        }
    }

    /// The preferred width, 10 cells unless set.
    pub fn width(self, width: u32) -> Self {
        BarRenderer { width, ..self }
    }

    pub fn style(self, style: Style) -> Self {
        BarRenderer { style, ..self }
    }
}

impl<F: Fn(Key, Key) -> f64> CellRenderer for BarRenderer<F> {
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        let fraction = (self.fraction)(row, col);
        let fraction = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        let eighths = (fraction * f64::from(canvas.width()) * 8.0).floor() as u64;
        let y = canvas.visible_y().start;
        let (full, part) = (eighths / 8, (eighths % 8) as usize);
        for x in 0..full {
            canvas.set(x as u32, y, BAR[7], self.style);
        }
        if part > 0 {
            canvas.set(full as u32, y, BAR[part - 1], self.style);
        }
    }

    fn preferred_width(&self, _row: Key, _col: Key) -> u32 {
        self.width
    }
}

/// The width that fits column `col`: the widest of its header text and the
/// preferred widths of its cells in `rows` (the rows on screen, say), each
/// asked of the cell's own Cell Renderer. What a host answers a column's
/// auto-size Effect with.
pub fn fit_width(content: &impl Content, rows: &[Key], col: Key) -> u32 {
    rows.iter()
        .map(|&row| content.renderer(row, col).preferred_width(row, col))
        .chain([char_count(&content.column_header(col))])
        .max()
        .unwrap_or(0)
}
