//! Seam 1 properties for the corner grid: over any band sizes, tier sizes
//! and forms, the corner's tiers tile each band and name every level once.

use binnacle_core::{Axis, AxisConfig, Form, Grid, Grouping, Key, Level, Rect, ViewSpec, Viewport};
use binnacle_testkit::{Group, Scenario, TreeTable};
use hegel::TestCase;
use hegel::generators as gs;

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const QUARTER: Key = Key(3);

const V: Viewport = Viewport {
    width: 40.0,
    height: 20.0,
};

fn sizes(tc: &TestCase) -> Vec<f32> {
    (tc.draw(gs::vecs(gs::integers::<u32>().max_value(8)).max_size(4)))
        .into_iter()
        .map(|n| n as f32)
        .collect()
}

/// The rows grouped by region then port and, if drawn, the columns by
/// quarter then the values pseudo-level, in any forms, over bands and
/// tiers of any sizes.
fn scene(tc: &TestCase) -> (Scenario<TreeTable>, Grouping, [f32; 2]) {
    let rows = vec![Group::groups(
        100,
        "North",
        vec![Group::leaves(110, "Aberdeen", &[0, 1])],
    )];
    let quarters = vec![
        Group::leaves(200, "Q1", &[10, 11]),
        Group::leaves(201, "Q2", &[12]),
    ];
    let col_levels = vec![Level::field(QUARTER), Level::Values];
    let table = TreeTable::new(vec![REGION, PORT], 3, rows).cols_by(col_levels.clone(), quarters);
    let band = |tc: &TestCase| tc.draw(gs::integers::<u32>().max_value(16)) as f32;
    let (left, top) = (band(tc), band(tc));
    let grid = Grid::new(
        AxisConfig::new(2, 1.0).header(top),
        AxisConfig::new(3, 5.0).header(left),
    );
    let form = |tc: &TestCase| match tc.draw(gs::booleans()) {
        true => Form::Header,
        false => Form::Outline,
    };
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.form = form(tc);
    if tc.draw(gs::booleans()) {
        grouping.col_levels = col_levels;
        grouping.cols.form = form(tc);
    }
    let (row_tiers, col_tiers) = (sizes(tc), sizes(tc));
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    let spec = ViewSpec::default().with_grouping(Some(grouping.clone()));
    s.input(|g| {
        g.set_tiers(Axis::Rows, &row_tiers);
        g.set_tiers(Axis::Cols, &col_tiers);
        g.request_view(spec);
    });
    s.frame(V);
    (s, grouping, [left, top])
}

/// Each band's corner tiers run edge to edge without gaps or overlaps; in
/// header form they are the band's tiers then its items' own, each tier
/// naming its level, and in outline form one tier names every level; and
/// each cell is where its two tiers cross.
#[hegel::test(test_cases = 300)]
fn the_corner_tiers_tile_each_band_and_name_every_level_once(tc: TestCase) {
    let (mut s, grouping, [left, top]) = scene(&tc);
    let snap = s.frame(V);
    let corner = snap.corner();
    for (axis, thickness) in [(Axis::Rows, left), (Axis::Cols, top)] {
        let tiers = corner.tiers(axis);
        assert!(!tiers.is_empty());
        assert_eq!(tiers[0].across.start, 0.0);
        assert_eq!(tiers[tiers.len() - 1].across.end, thickness);
        for pair in tiers.windows(2) {
            assert_eq!(pair[0].across.end, pair[1].across.start);
        }
        let levels = grouping.levels(axis);
        let named: Vec<(u32, Option<Key>)> = (tiers.iter())
            .flat_map(|t| t.levels.iter().map(|l| (l.index, l.field)))
            .collect();
        let want: Vec<(u32, Option<Key>)> = (levels.iter().enumerate())
            .map(|(i, l)| (i as u32, l.field_key()))
            .collect();
        assert_eq!(named, want, "{axis:?} names each level once, in order");
        let header = !levels.is_empty() && grouping.axis(axis).form == Form::Header;
        match header {
            true => {
                let laid: Vec<_> = tiers[..tiers.len() - 1]
                    .iter()
                    .map(|t| t.across.clone())
                    .collect();
                assert_eq!(laid, snap.tiers(axis), "{axis:?}: the band's own tiers");
                assert!(tiers.iter().take(levels.len()).all(|t| t.levels.len() == 1));
                assert!(tiers[tiers.len() - 1].levels.is_empty());
            }
            false => assert_eq!(tiers.len(), 1, "{axis:?} in outline form is one tier"),
        }
    }
    let (rows, cols) = (corner.tiers(Axis::Rows), corner.tiers(Axis::Cols));
    for (j, c) in cols.iter().enumerate() {
        for (i, r) in rows.iter().enumerate() {
            let want = Rect {
                x: r.across.start,
                y: c.across.start,
                width: r.across.end - r.across.start,
                height: c.across.end - c.across.start,
            };
            assert_eq!(corner.cell(j, i), Some(want));
        }
    }
    assert_eq!(corner.cell(cols.len(), 0), None);
    assert_eq!(corner.cell(0, rows.len()), None);
}
