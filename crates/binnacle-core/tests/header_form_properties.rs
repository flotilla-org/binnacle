//! Seam 1 properties for header form: a real Grid over a hand-made tree
//! drawn with each row level a tier of the band, checked against what the
//! tree shows.

use binnacle_core::{
    Axis, AxisConfig, Depth, Edge, Expand, Form, Grid, Grouping, ItemKind, Key, Level, Outline,
    Snapshot, StripItem, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};
use hegel::TestCase;
use hegel::generators as gs;

const REGION: Key = Key(1);
const PORT: Key = Key(2);

/// A tree of regions and ports, some ports empty, with Leaves keyed from 0
/// and groups from 1000; returns it with every group's Key.
fn tree(tc: &TestCase) -> (Vec<Group>, Vec<Key>) {
    let mut next_leaf = 0;
    let mut next_group = 1000;
    let mut groups = Vec::new();
    let regions = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let mut tree = Vec::new();
    for _ in 0..regions {
        let ports = tc.draw(gs::integers::<usize>().min_value(1).max_value(3));
        let mut kids = Vec::new();
        for _ in 0..ports {
            let n = tc.draw(gs::integers::<u64>().max_value(4));
            let keys: Vec<u64> = (next_leaf..next_leaf + n).collect();
            next_leaf += n;
            groups.push(Key(next_group));
            kids.push(Group::leaves(next_group, "port", &keys));
            next_group += 1;
        }
        groups.push(Key(next_group));
        tree.push(Group::groups(next_group, "region", kids));
        next_group += 1;
    }
    (tree, groups)
}

fn spec(expand: Expand) -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    grouping.rows.form = Form::Header;
    ViewSpec::default().with_grouping(Some(grouping))
}

fn depth(tc: &TestCase) -> Depth {
    match tc.draw(gs::integers::<u32>().max_value(3)) {
        3 => Depth::All,
        n => Depth::Levels(n),
    }
}

/// The group whose span holds an item with `outline` at `tier`, as the
/// spec words it: its ancestor there, but an open group's own Subtotal
/// sits outside its group's span.
fn member(outline: &Outline, tier: usize, expand: &Expand) -> Option<Key> {
    let key = *outline.ancestors.get(tier)?;
    let own = outline.kind == ItemKind::Subtotal && tier + 1 == outline.ancestors.len();
    (!(own && expand.opens(tier as u32, key))).then_some(key)
}

/// The span key of the item at `position` at `tier`, if the Grid holds
/// its outline.
fn key_at(snap: &Snapshot, expand: &Expand, position: u64, tier: usize) -> Option<Key> {
    member(snap.outline(Axis::Rows, position)?, tier, expand)
}

/// A span as the property expects it: tier, Key, first and last
/// Positions, and its cut sides.
type Want = (u32, Key, u64, u64, bool, bool);

/// The spans a Strip of `items` drawn in `region` should have at `tier`:
/// each maximal run of neighbouring items under one group, with some part
/// in the region, cut where it is clipped or its group goes on at the
/// Position beside it.
fn strip_spans(
    snap: &Snapshot,
    expand: &Expand,
    items: &[StripItem],
    region: std::ops::Range<f32>,
    tier: usize,
) -> Vec<Want> {
    let count = snap.count(Axis::Rows);
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let Some(key) = key_at(snap, expand, items[i].position, tier) else {
            i += 1;
            continue;
        };
        let mut j = i;
        while j + 1 < items.len() && key_at(snap, expand, items[j + 1].position, tier) == Some(key)
        {
            j += 1;
        }
        let (first, last) = (items[i], items[j]);
        let goes_on = |p: Option<u64>| {
            p.filter(|&p| p < count)
                .is_some_and(|p| key_at(snap, expand, p, tier) == Some(key))
        };
        if first.offset.max(region.start) < last.end().min(region.end) {
            out.push((
                tier as u32,
                key,
                first.position,
                last.position,
                first.offset < region.start || goes_on(first.position.checked_sub(1)),
                last.end() > region.end || goes_on(last.position.checked_add(1)),
            ));
        }
        i = j + 1;
    }
    out
}

/// A grouped Grid in header form, with tiers and band of any sizes, put
/// through any toggles, depths, pins and scrolls; returns it with the
/// applied Expand State and its viewport.
fn scene(tc: &TestCase) -> (Scenario<TreeTable>, Expand, Viewport) {
    let (tree, groups) = tree(tc);
    let mut table = TreeTable::new(vec![REGION, PORT], 3, tree);
    if tc.draw(gs::booleans()) {
        table = table.subtotals_after();
    }
    let band = tc.draw(gs::integers::<u32>().max_value(20)) as f32;
    let grid = Grid::new(
        AxisConfig::new(0, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(band),
    );
    let tiers: Vec<f32> = (tc.draw(gs::vecs(gs::integers::<u32>().max_value(10)).max_size(3)))
        .into_iter()
        .map(|n| n as f32)
        .collect();
    let height = tc.draw(gs::integers::<u32>().min_value(3).max_value(30));
    let v = Viewport {
        width: 30.0,
        height: height as f32,
    };
    let mut s = Scenario::new(grid, table);
    s.frame(v);
    let mut expand = Expand::to(depth(tc));
    s.input(|g| {
        g.set_tiers(Axis::Rows, &tiers);
        g.request_view(spec(expand.clone()));
    });
    s.frame(v);
    let steps = tc.draw(gs::integers::<usize>().max_value(8));
    for _ in 0..steps {
        match tc.draw(gs::integers::<u8>().max_value(3)) {
            0 => {
                let key = tc.draw(gs::sampled_from(groups.clone()));
                s.input(|g| {
                    let _ = g.toggle(Axis::Rows, key);
                });
            }
            1 => {
                let d = depth(tc);
                s.input(|g| g.expand_to(Axis::Rows, d));
            }
            2 => {
                let shown: Vec<Key> = s.source.shown().iter().map(|(k, _)| *k).collect();
                if !shown.is_empty() {
                    let key = tc.draw(gs::sampled_from(shown));
                    let edge = tc.draw(gs::sampled_from(vec![Edge::Leading, Edge::Trailing]));
                    s.input(|g| g.pin(Axis::Rows, edge, key));
                }
            }
            _ => {
                let to = tc.draw(gs::integers::<u64>().max_value(40));
                s.input(|g| g.scroll_to_item(Axis::Rows, to));
            }
        }
        s.frame(v);
    }
    s.frame(v);
    if let Some(applied) =
        (s.frame(v).applied_view().grouping.as_ref()).map(|g| g.rows.expand.clone())
    {
        expand = applied;
    }
    (s, expand, v)
}

/// Through any toggles, depths, pins and scrolls, every span covers
/// exactly the neighbouring items of one Strip under its group, so it
/// never straddles a pinned edge; every item drawn under a group is in its
/// span; and cut sides are where the group goes on.
#[hegel::test(test_cases = 300)]
fn every_span_covers_exactly_the_items_under_its_group_within_one_strip(tc: TestCase) {
    let (mut s, expand, v) = scene(&tc);
    let snap = s.frame(v);
    let regions = [
        (
            snap.pinned(Axis::Rows, Edge::Leading),
            snap.pinned_region(Axis::Rows, Edge::Leading),
        ),
        (snap.strip(Axis::Rows), snap.scrolling(Axis::Rows)),
        (
            snap.pinned(Axis::Rows, Edge::Trailing),
            snap.pinned_region(Axis::Rows, Edge::Trailing),
        ),
    ];
    let mut want: Vec<Want> = Vec::new();
    for (items, region) in regions.clone() {
        for tier in 0..2 {
            want.extend(strip_spans(&snap, &expand, items, region.clone(), tier));
        }
    }
    let got: Vec<Want> = (snap.spans(Axis::Rows).iter())
        .map(|s| (s.tier, s.key, s.first, s.last, s.cut_before, s.cut_after))
        .collect();
    assert_eq!(got, want);

    // Each span lies inside one Strip's region, its label part inside the
    // span.
    for span in snap.spans(Axis::Rows) {
        let (top, bottom) = (span.visible.y, span.visible.y + span.visible.height);
        assert!(
            regions
                .iter()
                .any(|(_, r)| r.start <= top && bottom <= r.end && top < bottom),
            "{span:?} straddles a pinned edge"
        );
        assert!(span.rect.y <= top && bottom <= span.rect.y + span.rect.height);
    }
}

/// Each row header starts past the tiers its spans take, or past every
/// tier when spans take them all.
#[hegel::test(test_cases = 300)]
fn each_row_header_starts_past_the_tiers_its_spans_take(tc: TestCase) {
    let (mut s, expand, v) = scene(&tc);
    let snap = s.frame(v);
    let tiers = snap.tiers(Axis::Rows);
    for h in snap.headers(Axis::Rows) {
        let spanned = (0..2)
            .take_while(|&t| key_at(&snap, &expand, h.position, t).is_some())
            .count();
        let from = tiers.get(spanned).map_or(tiers[1].end, |t| t.start);
        assert_eq!(h.rect.x, from, "the header of {:?}", h.key);
    }
}
