//! Activating a cell of a Described Table: the Effect names the row by the
//! Key the source interned from its primary key, so the host maps it back.

mod support;

use binnacle_core::{Action, Axis, Effect, Grouping, Key, ViewSpec};
use binnacle_described::Wire;
use binnacle_testkit::Scenario;
use support::{Table, replay, scenario};

fn crews() -> Scenario<Table> {
    scenario(replay("crews.ndjson"))
}

fn activated(s: &mut Scenario<Table>, row: Key, col: Key) -> Effect {
    s.input(|g| {
        g.set_focus(row, col);
        g.action(Action::Activate);
    });
    (s.effects().into_iter())
        .find(|e| matches!(e, Effect::Activated { .. }))
        .expect("an Activated Effect")
}

#[test]
fn activating_a_cell_names_the_row_by_the_key_of_its_primary_key() {
    let mut s = crews();
    let name = s.source.0.column_key("name").unwrap();
    let cpus = s.source.0.column_key("cpus").unwrap();
    for crew in ["alpha", "charlie", "foxtrot"] {
        let tuple = [Wire::Text(crew.into())];
        let row = s.source.0.row_key(&tuple).unwrap();
        for col in [name, cpus] {
            assert_eq!(
                activated(&mut s, row, col),
                Effect::Activated {
                    row,
                    col,
                    drill_down: None
                }
            );
        }
        let identity = s.source.0.row_identity(row).unwrap();
        assert_eq!(identity[0], 0x81, "an array of one");
        assert_eq!(&identity[2..], crew.as_bytes(), "the tuple's text");
    }
}

#[test]
fn grouped_a_summary_item_carries_a_drill_down_and_a_leaf_its_own_key() {
    let mut s = crews();
    let phase = s.source.0.column_key("phase").unwrap();
    let cpus = s.source.0.column_key("cpus").unwrap();
    let spec = ViewSpec::default().with_grouping(Some(Grouping::rows_by(phase)));
    s.input(|g| g.request_view(spec));
    let snapshot = s.frame(support::VIEWPORT);
    let summary = (snapshot.drawn(Axis::Rows))
        .find(|i| {
            snapshot
                .outline(Axis::Rows, i.position)
                .is_some_and(|o| o.group.is_some())
        })
        .and_then(|i| i.key)
        .expect("a Group Item");
    let Effect::Activated {
        row, drill_down, ..
    } = activated(&mut s, summary, cpus)
    else {
        unreachable!()
    };
    assert_eq!(row, summary);
    assert!(drill_down.is_some(), "a Summary Item's cell");

    let alpha = s.source.0.row_key(&[Wire::Text("alpha".into())]).unwrap();
    assert_eq!(
        activated(&mut s, alpha, cpus),
        Effect::Activated {
            row: alpha,
            col: cpus,
            drill_down: None
        }
    );
}
