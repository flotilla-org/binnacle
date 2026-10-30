//! The Described Table source (`binnacle-described`) for a native renderer:
//! it takes the rows stream's messages, answers the Grid's Effects as an
//! In-Memory Table Source, fills its Windows, and gives each cell its Typed
//! Value and its text from the shared formatter (`binnacle-format`). Every
//! pointer a call fills points into buffers the [`BinnacleDescribedSource`]
//! handle owns, valid until the next call on it.

use binnacle_core::{Axis, Blanks, Condition, DistinctQuery, Effect, FilterSpec, Key};
use binnacle_described::{
    Column, DescribedSource, HostRequest, Message, Received, Scope, decode_cbor_item,
    decode_json_line,
};
use binnacle_format::{
    Align, FixedOffset, FormatContext, FormatOptions, Style, Timestamp, Value, ValueType, format,
};
use binnacle_memory::{PumpSource, pump};

use crate::{BinnacleGrid, BinnacleText, BinnacleWindowRequests, axis, borrowed, flag, write_out};

pub const BINNACLE_ENCODING_NDJSON: u32 = 1;
pub const BINNACLE_ENCODING_CBOR_SEQ: u32 = 2;

pub const BINNACLE_REQUEST_OTHER: u32 = 0;
pub const BINNACLE_REQUEST_OPEN_WATCH: u32 = 1;

pub const BINNACLE_TEXT_ALIGN_START: u32 = 0;
pub const BINNACLE_TEXT_ALIGN_END: u32 = 1;

pub const BINNACLE_STYLE_DIM: u32 = 1;
pub const BINNACLE_STYLE_WARNING: u32 = 2;
pub const BINNACLE_STYLE_PENDING: u32 = 4;

pub const BINNACLE_TYPED_NONE: u32 = 0;
pub const BINNACLE_TYPED_NULL: u32 = 1;
pub const BINNACLE_TYPED_BOOL: u32 = 2;
pub const BINNACLE_TYPED_INT: u32 = 3;
pub const BINNACLE_TYPED_UINT: u32 = 4;
pub const BINNACLE_TYPED_NUMBER: u32 = 5;
pub const BINNACLE_TYPED_TEXT: u32 = 6;
pub const BINNACLE_TYPED_BYTES: u32 = 7;
pub const BINNACLE_TYPED_DATETIME: u32 = 8;
pub const BINNACLE_TYPED_CATEGORY: u32 = 9;
pub const BINNACLE_TYPED_NESTED: u32 = 10;
pub const BINNACLE_TYPED_INVALID: u32 = 11;

pub const BINNACLE_ROLE_KEY: u32 = 1;
pub const BINNACLE_ROLE_TIME: u32 = 2;
pub const BINNACLE_ROLE_STATE: u32 = 4;

/// The opaque handle: a Described Table source plus the clock and offset
/// its cells format with, and the buffers its C-shaped answers point into.
pub struct BinnacleDescribedSource {
    source: DescribedSource,
    now: Option<Timestamp>,
    offset: i32,
    /// The bytes the last call's texts point into; each keeps its heap
    /// buffer where it is as the list grows.
    texts: Vec<Vec<u8>>,
    wheres: Vec<BinnacleText>,
    roles: Vec<BinnacleText>,
    categories: Vec<BinnacleCategory>,
    distinct: Vec<BinnacleDistinctValue>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleBytes {
    pub data: *const u8,
    pub len: usize,
}

impl Default for BinnacleBytes {
    fn default() -> Self {
        BinnacleBytes {
            data: std::ptr::null(),
            len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleCategory {
    pub value: BinnacleText,
    pub has_label: u32,
    pub label: BinnacleText,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleInvalid {
    pub raw: BinnacleText,
    pub expected: u32,
}

/// What a Typed Value holds beyond its kind; `kind` says which part is set.
#[repr(C)]
#[derive(Clone, Copy)]
pub union BinnacleTypedValueData {
    pub boolean: u32,
    pub int64: i64,
    pub uint64: u64,
    pub number: f64,
    pub text: BinnacleText,
    pub bytes: BinnacleBytes,
    pub datetime: i64,
    pub category: BinnacleCategory,
    pub nested: BinnacleText,
    pub invalid: BinnacleInvalid,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BinnacleTypedValue {
    pub kind: u32,
    pub data: BinnacleTypedValueData,
}

impl Default for BinnacleTypedValue {
    /// No such cell: kind 0, and every part zero.
    fn default() -> Self {
        // SAFETY: every field is plain data, for which all-zero bits are valid.
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BinnacleCell {
    pub size: u32,
    pub text: BinnacleText,
    pub align: u32,
    pub style: u32,
    pub value: BinnacleTypedValue,
}

impl Default for BinnacleCell {
    /// This header's size and no such cell.
    fn default() -> Self {
        BinnacleCell {
            size: size_of::<BinnacleCell>() as u32,
            text: BinnacleText::default(),
            align: BINNACLE_TEXT_ALIGN_START,
            style: 0,
            value: BinnacleTypedValue::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleWatchRequest {
    pub kind: u32,
    pub table: BinnacleText,
    pub path: BinnacleText,
    pub query: BinnacleText,
    pub target: BinnacleText,
    pub namespace_name: BinnacleText,
    pub wheres: *const BinnacleText,
    pub wheres_len: usize,
    pub limit: u64,
}

impl Default for BinnacleWatchRequest {
    fn default() -> Self {
        BinnacleWatchRequest {
            kind: BINNACLE_REQUEST_OTHER,
            table: BinnacleText::default(),
            path: BinnacleText::default(),
            query: BinnacleText::default(),
            target: BinnacleText::default(),
            namespace_name: BinnacleText::default(),
            wheres: std::ptr::null(),
            wheres_len: 0,
            limit: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleDescribedStatus {
    pub described: u32,
    pub synced: u32,
    pub partial: u32,
    pub has_count: u32,
    pub count: u64,
    pub estimate: u32,
    pub rows: u64,
    pub mismatches: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleColumnInfo {
    pub size: u32,
    pub key: u64,
    pub name: BinnacleText,
    pub has_title: u32,
    pub title: BinnacleText,
    pub header: BinnacleText,
    pub type_name: BinnacleText,
    pub value_kind: u32,
    pub has_unit: u32,
    pub unit: BinnacleText,
    pub nullable: u32,
    pub role_bits: u32,
    pub roles: *const BinnacleText,
    pub roles_len: usize,
    pub has_categories: u32,
    pub categories: *const BinnacleCategory,
    pub categories_len: usize,
    pub has_reference: u32,
    pub has_resource: u32,
    pub resource: BinnacleText,
    pub reference_field: BinnacleText,
    pub mismatches: u64,
}

impl Default for BinnacleColumnInfo {
    /// This header's size and every field empty.
    fn default() -> Self {
        BinnacleColumnInfo {
            size: size_of::<BinnacleColumnInfo>() as u32,
            key: 0,
            name: BinnacleText::default(),
            has_title: 0,
            title: BinnacleText::default(),
            header: BinnacleText::default(),
            type_name: BinnacleText::default(),
            value_kind: BINNACLE_TYPED_NONE,
            has_unit: 0,
            unit: BinnacleText::default(),
            nullable: 0,
            role_bits: 0,
            roles: std::ptr::null(),
            roles_len: 0,
            has_categories: 0,
            categories: std::ptr::null(),
            categories_len: 0,
            has_reference: 0,
            has_resource: 0,
            resource: BinnacleText::default(),
            reference_field: BinnacleText::default(),
            mismatches: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleDistinctValue {
    pub text: BinnacleText,
    pub has_count: u32,
    pub count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleDistinct {
    pub values: *const BinnacleDistinctValue,
    pub values_len: usize,
    pub has_blanks: u32,
    pub blanks_counted: u32,
    pub blanks: u64,
    pub partial: u32,
    pub narrowed: u32,
}

impl Default for BinnacleDistinct {
    fn default() -> Self {
        BinnacleDistinct {
            values: std::ptr::null(),
            values_len: 0,
            has_blanks: 0,
            blanks_counted: 0,
            blanks: 0,
            partial: 0,
            narrowed: 0,
        }
    }
}

/// The C kind of a Typed Value of type `ty`.
fn kind_of(ty: ValueType) -> u32 {
    match ty {
        ValueType::Null => BINNACLE_TYPED_NULL,
        ValueType::Bool => BINNACLE_TYPED_BOOL,
        ValueType::Int => BINNACLE_TYPED_INT,
        ValueType::UInt => BINNACLE_TYPED_UINT,
        ValueType::Number => BINNACLE_TYPED_NUMBER,
        ValueType::Text => BINNACLE_TYPED_TEXT,
        ValueType::Bytes => BINNACLE_TYPED_BYTES,
        ValueType::DateTime => BINNACLE_TYPED_DATETIME,
        ValueType::Category => BINNACLE_TYPED_CATEGORY,
        ValueType::Nested => BINNACLE_TYPED_NESTED,
        ValueType::Invalid => BINNACLE_TYPED_INVALID,
    }
}

fn style_bits(style: Style) -> u32 {
    (if style.contains(Style::DIM) {
        BINNACLE_STYLE_DIM
    } else {
        0
    }) | (if style.contains(Style::WARNING) {
        BINNACLE_STYLE_WARNING
    } else {
        0
    }) | (if style.contains(Style::PENDING) {
        BINNACLE_STYLE_PENDING
    } else {
        0
    })
}

fn role_bits(roles: &[String]) -> u32 {
    (roles.iter())
        .map(|r| match r.as_str() {
            "key" => BINNACLE_ROLE_KEY,
            "time" => BINNACLE_ROLE_TIME,
            "state" => BINNACLE_ROLE_STATE,
            _ => 0,
        })
        .fold(0, |bits, b| bits | b)
}

/// Whether the pump answers `effect` itself, so it is not passed on.
fn answered(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::ViewRequested { .. }
            | Effect::Locate { .. }
            | Effect::MoveRequested { .. }
            | Effect::FindStepRequested { .. }
    )
}

/// Borrowed C text as UTF-8, or `None` if it is not.
///
/// # Safety
/// A non-NULL `text.data` is valid for `text.len` reads.
unsafe fn text_in<'a>(text: BinnacleText) -> Option<&'a str> {
    // SAFETY: the caller's contract.
    std::str::from_utf8(unsafe { borrowed(text.data, text.len) }).ok()
}

/// The size a caller set at the start of a size-prefixed struct, or `None`
/// for NULL or one too small for this header's `T`.
///
/// # Safety
/// `out` is NULL or valid for reading a `u32`.
unsafe fn sized<T>(out: *mut T) -> Option<u32> {
    // SAFETY: the caller's contract.
    let size = unsafe { out.cast::<u32>().as_ref() }.copied()?;
    (size as usize >= size_of::<T>()).then_some(size)
}

impl BinnacleDescribedSource {
    /// A handle over `source`, its cells formatted with the system clock in
    /// UTC. `Box::into_raw` gives the pointer C uses, as
    /// [`binnacle_described_source_new`] returns it, freed by
    /// [`binnacle_described_source_free`].
    pub fn from_source(source: DescribedSource) -> Box<BinnacleDescribedSource> {
        Box::new(BinnacleDescribedSource {
            source,
            now: None,
            offset: 0,
            texts: Vec::new(),
            wheres: Vec::new(),
            roles: Vec::new(),
            categories: Vec::new(),
            distinct: Vec::new(),
        })
    }

    pub fn source(&self) -> &DescribedSource {
        &self.source
    }

    pub fn source_mut(&mut self) -> &mut DescribedSource {
        &mut self.source
    }

    /// Starts a call that fills pointers: the last call's are let go.
    fn clear(&mut self) {
        self.texts.clear();
        self.wheres.clear();
        self.roles.clear();
        self.categories.clear();
        self.distinct.clear();
    }

    /// `bytes` kept until the next call, as C text.
    fn keep(&mut self, bytes: impl Into<Vec<u8>>) -> BinnacleText {
        let bytes = bytes.into();
        let text = BinnacleText {
            data: bytes.as_ptr(),
            len: bytes.len(),
        };
        self.texts.push(bytes);
        text
    }

    fn category(&mut self, value: &str, label: Option<&str>) -> BinnacleCategory {
        BinnacleCategory {
            value: self.keep(value),
            has_label: flag(label.is_some()),
            label: label.map_or_else(BinnacleText::default, |l| self.keep(l)),
        }
    }

    fn now(&self) -> Timestamp {
        self.now.unwrap_or_else(|| {
            let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
            Timestamp::from_unix_nanos(since.map_or(0, |d| d.as_nanos() as i128))
        })
    }

    /// The cell where `row` meets `col`, formatted at `now`, its size
    /// `size`.
    fn cell(&mut self, row: Key, col: Key, now: Timestamp, size: u32) -> BinnacleCell {
        let mut cell = BinnacleCell {
            size,
            ..BinnacleCell::default()
        };
        let (Some(value), Some(column)) = (self.source.cell(row, col), self.source.column(col))
        else {
            return cell;
        };
        let zone = FixedOffset(self.offset);
        let ctx = FormatContext::new(now, &zone);
        let formatted = format(
            value,
            column.unit(),
            column.role(),
            &FormatOptions::default(),
            &ctx,
        );
        let value = value.clone();
        cell.text = self.keep(formatted.text);
        cell.align = match formatted.align {
            Align::Start => BINNACLE_TEXT_ALIGN_START,
            Align::End => BINNACLE_TEXT_ALIGN_END,
        };
        cell.style = style_bits(formatted.style);
        cell.value = self.typed(&value);
        cell
    }

    fn typed(&mut self, value: &Value) -> BinnacleTypedValue {
        let mut c = BinnacleTypedValue {
            kind: kind_of(value.value_type()),
            ..BinnacleTypedValue::default()
        };
        match value {
            Value::Null => {}
            Value::Bool(b) => c.data.boolean = flag(*b),
            Value::Int(i) => c.data.int64 = *i,
            Value::UInt(u) => c.data.uint64 = *u,
            Value::Number(x) => c.data.number = *x,
            Value::Text(t) => c.data.text = self.keep(t.as_str()),
            Value::Bytes(b) => {
                let t = self.keep(b.as_slice());
                c.data.bytes = BinnacleBytes {
                    data: t.data,
                    len: t.len,
                };
            }
            Value::DateTime(at) => c.data.datetime = at.unix_nanos_i64_saturating(),
            Value::Category { value, label } => {
                c.data.category = self.category(value, label.as_deref());
            }
            Value::Nested(json) => c.data.nested = self.keep(json.to_string()),
            Value::Invalid { raw, expected } => {
                c.data.invalid = BinnacleInvalid {
                    raw: self.keep(raw.as_str()),
                    expected: kind_of(*expected),
                };
            }
        }
        c
    }

    fn column_to_c(&mut self, column: &Column, size: u32) -> BinnacleColumnInfo {
        let roles: Vec<BinnacleText> = (column.roles().iter())
            .map(|r| self.keep(r.as_str()))
            .collect();
        self.roles = roles;
        let categories: Vec<BinnacleCategory> = (column.categories().unwrap_or_default())
            .iter()
            .map(|c| self.category(&c.value, c.label.as_deref()))
            .collect();
        self.categories = categories;
        let reference = column.reference();
        let resource = reference.and_then(|r| r.resource.as_deref());
        BinnacleColumnInfo {
            size,
            key: column.key().0,
            name: self.keep(column.name()),
            has_title: flag(column.title().is_some()),
            title: column
                .title()
                .map_or_else(BinnacleText::default, |t| self.keep(t)),
            header: self.keep(column.header()),
            type_name: self.keep(column.field_type().name()),
            value_kind: kind_of(column.value_type()),
            has_unit: flag(column.unit().is_some()),
            unit: column
                .unit()
                .map_or_else(BinnacleText::default, |u| self.keep(u)),
            nullable: flag(column.nullable()),
            role_bits: role_bits(column.roles()),
            roles: self.roles.as_ptr(),
            roles_len: self.roles.len(),
            has_categories: flag(column.categories().is_some()),
            categories: self.categories.as_ptr(),
            categories_len: self.categories.len(),
            has_reference: flag(reference.is_some()),
            has_resource: flag(resource.is_some()),
            resource: resource.map_or_else(BinnacleText::default, |r| self.keep(r)),
            reference_field: reference
                .map_or_else(BinnacleText::default, |r| self.keep(r.field.as_str())),
            mismatches: column.mismatches(),
        }
    }
}

/// Sends what the source did with a message on to `grid`.
fn apply(received: Received, grid: &mut BinnacleGrid) {
    // Anything else, such as a message this source does not handle yet,
    // applied nothing.
    if let Received::Applied(notices) = received {
        for (axis, notice) in notices {
            grid.grid.notify(axis, notice);
        }
    }
}

/// Runs `f` on the handle; 0 for NULL or if `f` gives `None`, else 1.
///
/// # Safety
/// `src` is NULL or a live handle.
unsafe fn with_source(
    src: *mut BinnacleDescribedSource,
    f: impl FnOnce(&mut BinnacleDescribedSource) -> Option<()>,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(h) = (unsafe { src.as_mut() }) else {
        return 0;
    };
    f(h).is_some().into()
}

/// Runs `f` on the source and the Grid; 0 if either is NULL or if `f`
/// gives `None`, else 1.
///
/// # Safety
/// `src` and `grid` are each NULL or a live handle.
unsafe fn with_both(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    f: impl FnOnce(&mut BinnacleDescribedSource, &mut BinnacleGrid) -> Option<()>,
) -> i32 {
    // SAFETY: the caller's contract.
    let (Some(h), Some(g)) = (unsafe { src.as_mut() }, unsafe { grid.as_mut() }) else {
        return 0;
    };
    f(h, g).is_some().into()
}

/// A new source for the rows of `table` within `namespace_name`, narrowed
/// by `wheres`; NULL for an empty namespace or text that is not UTF-8.
/// Free it with [`binnacle_described_source_free`].
///
/// # Safety
/// Each text's non-NULL `data` is valid for its `len` reads; a non-NULL
/// `wheres` is valid for `wheres_len` reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_new(
    table: BinnacleText,
    namespace_name: BinnacleText,
    wheres: *const BinnacleText,
    wheres_len: usize,
) -> *mut BinnacleDescribedSource {
    // SAFETY: the caller's contract.
    let (Some(table), Some(namespace)) = (unsafe { text_in(table) }, unsafe {
        text_in(namespace_name)
    }) else {
        return std::ptr::null_mut();
    };
    if namespace.is_empty() || (wheres.is_null() && wheres_len > 0) {
        return std::ptr::null_mut();
    }
    let mut scope = Scope::new(namespace);
    // SAFETY: the caller's contract.
    for &clause in unsafe { borrowed(wheres, wheres_len) } {
        // SAFETY: the caller's contract.
        let Some(clause) = (unsafe { text_in(clause) }) else {
            return std::ptr::null_mut();
        };
        scope = scope.with_where(clause);
    }
    Box::into_raw(BinnacleDescribedSource::from_source(DescribedSource::new(
        table, scope,
    )))
}

/// # Safety
/// `src` is NULL or a live handle, not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_free(src: *mut BinnacleDescribedSource) {
    if !src.is_null() {
        // SAFETY: the caller's contract.
        drop(unsafe { Box::from_raw(src) });
    }
}

/// # Safety
/// `src` and `grid` are NULL or live handles; a non-NULL `bytes` is valid
/// for `len` reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_receive(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    encoding: u32,
    bytes: *const u8,
    len: usize,
) -> i32 {
    if bytes.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let bytes = unsafe { borrowed(bytes, len) };
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            let message = match encoding {
                BINNACLE_ENCODING_NDJSON => decode_json_line(bytes),
                BINNACLE_ENCODING_CBOR_SEQ => decode_cbor_item(bytes),
                _ => return None,
            }
            .ok()?;
            let described = matches!(message, Message::Table(_));
            apply(h.source.receive(message), g);
            if described {
                h.source.declare(&mut g.grid);
            }
            Some(())
        })
    }
}

/// # Safety
/// `src` and `grid` are NULL or live handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_ended(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            apply(h.source.ended(), g);
            Some(())
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_poll_request(
    src: *mut BinnacleDescribedSource,
    out: *mut BinnacleWatchRequest,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            let request = h.source.poll_request()?;
            h.clear();
            let c = match request {
                HostRequest::OpenWatch(w) => {
                    let wheres: Vec<BinnacleText> = (w.scope.wheres().iter())
                        .map(|clause| h.keep(clause.as_str()))
                        .collect();
                    h.wheres = wheres;
                    BinnacleWatchRequest {
                        kind: BINNACLE_REQUEST_OPEN_WATCH,
                        table: h.keep(w.table.as_str()),
                        path: h.keep(w.path()),
                        query: h.keep(w.query()),
                        target: h.keep(w.target()),
                        namespace_name: h.keep(w.scope.namespace()),
                        wheres: h.wheres.as_ptr(),
                        wheres_len: h.wheres.len(),
                        limit: w.limit,
                    }
                }
                _ => BinnacleWatchRequest::default(),
            };
            write_out(out, c)
        })
    }
}

/// # Safety
/// `src` and `grid` are NULL or live handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_pump(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            let source = &mut h.source;
            g.pump_with(|grid, pass_on| {
                pump(grid, source, |_, stamped| {
                    if !answered(&stamped.effect) {
                        pass_on(stamped);
                    }
                })
            });
            Some(())
        })
    }
}

/// # Safety
/// `src` and `grid` are NULL or live handles; `requests` is NULL or valid
/// for a read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_fill(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    requests: *const BinnacleWindowRequests,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(&requests) = (unsafe { requests.as_ref() }) else {
        return 0;
    };
    if requests.axes.iter().any(|r| r.requested > 1) {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            for axis in Axis::ALL {
                let r = requests.axes[axis.index()];
                if r.requested == 0 {
                    continue;
                }
                let window = h.source.window(axis, r.start..r.end);
                g.grid.set_window(axis, r.start, &window.keys);
                if let Some(outlines) = window.outlines {
                    g.grid.set_outlines(axis, r.start, &outlines);
                }
            }
            Some(())
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_count(
    src: *mut BinnacleDescribedSource,
    axis_tag: u32,
    out: *mut u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with_source(src, |h| write_out(out, h.source.count(axis(axis_tag)?))) }
}

/// # Safety
/// `src` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_status(
    src: *mut BinnacleDescribedSource,
    out: *mut BinnacleDescribedStatus,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            let s = h.source.status();
            write_out(
                out,
                BinnacleDescribedStatus {
                    described: flag(s.described),
                    synced: flag(s.synced),
                    partial: flag(s.partial),
                    has_count: flag(s.count.is_some()),
                    count: s.count.map_or(0, |(n, _)| n),
                    estimate: flag(s.count.is_some_and(|(_, e)| e)),
                    rows: h.source.row_count() as u64,
                    mismatches: h.source.mismatches(),
                },
            )
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_set_now(
    src: *mut BinnacleDescribedSource,
    unix_nanos: i64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            h.now = Some(Timestamp::from_unix_nanos(i128::from(unix_nanos)));
            Some(())
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_set_utc_offset(
    src: *mut BinnacleDescribedSource,
    seconds_east: i32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            h.offset = seconds_east;
            Some(())
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle; `out` is NULL or valid for reading its
/// `size` and, if that is at least `sizeof(BinnacleCell)`, for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_cell(
    src: *mut BinnacleDescribedSource,
    row: u64,
    col: u64,
    out: *mut BinnacleCell,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(size) = (unsafe { sized(out) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            h.clear();
            let now = h.now();
            let cell = h.cell(Key(row), Key(col), now, size);
            write_out(out, cell)
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle; non-NULL `rows` and `cols` are valid for
/// `rows_len` and `cols_len` reads; a non-NULL `out` is valid for reading
/// and writing `out_len` cells.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_cells(
    src: *mut BinnacleDescribedSource,
    rows: *const u64,
    rows_len: usize,
    cols: *const u64,
    cols_len: usize,
    out: *mut BinnacleCell,
    out_len: usize,
) -> i32 {
    let null = |p: bool, len: usize| p && len > 0;
    if out.is_null() || null(rows.is_null(), rows_len) || null(cols.is_null(), cols_len) {
        return 0;
    }
    let Some(n) = rows_len.checked_mul(cols_len).filter(|&n| n <= out_len) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    let (rows, cols) = unsafe { (borrowed(rows, rows_len), borrowed(cols, cols_len)) };
    // SAFETY: the caller's contract.
    let sizes: Option<Vec<u32>> = (0..n).map(|i| unsafe { sized(out.add(i)) }).collect();
    let Some(sizes) = sizes else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            h.clear();
            let now = h.now();
            for (r, &row) in rows.iter().enumerate() {
                for (c, &col) in cols.iter().enumerate() {
                    let i = r * cols_len + c;
                    let cell = h.cell(Key(row), Key(col), now, sizes[i]);
                    write_out(out.add(i), cell)?;
                }
            }
            Some(())
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle; `out` is NULL or valid for reading its
/// `size` and, if that is at least `sizeof(BinnacleColumnInfo)`, for a
/// write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_column(
    src: *mut BinnacleDescribedSource,
    col: u64,
    out: *mut BinnacleColumnInfo,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(size) = (unsafe { sized(out) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            let column = h.source.column(Key(col))?.clone();
            h.clear();
            let c = h.column_to_c(&column, size);
            write_out(out, c)
        })
    }
}

/// # Safety
/// `src` is NULL or a live handle; `name`'s non-NULL `data` is valid for
/// its `len` reads; `json_out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_property(
    src: *mut BinnacleDescribedSource,
    col: u64,
    name: BinnacleText,
    json_out: *mut BinnacleText,
) -> i32 {
    if json_out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let Some(name) = (unsafe { text_in(name) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            let json = serde_json::to_vec(h.source.property(Key(col), name)?).ok()?;
            h.clear();
            let text = h.keep(json);
            write_out(json_out, text)
        })
    }
}

/// # Safety
/// `src` and `grid` are NULL or live handles; `search`'s non-NULL `data` is
/// valid for its `len` reads; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_distinct(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    col: u64,
    search: BinnacleText,
    limit: usize,
    out: *mut BinnacleDistinct,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let Some(search) = (unsafe { text_in(search) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            let col = Key(col);
            h.source.column(col)?;
            let applied = &g.grid.applied_view().axis(Axis::Rows).filter;
            let others: Vec<Condition> = (applied.conditions.conditions().iter())
                .filter(|c| c.key != col)
                .cloned()
                .collect();
            let narrow = FilterSpec::default()
                .with_quick(applied.quick.clone())
                .with_conditions(others.into());
            let query = DistinctQuery::new(limit).search(search).narrow(narrow);
            let distinct = h.source.distinct(Axis::Rows, col, &query)?;
            h.clear();
            let values: Vec<BinnacleDistinctValue> = (distinct.values.iter())
                .map(|v| BinnacleDistinctValue {
                    text: h.keep(v.text.as_str()),
                    has_count: flag(v.count.is_some()),
                    count: v.count.unwrap_or(0),
                })
                .collect();
            h.distinct = values;
            let (blanks_counted, blanks) = match distinct.blanks {
                Some(Blanks::Counted(n)) => (1, n),
                _ => (0, 0),
            };
            write_out(
                out,
                BinnacleDistinct {
                    values: h.distinct.as_ptr(),
                    values_len: h.distinct.len(),
                    has_blanks: flag(distinct.blanks.is_some()),
                    blanks_counted,
                    blanks,
                    partial: flag(distinct.partial),
                    narrowed: flag(distinct.narrowed),
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_is_its_value_types_code() {
        for ty in ValueType::ALL {
            assert_eq!(kind_of(ty), u32::from(ty.code()), "{ty:?}");
        }
    }

    #[test]
    fn each_style_bit_is_the_formatters() {
        for (style, bit) in [
            (Style::DIM, BINNACLE_STYLE_DIM),
            (Style::WARNING, BINNACLE_STYLE_WARNING),
            (Style::PENDING, BINNACLE_STYLE_PENDING),
        ] {
            assert_eq!(style_bits(style), bit);
            assert_eq!(u32::from(style.bits()), bit);
        }
    }
}
