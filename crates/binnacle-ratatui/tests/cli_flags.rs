//! The CLI's flags patch a base Layout, and its mode switch decides from
//! whether stdin and stdout are terminals.

use binnacle_layout::view::{
    AxisView, Comparison, Condition, ConditionKind, Direction, FilterSpec, Scalar, SortColumn,
    TextMatch, ViewSpec,
};
use binnacle_layout::{EdgePins, JsonId, Layout, Order, Pins};
use binnacle_ratatui::cli::{Flags, Format, Interactive, parse_where};

fn id(name: &str) -> JsonId {
    JsonId::from(name)
}

const FIELDS: [&str; 5] = ["name", "phase", "cpus", "started", "spec"];

fn fields() -> Vec<String> {
    FIELDS.iter().map(|f| f.to_string()).collect()
}

/// A default view: the nested `spec` hidden, `name` pinned leading.
fn base() -> Layout<JsonId, JsonId> {
    Layout {
        view: Some(ViewSpec {
            cols: AxisView {
                hidden: vec![id("spec")],
                ..AxisView::default()
            },
            ..ViewSpec::default()
        }),
        pins: Some(Pins {
            cols: EdgePins {
                leading: vec![id("name")],
                ..EdgePins::default()
            },
            ..Pins::default()
        }),
        ..Layout::default()
    }
}

#[test]
fn flags_patch_the_base_into_the_layout_written_by_hand() {
    let flags = Flags::parse([
        "--sort",
        "-cpus,name",
        "--where",
        ".phase==Running",
        "--where=cpus>=4",
        "--where",
        "name~al",
        "--where",
        "started<2026-10-29T00:00:00Z",
        "--where",
        "phase!=Done",
        "--format",
        "json",
        "--no-interactive",
    ])
    .unwrap();
    assert_eq!(flags.format, Some(Format::Json));
    assert_eq!(flags.interactive, Interactive::Never);
    let layout = flags.apply(&base(), &fields()).unwrap();
    let condition = |field: &str, kind, not| Condition {
        id: id(field),
        kind,
        not,
    };
    let want = Layout {
        view: Some(ViewSpec {
            rows: AxisView {
                filter: FilterSpec {
                    quick: None,
                    conditions: vec![
                        condition(
                            "phase",
                            ConditionKind::OneOf {
                                values: vec![Some("Running".into())],
                            },
                            false,
                        ),
                        condition(
                            "cpus",
                            ConditionKind::Compare {
                                comparison: Comparison::AtLeast {
                                    value: Scalar::Number { value: 4.0 },
                                },
                            },
                            false,
                        ),
                        condition(
                            "name",
                            ConditionKind::Text {
                                query: TextMatch {
                                    text: "al".into(),
                                    case_sensitive: false,
                                    regex: false,
                                },
                            },
                            false,
                        ),
                        condition(
                            "started",
                            ConditionKind::Compare {
                                comparison: Comparison::Less {
                                    value: Scalar::Date {
                                        value: 1_793_232_000_000.0,
                                    },
                                },
                            },
                            false,
                        ),
                        condition(
                            "phase",
                            ConditionKind::OneOf {
                                values: vec![Some("Done".into())],
                            },
                            true,
                        ),
                    ],
                },
                sort: vec![
                    SortColumn {
                        id: id("cpus"),
                        direction: Direction::Descending,
                    },
                    SortColumn {
                        id: id("name"),
                        direction: Direction::Ascending,
                    },
                ],
                hidden: Vec::new(),
            },
            cols: AxisView {
                hidden: vec![id("spec")],
                ..AxisView::default()
            },
            grouping: None,
        }),
        ..base()
    };
    assert_eq!(layout, want);
}

#[test]
fn sort_over_a_default_view_keeps_its_hidden_columns_and_pins() {
    let layout = Flags::parse(["--sort", "name"])
        .unwrap()
        .apply(&base(), &fields())
        .unwrap();
    let view = layout.view.as_ref().unwrap();
    assert_eq!(view.cols.hidden, [id("spec")], "patched, not layered");
    assert_eq!(layout.pins, base().pins);
    assert_eq!(view.rows.sort.len(), 1);
}

#[test]
fn columns_sets_the_order_and_hides_every_other_field() {
    let layout = Flags::parse(["--columns", "cpus,name"])
        .unwrap()
        .apply(&base(), &fields())
        .unwrap();
    assert_eq!(
        layout.order,
        Some(Order {
            rows: None,
            cols: Some(vec![id("cpus"), id("name")]),
        })
    );
    let hidden = &layout.view.as_ref().unwrap().cols.hidden;
    assert_eq!(*hidden, [id("phase"), id("started"), id("spec")]);
}

#[test]
fn wide_shows_the_hidden_columns_and_view_swaps_the_base() {
    let wide = Flags::parse(["--wide"])
        .unwrap()
        .apply(&base(), &fields())
        .unwrap();
    assert!(wide.view.unwrap().cols.hidden.is_empty());
    let other = Layout {
        pins: Some(Pins::default()),
        ..Layout::default()
    };
    let views = |name: &str| (name == "bare").then(|| other.clone());
    let swapped = (Flags::parse(["--view", "bare"]).unwrap())
        .apply_with(&base(), &fields(), views)
        .unwrap();
    assert_eq!(swapped.pins, Some(Pins::default()));
    let missing = (Flags::parse(["--view", "nope"]).unwrap()).apply_with(&base(), &fields(), views);
    assert!(missing.is_err());
}

#[test]
fn malformed_flags_are_errors() {
    for args in [
        vec!["--colour"],
        vec!["stray"],
        vec!["--sort"],
        vec!["--sort", "-"],
        vec!["--columns", "a,,b"],
        vec!["--columns", "a,b,a"],
        vec!["--where", "garbage"],
        vec!["--where", "==x"],
        vec!["--where", "cpus<many"],
        vec!["--where", "name~"],
        vec!["--format", "xml"],
        vec!["--wide=yes"],
    ] {
        assert!(Flags::parse(&args).is_err(), "{args:?}");
    }
    for args in [
        vec!["--sort", "ghost"],
        vec!["--columns", "name,ghost"],
        vec!["--where", "ghost==1"],
    ] {
        let flags = Flags::parse(&args).unwrap();
        assert!(flags.apply(&base(), &fields()).is_err(), "{args:?}");
    }
}

#[test]
fn a_host_takes_the_flags_it_does_not_know() {
    let (flags, rest) =
        Flags::parse_known(["--url", "http://x", "--wide", "--table", "hosts"]).unwrap();
    assert!(flags.wide);
    assert_eq!(rest, ["--url", "http://x", "--table", "hosts"]);
}

#[test]
fn where_reads_flotillas_leading_dot_and_dates() {
    let condition = parse_where(".metadata.name == alpha").unwrap();
    assert_eq!(condition.id, id("metadata.name"));
    let date = parse_where("started>=1970-01-01T00:00:01Z").unwrap();
    assert_eq!(
        date.kind,
        ConditionKind::Compare {
            comparison: Comparison::AtLeast {
                value: Scalar::Date { value: 1000.0 }
            }
        }
    );
    // Before 1970, a part of a millisecond floors, as a row's date does.
    let early = parse_where("started<1969-12-31T23:59:59.9995Z").unwrap();
    assert_eq!(
        early.kind,
        ConditionKind::Compare {
            comparison: Comparison::Less {
                value: Scalar::Date { value: -1.0 }
            }
        }
    );
}

#[test]
fn the_mode_switch_decides_from_both_terminals_under_each_override() {
    for stdin in [false, true] {
        for stdout in [false, true] {
            assert_eq!(Interactive::Auto.decide(stdin, stdout), stdin && stdout);
            assert!(Interactive::Always.decide(stdin, stdout));
            assert!(!Interactive::Never.decide(stdin, stdout));
        }
    }
    let parsed = |args: &[&str]| Flags::parse(args).unwrap().interactive;
    assert_eq!(parsed(&[]), Interactive::Auto);
    assert_eq!(parsed(&["--interactive"]), Interactive::Always);
    assert_eq!(parsed(&["--no-interactive"]), Interactive::Never);
}

#[test]
fn the_viewport_takes_what_the_rows_need_up_to_half_the_terminal() {
    use binnacle_ratatui::cli::inline_height;
    assert_eq!(
        inline_height(4, 40, None),
        7,
        "header, rows, bar and status"
    );
    assert_eq!(inline_height(0, 40, None), 3);
    assert_eq!(
        inline_height(u64::MAX, 40, None),
        20,
        "a live table takes the most"
    );
    assert_eq!(inline_height(u64::MAX, 40, Some(12)), 12);
}
