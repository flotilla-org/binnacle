//! `InitialSizes`: a size per column or row chosen by Key, applied once when
//! the item first appears, as the web widget's `initialColumnWidth` does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use binnacle_core::{Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    Content, InitialSize, InitialSizes, RenderContext, Theme, begin_frame, render_with,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const HEADERS: [&str; 6] = ["id", "name", "note", "extra", "more", "last"];
const NAMES: [&str; 4] = ["Ada Lovelace", "Alan", "Grace Hopper", "Edsger"];

struct People;

impl Content for People {
    fn cell(&self, row: Key, col: Key) -> String {
        match col.0 {
            0 => row.0.to_string(),
            1 => NAMES[row.0 as usize % NAMES.len()].into(),
            c => format!("{}{}", HEADERS[c as usize], row.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        HEADERS[col.0 as usize].into()
    }
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

struct Host {
    grid: Grid,
    sizes: InitialSizes,
    area: Rect,
}

impl Host {
    fn new(cols: u64, rows: u64, width: u16, sizes: InitialSizes) -> Self {
        Host {
            grid: Grid::new(
                AxisConfig::new(rows, 1.0).header(1.0),
                AxisConfig::new(cols, 6.0).header(2.0),
            ),
            sizes,
            area: Rect::new(0, 0, width, 6),
        }
    }

    /// One host frame: windows filled, sizes settled until they stop
    /// changing, then drawn.
    fn frame(&mut self) -> String {
        let (grid, area) = (&mut self.grid, self.area);
        let (rows, cols) = (
            grid.snapshot().count(Axis::Rows),
            grid.snapshot().count(Axis::Cols),
        );
        let theme = Theme::classic();
        let ctx = RenderContext::new(&theme).scrollbars(false);
        let fill = |grid: &mut Grid, viewport: Viewport| {
            for (axis, range) in grid.begin_frame(viewport) {
                let end = range.end.min([rows, cols][axis.index()]);
                let keys = IdentitySource::new(end).window(range.start..end);
                grid.set_window(axis, range.start, &keys);
            }
        };
        begin_frame(grid, &ctx, area, fill);
        for _ in 0..4 {
            if !self.sizes.settle(grid, &People) {
                break;
            }
            begin_frame(grid, &ctx, area, fill);
        }
        let mut buf = Buffer::empty(area);
        render_with(&grid.snapshot(), &People, &ctx, area, &mut buf);
        text(&buf)
    }

    fn width(&mut self, col: u64) -> Option<f32> {
        self.grid.size_override(Axis::Cols, Key(col))
    }
}

fn columns(f: impl Fn(u64) -> InitialSize + 'static) -> InitialSizes {
    InitialSizes::new().columns(move |key| f(key.0))
}

#[test]
fn a_table_with_a_fixed_a_fit_and_a_fill_column() {
    let sizes = columns(|c| match c {
        0 => InitialSize::Cells(10.0),
        1 => InitialSize::Fit,
        _ => InitialSize::Fill,
    });
    let mut host = Host::new(3, 4, 40, sizes);
    insta::assert_snapshot!(host.frame());
    assert_eq!(host.width(0), Some(10.0));
    assert_eq!(host.width(1), Some(12.0), "Grace Hopper is the widest");
    assert_eq!(host.width(2), Some(16.0), "the fill takes what is left");
}

#[test]
fn fills_share_the_room_equally() {
    let sizes = columns(|c| {
        if c == 0 {
            InitialSize::Cells(8.0)
        } else {
            InitialSize::Fill
        }
    });
    let mut host = Host::new(3, 4, 41, sizes);
    host.frame();
    let share = host.width(1);
    assert_eq!(share, host.width(2));
    assert_eq!(share, Some(15.0), "31 cells left, floored");
}

#[test]
fn an_axis_that_overflows_fills_nothing() {
    let sizes = columns(|c| {
        if c == 0 {
            InitialSize::Cells(3.0)
        } else {
            InitialSize::Fill
        }
    });
    let mut host = Host::new(8, 4, 30, sizes);
    host.frame();
    assert_eq!(host.width(1), None);
    let sizes: Vec<f32> = (host.grid.snapshot().strip(Axis::Cols).iter())
        .map(|item| item.size)
        .collect();
    assert!(sizes[1..].iter().all(|&size| size == 6.0), "{sizes:?}");
}

#[test]
fn a_size_the_user_sets_wins_over_every_kind() {
    let sizes = columns(|c| match c {
        0 => InitialSize::Cells(10.0),
        1 => InitialSize::Fit,
        _ => InitialSize::Fill,
    });
    let mut host = Host::new(3, 4, 40, sizes);
    for col in 0..3 {
        host.grid.set_size(Axis::Cols, Key(col), 5.0);
    }
    host.frame();
    assert_eq!([0, 1, 2].map(|c| host.width(c)), [Some(5.0); 3]);
}

#[test]
fn a_resize_survives_the_item_scrolling_out_and_back() {
    let asked = Rc::new(RefCell::new(HashMap::<u64, u32>::new()));
    let seen = asked.clone();
    let sizes = columns(move |c| {
        *seen.borrow_mut().entry(c).or_default() += 1;
        match c {
            0 => InitialSize::Cells(10.0),
            1 => InitialSize::Fit,
            _ => InitialSize::Default,
        }
    });
    let mut host = Host::new(6, 4, 24, sizes);
    host.frame();
    assert_eq!(host.width(0), Some(10.0));
    assert_eq!(host.width(1), Some(12.0));
    host.grid.set_size(Axis::Cols, Key(0), 4.0);
    host.grid.set_size(Axis::Cols, Key(1), 7.0);
    host.grid.scroll_to_item(Axis::Cols, 5);
    host.frame();
    host.grid.scroll_to_item(Axis::Cols, 0);
    host.frame();
    assert_eq!(host.width(0), Some(4.0));
    assert_eq!(host.width(1), Some(7.0));
    assert!(asked.borrow().values().all(|&n| n == 1), "{asked:?}");
}

#[test]
fn an_item_that_appears_later_gets_its_initial_size_once() {
    let asked = Rc::new(RefCell::new(Vec::<u64>::new()));
    let seen = asked.clone();
    let sizes = columns(move |c| {
        seen.borrow_mut().push(c);
        if c == 5 {
            InitialSize::Cells(9.0)
        } else {
            InitialSize::Default
        }
    });
    let mut host = Host::new(6, 4, 20, sizes);
    host.frame();
    assert_eq!(host.width(5), None);
    assert!(!asked.borrow().contains(&5), "not in the strip yet");
    host.grid.scroll_to_item(Axis::Cols, 5);
    host.frame();
    assert_eq!(host.width(5), Some(9.0));
    host.grid.set_size(Axis::Cols, Key(5), 3.0);
    host.grid.scroll_to_item(Axis::Cols, 0);
    host.frame();
    host.grid.scroll_to_item(Axis::Cols, 5);
    host.frame();
    assert_eq!(host.width(5), Some(3.0));
    assert_eq!(asked.borrow().iter().filter(|&&c| c == 5).count(), 1);
}

#[test]
fn rows_take_fixed_and_fill_heights_but_a_fit_is_the_default() {
    let sizes = InitialSizes::new().rows(|key| match key.0 {
        0 => InitialSize::Cells(2.0),
        1 => InitialSize::Fit,
        _ => InitialSize::Default,
    });
    let mut host = Host::new(2, 3, 20, sizes);
    host.frame();
    assert_eq!(host.grid.size_override(Axis::Rows, Key(0)), Some(2.0));
    assert_eq!(host.grid.size_override(Axis::Rows, Key(1)), None);
}

#[test]
fn a_fill_never_shrinks_an_item_below_its_default() {
    let sizes = columns(|c| {
        if c == 0 {
            InitialSize::Cells(10.0)
        } else {
            InitialSize::Fill
        }
    });
    let mut host = Host::new(3, 4, 22, sizes);
    host.frame();
    assert_eq!(
        [1, 2].map(|c| host.width(c)),
        [None; 2],
        "a share of 5 is under 6"
    );
}
