//! The Described Table source through C: a `BinnacleDescribedSource` fed a
//! rows stream draws what a Rust `DescribedSource`, a Grid and
//! `binnacle_memory::pump` draw, and passes on the Effects they leave. Kinds,
//! styles and alignments are read with the header's numbers, the
//! independent source here.

use std::path::PathBuf;

use binnacle_core::{
    Action, Axis, AxisConfig, Condition, Effect, FilterSpec, FindQuery, Grid, Key, PointerEvent,
    SortColumn, SortSpec, TextQuery, Value as CoreValue, ViewSpec, Viewport,
};
use binnacle_described::{
    DescribedSource, Message, Received, Scope, decode_cbor_item, decode_json_line,
};
use binnacle_ffi::*;
use binnacle_format::{Align, FixedOffset, FormatContext, FormatOptions, Timestamp, Value, format};
use binnacle_memory::{PumpSource, pump};
use hegel::Generator;
use hegel::TestCase;
use hegel::generators as gs;

/// The clock every cell here formats at: 2026-10-30T00:00:00Z.
const NOW: i64 = 1_793_318_400_000_000_000;
const VIEWPORT: (f32, f32) = (90.0, 12.0);

fn path(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../binnacle-described/tests");
    let made = root.join("fixtures").join(name);
    match made.exists() {
        true => made,
        false => root.join("data").join(name),
    }
}

/// The table a fixture's name says: `rows-hosts.ndjson` is `hosts`.
fn table(name: &str) -> &str {
    let stem = name.split('.').next().unwrap();
    let stem = stem.split_once('-').map_or(stem, |(_, rest)| rest);
    stem.split('-').next().unwrap()
}

/// A fixture's encoding and each of its messages' bytes.
fn items(name: &str) -> (u32, Vec<Vec<u8>>) {
    let bytes = std::fs::read(path(name)).unwrap();
    if name.ends_with(".ndjson") {
        let lines = (bytes.split(|&b| b == b'\n'))
            .filter(|l| !l.iter().all(u8::is_ascii_whitespace))
            .map(<[u8]>::to_vec)
            .collect();
        return (1, lines);
    }
    let mut items = Vec::new();
    let mut rest = bytes.as_slice();
    while !rest.is_empty() {
        let before = rest.len();
        let _: ciborium::Value = ciborium::from_reader(&mut rest).unwrap();
        let at = bytes.len() - before;
        items.push(bytes[at..at + before - rest.len()].to_vec());
    }
    (2, items)
}

fn decode(encoding: u32, bytes: &[u8]) -> Option<Message> {
    match encoding {
        1 => decode_json_line(bytes).ok(),
        2 => decode_cbor_item(bytes).ok(),
        _ => None,
    }
}

fn text(s: &str) -> BinnacleText {
    BinnacleText {
        data: s.as_ptr(),
        len: s.len(),
    }
}

/// # Safety
/// `t` points to `len` readable bytes, or is NULL with `len` 0.
unsafe fn read_text(t: BinnacleText) -> String {
    if t.data.is_null() {
        assert_eq!(t.len, 0);
        return String::new();
    }
    // SAFETY: the caller's contract.
    let bytes = unsafe { std::slice::from_raw_parts(t.data, t.len) };
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn grid() -> Box<BinnacleGrid> {
    BinnacleGrid::from_grid(Grid::new(
        AxisConfig::new(0, 1.0).header(1.0),
        AxisConfig::new(0, 8.0).header(1.0),
    ))
}

/// A Typed Value as plain data, to compare what C reads with what Rust
/// holds.
#[derive(Clone, Debug, PartialEq)]
enum Typed {
    None,
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Number(u64),
    Text(String),
    Bytes(Vec<u8>),
    DateTime(i64),
    Category(String, Option<String>),
    Nested(String),
    Invalid(String, u32),
}

fn typed(value: Option<&Value>) -> Typed {
    match value {
        None => Typed::None,
        Some(Value::Null) => Typed::Null,
        Some(Value::Bool(b)) => Typed::Bool(*b),
        Some(Value::Int(i)) => Typed::Int(*i),
        Some(Value::UInt(u)) => Typed::UInt(*u),
        Some(Value::Number(x)) => Typed::Number(x.to_bits()),
        Some(Value::Text(t)) => Typed::Text(t.clone()),
        Some(Value::Bytes(b)) => Typed::Bytes(b.clone()),
        Some(Value::DateTime(at)) => Typed::DateTime(at.unix_nanos_i64_saturating()),
        Some(Value::Category { value, label }) => Typed::Category(value.clone(), label.clone()),
        Some(Value::Nested(json)) => Typed::Nested(serde_json::to_string(json).unwrap()),
        Some(Value::Invalid { raw, expected }) => {
            Typed::Invalid(raw.clone(), u32::from(expected.code()))
        }
    }
}

/// # Safety
/// `v`'s texts are live.
unsafe fn typed_from_c(v: &BinnacleTypedValue) -> Typed {
    // SAFETY: `kind` names the part set, and the caller's contract.
    unsafe {
        let d = &v.data;
        match v.kind {
            0 => Typed::None,
            1 => Typed::Null,
            2 => Typed::Bool(match d.boolean {
                0 => false,
                1 => true,
                b => panic!("bool {b}"),
            }),
            3 => Typed::Int(d.int64),
            4 => Typed::UInt(d.uint64),
            5 => Typed::Number(d.number.to_bits()),
            6 => Typed::Text(read_text(d.text)),
            7 => Typed::Bytes(std::slice::from_raw_parts(d.bytes.data, d.bytes.len).to_vec()),
            8 => Typed::DateTime(d.datetime),
            9 => Typed::Category(
                read_text(d.category.value),
                (d.category.has_label == 1).then(|| read_text(d.category.label)),
            ),
            10 => Typed::Nested(read_text(d.nested)),
            11 => Typed::Invalid(read_text(d.invalid.raw), d.invalid.expected),
            kind => panic!("unknown kind {kind}"),
        }
    }
}

/// What a cell draws: its text, alignment, style bits and value.
type Drawn = (String, u32, u32, Typed);

/// The cell as Rust formats it at [`NOW`] in `offset`.
fn drawn(source: &DescribedSource, row: Key, col: Key, offset: i32) -> Drawn {
    let value = source.cell(row, col);
    let (Some(v), Some(column)) = (value, source.column(col)) else {
        return (String::new(), 0, 0, Typed::None);
    };
    let zone = FixedOffset(offset);
    let ctx = FormatContext::new(Timestamp::from_unix_nanos(i128::from(NOW)), &zone);
    let f = format(
        v,
        column.unit(),
        column.role(),
        &FormatOptions::default(),
        &ctx,
    );
    let align = match f.align {
        Align::Start => 0,
        Align::End => 1,
    };
    (f.text, align, u32::from(f.style.bits()), typed(value))
}

/// The cells where `rows` meet `cols`, row by row, read through C.
fn drawn_c(src: *mut BinnacleDescribedSource, rows: &[Key], cols: &[Key]) -> Vec<Drawn> {
    let rows: Vec<u64> = rows.iter().map(|k| k.0).collect();
    let cols: Vec<u64> = cols.iter().map(|k| k.0).collect();
    let mut out = vec![BinnacleCell::default(); rows.len() * cols.len()];
    // SAFETY: the arrays are live and `out` holds every cell.
    let ok = unsafe {
        binnacle_described_cells(
            src,
            rows.as_ptr(),
            rows.len(),
            cols.as_ptr(),
            cols.len(),
            out.as_mut_ptr(),
            out.len(),
        )
    };
    assert_eq!(ok, 1);
    out.iter()
        // SAFETY: the texts live until the next call on `src`.
        .map(|c| unsafe { (read_text(c.text), c.align, c.style, typed_from_c(&c.value)) })
        .collect()
}

/// A source fed through C, beside a Rust source and Grid fed the same.
struct Pair {
    src: *mut BinnacleDescribedSource,
    c_grid: *mut BinnacleGrid,
    rust: DescribedSource,
    rust_grid: *mut BinnacleGrid,
    offset: i32,
}

impl Pair {
    fn new(table: &str, offset: i32) -> Pair {
        let src = unsafe {
            binnacle_described_source_new(text(table), text("flotilla"), std::ptr::null(), 0)
        };
        assert!(!src.is_null());
        // SAFETY: `src` is live.
        unsafe {
            assert_eq!(binnacle_described_source_set_now(src, NOW), 1);
            assert_eq!(binnacle_described_source_set_utc_offset(src, offset), 1);
        }
        Pair {
            src,
            c_grid: Box::into_raw(grid()),
            rust: DescribedSource::new(table, Scope::new("flotilla")),
            rust_grid: Box::into_raw(grid()),
            offset,
        }
    }

    fn rust_grid(&mut self) -> &mut BinnacleGrid {
        // SAFETY: live until `drop`, and no other borrow is held.
        unsafe { &mut *self.rust_grid }
    }

    fn c_grid(&mut self) -> &mut BinnacleGrid {
        // SAFETY: as above.
        unsafe { &mut *self.c_grid }
    }

    /// Both take one message.
    fn feed(&mut self, encoding: u32, bytes: &[u8]) {
        // SAFETY: the handles are live; `bytes` outlives the call.
        let ok = unsafe {
            binnacle_described_source_receive(
                self.src,
                self.c_grid,
                encoding,
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        assert_eq!(ok, 1);
        let message = decode(encoding, bytes).unwrap();
        let described = matches!(message, Message::Table(_));
        let received = self.rust.receive(message);
        // SAFETY: as in `rust_grid`, borrowing only the Grid.
        let grid = unsafe { &mut *self.rust_grid }.grid_mut();
        if let Received::Applied(notices) = received {
            for (axis, notice) in notices {
                grid.notify(axis, notice);
            }
        }
        if described {
            self.rust.declare(grid);
        }
    }

    /// Both apply `f` to their Grid.
    fn both(&mut self, f: impl Fn(&mut Grid)) {
        f(self.c_grid().grid_mut());
        f(self.rust_grid().grid_mut());
    }

    /// Both pump and fill a frame, then draw the same.
    fn frame(&mut self) {
        // SAFETY: the handles are live.
        unsafe {
            assert_eq!(binnacle_described_source_pump(self.src, self.c_grid), 1);
            let mut requests = BinnacleWindowRequests::default();
            assert_eq!(
                binnacle_grid_begin_frame(self.c_grid, VIEWPORT.0, VIEWPORT.1, &mut requests),
                1
            );
            assert_eq!(
                binnacle_described_source_fill(self.src, self.c_grid, &requests),
                1
            );
        }
        let source = &mut self.rust;
        // SAFETY: as in `rust_grid`.
        let handle = unsafe { &mut *self.rust_grid };
        handle.pump_with(|grid, pass_on| {
            pump(grid, source, |_, stamped| {
                if !matches!(
                    stamped.effect,
                    Effect::ViewRequested { .. }
                        | Effect::Locate { .. }
                        | Effect::MoveRequested { .. }
                        | Effect::FindStepRequested { .. }
                ) {
                    pass_on(stamped);
                }
            })
        });
        let grid = handle.grid_mut();
        for (axis, range) in grid.begin_frame(Viewport::new(VIEWPORT.0, VIEWPORT.1)) {
            let window = source.window(axis, range.clone());
            grid.set_window(axis, range.start, &window.keys);
            if let Some(outlines) = window.outlines {
                grid.set_outlines(axis, range.start, &outlines);
            }
        }
        self.check();
    }

    /// The same Strips, applied View Spec and cells, and the same Effects
    /// passed on.
    fn check(&mut self) {
        let strips = |h: &mut BinnacleGrid| {
            let s = h.grid_mut().snapshot();
            let strips = Axis::ALL.map(|axis| {
                (s.drawn(axis))
                    .map(|i| (i.position, i.key))
                    .collect::<Vec<_>>()
            });
            (strips, s.applied_view().clone())
        };
        let (c_strips, c_view) = strips(self.c_grid());
        let (rust_strips, rust_view) = strips(self.rust_grid());
        assert_eq!(c_strips, rust_strips);
        assert_eq!(c_view, rust_view);
        let [rows, cols] =
            c_strips.map(|s| s.into_iter().filter_map(|(_, k)| k).collect::<Vec<_>>());
        let expected: Vec<Drawn> = (rows.iter())
            .flat_map(|&r| cols.iter().map(move |&c| (r, c)))
            .map(|(r, c)| drawn(&self.rust, r, c, self.offset))
            .collect();
        assert_eq!(drawn_c(self.src, &rows, &cols), expected);
        assert_eq!(effects(self.c_grid), effects(self.rust_grid));
    }
}

impl Drop for Pair {
    fn drop(&mut self) {
        // SAFETY: each handle is live and not used again.
        unsafe {
            binnacle_described_source_free(self.src);
            binnacle_grid_free(self.c_grid);
            binnacle_grid_free(self.rust_grid);
        }
    }
}

/// Every Effect `grid` holds, as C polls them.
fn effects(grid: *mut BinnacleGrid) -> Vec<(u64, u32, u64, u64, u32, u64)> {
    let mut out = Vec::new();
    let mut e = BinnacleEffect::default();
    // SAFETY: `grid` is live and `e` writable.
    while unsafe { binnacle_grid_poll_effect(grid, &mut e) } == 1 {
        out.push((e.generation, e.kind, e.row, e.col, e.axis, e.key));
    }
    out
}

const FIXTURES: &[&str] = &[
    "crews.ndjson",
    "rows-hosts.ndjson",
    "rows-hosts.cborseq",
    "rows-convoys-sort-where.ndjson",
    "rows-repositories.cborseq",
    "rows-vessels-page1.ndjson",
];

#[derive(Clone, Debug)]
enum Op {
    /// The rows sorted by these columns (indices, wrapped), descending if
    /// set, the quick filter on this text, and these columns hidden.
    Request(Vec<(usize, bool)>, Option<String>, Vec<usize>),
    GroupBy(usize),
    Click(f32, f32),
    Find(String),
    FindNext,
}

#[hegel::composite]
fn word(tc: &TestCase) -> String {
    tc.draw(gs::sampled_from(vec![
        "a", "e", "fe", "Running", "1", "2026", "ago", "∅", "",
    ]))
    .to_owned()
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let index = || gs::integers::<usize>().max_value(20);
    match tc.draw(gs::integers::<u8>().max_value(5)) {
        0 | 1 => Op::Request(
            tc.draw(gs::vecs(hegel::tuples!(index(), gs::booleans())).max_size(3)),
            tc.draw(gs::optional(word())),
            tc.draw(gs::vecs(index()).max_size(3)),
        ),
        2 => Op::GroupBy(tc.draw(index())),
        3 => Op::Click(
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(VIEWPORT.0)),
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(VIEWPORT.1)),
        ),
        4 => Op::Find(tc.draw(word())),
        _ => Op::FindNext,
    }
}

#[hegel::test(test_cases = 200)]
fn a_source_through_c_draws_as_the_rust_source_does(tc: TestCase) {
    let name = tc.draw(gs::sampled_from(FIXTURES.to_vec()));
    let (encoding, messages) = items(name);
    let fed = tc.draw(gs::integers::<usize>().max_value(messages.len()));
    let offset = tc.draw(gs::sampled_from(vec![0, 3600, -18_000, 19_800]));
    let mut pair = Pair::new(table(name), offset);
    for bytes in &messages[..fed] {
        pair.feed(encoding, bytes);
        pair.frame();
    }
    for op in tc.draw(gs::vecs(op()).max_size(8)) {
        let columns: Vec<Key> = pair.rust.columns().iter().map(|c| c.key()).collect();
        let col = |i: usize| columns[i % columns.len()];
        match op {
            _ if columns.is_empty() => {}
            Op::Request(sort, quick, hidden) => {
                let mut sorted = Vec::new();
                let sort: Vec<SortColumn> = (sort.into_iter())
                    .filter(|&(i, _)| {
                        !sorted.contains(&col(i)) && {
                            sorted.push(col(i));
                            true
                        }
                    })
                    .map(|(i, desc)| match desc {
                        true => SortColumn::descending(col(i)),
                        false => SortColumn::ascending(col(i)),
                    })
                    .collect();
                let spec = ViewSpec::default()
                    .with_sort(Axis::Rows, SortSpec::new(sort))
                    .with_hidden(Axis::Cols, hidden.into_iter().map(col).collect())
                    .with_filter(
                        Axis::Rows,
                        FilterSpec::default().with_quick(quick.map(TextQuery::new)),
                    );
                pair.both(|g| g.request_view(spec.clone()));
            }
            Op::GroupBy(i) => pair.both(|g| g.group_by(Axis::Rows, col(i))),
            Op::Click(x, y) => pair.both(|g| {
                g.pointer(PointerEvent::down(x, y));
                g.pointer(PointerEvent::up(x, y));
            }),
            Op::Find(t) => pair.both(|g| g.set_find(Some(FindQuery::new(TextQuery::new(&t))))),
            Op::FindNext => pair.both(|g| g.action(Action::FindNext)),
        }
        pair.frame();
    }
}

/// `source` wrapped as a C handle, formatting at [`NOW`] in UTC.
fn wrapped(source: DescribedSource) -> *mut BinnacleDescribedSource {
    let src = Box::into_raw(BinnacleDescribedSource::from_source(source));
    // SAFETY: `src` is live.
    assert_eq!(unsafe { binnacle_described_source_set_now(src, NOW) }, 1);
    src
}

/// A source fed every message of `name` through C.
fn fed(name: &str) -> (*mut BinnacleDescribedSource, *mut BinnacleGrid) {
    let (encoding, messages) = items(name);
    let src = unsafe {
        binnacle_described_source_new(text(table(name)), text("flotilla"), std::ptr::null(), 0)
    };
    let grid = Box::into_raw(grid());
    // SAFETY: the handles are live.
    unsafe {
        assert_eq!(binnacle_described_source_set_now(src, NOW), 1);
        for bytes in &messages {
            let ok =
                binnacle_described_source_receive(src, grid, encoding, bytes.as_ptr(), bytes.len());
            assert_eq!(ok, 1);
        }
    }
    (src, grid)
}

/// Every cell of the source's rows and columns, through C.
fn every_cell(src: *mut BinnacleDescribedSource) -> Vec<Drawn> {
    // SAFETY: `src` is live; the borrow ends before the C call.
    let (rows, cols) = unsafe {
        let s = (*src).source();
        let cols: Vec<Key> = s.columns().iter().map(|c| c.key()).collect();
        (s.row_keys().to_vec(), cols)
    };
    drawn_c(src, &rows, &cols)
}

#[test]
fn a_source_built_in_rust_reads_through_c_as_one_fed_through_c() {
    let mut drawn = 0;
    for name in FIXTURES {
        let (encoding, messages) = items(name);
        let mut source = DescribedSource::new(table(name), Scope::new("flotilla"));
        for bytes in &messages {
            let _ = source.receive(decode(encoding, bytes).unwrap());
        }
        let built = wrapped(source);
        let (src, grid) = fed(name);
        let cells = every_cell(built);
        drawn += cells.len();
        assert_eq!(cells, every_cell(src), "{name}");
        // SAFETY: each handle is live and not used again.
        unsafe {
            binnacle_described_source_free(built);
            binnacle_described_source_free(src);
            binnacle_grid_free(grid);
        }
    }
    assert!(drawn > 0);
}

#[derive(Clone, Debug)]
enum Hostile {
    Bytes(Vec<u8>),
    /// A message of a fixture cut short.
    Truncated(usize, usize, usize),
}

#[hegel::composite]
fn hostile(tc: &TestCase) -> Hostile {
    match tc.draw(gs::booleans()) {
        true => Hostile::Bytes(tc.draw(gs::binary().max_size(64))),
        false => Hostile::Truncated(
            tc.draw(gs::integers::<usize>().max_value(FIXTURES.len() - 1)),
            tc.draw(gs::integers::<usize>()),
            tc.draw(gs::integers::<usize>()),
        ),
    }
}

#[hegel::test(test_cases = 300)]
fn hostile_input_is_refused_and_changes_nothing(tc: TestCase) {
    let (src, grid) = fed("crews.ndjson");
    let before = every_cell(src);
    let input = tc.draw(hostile());
    let encoding = tc.draw(gs::sampled_from(vec![1, 2, 0, 3, u32::MAX]));
    let (bytes, truncated) = match input {
        Hostile::Bytes(b) => (b, false),
        Hostile::Truncated(f, m, at) => {
            let (e, messages) = items(FIXTURES[f]);
            let whole = &messages[m % messages.len()];
            // A JSON line cut short is not one message, nor is a CBOR item.
            let cut = whole[..at % whole.len()].to_vec();
            assert!(decode(e, &cut).is_none());
            (cut, true)
        }
    };
    // SAFETY: the handles are live; `bytes` outlives the call.
    let ok = unsafe {
        binnacle_described_source_receive(src, grid, encoding, bytes.as_ptr(), bytes.len())
    };
    let message = decode(encoding, &bytes);
    assert_eq!(ok, i32::from(message.is_some()));
    if truncated {
        assert_eq!(ok, 0);
    }
    if ok == 0 {
        assert_eq!(every_cell(src), before);
    }
    // SAFETY: each handle is live and not used again.
    unsafe {
        binnacle_described_source_free(src);
        binnacle_grid_free(grid);
    }
}

#[hegel::test(test_cases = 300)]
fn an_unknown_key_has_no_cell(tc: TestCase) {
    let (src, grid) = fed("crews.ndjson");
    // SAFETY: `src` is live; the borrow ends before the C calls.
    let (rows, cols) = unsafe {
        let s = (*src).source();
        let cols: Vec<Key> = s.columns().iter().map(|c| c.key()).collect();
        (s.row_keys().to_vec(), cols)
    };
    let row = tc.draw(gs::one_of(vec![
        gs::sampled_from(rows.clone()).boxed(),
        gs::integers::<u64>().map(Key).boxed(),
    ]));
    let col = tc.draw(gs::one_of(vec![
        gs::sampled_from(cols.clone()).boxed(),
        gs::integers::<u64>().map(Key).boxed(),
    ]));
    let known = rows.contains(&row) && cols.contains(&col);
    let mut cell = BinnacleCell::default();
    // SAFETY: the handles are live and `cell` writable.
    assert_eq!(
        unsafe { binnacle_described_cell(src, row.0, col.0, &mut cell) },
        1
    );
    assert_eq!(cell.value.kind != 0, known);
    if !known {
        assert_eq!((cell.text.len, cell.style, cell.align), (0, 0, 0));
    }
    let mut info = BinnacleColumnInfo::default();
    // SAFETY: as above.
    let listed = unsafe { binnacle_described_column(src, col.0, &mut info) };
    assert_eq!(listed, i32::from(cols.contains(&col)));
    // SAFETY: each handle is live and not used again.
    unsafe {
        binnacle_described_source_free(src);
        binnacle_grid_free(grid);
    }
}

#[test]
fn every_call_refuses_null() {
    let (src, grid) = fed("crews.ndjson");
    let null_src = std::ptr::null_mut();
    let null_grid = std::ptr::null_mut();
    let col = unsafe { (*src).source().columns()[0].key().0 };
    let row = unsafe { (*src).source().row_keys()[0].0 };
    let line = b"{\"type\":\"synced\"}";
    let requests = BinnacleWindowRequests::default();
    let mut request = BinnacleWatchRequest::default();
    let mut status = BinnacleDescribedStatus::default();
    let mut cell = BinnacleCell::default();
    let mut info = BinnacleColumnInfo::default();
    let mut json = BinnacleText::default();
    let mut distinct = BinnacleDistinct::default();
    let mut n = 0u64;
    let keys = [row];
    let cols = [col];
    // SAFETY: every non-NULL pointer is live.
    unsafe {
        let refused = [
            binnacle_described_source_receive(null_src, grid, 1, line.as_ptr(), line.len()),
            binnacle_described_source_receive(src, null_grid, 1, line.as_ptr(), line.len()),
            binnacle_described_source_receive(src, grid, 1, std::ptr::null(), 0),
            binnacle_described_source_ended(null_src, grid),
            binnacle_described_source_ended(src, null_grid),
            binnacle_described_source_poll_request(null_src, &mut request),
            binnacle_described_source_poll_request(src, std::ptr::null_mut()),
            binnacle_described_source_pump(null_src, grid),
            binnacle_described_source_pump(src, null_grid),
            binnacle_described_source_fill(null_src, grid, &requests),
            binnacle_described_source_fill(src, null_grid, &requests),
            binnacle_described_source_fill(src, grid, std::ptr::null()),
            binnacle_described_source_count(null_src, 0, &mut n),
            binnacle_described_source_count(src, 0, std::ptr::null_mut()),
            binnacle_described_source_status(null_src, &mut status),
            binnacle_described_source_status(src, std::ptr::null_mut()),
            binnacle_described_source_set_now(null_src, NOW),
            binnacle_described_source_set_utc_offset(null_src, 0),
            binnacle_described_cell(null_src, row, col, &mut cell),
            binnacle_described_cell(src, row, col, std::ptr::null_mut()),
            binnacle_described_cells(null_src, keys.as_ptr(), 1, cols.as_ptr(), 1, &mut cell, 1),
            binnacle_described_cells(src, std::ptr::null(), 1, cols.as_ptr(), 1, &mut cell, 1),
            binnacle_described_cells(src, keys.as_ptr(), 1, std::ptr::null(), 1, &mut cell, 1),
            binnacle_described_cells(
                src,
                keys.as_ptr(),
                1,
                cols.as_ptr(),
                1,
                std::ptr::null_mut(),
                1,
            ),
            binnacle_described_column(null_src, col, &mut info),
            binnacle_described_column(src, col, std::ptr::null_mut()),
            binnacle_described_property(null_src, col, text("type"), &mut json),
            binnacle_described_property(src, col, text("type"), std::ptr::null_mut()),
            binnacle_described_distinct(null_src, grid, col, text(""), 10, &mut distinct),
            binnacle_described_distinct(src, null_grid, col, text(""), 10, &mut distinct),
            binnacle_described_distinct(src, grid, col, text(""), 10, std::ptr::null_mut()),
        ];
        assert_eq!(refused, [0; 31]);
        // Each succeeds given every pointer.
        let done = [
            binnacle_described_source_receive(src, grid, 1, line.as_ptr(), line.len()),
            binnacle_described_source_ended(src, grid),
            binnacle_described_source_pump(src, grid),
            binnacle_described_source_fill(src, grid, &requests),
            binnacle_described_source_count(src, 0, &mut n),
            binnacle_described_source_status(src, &mut status),
            binnacle_described_cell(src, row, col, &mut cell),
            binnacle_described_cells(src, keys.as_ptr(), 1, cols.as_ptr(), 1, &mut cell, 1),
            binnacle_described_column(src, col, &mut info),
            binnacle_described_property(src, col, text("type"), &mut json),
            binnacle_described_distinct(src, grid, col, text(""), 10, &mut distinct),
        ];
        assert_eq!(done, [1; 11]);
        // A Window ending before it starts, or an unknown flag, is refused.
        let mut bad = requests;
        bad.axes[0] = BinnacleWindowRequest {
            requested: 1,
            start: 5,
            end: 2,
        };
        assert_eq!(binnacle_described_source_fill(src, grid, &bad), 0);
        bad.axes[0].requested = 2;
        bad.axes[0].end = 6;
        assert_eq!(binnacle_described_source_fill(src, grid, &bad), 0);
        // The request was polled once, when there was one.
        assert_eq!(binnacle_described_source_poll_request(src, &mut request), 1);
        assert_eq!(binnacle_described_source_poll_request(src, &mut request), 0);
        let wheres = [text("x")];
        assert!(binnacle_described_source_new(text("t"), text(""), std::ptr::null(), 0).is_null());
        assert!(
            binnacle_described_source_new(text("t"), text("ns"), std::ptr::null(), 1).is_null()
        );
        let bad = [0xffu8];
        let not_utf8 = BinnacleText {
            data: bad.as_ptr(),
            len: 1,
        };
        assert!(binnacle_described_source_new(not_utf8, text("ns"), std::ptr::null(), 0).is_null());
        assert!(binnacle_described_source_new(text("t"), text("ns"), &not_utf8, 1).is_null());
        let made = binnacle_described_source_new(text("t"), text("ns"), wheres.as_ptr(), 1);
        assert!(!made.is_null());
        binnacle_described_source_free(made);
        binnacle_described_source_free(src);
        binnacle_grid_free(grid);
    }
}

#[test]
fn the_watch_request_names_the_scope() {
    let wheres = [text(".status.phase == Active"), text("x=y")];
    // SAFETY: the texts outlive the calls, and `src` is live until freed.
    unsafe {
        let src =
            binnacle_described_source_new(text("hosts"), text("flotilla"), wheres.as_ptr(), 2);
        let mut r = BinnacleWatchRequest::default();
        assert_eq!(binnacle_described_source_poll_request(src, &mut r), 1);
        assert_eq!(r.kind, 1);
        assert_eq!(read_text(r.table), "hosts");
        assert_eq!(read_text(r.path), "/tables/hosts/rows");
        assert_eq!(
            read_text(r.query),
            "watch&limit=10000&namespace=flotilla&where=.status.phase%20%3D%3D%20Active&where=x%3Dy"
        );
        assert_eq!(
            read_text(r.target),
            format!("{}?{}", read_text(r.path), read_text(r.query))
        );
        assert_eq!(read_text(r.namespace_name), "flotilla");
        let clauses: Vec<String> = std::slice::from_raw_parts(r.wheres, r.wheres_len)
            .iter()
            .map(|&t| read_text(t))
            .collect();
        assert_eq!(clauses, [".status.phase == Active", "x=y"]);
        assert_eq!(r.limit, 10_000);
        binnacle_described_source_free(src);
    }
}

#[test]
fn a_column_reads_as_its_field_describes() {
    let (src, grid) = fed("crews.ndjson");
    let column = |name: &str| unsafe { (*src).source().column_key(name).unwrap().0 };
    let mut info = BinnacleColumnInfo::default();
    // SAFETY: the handles are live; each read ends before the next call.
    unsafe {
        assert_eq!(binnacle_described_column(src, column("host"), &mut info), 1);
        assert_eq!(read_text(info.name), "host");
        assert_eq!((info.has_reference, info.has_resource), (1, 1));
        assert_eq!(read_text(info.resource), "hosts");
        assert_eq!(read_text(info.reference_field), "metadata.name");
        assert_eq!(info.nullable, 1);
        assert_eq!(binnacle_described_column(src, column("load"), &mut info), 1);
        assert_eq!((info.has_unit, read_text(info.unit)), (1, "%".to_owned()));
        assert_eq!(
            (read_text(info.type_name), info.value_kind),
            ("number".to_owned(), 5)
        );
        assert_eq!(info.mismatches, 1);
        assert_eq!(
            binnacle_described_column(src, column("started"), &mut info),
            1
        );
        assert_eq!(info.role_bits, 2);
        assert_eq!(
            binnacle_described_column(src, column("phase"), &mut info),
            1
        );
        assert_eq!((info.role_bits, info.has_title), (4, 1));
        assert_eq!(read_text(info.header), "Phase");
        assert_eq!(binnacle_described_column(src, column("note"), &mut info), 1);
        assert_eq!(
            (info.has_title, read_text(info.header)),
            (0, "note".to_owned())
        );
        let mut status = BinnacleDescribedStatus::default();
        assert_eq!(binnacle_described_source_status(src, &mut status), 1);
        assert_eq!((status.described, status.synced, status.partial), (1, 1, 0));
        assert_eq!((status.rows, status.has_count), (6, 0));
        assert_eq!(status.mismatches, (*src).source().mismatches());
        assert!(status.mismatches > 0);
        let mut json = BinnacleText::default();
        assert_eq!(
            binnacle_described_property(src, column("host"), text("x-flotilla"), &mut json),
            1
        );
        assert_eq!(read_text(json), r#"{"link":"hosts"}"#);
        assert_eq!(
            binnacle_described_property(src, 7, text("type"), &mut json),
            0
        );
        binnacle_described_source_free(src);
        binnacle_grid_free(grid);
    }
}

/// Column `col`'s Distinct Values through C: each value's text, whether
/// some are blank, and whether they were narrowed.
fn distinct(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    col: Key,
) -> (Vec<String>, bool, bool) {
    let mut out = BinnacleDistinct::default();
    // SAFETY: the handles are live; the values live until the next call.
    unsafe {
        assert_eq!(
            binnacle_described_distinct(src, grid, col.0, text(""), 50, &mut out),
            1
        );
        let values = std::slice::from_raw_parts(out.values, out.values_len)
            .iter()
            .map(|v| read_text(v.text))
            .collect();
        (values, out.has_blanks == 1, out.narrowed == 1)
    }
}

#[test]
fn distinct_values_list_a_null_apart_from_an_empty_text_narrowed_by_the_other_filters() {
    let (src, grid) = fed("crews.ndjson");
    let key = |name: &str| unsafe { (*src).source().column_key(name).unwrap() };
    let (note, phase) = (key("note"), key("phase"));
    let (values, blanks, _) = distinct(src, grid, note);
    assert_eq!(values, ["", "check disk", "late"]);
    assert!(blanks);

    // Phase Running keeps alpha (""), delta ("") and foxtrot (null); the
    // note's own condition does not narrow its values.
    let running = Condition::one_of(phase, vec![CoreValue::Text("Running".into())]);
    let late = Condition::one_of(note, vec![CoreValue::Text("late".into())]);
    let filter = FilterSpec::default().with_conditions(vec![running, late].into());
    // SAFETY: `grid` is live; the borrow ends before the C calls.
    unsafe { &mut *grid }
        .grid_mut()
        .request_view(ViewSpec::default().with_filter(Axis::Rows, filter));
    // SAFETY: the handles are live.
    assert_eq!(unsafe { binnacle_described_source_pump(src, grid) }, 1);
    let conditions = unsafe { &mut *grid }
        .grid_mut()
        .applied_view()
        .axis(Axis::Rows)
        .filter
        .conditions
        .conditions()
        .len();
    assert_eq!(conditions, 2);
    let (values, blanks, narrowed) = distinct(src, grid, note);
    assert_eq!(values, [""]);
    assert!(blanks);
    assert!(narrowed);

    // A Key no column has: none.
    let mut out = BinnacleDistinct::default();
    // SAFETY: the handles are live.
    unsafe {
        assert_eq!(
            binnacle_described_distinct(src, grid, 7, text(""), 50, &mut out),
            0
        );
        binnacle_described_source_free(src);
        binnacle_grid_free(grid);
    }
}

#[test]
fn a_source_answers_distinct_values_as_its_pump_source_does() {
    // The same values the Rust source lists, unnarrowed.
    let (src, grid) = fed("crews.ndjson");
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    let (encoding, messages) = items("crews.ndjson");
    for bytes in &messages {
        let _ = source.receive(decode(encoding, bytes).unwrap());
    }
    let cols: Vec<Key> = source.columns().iter().map(|c| c.key()).collect();
    for col in cols {
        let query = binnacle_core::DistinctQuery::new(50);
        let Some(rust) = source.distinct(Axis::Rows, col, &query) else {
            continue;
        };
        let texts: Vec<String> = rust.values.iter().map(|v| v.text.clone()).collect();
        let (values, _, narrowed) = distinct(src, grid, col);
        assert_eq!(values, texts);
        assert_eq!(narrowed, rust.narrowed);
    }
    // SAFETY: each handle is live and not used again.
    unsafe {
        binnacle_described_source_free(src);
        binnacle_grid_free(grid);
    }
}
