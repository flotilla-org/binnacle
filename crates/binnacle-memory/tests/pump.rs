//! The pump drives any Table Source, including one the host holds as a trait
//! object (the legacy table's data trait had a generic method, so it could
//! not be one).

use binnacle_core::{
    Axis, AxisConfig, ChangeNotice, Effect, Grid, Key, Located, PointerEvent, SortColumn, SortSpec,
    ViewEdit, ViewSpec, Viewport,
};
use binnacle_memory::{PumpSource, Viewed, pump};

/// Views by recording the spec; locates nothing.
#[derive(Default)]
struct Recording {
    viewed: Vec<ViewSpec>,
}

impl PumpSource for Recording {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.viewed.push(spec.clone());
        Viewed {
            applied: spec.clone(),
            notices: vec![(Axis::Rows, ChangeNotice::Reordered)],
            ..Viewed::default()
        }
    }

    fn locate(&mut self, _axis: Axis, _key: Key) -> Located {
        Located::Gone { suggest: None }
    }
}

#[test]
fn the_pump_answers_a_table_source_held_as_a_trait_object() {
    // Column 0's header spans x 2..6 in the top band y 0..1.
    let mut grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0),
        AxisConfig::new(2, 4.0).header(2.0),
    );
    let v = Viewport::new(10.0, 4.0);
    for (axis, range) in grid.begin_frame(v) {
        let keys: Vec<Key> = range.clone().map(Key).collect();
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot();
    grid.pointer(PointerEvent::down(3.0, 0.5));
    grid.pointer(PointerEvent::up(3.0, 0.5));

    let mut recording = Recording::default();
    let source: &mut dyn PumpSource = &mut recording;
    let mut seen = Vec::new();
    pump(&mut grid, source, |_, stamped| seen.push(stamped.effect));

    let spec = ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::ascending(Key(0))]),
    );
    let edit = ViewEdit::CycleSort {
        axis: Axis::Rows,
        key: Key(0),
        multi: false,
    };
    assert_eq!(
        seen,
        vec![Effect::ViewRequested {
            spec: spec.clone(),
            edit
        }]
    );
    assert_eq!(recording.viewed, vec![spec.clone()]);
    assert_eq!(grid.snapshot().applied_view(), &spec);
}
