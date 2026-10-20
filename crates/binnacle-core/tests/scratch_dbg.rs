use binnacle_core::*;
#[test]
fn dbg() {
    let mut g = Grid::new(
        AxisConfig::new(1, 1.0).header(1.0),
        AxisConfig::new(1, 3.0).header(2.0),
    );
    let v = Viewport::new(4.0, 3.0);
    let frame = |g: &mut Grid, rows: &[Key]| {
        for (axis, r) in g.begin_frame(v) {
            match axis {
                Axis::Rows => g.set_window(
                    axis,
                    r.start,
                    &rows[r.start as usize..(r.end as usize).min(rows.len())],
                ),
                Axis::Cols => g.set_window(axis, r.start, &[Key(100)]),
            }
        }
        print!("{}", binnacle_testkit::render(&g.snapshot()));
    };
    frame(&mut g, &[Key(0)]);
    g.pin(Axis::Rows, Edge::Leading, Key(0));
    frame(&mut g, &[Key(0)]);
    g.notify(Axis::Rows, ChangeNotice::Reset(1));
    frame(&mut g, &[Key(1000)]);
    g.action(Action::MoveFocus {
        axis: Axis::Rows,
        by: FocusStep::Item,
        forward: false,
        extend: false,
    });
    frame(&mut g, &[Key(1000)]);
}
