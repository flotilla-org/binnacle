//! The Described Table source (`binnacle-described`) for a native renderer:
//! it takes the rows stream's messages, answers the Grid's Effects as an
//! In-Memory Table Source, fills its Windows, and gives each cell its Typed
//! Value and its text from the shared formatter (`binnacle-format`). Every
//! pointer a call fills points into buffers the [`BinnacleDescribedSource`]
//! handle owns, valid until the next call on it.

use std::panic::{AssertUnwindSafe, catch_unwind};

use binnacle_core::{Axis, Blanks, Condition, DistinctQuery, Effect, FilterSpec, Key};
use binnacle_described::{
    Column, DescribedSource, HostRequest, Message, Received, Scope, decode_cbor_item,
    decode_json_line,
};
use binnacle_format::{
    Align, FixedOffset, FormatContext, FormatOptions, Style, Timestamp, Value, ValueType, Zone,
    format,
};
use binnacle_memory::{PumpSource, pump};

use crate::{
    BinnacleGrid, BinnacleText, BinnacleWindowRequest, BinnacleWindowRequests, axis, borrowed,
    flag, write_out,
};

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

/// The opaque handle: a Described Table source plus the clock its cells
/// format with, the Grid it drives, and the buffers its C-shaped answers
/// point into. Its cells draw dates in the source's Zone, the one their
/// Stable Text is in.
///
/// It is `!Send`, as the [`DescribedSource`] it holds is: use it, and the
/// Grid it drives, from one thread.
pub struct BinnacleDescribedSource {
    source: DescribedSource,
    now: Option<Timestamp>,
    /// The address of the Grid it drives: the first any call gave it.
    driven: Option<usize>,
    /// A call panicked, so its state may be half-changed: every call after
    /// is refused.
    poisoned: bool,
    texts: Texts,
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

/// The bytes the last call's texts point into; each keeps its heap buffer
/// where it is as the list grows.
#[derive(Default)]
struct Texts(Vec<Vec<u8>>);

impl Texts {
    /// `bytes` kept until the next call, as C text.
    fn keep(&mut self, bytes: impl Into<Vec<u8>>) -> BinnacleText {
        let bytes = bytes.into();
        let text = BinnacleText {
            data: bytes.as_ptr(),
            len: bytes.len(),
        };
        self.0.push(bytes);
        text
    }

    fn category(&mut self, value: &str, label: Option<&str>) -> BinnacleCategory {
        BinnacleCategory {
            value: self.keep(value),
            has_label: flag(label.is_some()),
            label: label.map_or_else(BinnacleText::default, |l| self.keep(l)),
        }
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
}

impl BinnacleDescribedSource {
    /// A handle over `source`, its cells formatted with the system clock in
    /// the source's Zone. `Box::into_raw` gives the pointer C uses, as
    /// [`binnacle_described_source_new`] returns it, freed by
    /// [`binnacle_described_source_free`].
    pub fn from_source(source: DescribedSource) -> Box<BinnacleDescribedSource> {
        Box::new(BinnacleDescribedSource {
            source,
            now: None,
            driven: None,
            poisoned: false,
            texts: Texts::default(),
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

    /// The clock relative dates count from, or `None` for the system clock
    /// read at each call.
    pub fn set_now(&mut self, now: Option<Timestamp>) {
        self.now = now;
    }

    /// Dates draw, and their Stable Text is, in `zone` from now on
    /// ([`DescribedSource::set_zone`]), with the Change Notices that brings
    /// sent to `grid`. `false`, changing nothing, for a Grid other than the
    /// one this source drives.
    pub fn set_zone(&mut self, grid: &mut BinnacleGrid, zone: impl Zone + 'static) -> bool {
        if !self.drives(grid) {
            return false;
        }
        for (axis, notice) in self.source.set_zone(zone) {
            grid.grid.notify(axis, notice);
        }
        true
    }

    /// Whether this source drives `grid`: the first Grid it is given.
    fn drives(&mut self, grid: &BinnacleGrid) -> bool {
        let at = std::ptr::from_ref(grid) as usize;
        *self.driven.get_or_insert(at) == at
    }

    /// Starts a call that fills pointers: the last call's are let go.
    fn clear(&mut self) {
        self.texts.0.clear();
        self.wheres.clear();
        self.roles.clear();
        self.categories.clear();
        self.distinct.clear();
    }

    /// `bytes` kept until the next call, as C text.
    fn keep(&mut self, bytes: impl Into<Vec<u8>>) -> BinnacleText {
        self.texts.keep(bytes)
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
        let ctx = FormatContext::new(now, self.source.time_zone());
        let formatted = format(
            value,
            column.unit(),
            column.role(),
            &FormatOptions::default(),
            &ctx,
        );
        cell.text = self.texts.keep(formatted.text);
        cell.align = match formatted.align {
            Align::Start => BINNACLE_TEXT_ALIGN_START,
            Align::End => BINNACLE_TEXT_ALIGN_END,
        };
        cell.style = style_bits(formatted.style);
        // The source and the texts are apart, so the value is not copied.
        cell.value = self.texts.typed(value);
        cell
    }
}

/// `column` in C form, its texts kept in `texts`, its roles in `roles` and
/// its categories in `categories`.
fn column_to_c(
    column: &Column,
    size: u32,
    texts: &mut Texts,
    roles: &mut Vec<BinnacleText>,
    categories: &mut Vec<BinnacleCategory>,
) -> BinnacleColumnInfo {
    *roles = (column.roles().iter())
        .map(|r| texts.keep(r.as_str()))
        .collect();
    *categories = (column.categories().unwrap_or_default())
        .iter()
        .map(|c| texts.category(&c.value, c.label.as_deref()))
        .collect();
    let reference = column.reference();
    let resource = reference.and_then(|r| r.resource.as_deref());
    BinnacleColumnInfo {
        size,
        key: column.key().0,
        name: texts.keep(column.name()),
        has_title: flag(column.title().is_some()),
        title: column
            .title()
            .map_or_else(BinnacleText::default, |t| texts.keep(t)),
        header: texts.keep(column.header()),
        type_name: texts.keep(column.field_type().name()),
        value_kind: kind_of(column.value_type()),
        has_unit: flag(column.unit().is_some()),
        unit: column
            .unit()
            .map_or_else(BinnacleText::default, |u| texts.keep(u)),
        nullable: flag(column.nullable()),
        role_bits: role_bits(column.roles()),
        roles: roles.as_ptr(),
        roles_len: roles.len(),
        has_categories: flag(column.categories().is_some()),
        categories: categories.as_ptr(),
        categories_len: categories.len(),
        has_reference: flag(reference.is_some()),
        has_resource: flag(resource.is_some()),
        resource: resource.map_or_else(BinnacleText::default, |r| texts.keep(r)),
        reference_field: reference
            .map_or_else(BinnacleText::default, |r| texts.keep(r.field.as_str())),
        mismatches: column.mismatches(),
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

#[cfg(test)]
thread_local! {
    /// In tests, a panic the next guarded call raises, to show it is caught.
    static PANIC_NEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Panics if a test armed [`PANIC_NEXT`]; nothing otherwise.
fn probe() {
    #[cfg(test)]
    if PANIC_NEXT.take() {
        panic!("a probe panic");
    }
}

/// Runs `f`, catching a panic: `None` for one.
fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(|| {
        probe();
        f()
    }))
    .ok()
}

/// Runs `f` on a usable handle, 1 if it gives `Some`. 0 if it gives
/// `None`, for NULL or a poisoned handle, or if `f` panics, which poisons
/// the handle.
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
    if h.poisoned {
        return 0;
    }
    match guarded(|| f(&mut *h)) {
        Some(done) => done.is_some().into(),
        None => {
            h.poisoned = true;
            0
        }
    }
}

/// Runs `f` on the source and the Grid it drives, as [`with_source`]
/// does; 0 too if `grid` is NULL or another Grid.
///
/// # Safety
/// `src` and `grid` are each NULL or a live handle.
unsafe fn with_both(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    f: impl FnOnce(&mut BinnacleDescribedSource, &mut BinnacleGrid) -> Option<()>,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(g) = (unsafe { grid.as_mut() }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe { with_source(src, |h| h.drives(g).then(|| f(h, g))?) }
}

/// A new source for the rows of `table` within `namespace_name`, narrowed
/// by `wheres`; NULL for an empty table or namespace, or text that is not
/// UTF-8. Free it with [`binnacle_described_source_free`].
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
    let made = guarded(|| {
        // SAFETY: the caller's contract.
        let (table, namespace) = unsafe { (text_in(table)?, text_in(namespace_name)?) };
        if table.is_empty() || namespace.is_empty() || (wheres.is_null() && wheres_len > 0) {
            return None;
        }
        let mut scope = Scope::new(namespace);
        // SAFETY: the caller's contract.
        for &clause in unsafe { borrowed(wheres, wheres_len) } {
            // SAFETY: the caller's contract.
            scope = scope.with_where(unsafe { text_in(clause) }?);
        }
        let source = DescribedSource::new(table, scope);
        Some(Box::into_raw(BinnacleDescribedSource::from_source(source)))
    });
    made.flatten().unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// `src` is NULL or a live handle, not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_free(src: *mut BinnacleDescribedSource) {
    if !src.is_null() {
        // A panic while dropping leaks what was left.
        // SAFETY: the caller's contract.
        let _ = guarded(|| drop(unsafe { Box::from_raw(src) }));
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
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            if bytes.is_null() {
                return None;
            }
            // SAFETY: the caller's contract.
            let bytes = borrowed(bytes, len);
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
            if let Some((spec, totals)) = h.source.poll_view() {
                g.grid.view_changed(spec, totals);
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
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            if out.is_null() {
                return None;
            }
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
    unsafe {
        with_both(src, grid, |h, g| {
            // SAFETY: the caller's contract.
            let &requests = requests.as_ref()?;
            let bad = |r: &BinnacleWindowRequest| {
                r.requested > 1 || (r.requested == 1 && r.end < r.start)
            };
            if requests.axes.iter().any(bad) {
                return None;
            }
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
            h.set_now(Some(Timestamp::from_unix_nanos(i128::from(unix_nanos))));
            Some(())
        })
    }
}

/// # Safety
/// `src` and `grid` are NULL or live handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_described_source_set_utc_offset(
    src: *mut BinnacleDescribedSource,
    grid: *mut BinnacleGrid,
    seconds_east: i32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            h.set_zone(g, FixedOffset(seconds_east)).then_some(())
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
    unsafe {
        with_source(src, |h| {
            // SAFETY: the caller's contract.
            let size = sized(out)?;
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
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            let missing = |is_null: bool, len: usize| is_null && len > 0;
            if out.is_null()
                || missing(rows.is_null(), rows_len)
                || missing(cols.is_null(), cols_len)
            {
                return None;
            }
            let n = rows_len.checked_mul(cols_len).filter(|&n| n <= out_len)?;
            // SAFETY: the caller's contract.
            let (rows, cols) = (borrowed(rows, rows_len), borrowed(cols, cols_len));
            // SAFETY: the caller's contract.
            if !(0..n).all(|i| sized(out.add(i)).is_some()) {
                return None;
            }
            h.clear();
            let now = h.now();
            for (r, &row) in rows.iter().enumerate() {
                for (c, &col) in cols.iter().enumerate() {
                    let i = r * cols_len + c;
                    let size = out.add(i).cast::<u32>().read();
                    let cell = h.cell(Key(row), Key(col), now, size);
                    // `out` is not NULL and holds `n` cells, so this write
                    // cannot fail partway.
                    out.add(i).write(cell);
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
    unsafe {
        with_source(src, |h| {
            // SAFETY: the caller's contract.
            let size = sized(out)?;
            h.source.column(Key(col))?;
            h.clear();
            let BinnacleDescribedSource {
                source,
                texts,
                roles,
                categories,
                ..
            } = h;
            let column = source.column(Key(col))?;
            let c = column_to_c(column, size, texts, roles, categories);
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
    // SAFETY: the caller's contract.
    unsafe {
        with_source(src, |h| {
            if json_out.is_null() {
                return None;
            }
            // SAFETY: the caller's contract.
            let name = text_in(name)?;
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
    // SAFETY: the caller's contract.
    unsafe {
        with_both(src, grid, |h, g| {
            if out.is_null() {
                return None;
            }
            // SAFETY: the caller's contract.
            let search = text_in(search)?;
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
    use binnacle_core::{AxisConfig, Grid};

    use super::*;
    use crate::BinnacleWindowRequests;

    fn text(s: &str) -> BinnacleText {
        BinnacleText {
            data: s.as_ptr(),
            len: s.len(),
        }
    }

    type Src = *mut BinnacleDescribedSource;
    type Call = Box<dyn Fn(Src, *mut BinnacleGrid) -> i32>;

    /// A source told of a one-column table, and the Grid it drives.
    fn described() -> (Src, *mut BinnacleGrid) {
        let table = r#"{"type":"table","table":"t","schema":{"fields":[{"name":"x","type":"integer"}],"primaryKey":["x"]}}"#;
        let grid = Box::into_raw(BinnacleGrid::from_grid(Grid::new(
            AxisConfig::new(0, 1.0),
            AxisConfig::new(0, 8.0),
        )));
        // SAFETY: the texts outlive the calls; the handles are live.
        unsafe {
            let src = binnacle_described_source_new(text("t"), text("ns"), std::ptr::null(), 0);
            let ok = binnacle_described_source_receive(src, grid, 1, table.as_ptr(), table.len());
            assert_eq!(ok, 1);
            (src, grid)
        }
    }

    /// Every call that takes a source, each given what makes it succeed
    /// once on a source from [`described`].
    fn calls() -> Vec<(&'static str, Call)> {
        let col = |src: Src| unsafe { (*src).source().column_key("x").unwrap().0 };
        let synced = b"{\"type\":\"synced\"}";
        // SAFETY (each): the handles are live and every output writable.
        vec![
            (
                "receive",
                Box::new(move |s, g| unsafe {
                    binnacle_described_source_receive(s, g, 1, synced.as_ptr(), synced.len())
                }),
            ),
            (
                "ended",
                Box::new(|s, g| unsafe { binnacle_described_source_ended(s, g) }),
            ),
            (
                "poll_request",
                Box::new(|s, _| unsafe {
                    binnacle_described_source_poll_request(s, &mut BinnacleWatchRequest::default())
                }),
            ),
            (
                "pump",
                Box::new(|s, g| unsafe { binnacle_described_source_pump(s, g) }),
            ),
            (
                "fill",
                Box::new(|s, g| unsafe {
                    binnacle_described_source_fill(s, g, &BinnacleWindowRequests::default())
                }),
            ),
            (
                "count",
                Box::new(|s, _| unsafe { binnacle_described_source_count(s, 0, &mut 0) }),
            ),
            (
                "status",
                Box::new(|s, _| unsafe {
                    binnacle_described_source_status(s, &mut BinnacleDescribedStatus::default())
                }),
            ),
            (
                "set_now",
                Box::new(|s, _| unsafe { binnacle_described_source_set_now(s, 0) }),
            ),
            (
                "set_utc_offset",
                Box::new(|s, g| unsafe { binnacle_described_source_set_utc_offset(s, g, 3600) }),
            ),
            (
                "cell",
                Box::new(move |s, _| unsafe {
                    binnacle_described_cell(s, 1, col(s), &mut BinnacleCell::default())
                }),
            ),
            (
                "cells",
                Box::new(move |s, _| unsafe {
                    let mut out = BinnacleCell::default();
                    binnacle_described_cells(s, &1, 1, &col(s), 1, &mut out, 1)
                }),
            ),
            (
                "column",
                Box::new(move |s, _| unsafe {
                    binnacle_described_column(s, col(s), &mut BinnacleColumnInfo::default())
                }),
            ),
            (
                "property",
                Box::new(move |s, _| unsafe {
                    binnacle_described_property(
                        s,
                        col(s),
                        text("type"),
                        &mut BinnacleText::default(),
                    )
                }),
            ),
            (
                "distinct",
                Box::new(move |s, g| unsafe {
                    let mut out = BinnacleDistinct::default();
                    binnacle_described_distinct(s, g, col(s), text(""), 10, &mut out)
                }),
            ),
        ]
    }

    /// # Safety
    /// Each handle is live and not used again.
    unsafe fn free(src: Src, grid: *mut BinnacleGrid) {
        // SAFETY: the caller's contract.
        unsafe {
            binnacle_described_source_free(src);
            crate::binnacle_grid_free(grid);
        }
    }

    // No panic leaves a call: one inside any call makes it return 0 and
    // poisons the source, so every call after on it returns 0 too, where
    // each returns 1 on a source that did not panic. Making a source and
    // freeing one catch theirs as well.
    #[test]
    fn a_panic_in_a_call_is_caught_and_poisons_the_source() {
        let calls = calls();
        let (src, grid) = described();
        for (name, call) in &calls {
            assert_eq!(call(src, grid), 1, "{name} on a sound source");
        }
        // SAFETY: as named.
        unsafe { free(src, grid) };
        for (name, call) in &calls {
            let (src, grid) = described();
            PANIC_NEXT.set(true);
            assert_eq!(call(src, grid), 0, "{name} panicking");
            assert!(!PANIC_NEXT.get(), "{name} did not run guarded");
            for (after, call) in &calls {
                assert_eq!(call(src, grid), 0, "{after} after {name} panicked");
            }
            // SAFETY: as named.
            unsafe { free(src, grid) };
        }
        PANIC_NEXT.set(true);
        // SAFETY: the texts outlive the call.
        let made =
            unsafe { binnacle_described_source_new(text("t"), text("ns"), std::ptr::null(), 0) };
        assert!(made.is_null());
        assert!(!PANIC_NEXT.get());
        let (src, grid) = described();
        PANIC_NEXT.set(true);
        // SAFETY: as named.
        unsafe { free(src, grid) };
        assert!(!PANIC_NEXT.get());
    }

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
