//! Builds `tests/smoke.c` against `include/binnacle.h` and the static library
//! cargo built for this test run, then runs it; and checks that the header's
//! structs and unions are laid out as Rust lays them out.

use std::mem::{align_of, offset_of, size_of};
use std::path::Path;
use std::process::Command;

use binnacle_ffi::*;

/// What the Rust standard library needs from the system when linked
/// statically (`--print native-static-libs`).
const NATIVE_LIBS: &[&str] = if cfg!(target_os = "macos") {
    &["-lSystem", "-lc", "-lm"]
} else {
    &[
        "-lgcc_s",
        "-lutil",
        "-lrt",
        "-lpthread",
        "-lm",
        "-ldl",
        "-lc",
    ]
};

#[cfg(unix)]
#[test]
fn the_c_smoke_program_round_trips_a_frame() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = std::env::current_exe().unwrap();
    let staticlib = exe.with_file_name("libbinnacle_ffi.a");
    assert!(
        staticlib.exists(),
        "no static library at {}",
        staticlib.display()
    );
    let program = Path::new(env!("CARGO_TARGET_TMPDIR")).join("binnacle-smoke");

    let built = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(crate_dir.join("include"))
        .arg(crate_dir.join("tests/smoke.c"))
        .arg(&staticlib)
        .args(NATIVE_LIBS)
        .arg("-o")
        .arg(&program)
        .output()
        .expect("run cc");
    assert!(
        built.status.success(),
        "cc failed:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let ran = Command::new(&program)
        .arg(crate_dir.join("../binnacle-described/tests/data/rows-hosts.ndjson"))
        .output()
        .expect("run the smoke program");
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert!(
        ran.status.success(),
        "smoke program failed ({}):\n{stdout}{}",
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
    assert!(stdout.ends_with("ok\n"), "unexpected output:\n{stdout}");
}

/// A `#[repr(C)]` type as Rust lays it out: its size, alignment and each
/// field's offset, fields in declaration order.
struct Layout {
    name: &'static str,
    size: usize,
    align: usize,
    fields: Vec<(&'static str, usize)>,
}

macro_rules! layouts {
    ($($ty:ident { $($field:ident),* $(,)? })*) => {
        vec![$(Layout {
            name: stringify!($ty),
            size: size_of::<$ty>(),
            align: align_of::<$ty>(),
            fields: vec![$((stringify!($field), offset_of!($ty, $field))),*],
        }),*]
    };
}

/// Every struct and union the header declares. A union nested in a struct
/// has no C name; Rust names it after the struct and its field
/// (`BinnacleAction`'s `args` is `BinnacleActionArgs`).
fn rust_layouts() -> Vec<Layout> {
    layouts! {
        BinnacleAxisConfig { count, default_size, header, gap }
        BinnacleWindowRequest { requested, start, end }
        BinnacleWindowRequests { axes }
        BinnacleRange { start, end }
        BinnacleRect { x, y, width, height }
        BinnacleExtent { start, end }
        BinnacleCornerLevel { index, kind, field }
        BinnacleCornerTier { across, levels_start, levels_len }
        BinnacleHeaderSpan { axis, tier, key, has_field, field, open, first, last, rect, visible, cut_before, cut_after }
        BinnacleText { data, len }
        BinnacleValue { blank, text }
        BinnacleGroupOf { field, value }
        BinnacleOutline { kind, depth, open, ancestors, ancestors_len, has_group, field, value, span, path, path_len, has_set, set_index, set_size }
        BinnacleStripItem { position, has_key, key, offset, size, has_outline, outline, toggle_pending }
        BinnacleAggregate { name, params, params_len }
        BinnacleHeader { position, has_key, key, rect, flags, sort_place, support, aggregates, aggregates_len }
        BinnacleScrollAnchor { position, offset }
        BinnacleThumb { fraction, size }
        BinnacleFocusEnd { position, has_key, key }
        BinnacleTextQuery { text, flags, searched, keys, keys_len }
        BinnacleFindQuery { query, by }
        BinnacleFindMatch { row, col }
        BinnacleAxisSnapshot { count, extent, header, gap, anchor, thumb, strip, strip_len, headers, headers_len, focus, selection_runs, selection_runs_len, has_total, total, quick_filter, conditions, conditions_len, levels, levels_len, picked, picked_len, pinned_len, trailing_len, scrolling_start, scrolling_end, leading_start, leading_end, trailing_start, trailing_end, movable, hide_support, hidden, hidden_len, hide_cap, leading_pins, leading_pins_len, trailing_pins, trailing_pins_len, tiers, tiers_len, spans, spans_len, corner_tiers, corner_tiers_len, corner_levels, corner_levels_len, band_focused, sorts, sorts_len }
        BinnacleZone { axis, edge, offset, size, hot }
        BinnacleSnapshot { generation, damage, axes, has_focus, has_focus_rect, focus_rect, has_selection_range, selection_rects, selection_rects_len, cell_states, cell_states_len, mode, capturing, has_hovered_header, hovered_axis, hovered_index, has_find, find, has_find_match, find_match, find_support, view_pending, outline_focused, has_pending_move, pending_move_axis, pending_move_keys, pending_move_keys_len, has_move_refusal, move_refusal_axis, move_refusal, has_drop, drop_axis, drop_target, drop_offset, drop_position, drop_unpin, drop_edge, has_drop_before, drop_before, drop_reorder, drop_refusal, pin_zones, pin_zones_len, slicers, slicers_len, values, values_len, focused_tier, edit_row, edit_col, has_editor_rect, editor_rect }
        BinnacleSortColumn { key, direction }
        BinnacleScalar { kind, number, date }
        BinnacleCondition { key, kind, negated, text, values, values_len, compare, scalar, high }
        BinnacleListedSort { state, column }
        BinnacleListedCondition { state, condition }
        BinnacleAxisView { sort, sort_len, has_quick, quick, conditions, conditions_len, hidden, hidden_len }
        BinnacleLevel { kind, field, subtotal }
        BinnacleAggregateValue { has_field, field, aggregate, params, params_len }
        BinnacleListedLevel { state, level }
        BinnacleListedValue { state, value }
        BinnacleGroupedAxis { levels, levels_len, grand_total, form, expand_all, expand_depth, except, except_len }
        BinnacleGrouping { axes, values, values_len, slicers, slicers_len }
        BinnacleViewSpec { axes, has_grouping, grouping }
        BinnacleViewEdit { kind, axis, key, multi, keys, keys_len, expand_all, expand_depth, form, level, value }
        BinnacleTotal { known, count }
        BinnacleTotals { axes }
        BinnacleEffect { generation, kind, text, row, col, view, edit, axis, key, size, has_find, find, has_from, from, direction, keys, keys_len, to, edge, has_drill_down, drill_down, drill_down_len, copy_runs, copy_runs_len, copy_labels }
        BinnacleSizeOverrides { keys, sizes, len }
        BinnaclePointerEvent { kind, x, y, button, modifiers, clicks }
        BinnacleNamedKey { id, key }
        BinnacleAxisNames { names, names_len }
        BinnacleNames { axes }
        BinnacleMoveFocus { axis, by, forward, extend }
        BinnacleScroll { dx, dy }
        BinnacleScrollTo { axis, fraction }
        BinnacleScrollBy { axis, items }
        BinnacleAction { size, kind, args }
        BinnacleActionArgs { type_text, move_focus, scroll, scroll_to, scroll_by }
        BinnacleFigure { kind, number, text }
        BinnacleGroupStep { field, value }
        BinnacleGroupRequest { id, path, path_len, want, key, start, end }
        BinnacleGroupScope { levels, levels_len, values, values_len, slicers, slicers_len, rows }
        BinnacleGroupBatch { stamp, scope, requests, requests_len }
        BinnacleGroupRow { value, children, figures, figures_len }
        BinnacleGroupAnswer { stamp, has_id, id, path, path_len, start, body, total, groups, groups_len, keys, keys_len, key, found, figures, figures_len }
        BinnacleGroupNotice { axis, notice, at, n }
        BinnacleGroupViewed { applied, totals, notices, notices_len }
        BinnacleGroupItem { key, outline }
        BinnacleGroupSummary { kind, path, path_len, children, figures, figures_len }
        BinnacleBytes { data, len }
        BinnacleCategory { value, has_label, label }
        BinnacleInvalid { raw, expected }
        BinnacleTypedValue { kind, data }
        BinnacleTypedValueData { boolean, int64, uint64, number, text, bytes, datetime, category, nested, invalid }
        BinnacleCell { size, text, align, style, value }
        BinnacleWatchRequest { kind, table, path, query, target, namespace_name, wheres, wheres_len, limit }
        BinnacleDescribedStatus { described, synced, partial, has_count, count, estimate, rows, mismatches }
        BinnacleColumnInfo { size, key, name, has_title, title, header, type_name, value_kind, has_unit, unit, nullable, role_bits, roles, roles_len, has_categories, categories, categories_len, has_reference, has_resource, resource, reference_field, mismatches }
        BinnacleDistinctValue { text, has_count, count }
        BinnacleDistinct { values, values_len, has_blanks, blanks_counted, blanks, partial, narrowed }
    }
}

/// A struct or union the header declares: its name, its fields in order,
/// and for a nested union the struct and field it sits in.
#[derive(Debug, PartialEq)]
struct Declared {
    name: String,
    fields: Vec<String>,
    within: Option<(String, String)>,
}

/// The index of the brace closing the one at `open`.
fn closing(text: &str, open: usize) -> usize {
    let mut depth = 0;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return open + i;
                }
            }
            _ => {}
        }
    }
    panic!("unclosed brace at {open}");
}

/// The name a declaration such as `const uint64_t *keys` or
/// `BinnacleZone pin_zones[2]` declares.
fn declared_name(declaration: &str) -> String {
    let declaration = declaration.split('[').next().unwrap();
    let last = declaration.split_whitespace().last().unwrap();
    last.trim_start_matches('*').to_owned()
}

/// The word before the `;` that follows `from`.
fn name_after(text: &str, from: usize) -> (String, usize) {
    let end = from + text[from..].find(';').expect("a name and ;");
    (text[from..end].trim().to_owned(), end)
}

/// The header's structs and unions, comments stripped: `typedef struct {
/// ... } Name;`, `struct Name { ... };`, and unions nested in them.
fn declared(header: &str) -> Vec<Declared> {
    let mut code = String::new();
    let mut rest = header;
    while let Some(start) = rest.find("/*") {
        code.push_str(&rest[..start]);
        let end = rest[start..].find("*/").unwrap();
        rest = &rest[start + end + 2..];
    }
    code.push_str(rest);
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = ["typedef struct {", "\nstruct "]
        .iter()
        .filter_map(|p| code[at..].find(p).map(|i| (at + i, *p)))
        .min()
    {
        let (start, pattern) = found;
        let open = start + code[start..].find('{').unwrap();
        // `struct Name Name;` forward declarations have no body.
        if code[start..open].contains(';') {
            at = start + pattern.len();
            continue;
        }
        let close = closing(&code, open);
        let (name, end) = match pattern {
            "typedef struct {" => name_after(&code, close + 1),
            _ => (code[start + pattern.len()..open].trim().to_owned(), close),
        };
        let mut body = &code[open + 1..close];
        let mut fields = Vec::new();
        let mut nested = Vec::new();
        while !body.trim().is_empty() {
            let trimmed = body.trim_start();
            if trimmed.starts_with("union") {
                let open = body.find('{').unwrap();
                let close = closing(body, open);
                let members = (body[open + 1..close].split(';'))
                    .filter(|d| !d.trim().is_empty())
                    .map(declared_name)
                    .collect();
                let (field, end) = name_after(body, close + 1);
                fields.push(field.clone());
                nested.push((field, members));
                body = &body[end + 1..];
            } else {
                let end = body.find(';').expect("a declaration ends with ;");
                fields.push(declared_name(&body[..end]));
                body = &body[end + 1..];
            }
        }
        out.push(Declared {
            name: name.clone(),
            fields,
            within: None,
        });
        for (field, members) in nested {
            let mut camel = field.clone();
            camel[..1].make_ascii_uppercase();
            out.push(Declared {
                name: format!("{name}{camel}"),
                fields: members,
                within: Some((name.clone(), field)),
            });
        }
        at = end;
    }
    out
}

/// `_Static_assert`s that the header lays every struct and union out as
/// Rust does, for the C compiler to check.
fn asserts(declared: &[Declared], layouts: &[Layout]) -> String {
    let mut c = String::from("#include <stddef.h>\n\n#include \"binnacle.h\"\n\n");
    for (d, l) in declared.iter().zip(layouts) {
        let (ty, path) = match &d.within {
            None => (d.name.clone(), String::new()),
            Some((outer, field)) => (outer.clone(), format!("{field}.")),
        };
        match &d.within {
            None => {
                c += &format!(
                    "_Static_assert(sizeof({ty}) == {}, \"{ty} size\");\n",
                    l.size
                );
                c += &format!(
                    "_Static_assert(_Alignof({ty}) == {}, \"{ty} alignment\");\n",
                    l.align
                );
            }
            Some((outer, field)) => {
                let base = layouts
                    .iter()
                    .find(|o| o.name == outer)
                    .and_then(|o| o.fields.iter().find(|(f, _)| f == field))
                    .map(|(_, at)| *at)
                    .unwrap();
                c += &format!(
                    "_Static_assert(sizeof((({outer} *)0)->{field}) == {}, \"{}\");\n",
                    l.size, d.name
                );
                for (f, at) in &l.fields {
                    c += &format!(
                        "_Static_assert(offsetof({ty}, {path}{f}) == {}, \"{ty}.{path}{f}\");\n",
                        base + at
                    );
                }
                continue;
            }
        }
        for (f, at) in &l.fields {
            c += &format!("_Static_assert(offsetof({ty}, {f}) == {at}, \"{ty}.{f}\");\n");
        }
    }
    c
}

#[test]
fn the_rust_layouts_list_every_type_and_field_the_header_declares() {
    let header =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("include/binnacle.h"))
            .unwrap();
    let declared: Vec<(String, Vec<String>)> = declared(&header)
        .into_iter()
        .map(|d| (d.name, d.fields))
        .collect();
    let listed: Vec<(String, Vec<String>)> = rust_layouts()
        .into_iter()
        .map(|l| {
            let fields = l.fields.iter().map(|(f, _)| f.to_string()).collect();
            (l.name.to_owned(), fields)
        })
        .collect();
    assert_eq!(listed, declared, "list each header type in rust_layouts");
}

#[cfg(unix)]
#[test]
fn the_header_lays_out_every_type_as_rust_does() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let header = std::fs::read_to_string(crate_dir.join("include/binnacle.h")).unwrap();
    let declared = declared(&header);
    let layouts = rust_layouts();
    assert_eq!(declared.len(), layouts.len());
    let source = Path::new(env!("CARGO_TARGET_TMPDIR")).join("binnacle-layout.c");
    std::fs::write(&source, asserts(&declared, &layouts)).unwrap();
    let built = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()))
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsyntax-only",
            "-I",
        ])
        .arg(crate_dir.join("include"))
        .arg(&source)
        .output()
        .expect("run cc");
    assert!(
        built.status.success(),
        "the header's layout differs from Rust's ({}):\n{}",
        source.display(),
        String::from_utf8_lossy(&built.stderr)
    );
}
